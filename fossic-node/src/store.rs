use fossic::{ReadQuery, Store as FossicStore, SubscribeQuery, SubscriptionMode, WalkDirection};
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{
    iters::{FossicCausationIter, FossicCorrelationIter, FossicRangeIter},
    subscriptions::FossicSubscription,
    types::{
        parse_open_options, parse_sampling_mode, AppendJs, BranchInfoJs, CreateBranchJs, EventId,
        OpenOptionsJs, ReadOutcomeJs, ReadQueryJs, SamplingModeJs, StreamInfoJs, SubscribeQueryJs,
    },
};

fn parse_direction(direction: &str) -> Result<WalkDirection> {
    match direction {
        "forward" | "Forward" => Ok(WalkDirection::Forward),
        "backward" | "Backward" => Ok(WalkDirection::Backward),
        "both" | "Both" => Ok(WalkDirection::Both),
        other => Err(Error::new(
            Status::InvalidArg,
            format!("unknown direction: {other}"),
        )),
    }
}

// ── SnapshotStateJs ───────────────────────────────────────────────────────────

#[napi(object)]
pub struct SnapshotStateJs {
    pub version: BigInt,
    pub state_bytes: Buffer,
}

// ── Store wrapper ─────────────────────────────────────────────────────────────

/// The fossic event store.
///
/// All methods are async (Promise-based). `Store.open(path, options?)` is the
/// entry point. The store is cheaply cloneable (Arc-backed) — it's safe to share
/// across concurrent calls.
#[napi]
pub struct Store {
    pub(crate) inner: FossicStore,
}

#[napi]
impl Store {
    /// Open (or create) a fossic store at `path`.
    ///
    /// Returns a Promise that resolves to the opened `Store`.
    #[napi(factory)]
    pub fn open(path: String, options: Option<OpenOptionsJs>) -> Result<Self> {
        let opts = parse_open_options(options)?;
        let expanded = shellexpand::tilde(&path);
        FossicStore::open(expanded.as_ref(), opts)
            .map(|inner| Store { inner })
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
    }

    // ── Stream management ─────────────────────────────────────────────────────

    #[napi]
    pub async fn declare_stream(
        &self,
        stream_id: String,
        declared_by: String,
        description: Option<String>,
    ) -> Result<()> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .declare_stream(&stream_id, &declared_by, description.as_deref())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn streams(&self) -> Result<Vec<StreamInfoJs>> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .streams()
                .map(|v| v.into_iter().map(StreamInfoJs::from).collect())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn stream_exists(&self, stream_id: String) -> Result<bool> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .stream_exists(&stream_id)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Append ────────────────────────────────────────────────────────────────

