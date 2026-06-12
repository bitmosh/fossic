use crossbeam_channel as cc;
use fossic::{
    Aggregate, AggregateQuery, Error, PayloadTransform, Store, SubscribeQuery,
    SubscriptionMode, Upcaster, WalkDirection,
};
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::{
    errors::to_py_err,
    subscriptions::{PyQueueHandler, PyRawSubscriptionHandle},
    types::{
        json_to_py, py_to_json, PyAppend, PyAggregateQuery, PyBranchInfo, PyBranchSegment,
        PyCreateBranch, PyEventId, PyOpenOptions, PyReadQuery, PySnapshotInfo, PyStoredEvent,
        PyStreamInfo, PySubscriptionMode, SubModeKind,
    },
};

// ── Python-callable payload transform ────────────────────────────────────────

/// Wraps a Python callable as a `PayloadTransform`.
///
/// The callable receives `(event_type: str, payload: dict) -> dict`.
struct PyTransform {
    callable: Py<PyAny>,
}

impl PayloadTransform for PyTransform {
    fn transform(&self, event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        Python::attach(|py| {
            let v: serde_json::Value =
                rmp_serde::from_slice(payload).map_err(Error::MsgpackDecode)?;
            let py_payload = json_to_py(py, &v)
                .map_err(|e| Error::Internal(format!("transform payload → Python: {e}")))?;
            let result = self
                .callable
                .call1(py, (event_type, py_payload))
                .map_err(|e| Error::Internal(format!("transform callable raised: {e}")))?;
            let json_v = py_to_json(py, result.bind(py))
                .map_err(|e| Error::Internal(format!("transform result → JSON: {e}")))?;
            rmp_serde::to_vec_named(&json_v).map_err(Error::MsgpackEncode)
        })
    }
}

// ── Python-callable upcaster ─────────────────────────────────────────────────

/// Wraps a Python callable as an `Upcaster`.
///
/// The callable receives `(payload: dict) -> dict`.
struct PyUpcaster {
    callable: Py<PyAny>,
}

impl Upcaster for PyUpcaster {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        Python::attach(|py| {
            let v: serde_json::Value =
                rmp_serde::from_slice(payload).map_err(Error::MsgpackDecode)?;
            let py_payload = json_to_py(py, &v)
                .map_err(|e| Error::Internal(format!("upcaster payload → Python: {e}")))?;
            let result = self
                .callable
                .call1(py, (py_payload,))
                .map_err(|e| Error::Internal(format!("upcaster callable raised: {e}")))?;
            let json_v = py_to_json(py, result.bind(py))
                .map_err(|e| Error::Internal(format!("upcaster result → JSON: {e}")))?;
            rmp_serde::to_vec_named(&json_v).map_err(Error::MsgpackEncode)
        })
    }
}

// ── Collect-all aggregate ─────────────────────────────────────────────────────

/// `Aggregate` that collects every matching `StoredEvent`.  Used to implement
/// `Store.aggregate()` in Python — callers fold the returned list themselves.
struct CollectAll(Vec<fossic::StoredEvent>);

impl Aggregate for CollectAll {
    type Output = Vec<fossic::StoredEvent>;
    fn fold(&mut self, event: &fossic::StoredEvent) {
        self.0.push(event.clone());
    }
    fn finalize(self) -> Self::Output {
        self.0
    }
}

// ── Python-backed DynReducer ──────────────────────────────────────────────────

/// Bridges a Python reducer object into the Rust `DynReducer` trait.
///
/// The Python reducer must implement:
///   - `name: str`  (attribute or property)
///   - `version: int`  (attribute or property)
///   - `state_schema_version: int`  (attribute or property)
///   - `initial_state(self) -> Any`
///   - `apply(self, state: Any, event_payload: Any) -> Any`
///
/// State is serialised as msgpack via `rmp_serde`.
struct PyDynReducer {
    name: String,
    version: u32,
    state_schema_version: u32,
    py_obj: Py<PyAny>,
}

