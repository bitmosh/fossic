use crate::types::StoredEvent;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

// ── Public types ──────────────────────────────────────────────────────────────

/// How a subscription is delivered.
pub enum SubscriptionMode {
    /// Callback fires while the write connection's lock is held, before
    /// `store.append()` returns. Panics are caught; subscription marked degraded.
    Synchronous,
    /// Callback fires from a per-subscription handler thread fed by a bounded
    /// channel. The store-level dispatcher thread fans events into per-sub channels.
    PostCommit { queue_size: usize },
}

/// Describes which stream pattern + branch a subscription targets.
pub struct SubscribeQuery {
    /// Glob pattern for the stream(s) to watch.
    /// `*` matches one path segment; `**` matches zero or more segments.
    pub stream_pattern: String,
    pub branch: String,
    /// When `false` (default), events from system streams (`_`-prefixed) are
    /// suppressed even when the glob pattern would match them.
    pub include_system: bool,
}

impl SubscribeQuery {
    /// Subscribe to the `main` branch of `stream_pattern` with system events excluded.
    pub fn stream(id: impl Into<String>) -> Self {
        SubscribeQuery {
            stream_pattern: id.into(),
            branch: "main".to_string(),
            include_system: false,
        }
    }
}

/// Implement this to receive events from a subscription.
pub trait SubscriptionHandler: Send + Sync + 'static {
    fn on_event(&self, event: &StoredEvent);
}

/// Handle returned from `Store::subscribe`. Drop to unsubscribe.
pub struct SubscriptionHandle {
    pub(crate) id: u64,
    pub(crate) degraded: Arc<AtomicBool>,
    pub(crate) registry: Arc<SubscriptionRegistry>,
}

impl SubscriptionHandle {
    pub fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Acquire)
    }

    /// Internal registry ID for this subscription. Useful for diagnostic joins.
    pub fn registry_id(&self) -> u64 {
        self.id
    }

    /// Current fill of the PostCommit delivery queue. `None` for Synchronous subscribers.
    pub fn queue_depth(&self) -> Option<usize> {
        self.registry.queue_info(self.id).map(|(depth, _)| depth)
    }

    /// Capacity of the PostCommit delivery queue. `None` for Synchronous subscribers.
    pub fn queue_capacity(&self) -> Option<usize> {
        self.registry.queue_info(self.id).and_then(|(_, cap)| cap)
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        self.registry.unsubscribe(self.id);
    }
}

// ── Internal entry types ──────────────────────────────────────────────────────

enum SubscriberKind {
    Synchronous {
        handler: Arc<dyn SubscriptionHandler>,
    },
    PostCommit {
        tx: crossbeam_channel::Sender<StoredEvent>,
        /// For exact (non-glob) subscriptions: last version seen on this (stream, branch).
        wal_cursor: i64,
        /// For glob subscriptions: per-(stream_id, branch) last version seen.
        /// Populated lazily as events arrive. The WAL watcher uses min(stream_cursors)
        /// as the scan start, so streams already seen are not re-delivered.
        stream_cursors: HashMap<(String, String), i64>,
    },
}

