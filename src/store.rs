use crate::system_stream::SystemStreamWriter;
use parking_lot::RwLock as ParkingRwLock;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, RwLock,
    },
};

use rusqlite::Connection;

use crate::{
    append::{append_batch_impl, append_if_impl, append_impl, AppendOutcome},
    branches::{
        create_branch_impl, list_branches_impl, mark_branch_dead_end_impl, promote_branch_impl,
        resolve_branch_chain, BranchSegment,
    },
    cross_stream::{
        aggregate_bounded_impl, aggregate_impl, read_by_correlation_bounded_impl,
        read_by_correlation_impl, walk_causation_bounded_impl, walk_causation_impl, Aggregate,
        AggregateQuery, WalkDirection,
    },
    cursors::{get_cursor_impl, set_cursor_impl},
    deletion::{purge_event_impl, shred_stream_impl},
    error::Error,
    read::{
        read_batch_impl, read_by_external_id_impl, read_one_impl, read_range_bounded_impl,
        read_range_impl,
    },
    reducers::{BoxedReducer, DynReducer, Reducer, ReducerRegistry, ReducerState},
    schema::{bootstrap_meta, bootstrap_system_streams, run_migrations},
    snapshots::{
        find_latest_snapshot, gc_orphaned_snapshots_impl, snapshot_info_impl, write_snapshot,
    },
    stream::{declare_stream_impl, stream_exists_impl, streams_impl},
    subscriptions::{
        SubscribeQuery, SubscriptionHandle, SubscriptionHandler, SubscriptionMode,
        SubscriptionRegistry,
    },
    transforms::{apply_transforms, PayloadTransform, TransformEntry},
    types::{
        Append, BranchInfo, CheckpointMode, CreateBranch, CursorInner, EncryptionMode, EventId,
        FirstOpenPolicy, OpenOptions, ReadOutcome, ReadQuery, SamplingMode, SnapshotInfo,
        SnapshotPolicy, StoredEvent, StreamInfo, TruncationCursor,
    },
    upcasters::{apply_upcaster, Upcaster, UpcasterRegistry},
    wal_watch::WalWatcher,
};

// ── Read pool guard ───────────────────────────────────────────────────────────

/// RAII guard for a pooled read connection. Returns the connection to the pool on drop.
struct ReadGuard {
    conn: Option<Connection>,
    pool: crossbeam_channel::Sender<Connection>,
}

impl Drop for ReadGuard {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let _ = self.pool.send(conn);
        }
    }
}

impl std::ops::Deref for ReadGuard {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().unwrap()
    }
}

// ── State monitor ─────────────────────────────────────────────────────────────

struct StateMonitor {
    state_sizes: Vec<usize>,
    last_emission_us: i64,
    apply_costs_us: Vec<u64>,
}

impl StateMonitor {
    fn push_state_size(&mut self, size: usize) {
        if self.state_sizes.len() >= 32 {
            self.state_sizes.remove(0);
        }
        self.state_sizes.push(size);
    }

    fn push_apply_cost(&mut self, cost: u64) {
        if self.apply_costs_us.len() >= 32 {
            self.apply_costs_us.remove(0);
        }
        self.apply_costs_us.push(cost);
    }

    fn mean_state_size(&self) -> usize {
        if self.state_sizes.is_empty() {
            return 0;
        }
        self.state_sizes.iter().sum::<usize>() / self.state_sizes.len()
    }

    fn avg_apply_cost_us(&self) -> u64 {
        if self.apply_costs_us.is_empty() {
            return 0;
        }
        self.apply_costs_us.iter().sum::<u64>() / self.apply_costs_us.len() as u64
    }
}

impl Default for StateMonitor {
    fn default() -> Self {
        StateMonitor {
            state_sizes: Vec::with_capacity(32),
            last_emission_us: 0,
            apply_costs_us: Vec::with_capacity(32),
        }
    }
}

// ── Internal state ────────────────────────────────────────────────────────────

struct StoreInner {
    conn: Mutex<Connection>,
    #[allow(dead_code)]
    path: PathBuf,
    options: OpenOptions,
    transforms: RwLock<Vec<TransformEntry>>,
    upcasters: RwLock<UpcasterRegistry>,
    sub_registry: Arc<SubscriptionRegistry>,
    dispatch_tx: crossbeam_channel::Sender<StoredEvent>,
    /// Channel for notifying the dispatcher thread of sync-subscriber degradations.
    /// Each tuple is (sub_id, stream_id, branch, dropped_version). The dispatcher
    /// drains this after each PostCommit fan-out to emit SubscriptionDegraded events
    /// for sync panics (SR-10 A-5).
    sync_degraded_tx: crossbeam_channel::Sender<(u64, String, String, u64)>,
    _wal_watcher: Option<WalWatcher>,
    /// Cached ancestor chains keyed by (stream_id, branch_id). Invalidated (per stream)
    /// when a new branch is created so the next resolution re-reads from the DB.
    branch_cache: RwLock<BTreeMap<(String, String), Vec<BranchSegment>>>,
    reducers: RwLock<ReducerRegistry>,
    similarity_provider: Option<Arc<dyn crate::similarity::SimilaritySearchProvider>>,
    /// Pooled read connections. All pure-read methods acquire from here so they never
    /// contend with the write mutex or with each other.
    read_pool_rx: crossbeam_channel::Receiver<Connection>,
    read_pool_tx: crossbeam_channel::Sender<Connection>,
    /// Peak depth ever observed in the post-commit dispatch channel.
    /// Updated at each `dispatch_tx.send` site. Diagnostic / observability only.
    dispatch_channel_high_water_mark: Arc<AtomicUsize>,
    // ── PHASE 6+7+8 FIELDS ───────────────────────────────────────────────────
    // Phase 6: per-(stream_id, branch) event counter for EveryNEvents policy.
    // Incremented during read_state; reset to 0 when a snapshot is taken.
    // Does not survive Store reopen (reset via initial_state at re-open time).
    snapshot_counters: parking_lot::RwLock<HashMap<(String, String), u32>>,
    // Lazy-initialized writer for reducer-side system events (ReducerStateLarge).
    // Separate from the dispatcher's SystemStreamWriter — reducer apply can run on
    // any thread, so we own a dedicated connection wrapped in a Mutex.
    reducer_system_writer: parking_lot::Mutex<Option<SystemStreamWriter>>,
    // Rolling state-size + apply-cost buffers, keyed by (stream_id, branch).
    state_monitors: parking_lot::Mutex<HashMap<(String, String), StateMonitor>>,
    // ── PHASE 7 FIELDS ───────────────────────────────────────────────────────
    quiescence: Arc<crate::executor::QuiescenceMonitor>,
    // ── PHASE 8 FIELDS ───────────────────────────────────────────────────────
    // Lazy-initialized writer for project discovery events (ProjectRegistered,
    // RelayHeartbeat). Dedicated connection so relay threads never contend with
    // the dispatcher or reducer writers.
    project_registry_writer: parking_lot::Mutex<Option<SystemStreamWriter>>,
    background_executor: parking_lot::Mutex<Option<crate::executor::BackgroundExecutor>>,
    // Wall-clock time at store open, in microseconds since Unix epoch.
    // Used as last_snapshot_us fallback when no snapshot has been taken yet.
    store_open_us: i64,
    // Per-(stream_id, branch) timestamp of the most recent snapshot scheduling.
    // Updated optimistically at schedule time to prevent storm-scheduling.
    last_snapshot_us: parking_lot::RwLock<HashMap<(String, String), i64>>,
}

// ── Public Store ──────────────────────────────────────────────────────────────

/// A fossic event store backed by a single SQLite file in WAL mode.
///
/// `Store` is cheaply cloneable (Arc-backed) and safe to share across threads.
#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
}

impl Drop for Store {
    fn drop(&mut self) {
        if self.inner.options.auto_gc_orphans && Arc::strong_count(&self.inner) == 1 {
            let _ = self.gc_orphaned_snapshots();
        }
    }
}

