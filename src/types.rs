use crate::error::Error;
use std::fmt;

/// 32-byte blake3 content-addressed event identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(pub [u8; 32]);

impl EventId {
    pub fn from_bytes(b: [u8; 32]) -> Self {
        EventId(b)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    pub fn from_hex(s: &str) -> Result<Self, Error> {
        if s.len() != 64 {
            return Err(Error::InvalidEventId(format!(
                "expected 64 hex chars, got {}",
                s.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = hex_nibble(chunk[0]).map_err(|c| {
                Error::InvalidEventId(format!("invalid hex char '{}'", c))
            })?;
            let lo = hex_nibble(chunk[1]).map_err(|c| {
                Error::InvalidEventId(format!("invalid hex char '{}'", c))
            })?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(EventId(bytes))
    }
}

fn hex_nibble(b: u8) -> Result<u8, char> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(b as char),
    }
}

impl fmt::Debug for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EventId({})", self.to_hex())
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl serde::Serialize for EventId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for EventId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl rusqlite::ToSql for EventId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(
            rusqlite::types::ValueRef::Blob(&self.0),
        ))
    }
}

impl rusqlite::types::FromSql for EventId {
    fn column_result(
        value: rusqlite::types::ValueRef<'_>,
    ) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Blob(b) if b.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(b);
                Ok(EventId(arr))
            }
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

// ── Append ───────────────────────────────────────────────────────────────────

/// Builder for a single event write.
pub struct Append {
    pub stream_id: String,
    /// Branch to append to. Defaults to `"main"`.
    pub branch: String,
    pub event_type: String,
    pub type_version: u32,
    pub payload: serde_json::Value,
    pub causation_id: Option<EventId>,
    pub correlation_id: Option<EventId>,
    /// Consumer-supplied external ID (e.g. a ULID or UUID).
    pub external_id: Option<String>,
    /// JSON object projected for cross-stream aggregation queries.
    pub indexed_tags: Option<serde_json::Value>,
    /// Microseconds since Unix epoch. Defaults to now if `None`.
    pub timestamp_us: Option<i64>,
}

impl Default for Append {
    fn default() -> Self {
        Append {
            stream_id: String::new(),
            branch: "main".to_string(),
            event_type: String::new(),
            type_version: 1,
            payload: serde_json::Value::Null,
            causation_id: None,
            correlation_id: None,
            external_id: None,
            indexed_tags: None,
            timestamp_us: None,
        }
    }
}

// ── StoredEvent ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct StoredEvent {
    pub id: EventId,
    pub stream_id: String,
    pub branch: String,
    pub version: u64,
    pub timestamp_us: i64,
    pub causation_id: Option<EventId>,
    pub correlation_id: Option<EventId>,
    pub event_type: String,
    pub type_version: u32,
    /// Msgpack-encoded payload. Use `deserialize_payload` to decode.
    pub payload: Vec<u8>,
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,
}

impl StoredEvent {
    pub fn deserialize_payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
        rmp_serde::from_slice(&self.payload).map_err(Error::MsgpackDecode)
    }

    pub fn deserialize_payload_json(&self) -> Result<serde_json::Value, Error> {
        self.deserialize_payload::<serde_json::Value>()
    }
}

// ── ReadQuery ─────────────────────────────────────────────────────────────────

pub struct ReadQuery {
    pub stream_id: String,
    pub branch: String,
    pub from_version: Option<u64>,
    pub to_version: Option<u64>,
    pub limit: Option<usize>,
    pub event_type_filter: Option<String>,
}

