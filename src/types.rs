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
}

impl ReadQuery {
    pub fn stream(stream_id: impl Into<String>) -> Self {
        ReadQuery {
            stream_id: stream_id.into(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
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
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions {
            encryption: EncryptionMode::Plaintext,
            checkpoint_mode: CheckpointMode::Auto,
            on_first_open: FirstOpenPolicy::CreateIfMissing,
            similarity_provider: None,
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
