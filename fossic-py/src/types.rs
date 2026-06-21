use fossic::{
    Append, BranchInfo, BranchSegment, CheckpointMode, CreateBranch, EncryptionMode,
    EventId, FirstOpenPolicy, OpenOptions, ReadQuery, SnapshotInfo, StoredEvent, StreamInfo,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes};

use crate::errors::to_py_err;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Decode a msgpack payload to a Python object via JSON as an intermediate.
pub fn payload_to_py<'py>(
    py: Python<'py>,
    bytes: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    let v: serde_json::Value =
        rmp_serde::from_slice(bytes).map_err(|e| pyo3::exceptions::PyValueError::new_err(
            format!("msgpack decode error: {e}"),
        ))?;
    json_to_py(py, &v)
}

/// Convert a serde_json::Value to a Python object via `json.loads`.
pub fn json_to_py<'py>(py: Python<'py>, v: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    let s = serde_json::to_string(v).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("json serialize: {e}"))
    })?;
    py.import("json")?.call_method1("loads", (s.as_str(),))
}

/// Convert a Python object to a serde_json::Value via `json.dumps`.
pub fn py_to_json(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    let s: String = py
        .import("json")?
        .call_method1("dumps", (obj,))?
        .extract()?;
    serde_json::from_str(&s).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("json deserialize: {e}"))
    })
}

// ── EventId ───────────────────────────────────────────────────────────────────

/// 32-byte content-addressed event identity.
///
/// Internally stores raw bytes. Use `.hex()` for a human-readable form and
/// `EventId.from_hex(s)` to parse one back. Equality comparison works on the bytes.
#[pyclass(name = "EventId", eq, from_py_object)]
#[derive(Clone, PartialEq, Eq)]
pub struct PyEventId {
    pub inner: EventId,
}

impl From<EventId> for PyEventId {
    fn from(v: EventId) -> Self {
        PyEventId { inner: v }
    }
}

#[pymethods]
impl PyEventId {
    /// Return the raw 32-byte representation.
    fn as_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.inner.as_bytes())
    }

    /// Hex-encoded string (64 lowercase hex chars).
    fn hex(&self) -> String {
        self.inner.to_hex()
    }

    /// Parse a hex-encoded EventId.
    #[staticmethod]
    fn from_hex(s: &str) -> PyResult<Self> {
        EventId::from_hex(s)
            .map(PyEventId::from)
            .map_err(to_py_err)
    }

    fn __repr__(&self) -> String {
        format!("EventId({})", self.inner.to_hex())
    }

    fn __str__(&self) -> String {
        self.inner.to_hex()
    }

    fn __hash__(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.inner.as_bytes().hash(&mut h);
        h.finish()
    }
}

// ── StoredEvent ───────────────────────────────────────────────────────────────

/// An event as returned by read operations.
#[pyclass(name = "StoredEvent", skip_from_py_object)]
#[derive(Clone)]
pub struct PyStoredEvent {
    pub inner: StoredEvent,
}

impl From<StoredEvent> for PyStoredEvent {
    fn from(v: StoredEvent) -> Self {
        PyStoredEvent { inner: v }
    }
}

#[pymethods]
impl PyStoredEvent {
    #[getter]
    fn id(&self) -> PyEventId {
        PyEventId::from(self.inner.id)
    }

    #[getter]
    fn stream_id(&self) -> &str {
        &self.inner.stream_id
    }

    #[getter]
    fn branch(&self) -> &str {
        &self.inner.branch
    }

    #[getter]
    fn version(&self) -> u64 {
        self.inner.version
    }

    #[getter]
    fn timestamp_us(&self) -> i64 {
        self.inner.timestamp_us
    }

    #[getter]
    fn causation_id(&self) -> Option<PyEventId> {
        self.inner.causation_id.map(PyEventId::from)
    }

    #[getter]
    fn correlation_id(&self) -> Option<PyEventId> {
        self.inner.correlation_id.map(PyEventId::from)
    }

    #[getter]
    fn event_type(&self) -> &str {
        &self.inner.event_type
    }

    #[getter]
    fn type_version(&self) -> u32 {
        self.inner.type_version
    }

    /// Return the payload as a Python dict/list/etc. (decoded from msgpack via JSON).
    fn payload<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        payload_to_py(py, &self.inner.payload)
    }

    /// Return the raw msgpack payload bytes.
    fn payload_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.payload)
    }

    #[getter]
    fn external_id(&self) -> Option<&str> {
        self.inner.external_id.as_deref()
    }

    /// Return indexed_tags as a Python dict, or None.
    fn indexed_tags<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.inner.indexed_tags {
            None => Ok(None),
            Some(v) => json_to_py(py, v).map(Some),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "StoredEvent(stream_id={:?}, version={}, event_type={:?})",
            self.inner.stream_id, self.inner.version, self.inner.event_type,
        )
    }
}