impl ReadQuery {
    pub fn stream(stream_id: impl Into<String>) -> Self {
        ReadQuery {
            stream_id: stream_id.into(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: None,
        }
    }
}

// ── OpenOptions ───────────────────────────────────────────────────────────────

pub struct OpenOptions {
    pub encryption: EncryptionMode,
    pub checkpoint_mode: CheckpointMode,
    pub on_first_open: FirstOpenPolicy,
    /// Optional similarity search backend. `None` (default) means similarity queries return
    /// `Error::NotImplemented`. Inject a custom provider for semantic search on event payloads.
    pub similarity_provider: Option<std::sync::Arc<dyn crate::similarity::SimilaritySearchProvider>>,
    /// Number of read connections opened at store startup and held in the pool.
    /// Defaults to 4. All read methods (read_range, aggregate, etc.) draw from this pool
    /// and return their connection on drop, so reads never block each other or the write path.
    /// Minimum 1; values below 1 are clamped to 1.
    pub read_pool_size: usize,
    /// Maximum time in milliseconds to wait for a read connection from the pool.
    /// Defaults to 30,000ms (30 seconds). If all connections are busy for longer than this,
    /// read methods return `Error::PoolExhausted`. Set lower in tests to make exhaustion
    /// observable without a 30-second wait.
    pub read_pool_timeout_ms: u64,
    /// Default upper bound on the number of events returned by a single bounded read.
    /// `None` means no default result-count limit (callers must supply a per-call budget).
    pub default_max_results: Option<usize>,
    /// Default upper bound on the total payload bytes returned by a single bounded read.
    /// `None` means no default byte-size limit.
    pub default_max_bytes: Option<usize>,
    /// Rolling-mean state-size threshold (bytes) above which a `ReducerStateLarge` event is
    /// emitted to `_fossic/system`. Computed over the last 32 `apply_bytes` results per
    /// `(stream_id, branch)`. Emission is throttled to at most once per 60 seconds per group.
    /// Default: 1 MiB (1_048_576). Set to `usize::MAX` to disable.
    pub reducer_state_large_threshold_bytes: usize,
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions {
            encryption: EncryptionMode::Plaintext,
            checkpoint_mode: CheckpointMode::Auto,
            on_first_open: FirstOpenPolicy::CreateIfMissing,
            similarity_provider: None,
            read_pool_size: 4,
            read_pool_timeout_ms: 30_000,
            default_max_results: None,
            default_max_bytes: None,
            reducer_state_large_threshold_bytes: 1_048_576,
        }
    }
}

pub enum EncryptionMode {
    Plaintext,
    OsKeyring,
    EnvVar(String),
}

pub enum CheckpointMode {
    Auto,
    Manual { interval_ms: u64 },
}

pub enum FirstOpenPolicy {
    CreateIfMissing,
    RequireExisting,
}

// ── StreamInfo ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub id: String,
    pub declared_by: String,
    pub declared_at: i64,
    pub description: Option<String>,
}

// ── CreateBranch ──────────────────────────────────────────────────────────────

pub struct CreateBranch {
    pub stream_id: String,
    pub branch_id: String,
    /// Parent branch ID. Use `"main"` for root branches.
    pub parent_id: String,
    /// Version on the parent branch where this branch diverges.
    pub parent_version: u64,
    pub description: Option<String>,
    /// Must be a JSON array if `Some`.
    pub alternatives: Option<serde_json::Value>,
}

// ── BranchInfo ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub id: String,
    pub stream_id: String,
    pub parent_id: String,
    pub parent_version: u64,
    pub description: Option<String>,
    pub created_at: i64,
    /// `"ephemeral"` | `"promoted"` | `"dead_end"`
    pub lifecycle: String,
    pub closed_at: Option<i64>,
    pub closed_reason: Option<String>,
    pub alternatives: Option<serde_json::Value>,
}

// ── SnapshotInfo ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub stream_id: String,
    pub branch: String,
    /// Highest event version included in this snapshot.
    pub version: u64,
    pub reducer_name: String,
    pub reducer_version: u32,
    pub state_schema_version: u32,
    pub created_at: i64,
}

// ── PHASE 1 TYPES ─────────────────────────────────────────────────────────────
// All Phase 1 (Bounded Resource API) types live below this marker.

/// Which budget limit was hit during a bounded read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetKind {
    /// The result-count ceiling (`default_max_results` / per-call limit) was reached.
    ResultCount,
    /// The byte-size ceiling (`default_max_bytes` / per-call limit) was reached.
    ByteSize,
}

/// Outcome of a bounded read operation.
#[derive(Debug)]
pub enum ReadOutcome<T> {
    /// All matching events were returned; no truncation occurred.
    Complete(T),
    /// The result was truncated by a budget limit. Feed `cursor` to the
    /// same bounded read method to resume from where this one stopped.
    Truncated {
        data: T,
        cursor: TruncationCursor,
        reason: TruncationReason,
    },
}

