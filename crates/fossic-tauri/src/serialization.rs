use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fossic::{
    BranchInfo, ReadOutcome, SamplingMode, StoredEvent, StreamInfo, TruncationCursor,
    TruncationReason, WalkDirection,
};
use serde::{Deserialize, Serialize};

// ── Serialized types for JSON IPC ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    pub id: String,
    pub stream_id: String,
    pub branch: String,
    pub version: u64,
    pub timestamp_us: i64,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub event_type: String,
    pub type_version: u32,
    /// Payload decoded from msgpack to JSON. This is the IPC boundary representation;
    /// the storage layer uses msgpack.
    pub payload: serde_json::Value,
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,
}

impl SerializedEvent {
    pub fn from_stored(e: &StoredEvent) -> Self {
        let payload = e
            .deserialize_payload_json()
            .unwrap_or(serde_json::Value::Null);
        SerializedEvent {
            id: e.id.to_hex(),
            stream_id: e.stream_id.clone(),
            branch: e.branch.clone(),
            version: e.version,
            timestamp_us: e.timestamp_us,
            causation_id: e.causation_id.map(|id| id.to_hex()),
            correlation_id: e.correlation_id.map(|id| id.to_hex()),
            event_type: e.event_type.clone(),
            type_version: e.type_version,
            payload,
            external_id: e.external_id.clone(),
            indexed_tags: e.indexed_tags.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedStreamInfo {
    pub id: String,
    pub declared_by: String,
    pub declared_at: i64,
    pub description: Option<String>,
}

impl From<StreamInfo> for SerializedStreamInfo {
    fn from(s: StreamInfo) -> Self {
        SerializedStreamInfo {
            id: s.id,
            declared_by: s.declared_by,
            declared_at: s.declared_at,
            description: s.description,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedBranchInfo {
    pub id: String,
    pub stream_id: String,
    pub parent_id: String,
    pub parent_version: u64,
    pub description: Option<String>,
    pub created_at: i64,
    pub lifecycle: String,
    pub closed_at: Option<i64>,
    pub closed_reason: Option<String>,
}

impl From<BranchInfo> for SerializedBranchInfo {
    fn from(b: BranchInfo) -> Self {
        SerializedBranchInfo {
            id: b.id,
            stream_id: b.stream_id,
            parent_id: b.parent_id,
            parent_version: b.parent_version,
            description: b.description,
            created_at: b.created_at,
            lifecycle: b.lifecycle,
            closed_at: b.closed_at,
            closed_reason: b.closed_reason,
        }
    }
}

pub fn map_err(e: fossic::Error) -> String {
    e.to_string()
}

// ── Bounded read outcome ──────────────────────────────────────────────────────

/// JSON-serializable shape matching the TypeScript ReadOutcome discriminated union.
/// `reason` and `next_cursor` are omitted (not just null) for `complete` outcomes.
#[derive(Debug, Clone, Serialize)]
pub struct SerializedReadOutcome {
    pub kind: &'static str,
    pub results: Vec<SerializedEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl SerializedReadOutcome {
    pub fn from_outcome(outcome: ReadOutcome<Vec<StoredEvent>>) -> Self {
        match outcome {
            ReadOutcome::Complete(events) => SerializedReadOutcome {
                kind: "complete",
                results: events.iter().map(SerializedEvent::from_stored).collect(),
                reason: None,
                next_cursor: None,
            },
            ReadOutcome::Truncated {
                data,
                cursor,
                reason,
            } => SerializedReadOutcome {
                kind: "truncated",
                results: data.iter().map(SerializedEvent::from_stored).collect(),
                reason: Some(match reason {
                    TruncationReason::ResultCount => "result_count",
                    TruncationReason::ByteSize => "byte_size",
                }),
                next_cursor: cursor.map(|c| B64.encode(c.as_bytes())),
            },
        }
    }
}

// ── Cursor helpers ────────────────────────────────────────────────────────────

pub fn parse_cursor(s: &str) -> Result<TruncationCursor, String> {
    B64.decode(s)
        .map(TruncationCursor::from_bytes)
        .map_err(|e| format!("invalid cursor: {e}"))
}

// ── Direction helpers ─────────────────────────────────────────────────────────

pub fn parse_direction(s: &str) -> Result<WalkDirection, String> {
    match s {
        "forward" | "Forward" => Ok(WalkDirection::Forward),
        "backward" | "Backward" => Ok(WalkDirection::Backward),
        "both" | "Both" => Ok(WalkDirection::Both),
        other => Err(format!(
            "unknown direction: {other}; use 'forward', 'backward', or 'both'"
        )),
    }
}

// ── SamplingMode helpers ──────────────────────────────────────────────────────

/// Parses a JSON tagged object into `SamplingMode`.
/// Accepts: `{"kind":"exhaustive"}` | `{"kind":"breadthFirst","maxPerLevel":N}` | `{"kind":"adaptive","targetCount":N}`
/// Absent/null → `Exhaustive`.
pub fn parse_sampling_mode(v: Option<serde_json::Value>) -> Result<SamplingMode, String> {
    let Some(v) = v else {
        return Ok(SamplingMode::Exhaustive);
    };
    let kind = v
        .get("kind")
        .and_then(|k| k.as_str())
        .unwrap_or("exhaustive");
    match kind {
        "exhaustive" => Ok(SamplingMode::Exhaustive),
        "breadthFirst" | "breadth_first" => {
            let n = v
                .get("maxPerLevel")
                .or_else(|| v.get("max_per_level"))
                .and_then(|n| n.as_u64())
                .unwrap_or(100) as usize;
            Ok(SamplingMode::BreadthFirst { max_per_level: n })
        }
        "adaptive" => {
            let n = v
                .get("targetCount")
                .or_else(|| v.get("target_count"))
                .and_then(|n| n.as_u64())
                .unwrap_or(100) as usize;
            Ok(SamplingMode::Adaptive { target_count: n })
        }
        other => Err(format!("unknown sampling mode: {other}")),
    }
}
