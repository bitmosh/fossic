use fossic::{BranchInfo, StoredEvent, StreamInfo};
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