// ── Append ────────────────────────────────────────────────────────────────────

/// Parameters for a single event write.
#[pyclass(name = "Append")]
pub struct PyAppend {
    pub stream_id: String,
    pub branch: String,
    pub event_type: String,
    pub type_version: u32,
    /// Python dict / list / scalar — will be serialized.
    pub payload: Py<PyAny>,
    pub causation_id: Option<PyEventId>,
    pub correlation_id: Option<PyEventId>,
    pub external_id: Option<String>,
    pub indexed_tags: Option<Py<PyAny>>,
    pub timestamp_us: Option<i64>,
}

#[pymethods]
impl PyAppend {
    #[new]
    #[pyo3(signature = (
        stream_id,
        event_type,
        payload,
        branch = "main".to_string(),
        type_version = 1,
        causation_id = None,
        correlation_id = None,
        external_id = None,
        indexed_tags = None,
        timestamp_us = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        stream_id: String,
        event_type: String,
        payload: Py<PyAny>,
        branch: String,
        type_version: u32,
        causation_id: Option<PyEventId>,
        correlation_id: Option<PyEventId>,
        external_id: Option<String>,
        indexed_tags: Option<Py<PyAny>>,
        timestamp_us: Option<i64>,
    ) -> Self {
        PyAppend {
            stream_id,
            branch,
            event_type,
            type_version,
            payload,
            causation_id,
            correlation_id,
            external_id,
            indexed_tags,
            timestamp_us,
        }
    }
}

impl PyAppend {
    /// Convert to the Rust `Append` type, serializing the Python payload.
    pub fn to_rust(&self, py: Python<'_>) -> PyResult<Append> {
        let payload_json = py_to_json(py, self.payload.bind(py))?;

        let indexed_tags = match &self.indexed_tags {
            None => None,
            Some(obj) => Some(py_to_json(py, obj.bind(py))?),
        };

        Ok(Append {
            stream_id: self.stream_id.clone(),
            branch: self.branch.clone(),
            event_type: self.event_type.clone(),
            type_version: self.type_version,
            payload: payload_json,
            causation_id: self.causation_id.as_ref().map(|e| e.inner),
            correlation_id: self.correlation_id.as_ref().map(|e| e.inner),
            external_id: self.external_id.clone(),
            indexed_tags,
            timestamp_us: self.timestamp_us,
        })
    }
}

// ── ReadQuery ─────────────────────────────────────────────────────────────────

#[pyclass(name = "ReadQuery")]
pub struct PyReadQuery {
    pub stream_id: String,
    pub branch: String,
    pub from_version: Option<u64>,
    pub to_version: Option<u64>,
    pub limit: Option<usize>,
    pub event_type_filter: Option<String>,
}

#[pymethods]
impl PyReadQuery {
    #[new]
    #[pyo3(signature = (stream_id, branch = "main".to_string(), from_version = None, to_version = None, limit = None, event_type_filter = None))]
    fn new(
        stream_id: String,
        branch: String,
        from_version: Option<u64>,
        to_version: Option<u64>,
        limit: Option<usize>,
        event_type_filter: Option<String>,
    ) -> Self {
        PyReadQuery { stream_id, branch, from_version, to_version, limit, event_type_filter }
    }
}

impl From<&PyReadQuery> for ReadQuery {
    fn from(q: &PyReadQuery) -> Self {
        ReadQuery {
            stream_id: q.stream_id.clone(),
            branch: q.branch.clone(),
            from_version: q.from_version,
            to_version: q.to_version,
            limit: q.limit,
            event_type_filter: q.event_type_filter.clone(),
        }
    }
}

// ── OpenOptions ───────────────────────────────────────────────────────────────

#[pyclass(name = "OpenOptions")]
pub struct PyOpenOptions {
    pub encryption: String,
    pub on_first_open: String,
}

#[pymethods]
impl PyOpenOptions {
    #[new]
    #[pyo3(signature = (encryption = "plaintext".to_string(), on_first_open = "create_if_missing".to_string()))]
    fn new(encryption: String, on_first_open: String) -> Self {
        PyOpenOptions { encryption, on_first_open }
    }
}

impl TryFrom<&PyOpenOptions> for OpenOptions {
    type Error = PyErr;

