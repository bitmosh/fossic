use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, RwLock},
};

use rusqlite::{Connection, TransactionBehavior};

use crate::{
    append::{append_batch_impl, append_if_impl, append_impl, AppendOutcome},
    branches::{
        create_branch_impl, list_branches_impl, mark_branch_dead_end_impl, promote_branch_impl,
        resolve_branch_chain, BranchSegment,
    },
    cross_stream::{
        aggregate_impl, read_by_correlation_impl, walk_causation_impl, Aggregate, AggregateQuery,
        WalkDirection,
    },
    cursors::{get_cursor_impl, set_cursor_impl},
    deletion::{purge_event_impl, shred_stream_impl},
    error::Error,
    read::{read_batch_impl, read_by_external_id_impl, read_one_impl, read_range_impl},
    reducers::{BoxedReducer, DynReducer, Reducer, ReducerRegistry, ReducerState},
    schema::{bootstrap_meta, bootstrap_system_streams, now_us, run_migrations},
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
        Append, BranchInfo, CheckpointMode, CreateBranch, EncryptionMode, EventId, FirstOpenPolicy,
        OpenOptions, ReadQuery, SnapshotInfo, StoredEvent, StreamInfo,
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
}

// ── Public Store ──────────────────────────────────────────────────────────────

/// A fossic event store backed by a single SQLite file in WAL mode.
///
/// `Store` is cheaply cloneable (Arc-backed) and safe to share across threads.
#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
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
        let (dispatch_tx, dispatch_rx) =
            crossbeam_channel::unbounded::<StoredEvent>();

        start_dispatcher(path.clone(), dispatch_rx, Arc::clone(&sub_registry));

        let wal_watcher = WalWatcher::start(
            path.clone(),
            dispatch_tx.clone(),
            Arc::clone(&sub_registry),
        )
        .map_err(|e| {
            eprintln!("[WARN fossic] WAL watcher failed to start: {e}");
        })
        .ok();

        let similarity_provider = options.similarity_provider.clone();

        let pool_size = options.read_pool_size.max(1);
        let (read_pool_tx, read_pool_rx) =
            crossbeam_channel::bounded::<Connection>(pool_size);
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

        Ok(Store {
            inner: Arc::new(StoreInner {
                conn: Mutex::new(conn),
                path,
                options,
                transforms: RwLock::new(Vec::new()),
                upcasters: RwLock::new(UpcasterRegistry::default()),
                sub_registry,
                dispatch_tx,
                _wal_watcher: wal_watcher,
                branch_cache: RwLock::new(BTreeMap::new()),
                reducers: RwLock::new(ReducerRegistry::default()),
                similarity_provider,
                read_pool_rx,
                read_pool_tx,
            }),
        })
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

        let (event_id, post_commit) = {
            let mut conn = self.lock()?;
            let outcome: AppendOutcome =
                append_impl(&mut conn, &a, payload_val, payload_bytes)?;

            let stored = if outcome.is_new && has_subs && !is_system {
                let s = build_stored_event(&outcome, &a);
                self.inner.sub_registry.dispatch_sync(&s);
                Some(s)
            } else {
                None
            };

            (outcome.event_id, stored)
        }; // conn lock released

        if let Some(s) = post_commit {
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

        let (ids, post_commits) = {
            let mut conn = self.lock()?;
            let outcomes = append_batch_impl(&mut conn, appends, &prepared)?;

            let mut ids = Vec::with_capacity(outcomes.len());
            let mut post_commits = Vec::new();

            for (outcome, a) in outcomes.iter().zip(appends.iter()) {
                ids.push(outcome.event_id);
                let is_system = a.stream_id.starts_with("_fossic/");
                if outcome.is_new && has_subs && !is_system {
                    let s = build_stored_event(outcome, a);
                    self.inner.sub_registry.dispatch_sync(&s);
                    post_commits.push(s);
                }
            }

            (ids, post_commits)
        }; // conn lock released

        for s in post_commits {
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

        let (event_id_opt, post_commit) = {
            let mut conn = self.lock()?;
            let outcome =
                append_if_impl(&mut conn, &a, payload_val, payload_bytes, condition)?;

            match outcome {
                None => (None, None),
                Some(outcome) => {
                    let stored = if outcome.is_new && has_subs && !is_system {
                        let s = build_stored_event(&outcome, &a);
                        self.inner.sub_registry.dispatch_sync(&s);
                        Some(s)
                    } else {
                        None
                    };
                    (Some(outcome.event_id), stored)
                }
            }
        }; // conn lock released

        if let Some(s) = post_commit {
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
        let (id, degraded) =
            self.inner
                .sub_registry
                .subscribe(q, mode, initial_cursor, initial_stream_cursors, handler_arc);

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

    pub fn read_by_correlation(
        &self,
        correlation_id: EventId,
    ) -> Result<Vec<StoredEvent>, Error> {
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
    pub fn register_reducer<R: Reducer>(
        &self,
        pattern: &str,
        reducer: R,
    ) -> Result<(), Error> {
        let mut reg = self
            .inner
            .reducers
            .write()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.register(pattern, reducer)
    }

    /// Register a DynReducer for the given glob pattern.
    pub fn register_dyn_reducer(&self, pattern: &str, reducer: Box<dyn DynReducer>) -> Result<(), Error> {
        let mut reg = self
            .inner
            .reducers
            .write()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.register_dyn(pattern, reducer)
    }

    /// Fold all events on `(stream_id, branch)` through the registered reducer
    /// and return the resulting state. Uses the most recent matching snapshot as a
    /// starting point, falling back to the initial state if none exists.
    pub fn read_state<S: ReducerState>(
        &self,
        stream_id: &str,
        branch: &str,
    ) -> Result<S, Error> {
        let reducer = self.get_reducer(stream_id)?;
        let (mut state_bytes, events) = self.compute_state_bytes(
            &reducer,
            stream_id,
            branch,
            None,
        )?;
        for event in &events {
            state_bytes = reducer.apply_bytes(&state_bytes, &event.payload)?;
        }
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
        let (mut state_bytes, events) = self.compute_state_bytes(
            &reducer,
            stream_id,
            branch,
            Some(version),
        )?;
        for event in &events {
            state_bytes = reducer.apply_bytes(&state_bytes, &event.payload)?;
        }
        rmp_serde::from_slice(&state_bytes).map_err(Error::MsgpackDecode)
    }

    /// Like `read_state` but returns raw msgpack bytes instead of deserializing.
    pub fn read_state_bytes(&self, stream_id: &str, branch: &str) -> Result<Vec<u8>, Error> {
        let reducer = self.get_reducer(stream_id)?;
        let (mut state_bytes, events) =
            self.compute_state_bytes(&reducer, stream_id, branch, None)?;
        for event in &events {
            state_bytes = reducer.apply_bytes(&state_bytes, &event.payload)?;
        }
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
            state_bytes = reducer.apply_bytes(&state_bytes, &event.payload)?;
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
            state_bytes = reducer.apply_bytes(&state_bytes, &event.payload)?;
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
                return snapshot_info_impl(&conn, stream_id, branch, reducer.name())
                    .map(|opt| opt.ok_or_else(|| Error::NoEventsToSnapshot {
                        stream_id: stream_id.into(),
                        branch: branch.into(),
                    }))?;
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
            state = reducer.apply_bytes(&state, &event.payload)?;
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
        find_latest_snapshot(&conn, stream_id, branch, reducer_name, state_schema_version, None)
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

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, Error> {
        self.inner
            .conn
            .lock()
            .map_err(|_| Error::Internal("store mutex poisoned".to_string()))
    }

    /// Acquire a read connection from the pool. Blocks up to 30s if all connections are busy.
    fn read_conn(&self) -> Result<ReadGuard, Error> {
        let pool_size = self.inner.options.read_pool_size.max(1);
        self.inner
            .read_pool_rx
            .recv_timeout(std::time::Duration::from_millis(30_000))
            .map(|conn| ReadGuard {
                conn: Some(conn),
                pool: self.inner.read_pool_tx.clone(),
            })
            .map_err(|_| Error::PoolExhausted {
                pool_size,
                timeout_ms: 30_000,
            })
    }

    /// Look up the reducer Arc for `stream_id`, or return `ReducerNotFound`.
    fn get_reducer(&self, stream_id: &str) -> Result<Arc<dyn BoxedReducer>, Error> {
        let reg = self
            .inner
            .reducers
            .read()
            .map_err(|_| Error::Internal("reducers lock poisoned".into()))?;
        reg.find_arc(stream_id).ok_or_else(|| Error::ReducerNotFound {
            stream_id: stream_id.into(),
        })
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
        let final_bytes =
            apply_transforms(&transforms, stream_id, event_type, raw_bytes)?;
        // If no transforms matched the bytes are unchanged; decode for CCE.
        let final_value: serde_json::Value = rmp_serde::from_slice(&final_bytes)?;
        Ok((final_value, final_bytes))
    }
}

// ── Module-level helpers ──────────────────────────────────────────────────────

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
    registry: Arc<SubscriptionRegistry>,
) {
    std::thread::spawn(move || {
        // Open a dedicated write connection for SubscriptionDegraded events.
        let mut sys_conn = Connection::open(&db_path)
            .inspect(|c| {
                let _ = c.execute_batch(
                    "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 30000;",
                );
            })
            .map_err(|e| {
                eprintln!("[WARN fossic] dispatcher: failed to open system connection: {e}");
            })
            .ok();

        for event in &dispatch_rx {
            // Never dispatch events on internal _fossic/* streams to avoid degraded loops.
            if event.stream_id.starts_with("_fossic/") {
                continue;
            }

            let newly_degraded = registry.dispatch_post_commit(&event);

            if let Some(ref mut conn) = sys_conn {
                for sub_id in newly_degraded {
                    write_degraded_event(conn, sub_id, &event.stream_id, &event.branch, event.version);
                }
            }
        }
    });
}

/// Write a `SubscriptionDegraded` event to `_fossic/system`.
/// Errors are silently ignored — best-effort delivery for diagnostic purposes.
fn write_degraded_event(
    conn: &mut Connection,
    sub_id: u64,
    stream_id: &str,
    branch: &str,
    dropped_version: u64,
) {
    use crate::cce::derive_event_id;

    let payload = serde_json::json!({
        "subscription_id": sub_id,
        "stream_id": stream_id,
        "branch": branch,
        "dropped_version": dropped_version,
    });

    let payload_bytes = match rmp_serde::to_vec(&payload) {
        Ok(b) => b,
        Err(_) => return,
    };

    let event_id_bytes = match derive_event_id("SubscriptionDegraded", 1, None, &payload) {
        Ok(b) => b,
        Err(_) => return,
    };
    let event_id = EventId::from_bytes(event_id_bytes);

    let ts = now_us();

    let tx = match conn.transaction_with_behavior(TransactionBehavior::Immediate) {
        Ok(t) => t,
        Err(_) => return,
    };

    let next_version: i64 = match tx.query_row(
        "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
         WHERE stream_id = '_fossic/system' AND branch = 'main'",
        [],
        |r| r.get(0),
    ) {
        Ok(v) => v,
        Err(_) => return,
    };

    let _ = tx.execute(
        "INSERT OR IGNORE INTO events \
         (id, stream_id, branch, version, timestamp_us, event_type, type_version, payload) \
         VALUES (?1, '_fossic/system', 'main', ?2, ?3, 'SubscriptionDegraded', 1, ?4)",
        rusqlite::params![event_id, next_version, ts, payload_bytes],
    );
    let _ = tx.commit();
}