    /// Append a single event and return its content-addressed `EventId`.
    ///
    /// If an identical event already exists (same CCE hash), `is_new` is false
    /// and the existing ID is returned without re-dispatching.
    #[napi]
    pub async fn append(&self, append: AppendJs) -> Result<EventId> {
        let store = self.inner.clone();
        let rust_append = fossic::Append::try_from(append)?;
        tokio::task::spawn_blocking(move || {
            store
                .append(rust_append)
                .map(|id| EventId { inner: id })
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn append_batch(&self, appends: Vec<AppendJs>) -> Result<Vec<EventId>> {
        let store = self.inner.clone();
        let rust_appends: Result<Vec<_>> = appends.into_iter().map(fossic::Append::try_from).collect();
        let rust_appends = rust_appends?;
        tokio::task::spawn_blocking(move || {
            store
                .append_batch(&rust_appends)
                .map(|ids| ids.into_iter().map(|id| EventId { inner: id }).collect())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    #[napi]
    pub async fn read_range(&self, query: ReadQueryJs) -> Result<Vec<crate::types::StoredEventJs>> {
        let store = self.inner.clone();
        let q = ReadQuery::from(query);
        tokio::task::spawn_blocking(move || {
            store
                .read_range(q)
                .map(|v| v.iter().map(crate::types::StoredEventJs::from).collect())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn read_one(&self, event_id: &EventId) -> Result<Option<crate::types::StoredEventJs>> {
        let store = self.inner.clone();
        let id = event_id.inner;
        tokio::task::spawn_blocking(move || {
            store
                .read_one(id)
                .map(|opt| opt.as_ref().map(crate::types::StoredEventJs::from))
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn read_by_external_id(
        &self,
        stream_id: String,
        external_id: String,
    ) -> Result<Option<crate::types::StoredEventJs>> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .read_by_external_id(&stream_id, &external_id)
                .map(|opt| opt.as_ref().map(crate::types::StoredEventJs::from))
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn read_by_correlation(
        &self,
        correlation_id: &EventId,
    ) -> Result<Vec<crate::types::StoredEventJs>> {
        let store = self.inner.clone();
        let id = correlation_id.inner;
        tokio::task::spawn_blocking(move || {
            store
                .read_by_correlation(id)
                .map(|v| v.iter().map(crate::types::StoredEventJs::from).collect())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn walk_causation(
        &self,
        start: &EventId,
        direction: String,
        max_depth: Option<u32>,
    ) -> Result<Vec<crate::types::StoredEventJs>> {
        let store = self.inner.clone();
        let start_id = start.inner;
        let dir = parse_direction(&direction)?;
        let depth = max_depth.map(|d| d as usize).unwrap_or(i64::MAX as usize);
        tokio::task::spawn_blocking(move || {
            store
                .walk_causation(start_id, dir, depth)
                .map(|v| v.iter().map(crate::types::StoredEventJs::from).collect())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Bounded reads ─────────────────────────────────────────────────────────

    /// Bounded range read. `cursor` is raw bytes from a previous truncated outcome's
    /// `nextCursor` field. The JS layer in `index.js` converts `TruncationCursor` ↔ Buffer.
    #[napi]
    pub async fn read_range_bounded(
        &self,
        query: ReadQueryJs,
        max_results: Option<u32>,
        max_bytes: Option<u32>,
        cursor: Option<Buffer>,
    ) -> Result<ReadOutcomeJs> {
        let store = self.inner.clone();
        let q = ReadQuery::from(query);
        let mr = max_results.map(|n| n as usize);
        let mb = max_bytes.map(|n| n as usize);
        let rust_cursor = cursor.map(|buf| fossic::TruncationCursor::from_bytes(buf.to_vec()));
        tokio::task::spawn_blocking(move || {
            store
                .read_range_bounded(q, mr, mb, rust_cursor)
                .map(ReadOutcomeJs::from_outcome)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn read_by_correlation_bounded(
        &self,
        correlation_id: &EventId,
        max_results: Option<u32>,
        max_bytes: Option<u32>,
        cursor: Option<Buffer>,
    ) -> Result<ReadOutcomeJs> {
        let store = self.inner.clone();
        let id = correlation_id.inner;
        let mr = max_results.map(|n| n as usize);
        let mb = max_bytes.map(|n| n as usize);
        let rust_cursor = cursor.map(|buf| fossic::TruncationCursor::from_bytes(buf.to_vec()));
        tokio::task::spawn_blocking(move || {
            store
                .read_by_correlation_bounded(id, mr, mb, rust_cursor)
                .map(ReadOutcomeJs::from_outcome)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn walk_causation_bounded(
        &self,
        start: &EventId,
        direction: String,
        max_depth: Option<u32>,
        sampling: Option<SamplingModeJs>,
        max_results: Option<u32>,
        max_bytes: Option<u32>,
        cursor: Option<Buffer>,
    ) -> Result<ReadOutcomeJs> {
        let store = self.inner.clone();
        let start_id = start.inner;
        let dir = parse_direction(&direction)?;
        let depth = max_depth.map(|d| d as usize).unwrap_or(i64::MAX as usize);
        let samp = parse_sampling_mode(sampling);
        let mr = max_results.map(|n| n as usize);
        let mb = max_bytes.map(|n| n as usize);
        let rust_cursor = cursor.map(|buf| fossic::TruncationCursor::from_bytes(buf.to_vec()));
        tokio::task::spawn_blocking(move || {
            store
                .walk_causation_bounded(start_id, dir, depth, samp, mr, mb, rust_cursor)
                .map(ReadOutcomeJs::from_outcome)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Streaming iterators ───────────────────────────────────────────────────

    /// Returns a lazy iterator over a range query. Pool connection released between batches.
    /// The JS layer in `index.js` wraps this with `[Symbol.asyncIterator]`.
    #[napi]
    pub fn read_range_iter(&self, query: ReadQueryJs) -> FossicRangeIter {
        let q = ReadQuery::from(query);
        FossicRangeIter::new(self.inner.read_range_iter(q))
    }

    #[napi]
    pub fn read_by_correlation_iter(&self, correlation_id: &EventId) -> FossicCorrelationIter {
        FossicCorrelationIter::new(self.inner.read_by_correlation_iter(correlation_id.inner))
    }

    #[napi]
    pub fn walk_causation_iter(
        &self,
        start: &EventId,
        direction: String,
        max_depth: Option<u32>,
        sampling: Option<SamplingModeJs>,
    ) -> Result<FossicCausationIter> {
        let dir = parse_direction(&direction)?;
        let depth = max_depth.map(|d| d as usize).unwrap_or(i64::MAX as usize);
        let samp = parse_sampling_mode(sampling);
        Ok(FossicCausationIter::new(
            self.inner.walk_causation_iter(start.inner, dir, depth, samp),
        ))
    }

    // ── Branches ──────────────────────────────────────────────────────────────

    #[napi]
    pub async fn list_branches(&self, stream_id: String) -> Result<Vec<BranchInfoJs>> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .list_branches(&stream_id)
                .map(|v| v.into_iter().map(BranchInfoJs::from).collect())
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn create_branch(&self, b: CreateBranchJs) -> Result<()> {
        let store = self.inner.clone();
        let rust_b = fossic::CreateBranch::try_from(b)?;
        tokio::task::spawn_blocking(move || {
            store
                .create_branch(&rust_b)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn promote_branch(&self, stream_id: String, branch_id: String) -> Result<()> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .promote_branch(&stream_id, &branch_id, None)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    #[napi]
    pub async fn mark_branch_dead_end(&self, stream_id: String, branch_id: String) -> Result<()> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .mark_branch_dead_end(&stream_id, &branch_id, None)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Subscription ──────────────────────────────────────────────────────────

    /// Subscribe to events on a stream. Returns a `FossicSubscription` that is
    /// both an `AsyncIterable<StoredEvent>` and has an explicit `unsubscribe()`.
    ///
    /// The subscription uses PostCommit mode with a bounded queue.
    /// When the queue overflows, the subscription is marked degraded.
    #[napi]
    pub fn subscribe(&self, query: SubscribeQueryJs) -> Result<FossicSubscription> {
        let (tx, rx) = crossbeam_channel::bounded::<fossic::StoredEvent>(
            query.queue_size.unwrap_or(1024) as usize,
        );
        let q = SubscribeQuery {
            stream_pattern: query.stream_pattern,
            branch: query.branch.unwrap_or_else(|| "main".to_string()),
            include_system: query.include_system.unwrap_or(false),
        };
        let mode = SubscriptionMode::PostCommit {
            queue_size: query.queue_size.unwrap_or(1024) as usize,
        };
        let handler = ChannelHandler { tx };
        let handle = self
            .inner
            .subscribe(q, mode, handler)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        Ok(FossicSubscription::new(handle, rx, closed))
    }

    // ── Cursor ────────────────────────────────────────────────────────────────

    /// Get the cursor for a consumer on a specific stream and branch.
    /// Returns `null` if no cursor has been set.
    #[napi]
    pub async fn get_cursor(
        &self,
        consumer_id: String,
        stream_id: String,
        branch: String,
    ) -> Result<Option<BigInt>> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .get_cursor(&consumer_id, &stream_id, &branch)
                .map(|opt| opt.map(BigInt::from))
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    /// Set the cursor for a consumer on a specific stream and branch.
    #[napi]
    pub async fn set_cursor(
        &self,
        consumer_id: String,
        stream_id: String,
        branch: String,
        version: BigInt,
    ) -> Result<()> {
        let store = self.inner.clone();
        let v = version.get_u64().1;
        tokio::task::spawn_blocking(move || {
            store
                .set_cursor(&consumer_id, &stream_id, &branch, v)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Snapshot primitives (for JS-side reducer support) ─────────────────────────

    /// Return the raw snapshot state bytes and version for the given reducer key.
    /// Returns `null` if no snapshot exists.
    #[napi]
    pub async fn get_snapshot_state(
        &self,
        stream_id: String,
        branch: String,
        reducer_name: String,
        state_schema_version: u32,
    ) -> Result<Option<SnapshotStateJs>> {
        let store = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            store
                .get_snapshot_state(&stream_id, &branch, &reducer_name, state_schema_version)
                .map(|opt| {
                    opt.map(|(version, bytes)| SnapshotStateJs {
                        version: BigInt::from(version),
                        state_bytes: bytes.into(),
                    })
                })
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    /// Write a snapshot row directly.
    ///
    /// Used by JS-side reducers to persist state for future `readState` calls.
    #[allow(clippy::too_many_arguments)]
    #[napi]
    pub async fn write_snapshot_state(
        &self,
        stream_id: String,
        branch: String,
        version: BigInt,
        reducer_name: String,
        reducer_version: u32,
        state_schema_version: u32,
        state_bytes: Buffer,
    ) -> Result<crate::types::SnapshotInfoJs> {
        let store = self.inner.clone();
        let v = version.get_u64().1;
        let bytes: Vec<u8> = state_bytes.to_vec();
        tokio::task::spawn_blocking(move || {
            store
                .write_snapshot_state(
                    &stream_id,
                    &branch,
                    v,
                    &reducer_name,
                    reducer_version,
                    state_schema_version,
                    &bytes,
                )
                .map(crate::types::SnapshotInfoJs::from)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }

    // ── Close ─────────────────────────────────────────────────────────────────

    /// Flush WAL hint. The store auto-closes when all JS references are GC'd.
    /// Calling this is optional; it is a no-op in the v1 binding.
    #[napi]
    pub fn close(&self) -> Result<()> {
        Ok(())
    }
}

// ── Internal subscription handler ────────────────────────────────────────────

struct ChannelHandler {
    tx: crossbeam_channel::Sender<fossic::StoredEvent>,
}

impl fossic::SubscriptionHandler for ChannelHandler {
    fn on_event(&self, event: &fossic::StoredEvent) {
        let _ = self.tx.try_send(event.clone());
    }
}