    fn try_from(o: &PyOpenOptions) -> PyResult<Self> {
        let encryption = match o.encryption.as_str() {
            "plaintext" => EncryptionMode::Plaintext,
            "os_keyring" => EncryptionMode::OsKeyring,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown encryption mode {other:?}; expected \"plaintext\" or \"os_keyring\""
                )))
            }
        };
        let on_first_open = match o.on_first_open.as_str() {
            "create_if_missing" => FirstOpenPolicy::CreateIfMissing,
            "require_existing" => FirstOpenPolicy::RequireExisting,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown on_first_open {other:?}; expected \"create_if_missing\" or \"require_existing\""
                )))
            }
        };
        Ok(OpenOptions {
            encryption,
            checkpoint_mode: CheckpointMode::Auto,
            on_first_open,
            similarity_provider: None,
            read_pool_size: 4,
            read_pool_timeout_ms: 30_000,
            default_max_results: None,
            default_max_bytes: None,
            reducer_state_large_threshold_bytes: 1_048_576,
            auto_gc_orphans: false,
            background_executor_grace_timeout_ms: 10_000,
            executor_quiescence_window_ms: 2_000,
        })
    }
}

// ── StreamInfo ────────────────────────────────────────────────────────────────

#[pyclass(name = "StreamInfo", skip_from_py_object)]
#[derive(Clone)]
pub struct PyStreamInfo {
    inner: StreamInfo,
}

impl From<StreamInfo> for PyStreamInfo {
    fn from(v: StreamInfo) -> Self {
        PyStreamInfo { inner: v }
    }
}

#[pymethods]
impl PyStreamInfo {
    #[getter] fn id(&self) -> &str { &self.inner.id }
    #[getter] fn declared_by(&self) -> &str { &self.inner.declared_by }
    #[getter] fn declared_at(&self) -> i64 { self.inner.declared_at }
    #[getter] fn description(&self) -> Option<&str> { self.inner.description.as_deref() }
    fn __repr__(&self) -> String {
        format!("StreamInfo(id={:?})", self.inner.id)
    }
}

// ── BranchInfo ────────────────────────────────────────────────────────────────

#[pyclass(name = "BranchInfo", skip_from_py_object)]
#[derive(Clone)]
pub struct PyBranchInfo {
    inner: BranchInfo,
}

impl From<BranchInfo> for PyBranchInfo {
    fn from(v: BranchInfo) -> Self {
        PyBranchInfo { inner: v }
    }
}

#[pymethods]
impl PyBranchInfo {
    #[getter] fn id(&self) -> &str { &self.inner.id }
    #[getter] fn stream_id(&self) -> &str { &self.inner.stream_id }
    #[getter] fn parent_id(&self) -> &str { &self.inner.parent_id }
    #[getter] fn parent_version(&self) -> u64 { self.inner.parent_version }
    #[getter] fn description(&self) -> Option<&str> { self.inner.description.as_deref() }
    #[getter] fn created_at(&self) -> i64 { self.inner.created_at }
    #[getter] fn lifecycle(&self) -> &str { &self.inner.lifecycle }
    #[getter] fn closed_at(&self) -> Option<i64> { self.inner.closed_at }
    #[getter] fn closed_reason(&self) -> Option<&str> { self.inner.closed_reason.as_deref() }

    fn alternatives<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.inner.alternatives {
            None => Ok(None),
            Some(v) => json_to_py(py, v).map(Some),
        }
    }

    fn __repr__(&self) -> String {
        format!("BranchInfo(id={:?}, lifecycle={:?})", self.inner.id, self.inner.lifecycle)
    }
}

// ── BranchSegment ─────────────────────────────────────────────────────────────

#[pyclass(name = "BranchSegment", skip_from_py_object)]
#[derive(Clone)]
pub struct PyBranchSegment {
    inner: BranchSegment,
}

impl From<BranchSegment> for PyBranchSegment {
    fn from(v: BranchSegment) -> Self {
        PyBranchSegment { inner: v }
    }
}

#[pymethods]
impl PyBranchSegment {
    #[getter] fn branch_id(&self) -> &str { &self.inner.branch_id }
    #[getter] fn to_version(&self) -> Option<u64> { self.inner.to_version }
    fn __repr__(&self) -> String {
        format!("BranchSegment(branch_id={:?}, to_version={:?})", self.inner.branch_id, self.inner.to_version)
    }
}

// ── CreateBranch ──────────────────────────────────────────────────────────────

#[pyclass(name = "CreateBranch")]
pub struct PyCreateBranch {
    pub stream_id: String,
    pub branch_id: String,
    pub parent_id: String,
    pub parent_version: u64,
    pub description: Option<String>,
    pub alternatives: Option<Py<PyAny>>,
}