impl Store {
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    pub fn open(path: impl AsRef<Path>, options: OpenOptions) -> Result<Self, Error> {
        match options.encryption {
            EncryptionMode::Plaintext => {}
            EncryptionMode::OsKeyring | EncryptionMode::EnvVar(_) => {
                return Err(Error::NotImplemented {
                    feature: "encryption (OsKeyring / EnvVar); use Plaintext in v1",
                });
            }
        }

        match options.checkpoint_mode {
            CheckpointMode::Auto => {}
            CheckpointMode::Manual { .. } => {
                return Err(Error::NotImplemented {
                    feature: "CheckpointMode::Manual; only Auto is implemented in v1",
                });
            }
        }

        let path = path.as_ref().to_path_buf();

        match options.on_first_open {
            FirstOpenPolicy::RequireExisting if !path.exists() => {
                return Err(Error::StoreNotFound {
                    path: path.to_string_lossy().into_owned(),
                });
            }
            FirstOpenPolicy::CreateIfMissing => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)?;
                    }
                }
            }
            _ => {}
        }

        let conn = Connection::open(&path)?;

        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch("PRAGMA busy_timeout = 30000;")?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        run_migrations(&conn)?;
        bootstrap_meta(&conn, "plaintext")?;
        bootstrap_system_streams(&conn)?;

        let sub_registry = SubscriptionRegistry::new();
        let (dispatch_tx, dispatch_rx) = crossbeam_channel::unbounded::<StoredEvent>();
        let dispatch_hwm = Arc::new(AtomicUsize::new(0));

        let quiescence = Arc::new(crate::executor::QuiescenceMonitor::new());
        let (sync_degraded_tx, sync_degraded_rx) =
            crossbeam_channel::unbounded::<(u64, String, String, u64)>();
        start_dispatcher(
            path.clone(),
            dispatch_rx,
            sync_degraded_rx,
            Arc::clone(&sub_registry),
            Arc::clone(&quiescence),
        );

        let wal_watcher =
            WalWatcher::start(path.clone(), dispatch_tx.clone(), Arc::clone(&sub_registry))
                .map_err(|e| {
                    eprintln!("[WARN fossic] WAL watcher failed to start: {e}");
                })
                .ok();

        let similarity_provider = options.similarity_provider.clone();

        let pool_size = options.read_pool_size.max(1);
        let (read_pool_tx, read_pool_rx) = crossbeam_channel::bounded::<Connection>(pool_size);
        for _ in 0..pool_size {
            let rc = Connection::open(&path)?;
            rc.execute_batch(
                "PRAGMA journal_mode = WAL; \
                 PRAGMA busy_timeout = 30000; \
                 PRAGMA query_only = ON;",
            )?;
            read_pool_tx
                .send(rc)
                .map_err(|_| Error::Internal("read pool send failed during init".into()))?;
        }

        let grace_timeout =
            std::time::Duration::from_millis(options.background_executor_grace_timeout_ms);
        let quiescence_window_us = options.executor_quiescence_window_ms as i64 * 1_000;
        let store_open_us = crate::schema::now_us();

        let inner = Arc::new(StoreInner {
            conn: Mutex::new(conn),
            path,
            options,
            transforms: RwLock::new(Vec::new()),
            upcasters: RwLock::new(UpcasterRegistry::default()),
            sub_registry,
            dispatch_tx,
            sync_degraded_tx,
            _wal_watcher: wal_watcher,
            branch_cache: RwLock::new(BTreeMap::new()),
            reducers: RwLock::new(ReducerRegistry::default()),
            similarity_provider,
            read_pool_rx,
            read_pool_tx,
            dispatch_channel_high_water_mark: dispatch_hwm,
            snapshot_counters: ParkingRwLock::new(HashMap::new()),
            reducer_system_writer: parking_lot::Mutex::new(None),
            state_monitors: parking_lot::Mutex::new(HashMap::new()),
            quiescence: Arc::clone(&quiescence),
            background_executor: parking_lot::Mutex::new(None),
            store_open_us,
            last_snapshot_us: parking_lot::RwLock::new(HashMap::new()),
            project_registry_writer: parking_lot::Mutex::new(None),
        });

        // Spawn executor after Arc creation so we can downgrade to Weak.
        {
            let strong: Arc<dyn crate::executor::StoreOps> = inner.clone();
            let weak = Arc::downgrade(&strong);
            match crate::executor::BackgroundExecutor::spawn(
                weak,
                quiescence,
                inner.path.clone(),
                grace_timeout,
                quiescence_window_us,
            ) {
                Ok(executor) => {
                    *inner.background_executor.lock() = Some(executor);
                }
                Err(e) => {
                    eprintln!("[WARN fossic] fossic-bg spawn failed: {e}");
                }
            }
        }

        // Schedule recurring background GC when auto_gc_orphans is enabled.
        if inner.options.auto_gc_orphans {
            let ex = inner.background_executor.lock();
            if let Some(ref executor) = *ex {
                executor.schedule(crate::executor::BacklogTask {
                    priority: crate::executor::TaskPriority::Low,
                    deadline_us: store_open_us,
                    persist_on_drop: false,
                    kind: crate::executor::TaskKind::GcOrphanSnapshots,
                    recurring_interval: Some(std::time::Duration::from_secs(3600)),
                });
            }
        }

        Ok(Store { inner })
    }

    pub fn close(self) -> Result<(), Error> {
        drop(self);
        Ok(())
    }

    // ── Stream registry ───────────────────────────────────────────────────────

    pub fn declare_stream(
        &self,
        stream_id: &str,
        declared_by: &str,
        description: Option<&str>,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        declare_stream_impl(&conn, stream_id, declared_by, description)
    }

    pub fn streams(&self) -> Result<Vec<StreamInfo>, Error> {
        let conn = self.read_conn()?;
        streams_impl(&conn)
    }

    pub fn stream_exists(&self, stream_id: &str) -> Result<bool, Error> {
        let conn = self.read_conn()?;
        stream_exists_impl(&conn, stream_id)
    }

    // ── Append ────────────────────────────────────────────────────────────────

    /// Append a single event, firing registered payload transforms before
    /// CCE encoding so the resulting id reflects the transformed payload.
    pub fn append(&self, a: Append) -> Result<EventId, Error> {
        let has_subs = self.inner.sub_registry.has_subscribers();
        let is_system = a.stream_id.starts_with("_fossic/");

        let (payload_val, payload_bytes) =
            self.prepare_payload(&a.stream_id, &a.event_type, &a.payload)?;

        let (event_id, post_commit, sync_degraded) = {
            let mut conn = self.lock()?;
            let outcome: AppendOutcome = append_impl(&mut conn, &a, payload_val, payload_bytes)?;

            let (stored, degraded_ids) = if outcome.is_new && has_subs && !is_system {
                let s = build_stored_event(&outcome, &a);
                let ids = self.inner.sub_registry.dispatch_sync(&s);
                (Some(s), ids)
            } else {
                (None, Vec::new())
            };

            (outcome.event_id, stored, degraded_ids)
        }; // conn lock released

        self.inner.quiescence.note_write();

        if let Some(ref s) = post_commit {
            for sub_id in sync_degraded {
                let _ = self.inner.sync_degraded_tx.send((
                    sub_id,
                    s.stream_id.clone(),
                    s.branch.clone(),
                    s.version,
                ));
            }
        }
        if let Some(s) = post_commit {
            let depth = self.inner.dispatch_tx.len() + 1;
            self.inner
                .dispatch_channel_high_water_mark
                .fetch_max(depth, Ordering::Relaxed);
            let _ = self.inner.dispatch_tx.send(s);
        }

        Ok(event_id)
    }

    pub fn append_batch(&self, appends: &[Append]) -> Result<Vec<EventId>, Error> {
        if appends.is_empty() {
            return Ok(Vec::new());
        }
        let has_subs = self.inner.sub_registry.has_subscribers();

        let prepared: Vec<(serde_json::Value, Vec<u8>)> = appends
            .iter()
            .map(|a| self.prepare_payload(&a.stream_id, &a.event_type, &a.payload))
            .collect::<Result<_, _>>()?;

        let (ids, post_commits, sync_degraded) = {
            let mut conn = self.lock()?;
            let outcomes = append_batch_impl(&mut conn, appends, &prepared)?;

            let mut ids = Vec::with_capacity(outcomes.len());
            let mut post_commits = Vec::new();
            let mut sync_degraded: Vec<(u64, String, String, u64)> = Vec::new();

            for (outcome, a) in outcomes.iter().zip(appends.iter()) {
                ids.push(outcome.event_id);
                let is_system = a.stream_id.starts_with("_fossic/");
                if outcome.is_new && has_subs && !is_system {
                    let s = build_stored_event(outcome, a);
                    let newly_deg = self.inner.sub_registry.dispatch_sync(&s);
                    for sub_id in newly_deg {
                        sync_degraded.push((
                            sub_id,
                            s.stream_id.clone(),
                            s.branch.clone(),
                            s.version,
                        ));
                    }
                    post_commits.push(s);
                }
            }

            (ids, post_commits, sync_degraded)
        }; // conn lock released

        if !ids.is_empty() {
            self.inner.quiescence.note_write();
        }

        for (sub_id, stream_id, branch, version) in sync_degraded {
            let _ = self
                .inner
                .sync_degraded_tx
                .send((sub_id, stream_id, branch, version));
        }

        for s in post_commits {
            let depth = self.inner.dispatch_tx.len() + 1;
            self.inner
                .dispatch_channel_high_water_mark
                .fetch_max(depth, Ordering::Relaxed);
            let _ = self.inner.dispatch_tx.send(s);
        }

        Ok(ids)
    }

    /// Conditionally append a single event.
    ///
    /// `condition` is evaluated inside the IMMEDIATE transaction that would write
    /// the event. If it returns `Ok(false)`, the transaction is rolled back and
    /// `Ok(None)` is returned — no event is written and the stream version is
    /// unchanged. If it returns `Ok(true)`, the append proceeds and `Ok(Some(id))`
    /// is returned.
    ///
    /// The condition receives a `&rusqlite::Connection` (the in-progress transaction
    /// dereffed) and may run any read queries. It must not write. Errors returned by
    /// the condition propagate as `Err`.
    ///
    /// Typical use — compare-and-swap on stream version:
    /// ```ignore
    /// let id = store.append_if(a, |conn| {
    ///     let v: i64 = conn.query_row(
    ///         "SELECT COALESCE(MAX(version), -1) FROM events WHERE stream_id = ?1 AND branch = ?2",
    ///         rusqlite::params!["my/stream", "main"],
    ///         |r| r.get(0),
    ///     )?;
    ///     Ok(v == expected_version)
    /// })?;
    /// ```
    pub fn append_if<F>(&self, a: Append, condition: F) -> Result<Option<EventId>, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<bool, Error>,
    {
        let has_subs = self.inner.sub_registry.has_subscribers();
        let is_system = a.stream_id.starts_with("_fossic/");

        let (payload_val, payload_bytes) =
            self.prepare_payload(&a.stream_id, &a.event_type, &a.payload)?;

        let (event_id_opt, post_commit, sync_degraded) = {
            let mut conn = self.lock()?;
            let outcome = append_if_impl(&mut conn, &a, payload_val, payload_bytes, condition)?;

            match outcome {
                None => (None, None, Vec::new()),
                Some(outcome) => {
                    let (stored, degraded_ids) = if outcome.is_new && has_subs && !is_system {
                        let s = build_stored_event(&outcome, &a);
                        let ids = self.inner.sub_registry.dispatch_sync(&s);
                        (Some(s), ids)
                    } else {
                        (None, Vec::new())
                    };
                    (Some(outcome.event_id), stored, degraded_ids)
                }
            }
        }; // conn lock released

        if event_id_opt.is_some() {
            self.inner.quiescence.note_write();
        }

        if let Some(ref s) = post_commit {
            for sub_id in sync_degraded {
                let _ = self.inner.sync_degraded_tx.send((
                    sub_id,
                    s.stream_id.clone(),
                    s.branch.clone(),
                    s.version,
                ));
            }
        }
        if let Some(s) = post_commit {
            let depth = self.inner.dispatch_tx.len() + 1;
            self.inner
                .dispatch_channel_high_water_mark
                .fetch_max(depth, Ordering::Relaxed);
            let _ = self.inner.dispatch_tx.send(s);
        }

        Ok(event_id_opt)
    }

    // ── Subscriptions ─────────────────────────────────────────────────────────

    /// Subscribe to events on a stream+branch.
    ///
    /// The `SubscriptionHandle` must be held for as long as events should be
    /// delivered. Dropping it unsubscribes.
    pub fn subscribe<H: SubscriptionHandler>(
        &self,
        q: SubscribeQuery,
        mode: SubscriptionMode,
        handler: H,
    ) -> Result<SubscriptionHandle, Error> {
        // Seed the subscription cursor(s) from the current state so that
        // already-committed events are not replayed. For exact-stream subscriptions
        // this is a single MAX(version) query. For glob subscriptions we snapshot
        // MAX(version) per matching stream into stream_cursors; streams created after
        // subscription receive their first event correctly because dispatch uses
        // unwrap_or(&-1) for unknown streams.
        let is_glob = q.stream_pattern.contains('*');
        let (initial_cursor, initial_stream_cursors) = if is_glob {
            let conn = self.read_conn()?;
            let mut stmt = conn.prepare(
                "SELECT stream_id, COALESCE(MAX(version), -1) \
                 FROM events WHERE branch = ?1 GROUP BY stream_id",
            )?;
            let rows = stmt.query_map(rusqlite::params![q.branch], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            let mut seed: HashMap<(String, String), i64> = HashMap::new();
            for row in rows {
                let (stream_id, max_version) = row?;
                if crate::glob::matches(&q.stream_pattern, &stream_id) {
                    seed.insert((stream_id, q.branch.clone()), max_version);
                }
            }
            (-1i64, seed)
        } else {
            let conn = self.read_conn()?;
            let cursor = conn.query_row(
                "SELECT COALESCE(MAX(version), -1) \
                 FROM events WHERE stream_id = ?1 AND branch = ?2",
                rusqlite::params![q.stream_pattern, q.branch],
                |r| r.get(0),
            )?;
            (cursor, HashMap::new())
        };

        let handler_arc: Arc<dyn SubscriptionHandler> = Arc::new(handler);
        let (id, degraded) = self.inner.sub_registry.subscribe(
            q,
            mode,
            initial_cursor,
            initial_stream_cursors,
            handler_arc,
        );

        Ok(SubscriptionHandle {
            id,
            degraded,
            registry: Arc::clone(&self.inner.sub_registry),
        })
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    pub fn read_range(&self, q: ReadQuery) -> Result<Vec<StoredEvent>, Error> {
        let events = {
            let conn = self.read_conn()?;
            read_range_impl(&conn, q)?
        };
        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        events
            .into_iter()
            .map(|e| apply_upcaster(&upcasters, e))
            .collect()
    }

    /// Bounded variant of `read_range`. Stops at `max_results` events or `max_bytes`
    /// of payload, whichever comes first. Always returns at least one event.
    ///
    /// Budget resolution (per-call takes precedence over store-level defaults):
    /// - effective limit = `max_results` ?? `OpenOptions::default_max_results` ?? unbounded
    /// - effective bytes = `max_bytes` ?? `OpenOptions::default_max_bytes` ?? unbounded
    ///
    /// Pass `resume` from a previous `ReadOutcome::Truncated` to continue a paged read.
    pub fn read_range_bounded(
        &self,
        q: ReadQuery,
        max_results: Option<usize>,
        max_bytes: Option<usize>,
        resume: Option<TruncationCursor>,
    ) -> Result<ReadOutcome<Vec<StoredEvent>>, Error> {
        let effective_results = max_results.or(self.inner.options.default_max_results);
        let effective_bytes = max_bytes.or(self.inner.options.default_max_bytes);

        let resume_version = match resume {
            Some(cursor) => match cursor.decode()? {
                CursorInner::Range { next_version, .. } => Some(next_version),
                _ => {
                    return Err(Error::Internal(
                        "cursor type mismatch: expected Range".into(),
                    ))
                }
            },
            None => None,
        };

        let outcome = {
            let conn = self.read_conn()?;
            read_range_bounded_impl(
                &conn,
                &q,
                resume_version,
                effective_results,
                effective_bytes,
            )?
        };

        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;

        match outcome {
            ReadOutcome::Complete(events) => Ok(ReadOutcome::Complete(
                events
                    .into_iter()
                    .map(|e| apply_upcaster(&upcasters, e))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            ReadOutcome::Truncated {
                data,
                cursor,
                reason,
            } => Ok(ReadOutcome::Truncated {
                data: data
                    .into_iter()
                    .map(|e| apply_upcaster(&upcasters, e))
                    .collect::<Result<Vec<_>, _>>()?,
                cursor,
                reason,
            }),
        }
    }

    pub fn read_one(&self, id: EventId) -> Result<Option<StoredEvent>, Error> {
        let event = {
            let conn = self.read_conn()?;
            read_one_impl(&conn, id)?
        };
        match event {
            None => Ok(None),
            Some(e) => {
                let upcasters = self
                    .inner
                    .upcasters
                    .read()
                    .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
                Ok(Some(apply_upcaster(&upcasters, e)?))
            }
        }
    }

    /// Fetch multiple events by their CCE event IDs in a single query.
    ///
    /// Results are ordered by `timestamp_us ASC`. IDs not present in the store are
    /// silently omitted — compare the returned `Vec` length against the input to
    /// detect missing events. Upcasters are applied to every returned event.
    ///
    /// **SQLite parameter limit:** keep batch sizes ≤ 4,096 IDs per call.
    /// SQLite allows at most 32,766 bound parameters per statement; exceeding it
    /// returns a `StorageError`. Callers that need larger batches should chunk
    /// the input and call `read_batch` multiple times.
    pub fn read_batch(&self, ids: &[EventId]) -> Result<Vec<StoredEvent>, Error> {
        let events = {
            let conn = self.read_conn()?;
            read_batch_impl(&conn, ids)?
        };
        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        events
            .into_iter()
            .map(|e| apply_upcaster(&upcasters, e))
            .collect()
    }

    pub fn read_by_external_id(
        &self,
        stream_id: &str,
        external_id: &str,
    ) -> Result<Option<StoredEvent>, Error> {
        let event = {
            let conn = self.read_conn()?;
            read_by_external_id_impl(&conn, stream_id, external_id)?
        };
        match event {
            None => Ok(None),
            Some(e) => {
                let upcasters = self
                    .inner
                    .upcasters
                    .read()
                    .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
                Ok(Some(apply_upcaster(&upcasters, e)?))
            }
        }
    }

    // ── Cross-stream queries ──────────────────────────────────────────────────

    pub fn read_by_correlation(&self, correlation_id: EventId) -> Result<Vec<StoredEvent>, Error> {
        let events = {
            let conn = self.read_conn()?;
            read_by_correlation_impl(&conn, correlation_id)?
        };
        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        events
            .into_iter()
            .map(|e| apply_upcaster(&upcasters, e))
            .collect()
    }

    /// Bounded variant of `read_by_correlation`. Orders by `id ASC` (BLOB lexicographic)
    /// for deterministic resume — the cursor predicate is `id > last_seen_id`.
    ///
    /// Budget resolution (per-call takes precedence over store-level defaults):
    /// - effective limit = `max_results` ?? `OpenOptions::default_max_results` ?? unbounded
    /// - effective bytes = `max_bytes` ?? `OpenOptions::default_max_bytes` ?? unbounded
    ///
    /// Pass `resume` from a previous `ReadOutcome::Truncated` to continue a paged read.
    pub fn read_by_correlation_bounded(
        &self,
        correlation_id: EventId,
        max_results: Option<usize>,
        max_bytes: Option<usize>,
        resume: Option<TruncationCursor>,
    ) -> Result<ReadOutcome<Vec<StoredEvent>>, Error> {
        let effective_results = max_results.or(self.inner.options.default_max_results);
        let effective_bytes = max_bytes.or(self.inner.options.default_max_bytes);

        let resume_after_id = match resume {
            Some(cursor) => match cursor.decode()? {
                CursorInner::Correlation { last_seen_id, .. } => Some(last_seen_id),
                _ => {
                    return Err(Error::Internal(
                        "cursor type mismatch: expected Correlation".into(),
                    ))
                }
            },
            None => None,
        };

        let outcome = {
            let conn = self.read_conn()?;
            read_by_correlation_bounded_impl(
                &conn,
                correlation_id,
                resume_after_id,
                effective_results,
                effective_bytes,
            )?
        };

        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;

        match outcome {
            ReadOutcome::Complete(events) => Ok(ReadOutcome::Complete(
                events
                    .into_iter()
                    .map(|e| apply_upcaster(&upcasters, e))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            ReadOutcome::Truncated {
                data,
                cursor,
                reason,
            } => Ok(ReadOutcome::Truncated {
                data: data
                    .into_iter()
                    .map(|e| apply_upcaster(&upcasters, e))
                    .collect::<Result<Vec<_>, _>>()?,
                cursor,
                reason,
            }),
        }
    }

    pub fn walk_causation(
        &self,
        start: EventId,
        direction: WalkDirection,
        max_depth: usize,
    ) -> Result<Vec<StoredEvent>, Error> {
        let events = {
            let conn = self.read_conn()?;
            walk_causation_impl(&conn, start, direction, max_depth)?
        };
        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        events
            .into_iter()
            .map(|e| apply_upcaster(&upcasters, e))
            .collect()
    }

    /// Bounded BFS causation walk. Cuts at BFS level boundaries — always yields
    /// whole levels; the first level is always returned even if it exceeds the budget.
    ///
    /// Budget resolution (per-call takes precedence over store-level defaults):
    /// - effective limit = `max_results` ?? `OpenOptions::default_max_results` ?? unbounded
    /// - effective bytes = `max_bytes` ?? `OpenOptions::default_max_bytes` ?? unbounded
    ///
    /// `sampling` controls per-level node selection:
    /// - `Exhaustive` — all nodes at each level
    /// - `BreadthFirst { max_per_level }` — first `max_per_level` by `id ASC`
    /// - `Adaptive { target_count }` — computes `max_per_level = max(1, target_count / max_depth)`
    ///
    /// Pass `resume` from a previous `ReadOutcome::Truncated` to continue a paged walk.
    #[allow(clippy::too_many_arguments)]
    pub fn walk_causation_bounded(
        &self,
        start: EventId,
        direction: WalkDirection,
        max_depth: usize,
        sampling: SamplingMode,
        max_results: Option<usize>,
        max_bytes: Option<usize>,
        resume: Option<TruncationCursor>,
    ) -> Result<ReadOutcome<Vec<StoredEvent>>, Error> {
        let effective_results = max_results.or(self.inner.options.default_max_results);
        let effective_bytes = max_bytes.or(self.inner.options.default_max_bytes);

        let resume_state = match resume {
            Some(cursor) => match cursor.decode()? {
                CursorInner::Causation {
                    frontier,
                    direction: cursor_dir,
                    depth_consumed,
                } => {
                    let expected_dir: u8 = match &direction {
                        WalkDirection::Forward => 0,
                        WalkDirection::Backward => 1,
                        WalkDirection::Both => 2,
                    };
                    if cursor_dir != expected_dir {
                        return Err(Error::Internal(
                            "cursor direction mismatch for walk_causation_bounded".into(),
                        ));
                    }
                    Some((frontier, depth_consumed))
                }
                _ => {
                    return Err(Error::Internal(
                        "cursor type mismatch: expected Causation".into(),
                    ))
                }
            },
            None => None,
        };

        let outcome = {
            let conn = self.read_conn()?;
            walk_causation_bounded_impl(
                &conn,
                start,
                &direction,
                max_depth,
                sampling,
                resume_state,
                effective_results,
                effective_bytes,
            )?
        };

        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;

        match outcome {
            ReadOutcome::Complete(events) => Ok(ReadOutcome::Complete(
                events
                    .into_iter()
                    .map(|e| apply_upcaster(&upcasters, e))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            ReadOutcome::Truncated {
                data,
                cursor,
                reason,
            } => Ok(ReadOutcome::Truncated {
                data: data
                    .into_iter()
                    .map(|e| apply_upcaster(&upcasters, e))
                    .collect::<Result<Vec<_>, _>>()?,
                cursor,
                reason,
            }),
        }
    }

    pub fn aggregate<A: Aggregate>(
        &self,
        query: AggregateQuery,
        agg: A,
    ) -> Result<A::Output, Error> {
        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        let conn = self.read_conn()?;
        aggregate_impl(&conn, query, agg, &upcasters)
    }

    /// Bounded variant of `aggregate`. Folds events until `max_events_scanned` events have
    /// been processed or `max_bytes` of payload have accumulated, then returns
    /// `ReadOutcome::Complete(output)` or `ReadOutcome::Truncated { data: output, cursor: None, .. }`.
    ///
    /// On truncation the aggregator is cloned at the cut point and `finalize()` runs on
    /// the clone. `cursor` is always `None` — fold-resume requires re-feeding partial state
    /// into a new aggregator instance, which `Aggregate` does not yet support. Deferred to v1.2.x.
    ///
    /// Budget resolution (per-call takes precedence over store-level defaults):
    /// - effective events = `max_events_scanned` ?? `OpenOptions::default_max_results` ?? unbounded
    /// - effective bytes  = `max_bytes` ?? `OpenOptions::default_max_bytes` ?? unbounded
    pub fn aggregate_bounded<A: Aggregate + Clone>(
        &self,
        query: AggregateQuery,
        agg: A,
        max_events_scanned: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<ReadOutcome<A::Output>, Error> {
        let effective_events = max_events_scanned.or(self.inner.options.default_max_results);
        let effective_bytes = max_bytes.or(self.inner.options.default_max_bytes);
        let upcasters = self
            .inner
            .upcasters
            .read()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        let conn = self.read_conn()?;
        aggregate_bounded_impl(
            &conn,
            query,
            agg,
            &upcasters,
            effective_events,
            effective_bytes,
        )
    }

    // ── Streaming Iterators ───────────────────────────────────────────────────

    /// Returns an iterator over all events matching `query`, fetching `ITER_BATCH_SIZE` events
    /// per pool-connection acquire/release cycle. The pool connection is always released before
    /// the iterator yields to the caller — safe to use in long-running consumer loops.
    pub fn read_range_iter(&self, query: ReadQuery) -> RangeIter {
        RangeIter {
            store: self.clone(),
            query,
            resume: None,
            buffer: VecDeque::new(),
            exhausted: false,
        }
    }

    /// Returns an iterator over all events sharing `correlation_id`, fetching in batches.
    /// Pool connection is released before each yield.
    pub fn read_by_correlation_iter(&self, correlation_id: EventId) -> CorrelationIter {
        CorrelationIter {
            store: self.clone(),
            correlation_id,
            resume: None,
            buffer: VecDeque::new(),
            exhausted: false,
        }
    }

    /// Returns an iterator over the causation graph rooted at `start`, fetching in batches.
    /// Pool connection is released before each yield.
    pub fn walk_causation_iter(
        &self,
        start: EventId,
        direction: WalkDirection,
        max_depth: usize,
        sampling: SamplingMode,
    ) -> CausationIter {
        CausationIter {
            store: self.clone(),
            start,
            direction,
            max_depth,
            sampling,
            resume: None,
            buffer: VecDeque::new(),
            exhausted: false,
        }
    }

    // ── Upcasters ─────────────────────────────────────────────────────────────

    pub fn register_upcaster<U: Upcaster>(
        &self,
        event_type: &str,
        from: u32,
        to: u32,
        upcaster: U,
    ) -> Result<(), Error> {
        // Record registration in the audit table first (conn lock acquired and released).
        {
            let conn = self.lock()?;
            let now = crate::schema::now_us();
            conn.execute(
                "INSERT OR IGNORE INTO upcasters_registered \
                 (event_type, from_version, to_version, registered_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![event_type, from as i64, to as i64, now],
            )?;
        }
        // Then update the in-memory registry.
        let mut reg = self
            .inner
            .upcasters
            .write()
            .map_err(|_| Error::Internal("upcasters lock poisoned".into()))?;
        reg.register(event_type, from, to, Box::new(upcaster));
        Ok(())
    }

    // ── Payload transforms ────────────────────────────────────────────────────

    pub fn register_payload_transform<T: PayloadTransform>(
        &self,
        stream_pattern: &str,
        transform: T,
    ) -> Result<(), Error> {
        let mut transforms = self
            .inner
            .transforms
            .write()
            .map_err(|_| Error::Internal("transforms lock poisoned".into()))?;
        transforms.push(TransformEntry {
            pattern: stream_pattern.to_string(),
            transform: Box::new(transform),
        });
        Ok(())
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    pub fn purge_event(
        &self,
        id: EventId,
        confirm: &str,
        reason: &str,
        purged_by: &str,
    ) -> Result<(), Error> {
        let mut conn = self.lock()?;
        purge_event_impl(&mut conn, id, confirm, reason, purged_by)
    }

    pub fn shred_stream(&self, stream_id: &str, reason: &str) -> Result<(), Error> {
        shred_stream_impl(&self.inner.options.encryption, stream_id, reason)
    }

    // ── Cursors ───────────────────────────────────────────────────────────────

    pub fn get_cursor(
        &self,
        consumer_id: &str,
        stream_id: &str,
        branch: &str,
    ) -> Result<Option<u64>, Error> {
        let conn = self.read_conn()?;
        get_cursor_impl(&conn, consumer_id, stream_id, branch)
    }

    pub fn set_cursor(
        &self,
        consumer_id: &str,
        stream_id: &str,
        branch: &str,
        version: u64,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        set_cursor_impl(&conn, consumer_id, stream_id, branch, version)
    }

    // ── Branches ─────────────────────────────────────────────────────────────

    pub fn create_branch(&self, b: &CreateBranch) -> Result<(), Error> {
        let conn = self.lock()?;
        create_branch_impl(&conn, b)?;
        // Invalidate cached chains for this stream so the next resolve re-reads from DB.
        if let Ok(mut cache) = self.inner.branch_cache.write() {
            cache.retain(|(stream, _), _| stream != &b.stream_id);
        }
        Ok(())
    }

    pub fn promote_branch(
        &self,
        stream_id: &str,
        branch_id: &str,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        promote_branch_impl(&conn, stream_id, branch_id, reason)
    }

    pub fn mark_branch_dead_end(
        &self,
        stream_id: &str,
        branch_id: &str,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        let conn = self.lock()?;
        mark_branch_dead_end_impl(&conn, stream_id, branch_id, reason)
    }

    /// Returns only explicitly created diverged branches for `stream_id`.
    ///
    /// The implicit 'main' trunk is NOT included — it has no stored row.
    /// An empty `Vec` means the stream exists but no branches have been forked yet.
    /// Consumers wanting "is this an undiverged stream?" should check whether the
    /// returned slice is empty.
    pub fn list_branches(&self, stream_id: &str) -> Result<Vec<BranchInfo>, Error> {
        let conn = self.read_conn()?;
        list_branches_impl(&conn, stream_id)
    }

    /// Resolve the ancestor chain for a branch. Cached in memory after the first call.
    pub fn resolve_chain(
        &self,
        stream_id: &str,
        branch_id: &str,
    ) -> Result<Vec<BranchSegment>, Error> {
        let key = (stream_id.to_string(), branch_id.to_string());
        // Check cache first.
        if let Ok(cache) = self.inner.branch_cache.read() {
            if let Some(chain) = cache.get(&key) {
                return Ok(chain.clone());
            }
        }
        // Resolve from DB.
        let chain = {
            let conn = self.read_conn()?;
            resolve_branch_chain(&conn, stream_id, branch_id)?
        };
        // Insert into cache.
        if let Ok(mut cache) = self.inner.branch_cache.write() {
            cache.insert(key, chain.clone());
        }
        Ok(chain)
    }

    // ── Reducers ──────────────────────────────────────────────────────────────

    /// Register a reducer for all streams matching `pattern`.
    ///
    /// Pattern syntax: `*` = one segment, `**` = any number of segments.
    /// Raises `ReducerPatternAmbiguous` if the new pattern conflicts with an
    /// existing registration at the same specificity level.
    pub fn register_reducer<R: Reducer>(&self, pattern: &str, reducer: R) -> Result<(), Error> {
        let mut reg = self
            .inner
            .reducers
            .write()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.register(pattern, reducer)
    }

    /// Register a DynReducer for the given glob pattern.
    pub fn register_dyn_reducer(
        &self,
        pattern: &str,
        reducer: Box<dyn DynReducer>,
    ) -> Result<(), Error> {
        let mut reg = self
            .inner
            .reducers
            .write()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.register_dyn(pattern, reducer)
    }

    // ── PHASE 6+7+8 REDUCER METHODS ──────────────────────────────────────────

    /// Register a reducer with an explicit `SnapshotPolicy`.
    ///
    /// `SnapshotPolicy::Manual` is the default (same as `register_reducer`).
    /// `SnapshotPolicy::EveryNEvents(N)` automatically takes a snapshot after every
    /// N cumulative events applied during `read_state`. N = 0 returns
    /// `Error::SnapshotPolicyInvalid`.
    /// `SnapshotPolicy::EveryNSeconds(N)` schedules a background snapshot via
    /// `BackgroundExecutor` after N seconds of quiet time (quiescent window).
    /// N = 0 returns `Error::SnapshotPolicyInvalid`.
    pub fn register_reducer_with_policy<R: Reducer>(
        &self,
        pattern: &str,
        reducer: R,
        policy: SnapshotPolicy,
    ) -> Result<(), Error> {
        let mut reg = self
            .inner
            .reducers
            .write()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.register_with_policy(pattern, reducer, policy)
    }

    /// Register a DynReducer with an explicit `SnapshotPolicy`.
    pub fn register_dyn_reducer_with_policy(
        &self,
        pattern: &str,
        reducer: Box<dyn DynReducer>,
        policy: SnapshotPolicy,
    ) -> Result<(), Error> {
        let mut reg = self
            .inner
            .reducers
            .write()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.register_dyn_with_policy(pattern, reducer, policy)
    }

    /// Fold all events on `(stream_id, branch)` through the registered reducer
    /// and return the resulting state. Uses the most recent matching snapshot as a
    /// starting point, falling back to the initial state if none exists.
    pub fn read_state<S: ReducerState>(&self, stream_id: &str, branch: &str) -> Result<S, Error> {
        let (reducer, policy) = self.get_reducer_with_policy(stream_id)?;
        let (mut state_bytes, events) =
            self.compute_state_bytes(&reducer, stream_id, branch, None)?;
        let events_applied = events.len() as u32;
        for event in &events {
            let t0 = crate::schema::now_us();
            state_bytes = apply_reducer_guarded(&*reducer, &state_bytes, event, stream_id)?;
            let cost_us = (crate::schema::now_us() - t0).max(0) as u64;
            self.update_state_monitor(stream_id, branch, state_bytes.len(), cost_us);
        }
        self.maybe_emit_state_large(stream_id, branch)?;
        self.maybe_auto_snapshot(stream_id, branch, events_applied, &policy)?;
        rmp_serde::from_slice(&state_bytes).map_err(Error::MsgpackDecode)
    }

    /// Like `read_state` but only folds events up to and including `version`.
    pub fn read_state_at_version<S: ReducerState>(
        &self,
        stream_id: &str,
        branch: &str,
        version: u64,
    ) -> Result<S, Error> {
        let reducer = self.get_reducer(stream_id)?;
        let (mut state_bytes, events) =
            self.compute_state_bytes(&reducer, stream_id, branch, Some(version))?;
        for event in &events {
            state_bytes = apply_reducer_guarded(&*reducer, &state_bytes, event, stream_id)?;
        }
        rmp_serde::from_slice(&state_bytes).map_err(Error::MsgpackDecode)
    }

    /// Like `read_state` but returns raw msgpack bytes instead of deserializing.
    pub fn read_state_bytes(&self, stream_id: &str, branch: &str) -> Result<Vec<u8>, Error> {
        let (reducer, policy) = self.get_reducer_with_policy(stream_id)?;
        let (mut state_bytes, events) =
            self.compute_state_bytes(&reducer, stream_id, branch, None)?;
        let events_applied = events.len() as u32;
        for event in &events {
            let t0 = crate::schema::now_us();
            state_bytes = apply_reducer_guarded(&*reducer, &state_bytes, event, stream_id)?;
            let cost_us = (crate::schema::now_us() - t0).max(0) as u64;
            self.update_state_monitor(stream_id, branch, state_bytes.len(), cost_us);
        }
        self.maybe_emit_state_large(stream_id, branch)?;
        self.maybe_auto_snapshot(stream_id, branch, events_applied, &policy)?;
        Ok(state_bytes)
    }

    /// Like `read_state_at_version` but returns raw msgpack bytes.
    pub fn read_state_bytes_at_version(
        &self,
        stream_id: &str,
        branch: &str,
        version: u64,
    ) -> Result<Vec<u8>, Error> {
        let reducer = self.get_reducer(stream_id)?;
        let (mut state_bytes, events) =
            self.compute_state_bytes(&reducer, stream_id, branch, Some(version))?;
        for event in &events {
            state_bytes = apply_reducer_guarded(&*reducer, &state_bytes, event, stream_id)?;
        }
        Ok(state_bytes)
    }

    /// Like `read_state_at_version` but looks up the reducer by name rather than stream pattern.
    pub fn read_state_at_version_with_reducer<S: ReducerState>(
        &self,
        stream_id: &str,
        branch: &str,
        version: u64,
        reducer_name: &str,
    ) -> Result<S, Error> {
        let reducer = {
            let reg = self
                .inner
                .reducers
                .read()
                .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
            reg.find_by_name(reducer_name)
                .ok_or_else(|| Error::ReducerNotFoundByName {
                    name: reducer_name.to_string(),
                })?
        };
        let (mut state_bytes, events) =
            self.compute_state_bytes(&reducer, stream_id, branch, Some(version))?;
        for event in &events {
            state_bytes = apply_reducer_guarded(&*reducer, &state_bytes, event, stream_id)?;
        }
        rmp_serde::from_slice(&state_bytes).map_err(Error::MsgpackDecode)
    }

    // ── Snapshots ─────────────────────────────────────────────────────────────

    /// Compute the current state and persist it as a snapshot.
    ///
    /// Returns `NoEventsToSnapshot` when there are no events and no prior snapshot
    /// to base the new snapshot on.
    pub fn take_snapshot(&self, stream_id: &str, branch: &str) -> Result<SnapshotInfo, Error> {
        let reducer = self.get_reducer(stream_id)?;

        // TD-001: two separate acquisitions; a concurrent append between read and write
        // could produce a snapshot that misses recent events. See blast-radius pass-1.0.0w.
        let (snapshot_version, state_bytes, events) = {
            let conn = self.read_conn()?;
            let snap = find_latest_snapshot(
                &conn,
                stream_id,
                branch,
                reducer.name(),
                reducer.state_schema_version(),
                None,
            )?;
            let (start_v, bytes, prior_v) = match snap {
                Some((v, b)) => (v + 1, b, Some(v)),
                None => (0u64, reducer.initial_state_bytes()?, None),
            };
            let evs = read_range_impl(
                &conn,
                ReadQuery {
                    stream_id: stream_id.to_string(),
                    branch: branch.to_string(),
                    from_version: Some(start_v),
                    to_version: None,
                    limit: None,
                    event_type_filter: None,
                },
            )?;
            let snap_ver = if let Some(last) = evs.last() {
                last.version
            } else if prior_v.is_some() {
                // No new events since last snapshot — return existing info.
                return snapshot_info_impl(&conn, stream_id, branch, reducer.name()).map(
                    |opt| {
                        opt.ok_or_else(|| Error::NoEventsToSnapshot {
                            stream_id: stream_id.into(),
                            branch: branch.into(),
                        })
                    },
                )?;
            } else {
                return Err(Error::NoEventsToSnapshot {
                    stream_id: stream_id.into(),
                    branch: branch.into(),
                });
            };
            (snap_ver, bytes, evs)
        };

        let mut state = state_bytes;
        for event in &events {
            state = apply_reducer_guarded(&*reducer, &state, event, stream_id)?;
        }

        let conn = self.lock()?;
        write_snapshot(
            &conn,
            stream_id,
            branch,
            snapshot_version,
            reducer.name(),
            reducer.version(),
            reducer.state_schema_version(),
            &state,
        )
    }

    /// Return metadata for the most recent snapshot on `(stream_id, branch)`.
    pub fn snapshot_info(
        &self,
        stream_id: &str,
        branch: &str,
        reducer_name: &str,
    ) -> Result<Option<SnapshotInfo>, Error> {
        let conn = self.read_conn()?;
        snapshot_info_impl(&conn, stream_id, branch, reducer_name)
    }

    /// Delete snapshots whose `(reducer_name, state_schema_version)` no longer matches
    /// any currently registered reducer.
    pub fn gc_orphaned_snapshots(&self) -> Result<usize, Error> {
        let keys = {
            let reg = self
                .inner
                .reducers
                .read()
                .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
            reg.active_keys()
        };
        let conn = self.lock()?;
        gc_orphaned_snapshots_impl(&conn, &keys)
    }

    /// Return the raw state bytes and version from the latest snapshot for the given key.
    /// Returns None if no snapshot exists.
    pub fn get_snapshot_state(
        &self,
        stream_id: &str,
        branch: &str,
        reducer_name: &str,
        state_schema_version: u32,
    ) -> Result<Option<(u64, Vec<u8>)>, Error> {
        let conn = self.read_conn()?;
        find_latest_snapshot(
            &conn,
            stream_id,
            branch,
            reducer_name,
            state_schema_version,
            None,
        )
    }

    /// Write a snapshot row directly (used by foreign-language reducers that manage their own state).
    #[allow(clippy::too_many_arguments)]
    pub fn write_snapshot_state(
        &self,
        stream_id: &str,
        branch: &str,
        version: u64,
        reducer_name: &str,
        reducer_version: u32,
        state_schema_version: u32,
        state_bytes: &[u8],
    ) -> Result<SnapshotInfo, Error> {
        let conn = self.lock()?;
        write_snapshot(
            &conn,
            stream_id,
            branch,
            version,
            reducer_name,
            reducer_version,
            state_schema_version,
            state_bytes,
        )
    }

    // ── Phase 8: Project Registry ─────────────────────────────────────────────

    /// Emit a `ProjectRegistered` event to `_fossic/system`.
    ///
    /// Call on relay agent startup and on first hub-direct write to announce
    /// this project's local store and relay pattern. Best-effort: errors are
    /// logged internally and the call always returns `Ok(())`.
    pub fn emit_project_registered(
        &self,
        source_store: &str,
        local_store_path: &str,
        subscribe_pattern: &str,
        project_description: &str,
    ) -> Result<(), Error> {
        let mut guard = self.inner.project_registry_writer.lock();
        if guard.is_none() {
            *guard = SystemStreamWriter::new(&self.inner.path);
        }
        if let Some(writer) = guard.as_mut() {
            crate::registry::emit_project_registered(
                writer,
                source_store,
                local_store_path,
                subscribe_pattern,
                project_description,
            );
        }
        Ok(())
    }

    /// Emit a `RelayHeartbeat` event to `_fossic/system`.
    ///
    /// Call from the relay heartbeat thread at the configured interval.
    /// Best-effort: errors are logged internally and the call always returns `Ok(())`.
    pub fn emit_relay_heartbeat(
        &self,
        source_store: &str,
        last_event_version: i64,
        queue_lag: u64,
        uptime_us: i64,
    ) -> Result<(), Error> {
        let mut guard = self.inner.project_registry_writer.lock();
        if guard.is_none() {
            *guard = SystemStreamWriter::new(&self.inner.path);
        }
        if let Some(writer) = guard.as_mut() {
            crate::registry::emit_relay_heartbeat(
                writer,
                source_store,
                last_event_version,
                queue_lag,
                uptime_us,
            );
        }
        Ok(())
    }

    // ── Similarity ────────────────────────────────────────────────────────────

    pub fn similarity_query(
        &self,
        q: crate::similarity::SimilarityQuery,
    ) -> Result<Vec<crate::similarity::SimilarityHit>, Error> {
        match &self.inner.similarity_provider {
            Some(provider) => provider.query(q),
            None => Err(Error::NotImplemented {
                feature: "similarity_query: no SimilaritySearchProvider wired in OpenOptions",
            }),
        }
    }

    // ── Observability ─────────────────────────────────────────────────────────

    /// Current number of undelivered events queued in the post-commit dispatch channel.
    pub fn dispatch_channel_pressure(&self) -> usize {
        self.inner.dispatch_tx.len()
    }

    /// Historical peak depth ever observed in the post-commit dispatch channel
    /// since this store instance was opened. Useful for tuning queue sizes and
    /// detecting back-pressure under high write load.
    pub fn dispatch_channel_high_water_mark(&self) -> usize {
        self.inner
            .dispatch_channel_high_water_mark
            .load(Ordering::Relaxed)
    }

    /// Schedule a custom background task on this store's executor.
    ///
    /// No-op if the store was opened without a background executor (rare).
    /// The task executes on the next quiescent window after `task.deadline_us`.
    pub fn schedule_task(&self, task: crate::executor::BacklogTask) {
        if let Some(ref exec) = *self.inner.background_executor.lock() {
            exec.schedule(task);
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, Error> {
        self.inner
            .conn
            .lock()
            .map_err(|_| Error::Internal("store mutex poisoned".to_string()))
    }

    /// Acquire a read connection from the pool. Blocks up to `read_pool_timeout_ms` if all connections are busy.
    fn read_conn(&self) -> Result<ReadGuard, Error> {
        let pool_size = self.inner.options.read_pool_size.max(1);
        let timeout_ms = self.inner.options.read_pool_timeout_ms;
        self.inner
            .read_pool_rx
            .recv_timeout(std::time::Duration::from_millis(timeout_ms))
            .map(|conn| ReadGuard {
                conn: Some(conn),
                pool: self.inner.read_pool_tx.clone(),
            })
            .map_err(|_| Error::PoolExhausted {
                pool_size,
                timeout_ms,
            })
    }

    /// Acquire a read connection and hold it for `hold_ms` milliseconds, then release.
    /// Test-only helper for simulating pool exhaustion; named with `_test_` prefix by convention.
    pub fn _test_hold_read_conn(&self, hold_ms: u64) {
        let _guard = self.read_conn().expect("acquire read conn for test");
        std::thread::sleep(std::time::Duration::from_millis(hold_ms));
    }

    /// Look up the reducer Arc for `stream_id`, or return `ReducerNotFound`.
    fn get_reducer(&self, stream_id: &str) -> Result<Arc<dyn BoxedReducer>, Error> {
        let reg = self
            .inner
            .reducers
            .read()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.find_arc(stream_id)
            .ok_or_else(|| Error::ReducerNotFound {
                stream_id: stream_id.into(),
            })
    }

    /// Look up the reducer Arc + its SnapshotPolicy for `stream_id`.
    fn get_reducer_with_policy(
        &self,
        stream_id: &str,
    ) -> Result<(Arc<dyn BoxedReducer>, SnapshotPolicy), Error> {
        let reg = self
            .inner
            .reducers
            .read()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.find_arc_with_policy(stream_id)
            .ok_or_else(|| Error::ReducerNotFound {
                stream_id: stream_id.into(),
            })
    }

    /// Check whether the SnapshotPolicy for this (stream_id, branch) has fired;
    /// take a snapshot in-band if so and reset the counter.
    ///
    /// Only wired for `read_state` / `read_state_bytes` (full reads).
    /// `read_state_at_version` and `read_state_bytes_at_version` are historical
    /// reads and do not advance the auto-snapshot counter.
    fn maybe_auto_snapshot(
        &self,
        stream_id: &str,
        branch: &str,
        events_applied: u32,
        policy: &SnapshotPolicy,
    ) -> Result<(), Error> {
        let should_snap = match policy {
            SnapshotPolicy::EveryNEvents(n) => {
                let mut counters = self.inner.snapshot_counters.write();
                let key = (stream_id.to_string(), branch.to_string());
                let counter = counters.entry(key).or_insert(0);
                *counter += events_applied;
                if *counter >= *n {
                    *counter = 0;
                    true
                } else {
                    false
                }
            }
            SnapshotPolicy::StateAdaptive {
                target_replay_cost_us,
                min_events_between,
            } => {
                let accumulated = {
                    let mut counters = self.inner.snapshot_counters.write();
                    let key = (stream_id.to_string(), branch.to_string());
                    let counter = counters.entry(key).or_insert(0);
                    *counter += events_applied;
                    *counter
                };
                if accumulated < *min_events_between {
                    return Ok(());
                }
                let avg_cost = {
                    let monitors = self.inner.state_monitors.lock();
                    monitors
                        .get(&(stream_id.to_string(), branch.to_string()))
                        .map(|m| m.avg_apply_cost_us())
                        .unwrap_or(0)
                };
                let replay_cost_estimate = (accumulated as u64).saturating_mul(avg_cost);
                if replay_cost_estimate > *target_replay_cost_us as u64 {
                    let mut counters = self.inner.snapshot_counters.write();
                    let key = (stream_id.to_string(), branch.to_string());
                    if let Some(c) = counters.get_mut(&key) {
                        *c = 0;
                    }
                    true
                } else {
                    false
                }
            }
            SnapshotPolicy::EveryNSeconds(n) => {
                let key = (stream_id.to_string(), branch.to_string());
                let window_us = *n as i64 * 1_000_000;
                let now = crate::schema::now_us();
                let last = {
                    let map = self.inner.last_snapshot_us.read();
                    map.get(&key).copied().unwrap_or(self.inner.store_open_us)
                };
                if now - last >= window_us {
                    // Optimistic update prevents storm-scheduling between
                    // the schedule call and the executor's next quiescent window.
                    {
                        let mut map = self.inner.last_snapshot_us.write();
                        map.insert(key, now);
                    }
                    self.schedule_background_snapshot(stream_id, branch);
                }
                return Ok(());
            }
            _ => return Ok(()),
        };

        if should_snap {
            self.take_snapshot(stream_id, branch)
                .map(|_| ())
                .or_else(|e| match e {
                    // NoEventsToSnapshot is benign — the stream is empty or
                    // the snapshot is already current; skip silently.
                    Error::NoEventsToSnapshot { .. } => Ok(()),
                    other => Err(other),
                })?;
        }
        Ok(())
    }

    fn schedule_background_snapshot(&self, stream_id: &str, branch: &str) {
        let ex = self.inner.background_executor.lock();
        if let Some(ref executor) = *ex {
            executor.schedule(crate::executor::BacklogTask {
                priority: crate::executor::TaskPriority::Normal,
                deadline_us: crate::schema::now_us(),
                persist_on_drop: false,
                kind: crate::executor::TaskKind::TakeSnapshot {
                    stream_id: stream_id.to_string(),
                    branch: branch.to_string(),
                },
                recurring_interval: None,
            });
        }
    }

    fn update_state_monitor(
        &self,
        stream_id: &str,
        branch: &str,
        state_len: usize,
        apply_cost_us: u64,
    ) {
        let mut monitors = self.inner.state_monitors.lock();
        let key = (stream_id.to_string(), branch.to_string());
        let monitor = monitors.entry(key).or_default();
        monitor.push_state_size(state_len);
        monitor.push_apply_cost(apply_cost_us);
    }

    fn maybe_emit_state_large(&self, stream_id: &str, branch: &str) -> Result<(), Error> {
        let threshold = self.inner.options.reducer_state_large_threshold_bytes;
        if threshold == usize::MAX {
            return Ok(());
        }
        let mean_size = {
            let mut monitors = self.inner.state_monitors.lock();
            let key = (stream_id.to_string(), branch.to_string());
            match monitors.get_mut(&key) {
                None => return Ok(()),
                Some(m) => {
                    let mean = m.mean_state_size();
                    if mean <= threshold {
                        return Ok(());
                    }
                    let now = crate::schema::now_us();
                    if now - m.last_emission_us < 60_000_000 {
                        return Ok(());
                    }
                    m.last_emission_us = now;
                    mean
                }
            }
        };
        let payload = serde_json::json!({
            "stream_id": stream_id,
            "branch": branch,
            "mean_state_bytes": mean_size,
            "threshold_bytes": threshold,
        });
        let mut writer_guard = self.inner.reducer_system_writer.lock();
        if writer_guard.is_none() {
            *writer_guard = SystemStreamWriter::new(&self.inner.path);
        }
        if let Some(ref mut writer) = *writer_guard {
            writer.emit("ReducerStateLarge", &payload, None);
        }
        Ok(())
    }

    /// Load snapshot + read events, returning `(initial_state_bytes, events_to_apply)`.
    ///
    /// When `max_version` is `Some(v)`, only events with version <= v are returned
    /// and the snapshot is bounded to version <= v.
    fn compute_state_bytes(
        &self,
        reducer: &Arc<dyn BoxedReducer>,
        stream_id: &str,
        branch: &str,
        max_version: Option<u64>,
    ) -> Result<(Vec<u8>, Vec<StoredEvent>), Error> {
        let conn = self.read_conn()?;
        let snap = find_latest_snapshot(
            &conn,
            stream_id,
            branch,
            reducer.name(),
            reducer.state_schema_version(),
            max_version,
        )?;
        let (start_v, bytes) = match snap {
            Some((v, b)) => (v + 1, b),
            None => (0u64, reducer.initial_state_bytes()?),
        };
        let events = read_range_impl(
            &conn,
            ReadQuery {
                stream_id: stream_id.to_string(),
                branch: branch.to_string(),
                from_version: Some(start_v),
                to_version: max_version,
                limit: None,
                event_type_filter: None,
            },
        )?;
        Ok((bytes, events))
    }

    /// Encode `payload` to msgpack, apply registered transforms, and decode back
    /// to `serde_json::Value` for CCE id derivation.
    ///
    /// Returns `(value_for_id, bytes_for_storage)`.
    fn prepare_payload(
        &self,
        stream_id: &str,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> Result<(serde_json::Value, Vec<u8>), Error> {
        let raw_bytes = rmp_serde::to_vec(payload)?;
        let transforms = self
            .inner
            .transforms
            .read()
            .map_err(|_| Error::Internal("transforms lock poisoned".into()))?;
        let final_bytes = apply_transforms(&transforms, stream_id, event_type, raw_bytes)?;
        // If no transforms matched the bytes are unchanged; decode for CCE.
        let final_value: serde_json::Value = rmp_serde::from_slice(&final_bytes)?;
        Ok((final_value, final_bytes))
    }
}

// ── StoreOps impl ─────────────────────────────────────────────────────────────

impl crate::executor::StoreOps for StoreInner {
    fn bg_gc_orphaned_snapshots(&self) -> Result<usize, crate::error::Error> {
        let keys = {
            let reg = self
                .reducers
                .read()
                .map_err(|_| crate::error::Error::Internal("reducers lock poisoned".into()))?;
            reg.active_keys()
        };
        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::error::Error::Internal("write conn lock poisoned".into()))?;
        gc_orphaned_snapshots_impl(&conn, &keys)
    }

    fn bg_take_snapshot(
        &self,
        stream_id: &str,
        branch: &str,
    ) -> Result<crate::types::SnapshotInfo, crate::error::Error> {
        // Replicates Store::take_snapshot using StoreInner's raw fields.
        let reducer = {
            let reg = self
                .reducers
                .read()
                .map_err(|_| crate::error::Error::Internal("reducers lock poisoned".into()))?;
            reg.find_arc(stream_id)
                .ok_or_else(|| crate::error::Error::ReducerNotFound {
                    stream_id: stream_id.into(),
                })?
        };

        let (snapshot_version, state_bytes, events) = {
            let conn = self
                .read_pool_rx
                .recv_timeout(std::time::Duration::from_millis(
                    self.options.read_pool_timeout_ms,
                ))
                .map(|c| ReadGuard {
                    conn: Some(c),
                    pool: self.read_pool_tx.clone(),
                })
                .map_err(|_| crate::error::Error::PoolExhausted {
                    pool_size: self.options.read_pool_size.max(1),
                    timeout_ms: self.options.read_pool_timeout_ms,
                })?;

            let snap = find_latest_snapshot(
                &conn,
                stream_id,
                branch,
                reducer.name(),
                reducer.state_schema_version(),
                None,
            )?;
            let (start_v, bytes, prior_v) = match snap {
                Some((v, b)) => (v + 1, b, Some(v)),
                None => (0u64, reducer.initial_state_bytes()?, None),
            };
            let evs = read_range_impl(
                &conn,
                ReadQuery {
                    stream_id: stream_id.to_string(),
                    branch: branch.to_string(),
                    from_version: Some(start_v),
                    to_version: None,
                    limit: None,
                    event_type_filter: None,
                },
            )?;
            let snap_ver = if let Some(last) = evs.last() {
                last.version
            } else if prior_v.is_some() {
                return snapshot_info_impl(&conn, stream_id, branch, reducer.name()).map(
                    |opt| {
                        opt.ok_or_else(|| crate::error::Error::NoEventsToSnapshot {
                            stream_id: stream_id.into(),
                            branch: branch.into(),
                        })
                    },
                )?;
            } else {
                return Err(crate::error::Error::NoEventsToSnapshot {
                    stream_id: stream_id.into(),
                    branch: branch.into(),
                });
            };
            (snap_ver, bytes, evs)
        };

        let mut state = state_bytes;
        for event in &events {
            state = apply_reducer_guarded(&*reducer, &state, event, stream_id)?;
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| crate::error::Error::Internal("store mutex poisoned".into()))?;
        let info = write_snapshot(
            &conn,
            stream_id,
            branch,
            snapshot_version,
            reducer.name(),
            reducer.version(),
            reducer.state_schema_version(),
            &state,
        )?;

        // Record snapshot timestamp for EveryNSeconds quiescence gate.
        let mut map = self.last_snapshot_us.write();
        map.insert(
            (stream_id.to_string(), branch.to_string()),
            crate::schema::now_us(),
        );

        Ok(info)
    }
}

// ── Streaming Iterators ───────────────────────────────────────────────────────
//
// Each iterator acquires a pool connection once per internal batch, releases it
// before yielding any events, then repeats. The pool is never held across an
// `Iterator::next` boundary, so long-running consumer loops cannot starve
// concurrent readers.
//
// ITER_BATCH_SIZE controls internal fetch granularity; it is not observable to callers.
// All three iterators are fused: after returning `None` they return `None` forever.

const ITER_BATCH_SIZE: usize = 100;

// ── RangeIter ─────────────────────────────────────────────────────────────────

/// Streaming iterator over `read_range`. See `Store::read_range_iter`.
pub struct RangeIter {
    store: Store,
    query: ReadQuery,
    resume: Option<TruncationCursor>,
    buffer: VecDeque<StoredEvent>,
    exhausted: bool,
}

impl RangeIter {
    fn fetch_batch(&mut self) -> Result<(), Error> {
        let resume = self.resume.take();
        match self.store.read_range_bounded(
            self.query.clone(),
            Some(ITER_BATCH_SIZE),
            None,
            resume,
        )? {
            ReadOutcome::Complete(events) => {
                self.buffer.extend(events);
                self.exhausted = true;
            }
            ReadOutcome::Truncated { data, cursor, .. } => {
                self.buffer.extend(data);
                self.resume = cursor;
            }
        }
        Ok(())
    }
}

impl Iterator for RangeIter {
    type Item = Result<StoredEvent, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted && self.buffer.is_empty() {
            return None;
        }
        if self.buffer.is_empty() {
            if let Err(e) = self.fetch_batch() {
                self.exhausted = true;
                return Some(Err(e));
            }
            if self.buffer.is_empty() {
                self.exhausted = true;
                return None;
            }
        }
        self.buffer.pop_front().map(Ok)
    }
}

impl std::iter::FusedIterator for RangeIter {}

// ── CorrelationIter ───────────────────────────────────────────────────────────

/// Streaming iterator over `read_by_correlation`. See `Store::read_by_correlation_iter`.
pub struct CorrelationIter {
    store: Store,
    correlation_id: EventId,
    resume: Option<TruncationCursor>,
    buffer: VecDeque<StoredEvent>,
    exhausted: bool,
}

impl CorrelationIter {
    fn fetch_batch(&mut self) -> Result<(), Error> {
        let resume = self.resume.take();
        match self.store.read_by_correlation_bounded(
            self.correlation_id,
            Some(ITER_BATCH_SIZE),
            None,
            resume,
        )? {
            ReadOutcome::Complete(events) => {
                self.buffer.extend(events);
                self.exhausted = true;
            }
            ReadOutcome::Truncated { data, cursor, .. } => {
                self.buffer.extend(data);
                self.resume = cursor;
            }
        }
        Ok(())
    }
}

impl Iterator for CorrelationIter {
    type Item = Result<StoredEvent, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted && self.buffer.is_empty() {
            return None;
        }
        if self.buffer.is_empty() {
            if let Err(e) = self.fetch_batch() {
                self.exhausted = true;
                return Some(Err(e));
            }
            if self.buffer.is_empty() {
                self.exhausted = true;
                return None;
            }
        }
        self.buffer.pop_front().map(Ok)
    }
}

impl std::iter::FusedIterator for CorrelationIter {}

// ── CausationIter ─────────────────────────────────────────────────────────────

/// Streaming iterator over `walk_causation`. See `Store::walk_causation_iter`.
pub struct CausationIter {
    store: Store,
    start: EventId,
    direction: WalkDirection,
    max_depth: usize,
    sampling: SamplingMode,
    resume: Option<TruncationCursor>,
    buffer: VecDeque<StoredEvent>,
    exhausted: bool,
}

impl CausationIter {
    fn fetch_batch(&mut self) -> Result<(), Error> {
        let resume = self.resume.take();
        match self.store.walk_causation_bounded(
            self.start,
            self.direction,
            self.max_depth,
            self.sampling,
            Some(ITER_BATCH_SIZE),
            None,
            resume,
        )? {
            ReadOutcome::Complete(events) => {
                self.buffer.extend(events);
                self.exhausted = true;
            }
            ReadOutcome::Truncated { data, cursor, .. } => {
                self.buffer.extend(data);
                self.resume = cursor;
            }
        }
        Ok(())
    }
}

impl Iterator for CausationIter {
    type Item = Result<StoredEvent, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted && self.buffer.is_empty() {
            return None;
        }
        if self.buffer.is_empty() {
            if let Err(e) = self.fetch_batch() {
                self.exhausted = true;
                return Some(Err(e));
            }
            if self.buffer.is_empty() {
                self.exhausted = true;
                return None;
            }
        }
        self.buffer.pop_front().map(Ok)
    }
}

impl std::iter::FusedIterator for CausationIter {}

// ── Module-level helpers ──────────────────────────────────────────────────────

/// Wrap one `reducer.apply_bytes` call with panic isolation.
///
/// On panic the caller's thread continues; the error carries stream/reducer/event context
/// for structured diagnosis (SR-10 A-11).
fn apply_reducer_guarded(
    reducer: &dyn crate::reducers::BoxedReducer,
    state_bytes: &[u8],
    event: &StoredEvent,
    stream_id: &str,
) -> Result<Vec<u8>, Error> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        reducer.apply_bytes(state_bytes, &event.payload)
    })) {
        Ok(inner) => inner,
        Err(panic_val) => {
            let msg = if let Some(s) = panic_val.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_val.downcast_ref::<String>() {
                s.clone()
            } else {
                "<non-string panic payload>".to_string()
            };
            let eid_hex: String = event
                .id
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            Err(Error::ReducerPanicked {
                stream_id: stream_id.to_string(),
                reducer_name: reducer.name().to_string(),
                event_id_hex: eid_hex,
                panic_message: msg,
            })
        }
    }
}

/// Build a `StoredEvent` from append outcome + original Append, without a DB round-trip.
fn build_stored_event(outcome: &AppendOutcome, a: &Append) -> StoredEvent {
    StoredEvent {
        id: outcome.event_id,
        stream_id: a.stream_id.clone(),
        branch: a.branch.clone(),
        version: outcome.version,
        timestamp_us: outcome.timestamp_us,
        causation_id: a.causation_id,
        correlation_id: a.correlation_id,
        event_type: a.event_type.clone(),
        type_version: a.type_version,
        payload: outcome.payload_bytes.clone(),
        external_id: a.external_id.clone(),
        indexed_tags: a.indexed_tags.clone(),
    }
}

/// Spawn the per-store dispatcher thread.
///
/// The thread exits when `dispatch_rx` disconnects (i.e., when `StoreInner` drops
/// and the last `dispatch_tx` clone is released).
fn start_dispatcher(
    db_path: PathBuf,
    dispatch_rx: crossbeam_channel::Receiver<StoredEvent>,
    sync_degraded_rx: crossbeam_channel::Receiver<(u64, String, String, u64)>,
    registry: Arc<SubscriptionRegistry>,
    quiescence: Arc<crate::executor::QuiescenceMonitor>,
) {
    std::thread::spawn(move || {
        let mut sys_writer = crate::system_stream::SystemStreamWriter::new(&db_path);

        for event in &dispatch_rx {
            // Never dispatch events on internal _fossic/* streams to avoid degraded loops.
            if event.stream_id.starts_with("_fossic/") {
                continue;
            }

            let newly_degraded = registry.dispatch_post_commit(&event);

            quiescence.note_dispatch();

            if let Some(ref mut writer) = sys_writer {
                // PostCommit overflow degradations from this event.
                for sub_id in newly_degraded {
                    writer.emit_subscription_degraded(
                        sub_id,
                        &event.stream_id,
                        &event.branch,
                        event.version,
                    );
                }
                // Sync-subscriber panic degradations forwarded from append paths (SR-10 A-5).
                // Drained here (after write lock released) to keep system event emission
                // off the write hot path.
                while let Ok((sub_id, stream_id, branch, version)) = sync_degraded_rx.try_recv() {
                    writer.emit_subscription_degraded(sub_id, &stream_id, &branch, version);
                }
            }
        }
    });
}
