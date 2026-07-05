use fossic::{
    AggregateQuery, EventId, ReadQuery, Store, SubscribeQuery, SubscriptionHandler,
    SubscriptionMode,
};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    serialization::{
        parse_cursor, parse_direction, parse_sampling_mode, SerializedBranchInfo, SerializedEvent,
        SerializedReadOutcome, SerializedStreamInfo,
    },
    SubscriptionMap,
};

// ── Error type ────────────────────────────────────────────────────────────────

/// Typed error returned by Tauri commands.
///
/// The `code` field is machine-readable; `message` is human-readable.
/// Tauri serialises this as JSON: `{ "code": "ReducerNotFound", "message": "..." }`.
#[derive(serde::Serialize)]
pub struct FossicTauriError {
    pub code: &'static str,
    pub message: String,
}

impl From<fossic::Error> for FossicTauriError {
    fn from(e: fossic::Error) -> Self {
        let code = match &e {
            fossic::Error::StreamNotDeclared { .. } => "StreamNotDeclared",
            fossic::Error::ReducerNotFound { .. } => "ReducerNotFound",
            fossic::Error::ReducerNotFoundByName { .. } => "ReducerNotFoundByName",
            fossic::Error::ReducerError { .. } => "ReducerError",
            fossic::Error::BranchNotFound { .. } => "BranchNotFound",
            fossic::Error::NotImplemented { .. } => "NotImplemented",
            fossic::Error::SchemaMismatch { .. } => "SchemaMismatch",
            _ => "StorageError",
        };
        FossicTauriError {
            code,
            message: e.to_string(),
        }
    }
}

fn direction_err(msg: String) -> FossicTauriError {
    FossicTauriError {
        code: "StorageError",
        message: msg,
    }
}

fn sampling_err(msg: String) -> FossicTauriError {
    FossicTauriError {
        code: "StorageError",
        message: msg,
    }
}

fn cursor_err(msg: String) -> FossicTauriError {
    FossicTauriError {
        code: "StorageError",
        message: msg,
    }
}