impl fossic::DynReducer for PyDynReducer {
    fn name(&self) -> &str {
        &self.name
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn state_schema_version(&self) -> u32 {
        self.state_schema_version
    }
    fn initial_state_bytes(&self) -> Result<Vec<u8>, fossic::Error> {
        Python::attach(|py| {
            let py_state = self
                .py_obj
                .call_method0(py, "initial_state")
                .map_err(|e| fossic::Error::ReducerError {
                    message: format!("initial_state() raised: {e}"),
                })?;
            let json_v = py_to_json(py, py_state.bind(py)).map_err(|e| {
                fossic::Error::ReducerError {
                    message: format!("initial_state result not JSON-serialisable: {e}"),
                }
            })?;
            rmp_serde::to_vec_named(&json_v).map_err(fossic::Error::MsgpackEncode)
        })
    }
    fn apply_bytes(
        &self,
        state_bytes: &[u8],
        event_payload: &[u8],
    ) -> Result<Vec<u8>, fossic::Error> {
        Python::attach(|py| {
            let state_json: serde_json::Value =
                rmp_serde::from_slice(state_bytes).map_err(fossic::Error::MsgpackDecode)?;
            let py_state = json_to_py(py, &state_json).map_err(|e| {
                fossic::Error::ReducerError {
                    message: format!("state deserialise to Python: {e}"),
                }
            })?;

            let event_json: serde_json::Value =
                rmp_serde::from_slice(event_payload).map_err(fossic::Error::MsgpackDecode)?;
            let py_event = json_to_py(py, &event_json).map_err(|e| {
                fossic::Error::ReducerError {
                    message: format!("event_payload deserialise to Python: {e}"),
                }
            })?;

            let new_state = self
                .py_obj
                .call_method1(py, "apply", (py_state, py_event))
                .map_err(|e| fossic::Error::ReducerError {
                    message: format!("apply() raised: {e}"),
                })?;
            let new_json = py_to_json(py, new_state.bind(py)).map_err(|e| {
                fossic::Error::ReducerError {
                    message: format!("apply result not JSON-serialisable: {e}"),
                }
            })?;
            rmp_serde::to_vec_named(&new_json).map_err(fossic::Error::MsgpackEncode)
        })
    }
}

// ── PyStore ───────────────────────────────────────────────────────────────────

/// A fossic event store. Thread-safe (Arc-backed internally).
#[pyclass(name = "Store")]
pub struct PyStore {
    inner: Store,
}

#[pymethods]
impl PyStore {
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Open (or create) a store at `path`.
    #[staticmethod]
    #[pyo3(signature = (path, options = None))]
    fn open(path: &str, options: Option<PyRef<PyOpenOptions>>) -> PyResult<Self> {
        let rust_opts = match options {
            Some(o) => fossic::OpenOptions::try_from(&*o)?,
            None => fossic::OpenOptions::default(),
        };
        let expanded = shellexpand::tilde(path);
        Store::open(expanded.as_ref(), rust_opts)
            .map(|inner| PyStore { inner })
            .map_err(to_py_err)
    }

    // ── Stream registry ───────────────────────────────────────────────────────

    fn declare_stream(
        &self,
        stream_id: &str,
        declared_by: &str,
        description: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .declare_stream(stream_id, declared_by, description)
            .map_err(to_py_err)
    }

    fn streams(&self) -> PyResult<Vec<PyStreamInfo>> {
        self.inner
            .streams()
            .map(|v| v.into_iter().map(PyStreamInfo::from).collect())
            .map_err(to_py_err)
    }

    fn stream_exists(&self, stream_id: &str) -> PyResult<bool> {
        self.inner.stream_exists(stream_id).map_err(to_py_err)
    }

    // ── Append ────────────────────────────────────────────────────────────────

    fn append(&self, py: Python<'_>, a: PyRef<PyAppend>) -> PyResult<PyEventId> {
        let rust_a = a.to_rust(py)?;
        self.inner.append(rust_a).map(PyEventId::from).map_err(to_py_err)
    }

