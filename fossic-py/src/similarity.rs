use crate::errors::to_py_err;
use fossic::{
    Error, EventId, OpenOptions, SimilarityQuery, SimilaritySearchProvider, Store, TaskPriority,
};
use fossic_similarity_hnsw::{DistanceMetric, HnswConfig, HnswProvider};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict};
use std::sync::Arc;

// ── PyHnswProvider ────────────────────────────────────────────────────────────

/// HNSW similarity search provider for fossic event stores.
///
/// Holds both an `Arc<HnswProvider>` (the HNSW index) and an internal `Store`
/// used as the background executor host for `schedule_save`. The Store uses
/// the same SQLite file as the index but its Custom tasks write only to the
/// `hnsw/` directory — no event rows are written.
///
/// ## Concurrent Store warning
/// If you also open a `fossic.Store` on the same `db_path`, two SQLite
/// connections will exist. Custom-task saves do not conflict (they write to
/// `hnsw/`, not to SQLite), but concurrent schema-setup writes during `open`
/// may serialize under SQLite's WAL writer lock. Both opens succeed; the
/// second may retry briefly.
#[pyclass(name = "HnswProvider")]
pub struct PyHnswProvider {
    provider: Arc<HnswProvider>,
    store: Store,
}

#[pymethods]
impl PyHnswProvider {
    /// Create (or reopen) an HNSW provider for the store at `db_path`.
    ///
    /// The `hnsw/` index directory is created beside the database file.
    /// If index files are present they are loaded; otherwise the index starts empty.
    ///
    /// Config keyword arguments correspond to `HnswConfig` fields:
    /// - `distance`: `"cosine"` (default), `"euclidean"` / `"l2"`, `"inner_product"` / `"dot"`
    /// - `max_elements`: capacity hint (default 100_000)
    /// - `ef_construction`: build-time recall knob (default 200)
    /// - `m`: graph degree per node (default 16)
    /// - `ef_search`: query-time recall knob (default 50)
    /// - `stream_filter_fudge_factor`: candidate expansion for stream-filtered queries (default 2)
    /// - `quiescence_window_ms`: how long the store must be idle before the background
    ///   executor fires a scheduled save (default 2000). Lower values speed up tests.
    #[new]
    #[pyo3(signature = (
        db_path,
        dimensions,
        *,
        distance = "cosine",
        max_elements = 100_000usize,
        ef_construction = 200usize,
        m = 16usize,
        ef_search = 50usize,
        stream_filter_fudge_factor = 2usize,
        quiescence_window_ms = 2_000u64,
    ))]
    fn new_py(
        db_path: &str,
        dimensions: usize,
        distance: &str,
        max_elements: usize,
        ef_construction: usize,
        m: usize,
        ef_search: usize,
        stream_filter_fudge_factor: usize,
        quiescence_window_ms: u64,
    ) -> PyResult<Self> {
        let dm = parse_distance(distance)?;
        let config = HnswConfig {
            max_elements,
            dimensions,
            ef_construction,
            m,
            ef_search,
            distance: dm,
            stream_filter_fudge_factor,
        };
        let provider = Arc::new(HnswProvider::new(db_path, config).map_err(hnsw_err)?);
        let store = Store::open(
            db_path,
            OpenOptions {
                executor_quiescence_window_ms: quiescence_window_ms,
                ..OpenOptions::default()
            },
        )
        .map_err(to_py_err)?;
        Ok(PyHnswProvider { provider, store })
    }

    /// Index an event: record its embedding in the HNSW graph.
    ///
    /// `event_id` must be 32 bytes (a fossic CCE event ID).
    /// `embedding` must have exactly `dimensions` elements.
    ///
    /// Events indexed via this method are not registered with a stream ID.
    /// `query` calls with `stream_pattern` will exclude them. Use
    /// `index_with_stream_id` when stream-pattern filtering is needed.
    fn index(&self, event_id: &[u8], embedding: Vec<f32>) -> PyResult<()> {
        let eid = bytes_to_event_id(event_id)?;
        self.provider.index(eid, &embedding).map_err(to_py_err)
    }

    /// Index an event together with its stream ID for stream-pattern filtering.
    ///
    /// Prefer over `index` when `query` calls may carry `stream_pattern`.
    fn index_with_stream_id(
        &self,
        event_id: &[u8],
        stream_id: &str,
        embedding: Vec<f32>,
    ) -> PyResult<()> {
        let eid = bytes_to_event_id(event_id)?;
        self.provider
            .index_with_stream_id(eid, stream_id, &embedding)
            .map_err(hnsw_err)
    }

    /// Run a k-nearest-neighbour query.
    ///
    /// `query` is a dict with keys:
    /// - `embedding` (`list[float]`, required) — query vector
    /// - `k` (`int`, required) — number of results
    /// - `stream_pattern` (`str | None`, optional) — glob filter (only
    ///   vectors indexed via `index_with_stream_id` are eligible)
    ///
    /// Returns a list of `{"event_id": bytes, "score": float}` dicts,
    /// ordered by distance (closest first for Euclidean/InnerProduct; for
    /// Cosine, lower score = more similar per hnsw_rs convention).
    fn query<'py>(&self, py: Python<'py>, query: &Bound<'_, PyDict>) -> PyResult<Vec<Py<PyAny>>> {
        let embedding: Vec<f32> = query
            .get_item("embedding")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("missing key 'embedding'"))?
            .extract()?;
        let k: usize = query
            .get_item("k")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("missing key 'k'"))?
            .extract()?;
        let stream_pattern: Option<String> = match query.get_item("stream_pattern")? {
            None => None,
            Some(v) if v.is_none() => None,
            Some(v) => Some(v.extract()?),
        };

        let q = SimilarityQuery {
            embedding,
            k,
            stream_pattern,
        };
        let hits = self.provider.query(q).map_err(to_py_err)?;

        hits.into_iter()
            .map(|h| {
                let d = PyDict::new(py);
                d.set_item("event_id", PyBytes::new(py, h.event_id.as_bytes()))?;
                d.set_item("score", h.score as f64)?;
                Ok(d.into_any().unbind())
            })
            .collect::<PyResult<Vec<_>>>()
    }

    /// Persist the index to disk synchronously.
    ///
    /// Writes `index.hnsw.data`, `index.hnsw.graph`, and `mappings.bin`
    /// atomically — if any write fails all three files are removed.
    fn save(&self) -> PyResult<()> {
        self.provider.save_to_disk().map_err(hnsw_err)
    }

    /// Schedule a deferred save via the background executor.
    ///
    /// No-op when dirty=False (no indexing since last save) or when a save
    /// is already pending (storm prevention). The executor fires after the
    /// store has been idle for `quiescence_window_ms` milliseconds (default
    /// 2000). If the provider is dropped before the executor window opens,
    /// no save occurs and in-memory changes are lost.
    ///
    /// `priority`: `"low"` (default), `"normal"`, or `"high"`.
    #[pyo3(signature = (priority = "low"))]
    fn schedule_save(&self, priority: &str) -> PyResult<()> {
        let prio = parse_priority(priority)?;
        HnswProvider::schedule_save(self.provider.clone(), &self.store, prio);
        Ok(())
    }

    /// Number of vectors in the index.
    fn len(&self) -> usize {
        self.provider.len()
    }

    /// True when the index contains no vectors.
    fn is_empty(&self) -> bool {
        self.provider.is_empty()
    }

    /// Remove a vector by event ID.
    ///
    /// Always raises `RuntimeError` in v1 — hnsw_rs does not expose point
    /// deletion. Full deletion support is deferred to v2.
    fn remove(&self, event_id: &[u8]) -> PyResult<()> {
        let eid = bytes_to_event_id(event_id)?;
        self.provider.remove(eid).map_err(hnsw_err)
    }

    /// True when the in-memory index has changes not yet persisted.
    fn is_dirty(&self) -> bool {
        self.provider.is_dirty()
    }

    /// True when a background save is pending execution.
    fn is_save_pending(&self) -> bool {
        self.provider.is_save_pending()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn bytes_to_event_id(b: &[u8]) -> PyResult<EventId> {
    b.try_into().map(EventId::from_bytes).map_err(|_| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "event_id must be 32 bytes, got {}",
            b.len()
        ))
    })
}

fn hnsw_err(e: fossic_similarity_hnsw::HnswError) -> PyErr {
    to_py_err(Error::Internal(e.to_string()))
}

fn parse_distance(s: &str) -> PyResult<DistanceMetric> {
    match s {
        "cosine" => Ok(DistanceMetric::Cosine),
        "euclidean" | "l2" => Ok(DistanceMetric::Euclidean),
        "inner_product" | "dot" => Ok(DistanceMetric::InnerProduct),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown distance metric '{other}', expected 'cosine', 'euclidean', or 'inner_product'"
        ))),
    }
}

fn parse_priority(s: &str) -> PyResult<TaskPriority> {
    match s {
        "low" => Ok(TaskPriority::Low),
        "normal" => Ok(TaskPriority::Normal),
        "high" => Ok(TaskPriority::High),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown priority '{other}', expected 'low', 'normal', or 'high'"
        ))),
    }
}