#[pymethods]
impl PyCreateBranch {
    #[new]
    #[pyo3(signature = (stream_id, branch_id, parent_id = "main".to_string(), parent_version = 0, description = None, alternatives = None))]
    fn new(
        stream_id: String,
        branch_id: String,
        parent_id: String,
        parent_version: u64,
        description: Option<String>,
        alternatives: Option<Py<PyAny>>,
    ) -> Self {
        PyCreateBranch {
            stream_id,
            branch_id,
            parent_id,
            parent_version,
            description,
            alternatives,
        }
    }
}

impl PyCreateBranch {
    pub fn to_rust(&self, py: Python<'_>) -> PyResult<CreateBranch> {
        let alternatives = match &self.alternatives {
            None => None,
            Some(obj) => Some(py_to_json(py, obj.bind(py))?),
        };
        Ok(CreateBranch {
            stream_id: self.stream_id.clone(),
            branch_id: self.branch_id.clone(),
            parent_id: self.parent_id.clone(),
            parent_version: self.parent_version,
            description: self.description.clone(),
            alternatives,
        })
    }
}

// ── SnapshotInfo ──────────────────────────────────────────────────────────────

#[pyclass(name = "SnapshotInfo", skip_from_py_object)]
#[derive(Clone)]
pub struct PySnapshotInfo {
    inner: SnapshotInfo,
}

impl From<SnapshotInfo> for PySnapshotInfo {
    fn from(v: SnapshotInfo) -> Self {
        PySnapshotInfo { inner: v }
    }
}

#[pymethods]
impl PySnapshotInfo {
    #[getter] fn stream_id(&self) -> &str { &self.inner.stream_id }
    #[getter] fn branch(&self) -> &str { &self.inner.branch }
    #[getter] fn version(&self) -> u64 { self.inner.version }
    #[getter] fn reducer_name(&self) -> &str { &self.inner.reducer_name }
    #[getter] fn reducer_version(&self) -> u32 { self.inner.reducer_version }
    #[getter] fn state_schema_version(&self) -> u32 { self.inner.state_schema_version }
    #[getter] fn created_at(&self) -> i64 { self.inner.created_at }
    fn __repr__(&self) -> String {
        format!("SnapshotInfo(stream_id={:?}, version={})", self.inner.stream_id, self.inner.version)
    }
}

// ── SubscriptionMode ──────────────────────────────────────────────────────────

#[pyclass(name = "SubscriptionMode", skip_from_py_object)]
#[derive(Clone)]
pub struct PySubscriptionMode {
    pub kind: SubModeKind,
}

#[derive(Clone)]
pub enum SubModeKind {
    Synchronous,
    PostCommit { queue_size: usize },
}

#[pymethods]
impl PySubscriptionMode {
    /// Synchronous delivery — fires while the write lock is held.
    #[staticmethod]
    fn synchronous() -> Self {
        PySubscriptionMode { kind: SubModeKind::Synchronous }
    }

    /// Post-commit delivery via a bounded queue.
    #[staticmethod]
    #[pyo3(signature = (queue_size = 1024))]
    fn post_commit(queue_size: usize) -> Self {
        PySubscriptionMode { kind: SubModeKind::PostCommit { queue_size } }
    }

    fn __repr__(&self) -> &'static str {
        match self.kind {
            SubModeKind::Synchronous => "SubscriptionMode.synchronous()",
            SubModeKind::PostCommit { .. } => "SubscriptionMode.post_commit(...)",
        }
    }
}

// ── AggregateQuery ────────────────────────────────────────────────────────────

#[pyclass(name = "AggregateQuery")]
pub struct PyAggregateQuery {
    pub stream_pattern: String,
    pub branch: String,
    pub event_type_filter: Option<String>,
    pub from_timestamp_us: Option<i64>,
    pub to_timestamp_us: Option<i64>,
    /// Flat AND exact-match filter on indexed_tags. Must be a dict of
    /// JSON-primitive values (str, bool, int, float, None).
    pub indexed_tags_filter: Option<Py<PyAny>>,
}

#[pymethods]
impl PyAggregateQuery {
    #[new]
    #[pyo3(signature = (
        stream_pattern = "*".to_string(),
        branch = "main".to_string(),
        event_type_filter = None,
        from_timestamp_us = None,
        to_timestamp_us = None,
        indexed_tags_filter = None,
    ))]
    fn new(
        stream_pattern: String,
        branch: String,
        event_type_filter: Option<String>,
        from_timestamp_us: Option<i64>,
        to_timestamp_us: Option<i64>,
        indexed_tags_filter: Option<Py<PyAny>>,
    ) -> Self {
        PyAggregateQuery {
            stream_pattern,
            branch,
            event_type_filter,
            from_timestamp_us,
            to_timestamp_us,
            indexed_tags_filter,
        }
    }
}