    fn append_batch(&self, py: Python<'_>, appends: Vec<PyRef<PyAppend>>) -> PyResult<Vec<PyEventId>> {
        let rust: Vec<fossic::Append> = appends
            .iter()
            .map(|a| a.to_rust(py))
            .collect::<PyResult<_>>()?;
        self.inner
            .append_batch(&rust)
            .map(|ids| ids.into_iter().map(PyEventId::from).collect())
            .map_err(to_py_err)
    }

    // ── Subscriptions ─────────────────────────────────────────────────────────

    /// Subscribe to events.
    ///
    /// Returns a `RawSubscriptionHandle` that supports `_wait_for_next_event`.
    /// The Python layer wraps this in a `SubscriptionHandle` that adds context
    /// manager support and the worker thread.
    #[pyo3(signature = (stream_pattern, branch = "main".to_string(), mode = None, include_system = false))]
    fn subscribe(
        &self,
        stream_pattern: String,
        branch: String,
        mode: Option<PyRef<PySubscriptionMode>>,
        include_system: bool,
    ) -> PyResult<PyRawSubscriptionHandle> {
        let mode = mode
            .as_ref()
            .map(|m| m.kind.clone())
            .unwrap_or(SubModeKind::PostCommit { queue_size: 1024 });

        let (tx, rx) = cc::unbounded::<fossic::StoredEvent>();
        let handler = PyQueueHandler::new(tx);

        let rust_mode = match mode {
            SubModeKind::Synchronous => SubscriptionMode::Synchronous,
            SubModeKind::PostCommit { queue_size } => SubscriptionMode::PostCommit { queue_size },
        };

        let q = SubscribeQuery { stream_pattern, branch, include_system };
        let handle = self.inner.subscribe(q, rust_mode, handler).map_err(to_py_err)?;

        Ok(PyRawSubscriptionHandle::new(rx, handle))
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    fn read_range(&self, query: PyRef<PyReadQuery>) -> PyResult<Vec<PyStoredEvent>> {
        self.inner
            .read_range(fossic::ReadQuery::from(&*query))
            .map(|v| v.into_iter().map(PyStoredEvent::from).collect())
            .map_err(to_py_err)
    }

    fn read_one(&self, event_id: PyRef<PyEventId>) -> PyResult<Option<PyStoredEvent>> {
        self.inner
            .read_one(event_id.inner)
            .map(|opt| opt.map(PyStoredEvent::from))
            .map_err(to_py_err)
    }

    fn read_by_external_id(
        &self,
        stream_id: &str,
        external_id: &str,
    ) -> PyResult<Option<PyStoredEvent>> {
        self.inner
            .read_by_external_id(stream_id, external_id)
            .map(|opt| opt.map(PyStoredEvent::from))
            .map_err(to_py_err)
    }

    // ── Cross-stream ──────────────────────────────────────────────────────────

    fn read_by_correlation(
        &self,
        correlation_id: PyRef<PyEventId>,
    ) -> PyResult<Vec<PyStoredEvent>> {
        self.inner
            .read_by_correlation(correlation_id.inner)
            .map(|v| v.into_iter().map(PyStoredEvent::from).collect())
            .map_err(to_py_err)
    }

    fn walk_causation(
        &self,
        start: PyRef<PyEventId>,
        direction: &str,
        max_depth: usize,
    ) -> PyResult<Vec<PyStoredEvent>> {
        let dir = match direction {
            "forward" => WalkDirection::Forward,
            "backward" => WalkDirection::Backward,
            "both" => WalkDirection::Both,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown direction {other:?}; expected \"forward\", \"backward\", or \"both\""
                )))
            }
        };
        self.inner
            .walk_causation(start.inner, dir, max_depth)
            .map(|v| v.into_iter().map(PyStoredEvent::from).collect())
            .map_err(to_py_err)
    }

    /// Run an aggregate query and return all matching events.
    ///
    /// The Python caller folds the events into the desired summary value.
    fn aggregate(&self, query: PyRef<PyAggregateQuery>) -> PyResult<Vec<PyStoredEvent>> {
        let aq = AggregateQuery {
            stream_pattern: query.stream_pattern.clone(),
            branch: query.branch.clone(),
            event_type_filter: query.event_type_filter.clone(),
            from_timestamp_us: query.from_timestamp_us,
            to_timestamp_us: query.to_timestamp_us,
        };
        self.inner
            .aggregate(aq, CollectAll(Vec::new()))
            .map(|v| v.into_iter().map(PyStoredEvent::from).collect())
            .map_err(to_py_err)
    }

    // ── Upcasters ─────────────────────────────────────────────────────────────

    /// Register a Python callable as an upcaster.
    ///
    /// `callable(payload: dict) -> dict`
    fn register_upcaster(
        &self,
        event_type: &str,
        from_version: u32,
        to_version: u32,
        callable: Py<PyAny>,
    ) -> PyResult<()> {
        self.inner
            .register_upcaster(event_type, from_version, to_version, PyUpcaster { callable })
            .map_err(to_py_err)
    }

    // ── Payload transforms ────────────────────────────────────────────────────

    /// Register a Python callable as a payload transform.
    ///
    /// `callable(event_type: str, payload: dict) -> dict`
    fn register_payload_transform(
        &self,
        stream_pattern: &str,
        callable: Py<PyAny>,
    ) -> PyResult<()> {
        self.inner
            .register_payload_transform(stream_pattern, PyTransform { callable })
            .map_err(to_py_err)
    }

    // ── Reducers ──────────────────────────────────────────────────────────────

    /// Register a Python reducer for streams matching *pattern*.
    ///
    /// The reducer object must implement the following protocol:
    ///
    /// Attributes (or properties):
    ///   - ``name: str`` — unique reducer name (used as snapshot key)
    ///   - ``version: int`` — reducer code version
    ///   - ``state_schema_version: int`` — state serialisation version
    ///
    /// Methods:
    ///   - ``initial_state(self) -> Any`` — returns the initial state dict
    ///   - ``apply(self, state: Any, event_payload: Any) -> Any`` — returns new state dict
    ///
    /// Unlike the legacy pure-Python approach, reducers registered here participate
    /// in snapshot caching — ``take_snapshot`` stores state to SQLite and future
    /// ``read_state`` calls start from the snapshot rather than replaying all events.
    fn register_reducer(
        &self,
        py: Python<'_>,
        pattern: &str,
        reducer: Py<PyAny>,
    ) -> PyResult<()> {
        let name: String = reducer
            .getattr(py, "name")
            .and_then(|v| v.extract::<String>(py))
            .map_err(|_| pyo3::exceptions::PyAttributeError::new_err(
                "reducer must have a `name: str` attribute",
            ))?;
        let version: u32 = reducer
            .getattr(py, "version")
            .and_then(|v| v.extract::<u32>(py))
            .map_err(|_| pyo3::exceptions::PyAttributeError::new_err(
                "reducer must have a `version: int` attribute",
            ))?;
        let state_schema_version: u32 = reducer
            .getattr(py, "state_schema_version")
            .and_then(|v| v.extract::<u32>(py))
            .map_err(|_| pyo3::exceptions::PyAttributeError::new_err(
                "reducer must have a `state_schema_version: int` attribute",
            ))?;

        let dyn_reducer = Box::new(PyDynReducer {
            name,
            version,
            state_schema_version,
            py_obj: reducer,
        });
        self.inner
            .register_dyn_reducer(pattern, dyn_reducer)
            .map_err(to_py_err)
    }

    /// Fold all events through the registered reducer and return the current state.
    ///
    /// Uses the most recent snapshot as the starting point (if one exists), so not
    /// all events need to be replayed from the beginning.
    fn read_state(&self, stream_id: &str, branch: &str) -> PyResult<Py<PyAny>> {
        let bytes = self.inner.read_state_bytes(stream_id, branch).map_err(to_py_err)?;
        Python::attach(|py| {
            let json_v: serde_json::Value =
                rmp_serde::from_slice(&bytes)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
            json_to_py(py, &json_v).map(|b| b.unbind())
        })
    }

    /// Like ``read_state`` but folds only events up to *version* inclusive.
    fn read_state_at_version(
        &self,
        stream_id: &str,
        branch: &str,
        version: u64,
    ) -> PyResult<Py<PyAny>> {
        let bytes = self
            .inner
            .read_state_bytes_at_version(stream_id, branch, version)
            .map_err(to_py_err)?;
        Python::attach(|py| {
            let json_v: serde_json::Value =
                rmp_serde::from_slice(&bytes)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
            json_to_py(py, &json_v).map(|b| b.unbind())
        })
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    fn purge_event(
        &self,
        event_id: PyRef<PyEventId>,
        confirm: &str,
        reason: &str,
        purged_by: &str,
    ) -> PyResult<()> {
        self.inner
            .purge_event(event_id.inner, confirm, reason, purged_by)
            .map_err(to_py_err)
    }

    fn shred_stream(&self, stream_id: &str, reason: &str) -> PyResult<()> {
        self.inner.shred_stream(stream_id, reason).map_err(to_py_err)
    }

    // ── Cursors ───────────────────────────────────────────────────────────────

    fn get_cursor(
        &self,
        consumer_id: &str,
        stream_id: &str,
        branch: &str,
    ) -> PyResult<Option<u64>> {
        self.inner.get_cursor(consumer_id, stream_id, branch).map_err(to_py_err)
    }

    fn set_cursor(
        &self,
        consumer_id: &str,
        stream_id: &str,
        branch: &str,
        version: u64,
    ) -> PyResult<()> {
        self.inner.set_cursor(consumer_id, stream_id, branch, version).map_err(to_py_err)
    }

    // ── Branches ─────────────────────────────────────────────────────────────

    fn create_branch(&self, py: Python<'_>, b: PyRef<PyCreateBranch>) -> PyResult<()> {
        let rust_b = b.to_rust(py)?;
        self.inner.create_branch(&rust_b).map_err(to_py_err)
    }

    fn promote_branch(
        &self,
        stream_id: &str,
        branch_id: &str,
        reason: Option<&str>,
    ) -> PyResult<()> {
        self.inner.promote_branch(stream_id, branch_id, reason).map_err(to_py_err)
    }

    fn mark_branch_dead_end(
        &self,
        stream_id: &str,
        branch_id: &str,
        reason: Option<&str>,
    ) -> PyResult<()> {
        self.inner.mark_branch_dead_end(stream_id, branch_id, reason).map_err(to_py_err)
    }

    fn list_branches(&self, stream_id: &str) -> PyResult<Vec<PyBranchInfo>> {
        self.inner
            .list_branches(stream_id)
            .map(|v| v.into_iter().map(PyBranchInfo::from).collect())
            .map_err(to_py_err)
    }

    fn resolve_chain(
        &self,
        stream_id: &str,
        branch_id: &str,
    ) -> PyResult<Vec<PyBranchSegment>> {
        self.inner
            .resolve_chain(stream_id, branch_id)
            .map(|v| v.into_iter().map(PyBranchSegment::from).collect())
            .map_err(to_py_err)
    }

    // ── Snapshots ─────────────────────────────────────────────────────────────

    fn snapshot_info(
        &self,
        stream_id: &str,
        branch: &str,
        reducer_name: &str,
    ) -> PyResult<Option<PySnapshotInfo>> {
        self.inner
            .snapshot_info(stream_id, branch, reducer_name)
            .map(|opt| opt.map(PySnapshotInfo::from))
            .map_err(to_py_err)
    }

    /// Take a snapshot of the current state.
    ///
    /// NOTE: requires a Rust reducer registered for `stream_id`.  Python-side
    /// reducers (registered via `Store.register_reducer`) are implemented in the
    /// pure-Python layer and do not interact with this method.  See the
    /// `register_reducer` docstring and the core-change request in FOSSIC-PY-NOTES.md.
    fn take_snapshot(&self, stream_id: &str, branch: &str) -> PyResult<PySnapshotInfo> {
        self.inner
            .take_snapshot(stream_id, branch)
            .map(PySnapshotInfo::from)
            .map_err(to_py_err)
    }

    fn gc_orphaned_snapshots(&self) -> PyResult<usize> {
        self.inner.gc_orphaned_snapshots().map_err(to_py_err)
    }
}