struct SubscriberEntry {
    stream_pattern: String,
    branch: String,
    include_system: bool,
    degraded: Arc<AtomicBool>,
    kind: SubscriberKind,
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub(crate) struct SubscriptionRegistry {
    entries: RwLock<HashMap<u64, SubscriberEntry>>,
    next_id: AtomicU64,
}

impl SubscriptionRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(SubscriptionRegistry {
            entries: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        })
    }

    /// Register a new subscription. Returns `(id, degraded_flag)`.
    ///
    /// For glob subscriptions, `initial_stream_cursors` pre-seeds the per-stream
    /// cursor map so that already-committed events are not replayed. Pass an empty
    /// map for exact-stream subscriptions (which use `initial_cursor` instead).
    pub fn subscribe(
        self: &Arc<Self>,
        q: SubscribeQuery,
        mode: SubscriptionMode,
        initial_cursor: i64,
        initial_stream_cursors: HashMap<(String, String), i64>,
        handler: Arc<dyn SubscriptionHandler>,
    ) -> (u64, Arc<AtomicBool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let degraded = Arc::new(AtomicBool::new(false));

        let kind = match mode {
            SubscriptionMode::Synchronous => SubscriberKind::Synchronous { handler },
            SubscriptionMode::PostCommit { queue_size } => {
                let (tx, rx) = crossbeam_channel::bounded::<StoredEvent>(queue_size);
                let handler_clone = Arc::clone(&handler);
                std::thread::spawn(move || {
                    for event in rx {
                        handler_clone.on_event(&event);
                    }
                });
                SubscriberKind::PostCommit {
                    tx,
                    wal_cursor: initial_cursor,
                    stream_cursors: initial_stream_cursors,
                }
            }
        };

        let entry = SubscriberEntry {
            stream_pattern: q.stream_pattern,
            branch: q.branch,
            include_system: q.include_system,
            degraded: Arc::clone(&degraded),
            kind,
        };

        self.entries.write().insert(id, entry);
        (id, degraded)
    }

    pub fn unsubscribe(&self, id: u64) {
        self.entries.write().remove(&id);
        // Dropping the entry drops the Sender, which closes the channel,
        // causing the handler thread to exit its for-loop.
    }

    pub fn has_subscribers(&self) -> bool {
        !self.entries.read().is_empty()
    }

    /// Fire all Synchronous subscribers. Panics are caught; subscription marked degraded.
    /// Must be called while the write lock is held (caller's responsibility).
    pub fn dispatch_sync(&self, event: &StoredEvent) {
        let entries = self.entries.read();
        for (id, entry) in entries.iter() {
            if entry.branch != event.branch {
                continue;
            }
            if event.stream_id.starts_with('_') && !entry.include_system {
                continue;
            }
            if !crate::glob::matches(&entry.stream_pattern, &event.stream_id) {
                continue;
            }
            if entry.degraded.load(Ordering::Acquire) {
                continue;
            }
            if let SubscriberKind::Synchronous { ref handler } = entry.kind {
                let handler_clone = Arc::clone(handler);
                let event_clone = event.clone();
                let degraded_flag = Arc::clone(&entry.degraded);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    handler_clone.on_event(&event_clone);
                }));
                if let Err(panic_val) = result {
                    let msg = if let Some(s) = panic_val.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_val.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    eprintln!(
                        "[WARN fossic] synchronous subscriber {} panicked: {}; marking degraded",
                        id, msg
                    );
                    degraded_flag.store(true, Ordering::Release);
                }
            }
        }
    }

    /// Try to send to all PostCommit subscribers. Returns list of newly degraded IDs.
    ///
    /// CURSOR OWNERSHIP INVARIANT: this is the ONLY code path that advances the
    /// in-process subscription cursors (`wal_cursor` / `stream_cursors` in
    /// `SubscriberKind::PostCommit`). The WAL watcher never advances cursors directly —
    /// it sends events through this same dispatcher, which then updates cursors after
    /// delivery. Exact subscriptions use `wal_cursor`; glob subscriptions use
    /// `stream_cursors` (a per-(stream_id, branch) map) to correctly track progress
    /// across multiple matched streams.
    pub fn dispatch_post_commit(&self, event: &StoredEvent) -> Vec<u64> {
        let mut newly_degraded = Vec::new();
        // (sub_id, stream_id, branch, version) — for cursor advancement after delivery
        let mut delivered: Vec<(u64, String, String, i64)> = Vec::new();

        {
            let entries = self.entries.read();
            for (id, entry) in entries.iter() {
                if entry.branch != event.branch {
                    continue;
                }
                if event.stream_id.starts_with('_') && !entry.include_system {
                    continue;
                }
                if !crate::glob::matches(&entry.stream_pattern, &event.stream_id) {
                    continue;
                }
                if entry.degraded.load(Ordering::Acquire) {
                    continue;
                }
                if let SubscriberKind::PostCommit { ref tx, wal_cursor, ref stream_cursors, .. } = entry.kind {
                    let is_exact = !entry.stream_pattern.contains('*');
                    let effective_cursor = if is_exact {
                        wal_cursor
                    } else {
                        *stream_cursors
                            .get(&(event.stream_id.clone(), event.branch.clone()))
                            .unwrap_or(&-1)
                    };
                    if (event.version as i64) <= effective_cursor {
                        continue;
                    }
                    match tx.try_send(event.clone()) {
                        Ok(_) => {
                            delivered.push((
                                *id,
                                event.stream_id.clone(),
                                event.branch.clone(),
                                event.version as i64,
                            ));
                        }
                        Err(crossbeam_channel::TrySendError::Full(_)) => {
                            entry.degraded.store(true, Ordering::Release);
                            newly_degraded.push(*id);
                        }
                        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                            // Handler thread exited (handle dropped); silently skip.
                        }
                    }
                }
            }
        } // read lock released

        // Advance cursors for delivered events (write lock, separate pass).
        if !delivered.is_empty() {
            let mut entries = self.entries.write();
            for (id, stream_id, branch, version) in delivered {
                if let Some(entry) = entries.get_mut(&id) {
                    if let SubscriberKind::PostCommit {
                        ref mut wal_cursor,
                        ref mut stream_cursors,
                        ..
                    } = entry.kind
                    {
                        let is_exact = !entry.stream_pattern.contains('*');
                        if is_exact {
                            if version > *wal_cursor {
                                *wal_cursor = version;
                            }
                        } else {
                            let e = stream_cursors.entry((stream_id, branch)).or_insert(-1);
                            if version > *e {
                                *e = version;
                            }
                        }
                    }
                }
            }
        }

        newly_degraded
    }

    /// Returns `(queue_depth, queue_capacity)` for the PostCommit subscriber with `id`.
    /// Returns `None` if the id is not found or is a Synchronous subscriber.
    pub(crate) fn queue_info(&self, id: u64) -> Option<(usize, Option<usize>)> {
        let entries = self.entries.read();
        entries.get(&id).and_then(|entry| {
            if let SubscriberKind::PostCommit { ref tx, .. } = entry.kind {
                Some((tx.len(), tx.capacity()))
            } else {
                None
            }
        })
    }

    /// Returns `(id, stream_pattern, branch, cursor)` for all PostCommit subscribers.
    ///
    /// For exact subscriptions: `cursor` is `wal_cursor`.
    /// For glob subscriptions: `cursor` is `min(stream_cursors)` (or -1 if empty).
    /// The WAL watcher uses this cursor as the scan-start for each group.
    pub fn post_commit_cursors(&self) -> Vec<(u64, String, String, i64)> {
        let entries = self.entries.read();
        let mut result = Vec::new();
        for (id, entry) in entries.iter() {
            if entry.degraded.load(Ordering::Acquire) {
                continue;
            }
            if let SubscriberKind::PostCommit { ref wal_cursor, ref stream_cursors, .. } = entry.kind {
                let is_exact = !entry.stream_pattern.contains('*');
                let cursor = if is_exact {
                    *wal_cursor
                } else if stream_cursors.is_empty() {
                    -1
                } else {
                    *stream_cursors.values().min().unwrap()
                };
                result.push((*id, entry.stream_pattern.clone(), entry.branch.clone(), cursor));
            }
        }
        result
    }
}
