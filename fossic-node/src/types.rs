use fossic::{BranchInfo, ReadOutcome, SnapshotInfo, StoredEvent, StreamInfo, TruncationReason};
use napi::bindgen_prelude::*;
use napi_derive::napi;

// ── EventId ───────────────────────────────────────────────────────────────────

/// Wraps a 32-byte content-addressed event identity as a typed JS class.
/// Crosses the napi boundary as `Uint8Array`; use `.toHex()` for display.
#[napi]
pub struct EventId {
    pub(crate) inner: fossic::EventId,
}

#[napi]
impl EventId {
    /// Construct from a 32-byte `Uint8Array`.
    #[napi(constructor)]
    pub fn new(bytes: Uint8Array) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(Error::new(
                Status::InvalidArg,
                format!("EventId requires 32 bytes, got {}", bytes.len()),
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(EventId {
            inner: fossic::EventId::from_bytes(arr),
        })
    }

    /// Parse from a 64-character hex string.
    #[napi(factory)]
    pub fn from_hex(hex: String) -> Result<Self> {
        fossic::EventId::from_hex(&hex)
            .map(|inner| EventId { inner })
            .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))
    }

    /// Return the event identity as a lowercase hex string.
    #[napi]
    pub fn to_hex(&self) -> String {
        self.inner.to_hex()
    }

    /// Return the raw 32 bytes as `Uint8Array`.
    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        self.inner.as_bytes().to_vec().into()
    }
}

// ── StoredEventJs ─────────────────────────────────────────────────────────────

/// A stored event as returned across the napi boundary.
///
/// `version` is BigInt; `id`, `causation_id`, `correlation_id` are hex strings.
/// `payload` is a JSON-decoded object.
#[napi(object)]
pub struct StoredEventJs {
    /// Hex-encoded 32-byte content-addressed event ID.
    pub id: String,
    pub stream_id: String,
    pub branch: String,
    /// Monotonic version number for this (stream, branch). BigInt in JS.
    pub version: BigInt,
    pub timestamp_us: i64,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub event_type: String,
    pub type_version: u32,
    /// Msgpack payload decoded to a JSON value.
    pub payload: serde_json::Value,
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,
}