// ── Stream commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn fossic_list_streams(
    store: State<'_, Store>,
) -> Result<Vec<SerializedStreamInfo>, FossicTauriError> {
    store
        .streams()
        .map(|v| v.into_iter().map(SerializedStreamInfo::from).collect())
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_list_branches(
    store: State<'_, Store>,
    stream_id: String,
) -> Result<Vec<SerializedBranchInfo>, FossicTauriError> {
    store
        .list_branches(&stream_id)
        .map(|v| v.into_iter().map(SerializedBranchInfo::from).collect())
        .map_err(FossicTauriError::from)
}

// ── Read commands ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn fossic_read_range(
    store: State<'_, Store>,
    stream_id: String,
    branch: Option<String>,
    from_version: Option<u64>,
    to_version: Option<u64>,
    limit: Option<usize>,
    event_type_filter: Option<String>,
) -> Result<Vec<SerializedEvent>, FossicTauriError> {
    let mut q = ReadQuery::stream(stream_id);
    if let Some(b) = branch {
        q.branch = b;
    }
    if let Some(v) = from_version {
        q.from_version = Some(v);
    }
    if let Some(v) = to_version {
        q.to_version = Some(v);
    }
    if let Some(n) = limit {
        q.limit = Some(n);
    }
    if let Some(f) = event_type_filter {
        q.event_type_filter = Some(f);
    }
    store
        .read_range(q)
        .map(|v| v.iter().map(SerializedEvent::from_stored).collect())
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_read_one(
    store: State<'_, Store>,
    event_id: String,
) -> Result<Option<SerializedEvent>, FossicTauriError> {
    let id = EventId::from_hex(&event_id).map_err(FossicTauriError::from)?;
    store
        .read_one(id)
        .map(|opt| opt.as_ref().map(SerializedEvent::from_stored))
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_read_by_external_id(
    store: State<'_, Store>,
    stream_id: String,
    external_id: String,
) -> Result<Option<SerializedEvent>, FossicTauriError> {
    store
        .read_by_external_id(&stream_id, &external_id)
        .map(|opt| opt.as_ref().map(SerializedEvent::from_stored))
        .map_err(FossicTauriError::from)
}

/// Fetch multiple events by their CCE event IDs in a single query.
///
/// `event_ids` is a JSON array of 64-character lowercase hex strings. Results
/// are returned ordered by `timestamp_us ASC`; IDs not found are silently
/// omitted. Keep batch sizes ≤ 4,096 IDs per call — SQLite caps bound
/// parameters at 32,766 and exceeding it returns a `StorageError`.
#[tauri::command]
pub fn fossic_read_batch(
    store: State<'_, Store>,
    event_ids: Vec<String>,
) -> Result<Vec<SerializedEvent>, FossicTauriError> {
    let ids = event_ids
        .iter()
        .map(|s| EventId::from_hex(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(FossicTauriError::from)?;
    store
        .read_batch(&ids)
        .map(|v| v.iter().map(SerializedEvent::from_stored).collect())
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_read_by_correlation(
    store: State<'_, Store>,
    correlation_id: String,
) -> Result<Vec<SerializedEvent>, FossicTauriError> {
    let id = EventId::from_hex(&correlation_id).map_err(FossicTauriError::from)?;
    store
        .read_by_correlation(id)
        .map(|v| v.iter().map(SerializedEvent::from_stored).collect())
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_walk_causation(
    store: State<'_, Store>,
    start: String,
    direction: String,
    max_depth: Option<usize>,
) -> Result<Vec<SerializedEvent>, FossicTauriError> {
    let start_id = EventId::from_hex(&start).map_err(FossicTauriError::from)?;
    let dir = parse_direction(&direction).map_err(direction_err)?;
    store
        .walk_causation(start_id, dir, max_depth.unwrap_or(i64::MAX as usize))
        .map(|v| v.iter().map(SerializedEvent::from_stored).collect())
        .map_err(FossicTauriError::from)
}

// ── Bounded read commands ─────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)] // Tauri command — args map 1:1 to JS call signature
#[tauri::command]
pub fn fossic_read_range_bounded(
    store: State<'_, Store>,
    stream_id: String,
    branch: Option<String>,
    from_version: Option<u64>,
    to_version: Option<u64>,
    event_type_filter: Option<String>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<SerializedReadOutcome, FossicTauriError> {
    let mut q = ReadQuery::stream(stream_id);
    if let Some(b) = branch {
        q.branch = b;
    }
    if let Some(v) = from_version {
        q.from_version = Some(v);
    }
    if let Some(v) = to_version {
        q.to_version = Some(v);
    }
    if let Some(f) = event_type_filter {
        q.event_type_filter = Some(f);
    }
    store
        .read_range_bounded(q, max_results, max_bytes, None)
        .map(SerializedReadOutcome::from_outcome)
        .map_err(FossicTauriError::from)
}

#[allow(clippy::too_many_arguments)] // Tauri command — args map 1:1 to JS call signature
#[tauri::command]
pub fn fossic_read_range_from_cursor(
    store: State<'_, Store>,
    stream_id: String,
    branch: Option<String>,
    from_version: Option<u64>,
    to_version: Option<u64>,
    event_type_filter: Option<String>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
    cursor: String,
) -> Result<SerializedReadOutcome, FossicTauriError> {
    let mut q = ReadQuery::stream(stream_id);
    if let Some(b) = branch {
        q.branch = b;
    }
    if let Some(v) = from_version {
        q.from_version = Some(v);
    }
    if let Some(v) = to_version {
        q.to_version = Some(v);
    }
    if let Some(f) = event_type_filter {
        q.event_type_filter = Some(f);
    }
    let c = parse_cursor(&cursor).map_err(cursor_err)?;
    store
        .read_range_bounded(q, max_results, max_bytes, Some(c))
        .map(SerializedReadOutcome::from_outcome)
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_read_by_correlation_bounded(
    store: State<'_, Store>,
    correlation_id: String,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<SerializedReadOutcome, FossicTauriError> {
    let id = EventId::from_hex(&correlation_id).map_err(FossicTauriError::from)?;
    store
        .read_by_correlation_bounded(id, max_results, max_bytes, None)
        .map(SerializedReadOutcome::from_outcome)
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_read_by_correlation_from_cursor(
    store: State<'_, Store>,
    correlation_id: String,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
    cursor: String,
) -> Result<SerializedReadOutcome, FossicTauriError> {
    let id = EventId::from_hex(&correlation_id).map_err(FossicTauriError::from)?;
    let c = parse_cursor(&cursor).map_err(cursor_err)?;
    store
        .read_by_correlation_bounded(id, max_results, max_bytes, Some(c))
        .map(SerializedReadOutcome::from_outcome)
        .map_err(FossicTauriError::from)
}

#[tauri::command]
pub fn fossic_walk_causation_bounded(
    store: State<'_, Store>,
    start: String,
    direction: String,
    max_depth: Option<usize>,
    sampling: Option<serde_json::Value>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<SerializedReadOutcome, FossicTauriError> {
    let start_id = EventId::from_hex(&start).map_err(FossicTauriError::from)?;
    let dir = parse_direction(&direction).map_err(direction_err)?;
    let depth = max_depth.unwrap_or(i64::MAX as usize);
    let samp = parse_sampling_mode(sampling).map_err(sampling_err)?;
    store
        .walk_causation_bounded(start_id, dir, depth, samp, max_results, max_bytes, None)
        .map(SerializedReadOutcome::from_outcome)
        .map_err(FossicTauriError::from)
}

#[allow(clippy::too_many_arguments)] // Tauri command — args map 1:1 to JS call signature
#[tauri::command]
pub fn fossic_walk_causation_from_cursor(
    store: State<'_, Store>,
    start: String,
    direction: String,
    max_depth: Option<usize>,
    sampling: Option<serde_json::Value>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
    cursor: String,
) -> Result<SerializedReadOutcome, FossicTauriError> {
    let start_id = EventId::from_hex(&start).map_err(FossicTauriError::from)?;
    let dir = parse_direction(&direction).map_err(direction_err)?;
    let depth = max_depth.unwrap_or(i64::MAX as usize);
    let samp = parse_sampling_mode(sampling).map_err(sampling_err)?;
    let c = parse_cursor(&cursor).map_err(cursor_err)?;
    store
        .walk_causation_bounded(start_id, dir, depth, samp, max_results, max_bytes, Some(c))
        .map(SerializedReadOutcome::from_outcome)
        .map_err(FossicTauriError::from)
}

// ── Aggregate bounded ─────────────────────────────────────────────────────────

/// Internal aggregate that collects matching events to a Vec<SerializedEvent>.
/// Enables cross-stream, timestamp-range, and indexed_tags_filter queries via
/// AggregateQuery, which ReadQuery's single-stream model cannot express.
struct CollectAggregate {
    events: Vec<SerializedEvent>,
}

impl Clone for CollectAggregate {
    fn clone(&self) -> Self {
        CollectAggregate {
            events: self.events.clone(),
        }
    }
}

impl fossic::Aggregate for CollectAggregate {
    type Output = Vec<SerializedEvent>;
    fn fold(&mut self, event: &fossic::StoredEvent) {
        self.events.push(SerializedEvent::from_stored(event));
    }
    fn finalize(self) -> Self::Output {
        self.events
    }
}

/// Aggregate-bounded outcome — `next_cursor` is always absent because
/// fold-resume is not yet supported (deferred to v1.2.x).
#[derive(serde::Serialize)]
pub struct SerializedAggregateOutcome {
    pub kind: &'static str,
    pub results: Vec<SerializedEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

#[allow(clippy::too_many_arguments)] // Tauri command — args map 1:1 to JS call signature
#[tauri::command]
pub fn fossic_aggregate_bounded(
    store: State<'_, Store>,
    stream_pattern: String,
    branch: Option<String>,
    event_type_filter: Option<String>,
    from_timestamp_us: Option<i64>,
    to_timestamp_us: Option<i64>,
    indexed_tags_filter: Option<serde_json::Value>,
    max_events_scanned: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<SerializedAggregateOutcome, FossicTauriError> {
    let q = AggregateQuery {
        stream_pattern,
        branch: branch.unwrap_or_else(|| "main".to_string()),
        event_type_filter,
        from_timestamp_us,
        to_timestamp_us,
        indexed_tags_filter,
    };
    let agg = CollectAggregate { events: Vec::new() };
    store
        .aggregate_bounded(q, agg, max_events_scanned, max_bytes)
        .map(|outcome| match outcome {
            fossic::ReadOutcome::Complete(events) => SerializedAggregateOutcome {
                kind: "complete",
                results: events,
                reason: None,
            },
            fossic::ReadOutcome::Truncated { data, reason, .. } => SerializedAggregateOutcome {
                kind: "truncated",
                results: data,
                reason: Some(match reason {
                    fossic::TruncationReason::ResultCount => "result_count",
                    fossic::TruncationReason::ByteSize => "byte_size",
                }),
            },
        })
        .map_err(FossicTauriError::from)
}

// ── Reducer / state command ───────────────────────────────────────────────────

/// Returns the reducer state at a given version, decoded from msgpack to JSON.
///
/// When `reducer_name` is provided, the named reducer is used for snapshot lookup;
/// this is required for streams where multiple reducers have been registered at
/// different times (e.g., after a state migration).  When omitted, the most
/// specific pattern-matched reducer is used.
#[tauri::command]
pub fn fossic_read_state_at_version(
    store: State<'_, Store>,
    stream_id: String,
    branch: String,
    version: u64,
    reducer_name: Option<String>,
) -> Result<serde_json::Value, FossicTauriError> {
    if let Some(name) = reducer_name {
        store
            .read_state_at_version_with_reducer::<serde_json::Value>(
                &stream_id, &branch, version, &name,
            )
            .map_err(FossicTauriError::from)
    } else {
        store
            .read_state_at_version::<serde_json::Value>(&stream_id, &branch, version)
            .map_err(FossicTauriError::from)
    }
}

// ── Subscription commands ─────────────────────────────────────────────────────

/// Handler that emits a Tauri event for each incoming fossic event.
struct EmitHandler<R: Runtime> {
    app: AppHandle<R>,
    sub_id: String,
}

impl<R: Runtime> SubscriptionHandler for EmitHandler<R> {
    fn on_event(&self, event: &fossic::StoredEvent) {
        let payload = serde_json::json!({
            "subscription_id": &self.sub_id,
            "event": SerializedEvent::from_stored(event),
        });
        let _ = self.app.emit("fossic:event", payload);
    }
}

#[tauri::command]
pub fn fossic_subscribe<R: Runtime>(
    app: AppHandle<R>,
    store: State<'_, Store>,
    subs: State<'_, SubscriptionMap>,
    stream_pattern: String,
    branch: Option<String>,
    include_system: Option<bool>,
    queue_size: Option<usize>,
) -> Result<String, FossicTauriError> {
    let sub_id = uuid::Uuid::new_v4().to_string();
    let q = SubscribeQuery {
        stream_pattern,
        branch: branch.unwrap_or_else(|| "main".to_string()),
        include_system: include_system.unwrap_or(false),
    };
    let mode = SubscriptionMode::PostCommit {
        queue_size: queue_size.unwrap_or(1024),
    };
    let handler = EmitHandler {
        app,
        sub_id: sub_id.clone(),
    };
    let stream_pattern = q.stream_pattern.clone();
    let branch = q.branch.clone();
    let handle = store
        .subscribe(q, mode, handler)
        .map_err(FossicTauriError::from)?;
    subs.insert(sub_id.clone(), handle, stream_pattern, branch);
    Ok(sub_id)
}

#[tauri::command]
pub fn fossic_list_subscribers(subs: State<'_, SubscriptionMap>) -> Vec<crate::SubscriberSnapshot> {
    subs.snapshot_all()
}

#[tauri::command]
pub fn fossic_subscription_status(
    subs: State<'_, SubscriptionMap>,
    subscription_id: String,
) -> serde_json::Value {
    match subs.snapshot_one(&subscription_id) {
        Some(snap) => serde_json::json!({
            "active": true,
            "degraded": snap.degraded,
            "queue_depth": snap.queue_depth,
            "queue_capacity": snap.queue_capacity,
        }),
        None => serde_json::json!({
            "active": false,
            "degraded": false,
            "queue_depth": null,
            "queue_capacity": null,
        }),
    }
}

#[tauri::command]
pub fn fossic_unsubscribe(
    subs: State<'_, SubscriptionMap>,
    subscription_id: String,
) -> Result<(), String> {
    subs.remove(&subscription_id);
    Ok(())
}

// ── Test-helpers feature ──────────────────────────────────────────────────────

#[cfg(feature = "test-helpers")]
#[tauri::command]
pub fn fossic_dispatch_test_event(
    store: State<'_, Store>,
    stream_id: String,
    event_type: String,
    payload: serde_json::Value,
) -> Result<String, FossicTauriError> {
    let a = fossic::Append {
        stream_id,
        event_type,
        payload,
        ..Default::default()
    };
    let id = store.append(a).map_err(FossicTauriError::from)?;
    Ok(id.to_hex())
}