/// Why a bounded read was truncated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationReason {
    ResultCount,
    ByteSize,
}

/// Opaque resume token returned when a bounded read is truncated.
///
/// The internal encoding is msgpack; callers must treat it as opaque bytes
/// and only pass it back to the same bounded read method that produced it.
#[derive(Debug)]
pub struct TruncationCursor(pub(crate) Vec<u8>);

impl TruncationCursor {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        TruncationCursor(bytes)
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Private inner cursor state. Serialized to/from msgpack inside `TruncationCursor`.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) enum CursorInner {
    /// Resume point for a `read_range`-style bounded read.
    Range {
        stream_id: String,
        branch: String,
        next_version: u64,
    },
    /// Resume point for a `read_by_correlation`-style bounded read.
    Correlation {
        correlation_id: [u8; 32],
        /// The `id` of the last event returned in the previous page.
        /// Resume predicate: `id > last_seen_id ORDER BY id ASC`.
        last_seen_id: [u8; 32],
    },
    /// Resume point for a `walk_causation`-style bounded read.
    Causation {
        /// IDs of the last-yielded BFS level's events. On resume, expand this set
        /// to obtain the next level (direction encoded in `direction`).
        frontier: Vec<[u8; 32]>,
        /// Traversal direction: 0 = Forward, 1 = Backward, 2 = Both.
        direction: u8,
        /// Number of BFS levels fully consumed before the cut point.
        depth_consumed: u32,
    },
}

impl TruncationCursor {
    pub(crate) fn encode(inner: &CursorInner) -> Result<Self, crate::error::Error> {
        let bytes = rmp_serde::to_vec(inner).map_err(crate::error::Error::MsgpackEncode)?;
        Ok(TruncationCursor(bytes))
    }

    pub(crate) fn decode(&self) -> Result<CursorInner, crate::error::Error> {
        rmp_serde::from_slice(&self.0).map_err(crate::error::Error::MsgpackDecode)
    }
}

/// Sampling strategy for graph-walk bounded reads (used by `walk_causation` bounded variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingMode {
    /// Return every event up to the budget; no sampling.
    Exhaustive,
    /// Visit at most `max_per_level` events per BFS depth level.
    BreadthFirst { max_per_level: usize },
    /// Aim for approximately `target_count` events, adjusting depth cutoff dynamically.
    Adaptive { target_count: usize },
}

// ── PHASE 6+7+8 TYPES ─────────────────────────────────────────────────────────
// All Phase 6, 7, 8 types insert below this marker, above the closing of the file.

/// Policy for when fossic automatically takes a snapshot of a reducer's state.
///
/// Registered per `(stream_pattern, reducer)` at registration time via
/// `Store::register_reducer_with_policy`. The store consults the policy after
/// each `read_state` call; when the policy fires, `take_snapshot` is called
/// in-band.
///
/// `EveryNSeconds` and `StateAdaptive` are Phase 7/v1.2.1 dependencies and
/// return `Error::NotImplemented` at registration time until those phases land.
#[derive(Debug, Clone)]
pub enum SnapshotPolicy {
    /// Current behavior: caller invokes `take_snapshot` explicitly.
    /// Default for backward compatibility.
    Manual,

    /// Take a snapshot every N events applied to the reducer across `read_state`
    /// calls. Fires synchronously after the Nth cumulative event applied since the
    /// last snapshot. N must be >= 1; N = 0 returns `Error::SnapshotPolicyInvalid`.
    EveryNEvents(u32),

    /// Take a snapshot every N seconds of wall-clock time.
    /// Requires Phase 7 background executor. Returns `Error::NotImplemented` at
    /// registration time until Phase 7 lands.
    EveryNSeconds(u32),

    /// Take a snapshot when estimated replay cost exceeds `target_replay_cost_us`
    /// microseconds AND at least `min_events_between` events have been applied
    /// since the last snapshot. Requires v1.2.1 state-size monitoring.
    /// Returns `Error::NotImplemented` at registration time until v1.2.1.
    StateAdaptive {
        target_replay_cost_us: u32,
        min_events_between: u32,
    },
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        SnapshotPolicy::Manual
    }
}