impl From<&StoredEvent> for StoredEventJs {
    fn from(e: &StoredEvent) -> Self {
        let payload = e
            .deserialize_payload_json()
            .unwrap_or(serde_json::Value::Null);
        StoredEventJs {
            id: e.id.to_hex(),
            stream_id: e.stream_id.clone(),
            branch: e.branch.clone(),
            version: BigInt::from(e.version),
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

// ── AppendJs ──────────────────────────────────────────────────────────────────

/// Input for appending an event to a stream.
#[napi(object)]
pub struct AppendJs {
    pub stream_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub type_version: Option<u32>,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,
}

impl TryFrom<AppendJs> for fossic::Append {
    type Error = Error;

    fn try_from(js: AppendJs) -> Result<Self> {
        let causation_id = js
            .causation_id
            .map(|h| fossic::EventId::from_hex(&h))
            .transpose()
            .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?;
        let correlation_id = js
            .correlation_id
            .map(|h| fossic::EventId::from_hex(&h))
            .transpose()
            .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?;
        Ok(fossic::Append {
            stream_id: js.stream_id,
            event_type: js.event_type,
            payload: js.payload,
            type_version: js.type_version.unwrap_or(1),
            causation_id,
            correlation_id,
            external_id: js.external_id,
            indexed_tags: js.indexed_tags,
            ..fossic::Append::default()
        })
    }
}

// ── ReadQueryJs ───────────────────────────────────────────────────────────────

#[napi(object)]
pub struct ReadQueryJs {
    pub stream_id: String,
    pub branch: Option<String>,
    /// Inclusive lower bound (BigInt in JS).
    pub from_version: Option<BigInt>,
    /// Inclusive upper bound (BigInt in JS).
    pub to_version: Option<BigInt>,
    pub limit: Option<u32>,
    pub event_type_filter: Option<String>,
}

impl From<ReadQueryJs> for fossic::ReadQuery {
    fn from(js: ReadQueryJs) -> Self {
        let mut q = fossic::ReadQuery::stream(js.stream_id);
        if let Some(b) = js.branch {
            q.branch = b;
        }
        if let Some(v) = js.from_version {
            q.from_version = Some(v.get_u64().1);
        }
        if let Some(v) = js.to_version {
            q.to_version = Some(v.get_u64().1);
        }
        if let Some(n) = js.limit {
            q.limit = Some(n as usize);
        }
        if let Some(f) = js.event_type_filter {
            q.event_type_filter = Some(f);
        }
        q
    }
}

// ── StreamInfoJs ──────────────────────────────────────────────────────────────

#[napi(object)]
pub struct StreamInfoJs {
    pub id: String,
    pub declared_by: String,
    pub declared_at: i64,
    pub description: Option<String>,
}

impl From<StreamInfo> for StreamInfoJs {
    fn from(s: StreamInfo) -> Self {
        StreamInfoJs {
            id: s.id,
            declared_by: s.declared_by,
            declared_at: s.declared_at,
            description: s.description,
        }
    }
}

// ── BranchInfoJs ──────────────────────────────────────────────────────────────

#[napi(object)]
pub struct BranchInfoJs {
    pub id: String,
    pub stream_id: String,
    pub parent_id: String,
    pub parent_version: BigInt,
    pub description: Option<String>,
    pub created_at: i64,
    pub lifecycle: String,
    pub closed_at: Option<i64>,
    pub closed_reason: Option<String>,
}

impl From<BranchInfo> for BranchInfoJs {
    fn from(b: BranchInfo) -> Self {
        BranchInfoJs {
            id: b.id,
            stream_id: b.stream_id,
            parent_id: b.parent_id,
            parent_version: BigInt::from(b.parent_version),
            description: b.description,
            created_at: b.created_at,
            lifecycle: b.lifecycle,
            closed_at: b.closed_at,
            closed_reason: b.closed_reason,
        }
    }
}

// ── SnapshotInfoJs ────────────────────────────────────────────────────────────

#[napi(object)]
pub struct SnapshotInfoJs {
    pub stream_id: String,
    pub branch: String,
    pub version: BigInt,
    pub reducer_name: String,
    pub reducer_version: u32,
    pub state_schema_version: u32,
    pub created_at: i64,
}

impl From<SnapshotInfo> for SnapshotInfoJs {
    fn from(s: SnapshotInfo) -> Self {
        SnapshotInfoJs {
            stream_id: s.stream_id,
            branch: s.branch,
            version: BigInt::from(s.version),
            reducer_name: s.reducer_name,
            reducer_version: s.reducer_version,
            state_schema_version: s.state_schema_version,
            created_at: s.created_at,
        }
    }
}

// ── OpenOptionsJs ─────────────────────────────────────────────────────────────

#[napi(object)]
pub struct OpenOptionsJs {
    /// `"plaintext"` (default) or `"per_stream"` (enables crypto-shredding).
    pub encryption: Option<String>,
    /// `"auto"` (default) | `"manual"` (reserved, not yet implemented).
    pub checkpoint_mode: Option<String>,
    /// `"create_if_missing"` (default) | `"fail_if_not_found"`.
    pub on_first_open: Option<String>,
    /// Default result-count ceiling for bounded reads on this store.
    pub default_max_results: Option<u32>,
    /// Default byte-size ceiling for bounded reads on this store.
    pub default_max_bytes: Option<u32>,
}

pub(crate) fn parse_open_options(js: Option<OpenOptionsJs>) -> Result<fossic::OpenOptions> {
    let mut opts = fossic::OpenOptions::default();
    if let Some(js) = js {
        if let Some(enc) = js.encryption {
            opts.encryption = match enc.as_str() {
                "plaintext" => fossic::EncryptionMode::Plaintext,
                other => {
                    return Err(Error::new(
                        Status::InvalidArg,
                        format!("unknown encryption mode: {other}; supported values: plaintext"),
                    ))
                }
            };
        }
        if let Some(policy) = js.on_first_open {
            opts.on_first_open = match policy.as_str() {
                "create_if_missing" => fossic::FirstOpenPolicy::CreateIfMissing,
                "fail_if_not_found" => fossic::FirstOpenPolicy::RequireExisting,
                other => {
                    return Err(Error::new(
                        Status::InvalidArg,
                        format!("unknown on_first_open policy: {other}"),
                    ))
                }
            };
        }
        if let Some(n) = js.default_max_results {
            opts.default_max_results = Some(n as usize);
        }
        if let Some(n) = js.default_max_bytes {
            opts.default_max_bytes = Some(n as usize);
        }
    }
    Ok(opts)
}

// ── TruncationCursorJs ────────────────────────────────────────────────────────

/// Opaque resume token for bounded reads. Callers must treat it as opaque
/// and only pass it back to the same bounded read method that produced it.
#[napi]
pub struct TruncationCursorJs {
    pub(crate) inner: fossic::TruncationCursor,
}

#[napi]
impl TruncationCursorJs {
    /// Serialize the cursor to raw bytes (for persistence / transport).
    #[napi]
    pub fn to_bytes(&self) -> Buffer {
        self.inner.as_bytes().to_vec().into()
    }

    /// Reconstruct a cursor from previously serialized bytes.
    #[napi(factory)]
    pub fn from_bytes(buf: Buffer) -> Self {
        TruncationCursorJs {
            inner: fossic::TruncationCursor::from_bytes(buf.to_vec()),
        }
    }
}

// ── SamplingModeJs ────────────────────────────────────────────────────────────

/// Tagged representation of the walk-causation sampling strategy.
/// Use the `SamplingMode` namespace in `index.js` to construct values.
#[napi(object)]
pub struct SamplingModeJs {
    pub kind: String,
    pub max_per_level: Option<u32>,
    pub target_count: Option<u32>,
}

pub(crate) fn parse_sampling_mode(js: Option<SamplingModeJs>) -> fossic::SamplingMode {
    match js {
        None => fossic::SamplingMode::Exhaustive,
        Some(s) => match s.kind.as_str() {
            "breadthFirst" => fossic::SamplingMode::BreadthFirst {
                max_per_level: s.max_per_level.unwrap_or(100) as usize,
            },
            "adaptive" => fossic::SamplingMode::Adaptive {
                target_count: s.target_count.unwrap_or(100) as usize,
            },
            _ => fossic::SamplingMode::Exhaustive,
        },
    }
}

// ── ReadOutcomeJs ─────────────────────────────────────────────────────────────

/// Result of a bounded read. Discriminated by `kind`: `"complete"` or `"truncated"`.
///
/// The `nextCursor` field is a raw `Buffer` (opaque bytes). The JS layer in
/// `index.js` wraps it into a `TruncationCursor` instance before returning.
#[napi(object)]
pub struct ReadOutcomeJs {
    /// `"complete"` or `"truncated"`
    pub kind: String,
    pub results: Vec<StoredEventJs>,
    /// `"result_count"` | `"byte_size"` — only set when `kind == "truncated"`.
    pub reason: Option<String>,
    /// Raw cursor bytes — only set when `kind == "truncated"`. Wrapped by JS into TruncationCursor.
    pub next_cursor: Option<Buffer>,
}

impl ReadOutcomeJs {
    pub fn from_outcome(outcome: ReadOutcome<Vec<StoredEvent>>) -> Self {
        match outcome {
            ReadOutcome::Complete(events) => ReadOutcomeJs {
                kind: "complete".into(),
                results: events.iter().map(StoredEventJs::from).collect(),
                reason: None,
                next_cursor: None,
            },
            ReadOutcome::Truncated { data, cursor, reason } => ReadOutcomeJs {
                kind: "truncated".into(),
                results: data.iter().map(StoredEventJs::from).collect(),
                reason: Some(match reason {
                    TruncationReason::ResultCount => "result_count".into(),
                    TruncationReason::ByteSize => "byte_size".into(),
                }),
                next_cursor: cursor.map(|c| c.as_bytes().to_vec().into()),
            },
        }
    }
}

// ── SubscribeQueryJs ──────────────────────────────────────────────────────────

#[napi(object)]
pub struct SubscribeQueryJs {
    /// Glob pattern for the stream(s) to subscribe to.
    /// `*` matches one path segment; `**` matches zero or more segments.
    pub stream_pattern: String,
    pub branch: Option<String>,
    /// When `false` (default), events from `_`-prefixed system streams are suppressed.
    pub include_system: Option<bool>,
    pub queue_size: Option<u32>,
}

// ── CreateBranchJs ────────────────────────────────────────────────────────────

#[napi(object)]
pub struct CreateBranchJs {
    pub stream_id: String,
    pub branch_id: String,
    pub parent_id: Option<String>,
    pub parent_version: BigInt,
    pub description: Option<String>,
}

impl TryFrom<CreateBranchJs> for fossic::CreateBranch {
    type Error = Error;

    fn try_from(js: CreateBranchJs) -> Result<Self> {
        Ok(fossic::CreateBranch {
            stream_id: js.stream_id,
            branch_id: js.branch_id,
            parent_id: js.parent_id.unwrap_or_else(|| "main".to_string()),
            parent_version: js.parent_version.get_u64().1,
            description: js.description,
            alternatives: None,
        })
    }
}
