use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("stream not declared: {stream_id}")]
    StreamNotDeclared { stream_id: String },

    #[error("invalid stream ID '{id}': {reason}")]
    InvalidStreamId { id: String, reason: String },

    #[error("indexed_tags must be a JSON object, got {got}")]
    InvalidIndexedTags { got: &'static str },

    #[error("invalid event ID: {0}")]
    InvalidEventId(String),

    #[error("store not found at '{path}'")]
    StoreNotFound { path: String },

    #[error("schema version {stored} is newer than this build supports ({required}); upgrade fossic")]
    SchemaMismatch { stored: u32, required: u32 },

    #[error("not implemented in v1: {feature}")]
    NotImplemented { feature: &'static str },

    #[error("branch not found: {stream_id}/{branch_id}")]
    BranchNotFound { stream_id: String, branch_id: String },

    #[error("branch lifecycle error: {reason}")]
    BranchLifecycleError { reason: String },

    #[error("invalid branch ID '{id}': {reason}")]
    InvalidBranchId { id: String, reason: String },

    #[error("alternatives must be a JSON array")]
    InvalidAlternatives,

    #[error("reducer patterns '{a}' and '{b}' are ambiguous (both match the same streams at equal specificity)")]
    ReducerPatternAmbiguous { a: String, b: String },

    #[error("no reducer registered matching stream '{stream_id}'")]
    ReducerNotFound { stream_id: String },

    #[error("no reducer registered with name '{name}'")]
    ReducerNotFoundByName { name: String },

    #[error("reducer error: {message}")]
    ReducerError { message: String },

    #[error("no events to snapshot for stream '{stream_id}' branch '{branch}'")]
    NoEventsToSnapshot { stream_id: String, branch: String },

    #[error(
        "purge_event confirmation mismatch; \
         confirm must be exactly \"I understand this breaks replay-from-zero\", got: \"{got}\""
    )]
    PurgeConfirmationError { got: String },

    #[error("event not found: {id}")]
    EventNotFound { id: String },

    #[error("upcaster chain gap: no upcaster registered for {event_type} from version {from}")]
    UpcasterChainGap { event_type: String, from: u32 },

    #[error("CCE encoding error: {0}")]
    Cce(#[from] CceError),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("msgpack encode error: {0}")]
    MsgpackEncode(#[from] rmp_serde::encode::Error),

    #[error("msgpack decode error: {0}")]
    MsgpackDecode(#[from] rmp_serde::decode::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("read pool exhausted: all {pool_size} connections busy after {timeout_ms}ms; increase OpenOptions::read_pool_size")]
    PoolExhausted { pool_size: usize, timeout_ms: u64 },

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum CceError {
    #[error("u64 value {0} exceeds i64::MAX; CCE integers are signed i64")]
    U64Overflow(u64),

    #[error("duplicate map keys after CCE encoding")]
    DuplicateKeys,

    #[error("string exceeds 64 MiB limit ({0} bytes)")]
    StringTooLarge(usize),
}
