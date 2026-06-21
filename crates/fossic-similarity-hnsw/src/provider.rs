use std::{collections::HashMap, path::PathBuf};

use hnsw_rs::anndists::dist::distances::{DistCosine, DistDot, DistL2};
use fossic::{Error, EventId, SimilarityHit, SimilarityQuery, SimilaritySearchProvider, SystemStreamWriter};
use hnsw_rs::hnsw::Hnsw;
use parking_lot::Mutex;

use crate::{config::{DistanceMetric, HnswConfig}, error::HnswError};

// ── HnswIndex ─────────────────────────────────────────────────────────────────

/// Typed wrapper over hnsw_rs Hnsw. One variant per distance metric.
/// hnsw_rs uses interior mutability (parking_lot RwLock), so all operations
/// take `&self`.
pub(crate) enum HnswIndex {
    Cosine(Hnsw<'static, f32, DistCosine>),
    Euclidean(Hnsw<'static, f32, DistL2>),
    InnerProduct(Hnsw<'static, f32, DistDot>),
}

impl HnswIndex {
    fn new(cfg: &HnswConfig) -> Self {
        match cfg.distance {
            DistanceMetric::Cosine => HnswIndex::Cosine(Hnsw::new(
                cfg.m,
                cfg.max_elements,
                16, // max_layer (= NB_LAYER_MAX)
                cfg.ef_construction,
                DistCosine,
            )),
            DistanceMetric::Euclidean => HnswIndex::Euclidean(Hnsw::new(
                cfg.m,
                cfg.max_elements,
                16,
                cfg.ef_construction,
                DistL2,
            )),
            DistanceMetric::InnerProduct => HnswIndex::InnerProduct(Hnsw::new(
                cfg.m,
                cfg.max_elements,
                16,
                cfg.ef_construction,
                DistDot,
            )),
        }
    }

    fn insert(&self, embedding: &[f32], id: usize) {
        match self {
            HnswIndex::Cosine(h) => h.insert((embedding, id)),
            HnswIndex::Euclidean(h) => h.insert((embedding, id)),
            HnswIndex::InnerProduct(h) => h.insert((embedding, id)),
        }
    }

    fn search(&self, embedding: &[f32], k: usize, ef: usize) -> Vec<hnsw_rs::hnsw::Neighbour> {
        match self {
            HnswIndex::Cosine(h) => h.search(embedding, k, ef),
            HnswIndex::Euclidean(h) => h.search(embedding, k, ef),
            HnswIndex::InnerProduct(h) => h.search(embedding, k, ef),
        }
    }

    fn nb_points(&self) -> usize {
        match self {
            HnswIndex::Cosine(h) => h.get_nb_point(),
            HnswIndex::Euclidean(h) => h.get_nb_point(),
            HnswIndex::InnerProduct(h) => h.get_nb_point(),
        }
    }
}

// ── HnswInner ─────────────────────────────────────────────────────────────────

/// All mutable HNSW state. Held behind `Mutex<Option<HnswInner>>` in
/// `HnswProvider` — `None` until the first `index()` call.
pub(crate) struct HnswInner {
    pub(crate) index: HnswIndex,
    /// Maps hnsw_rs DataId (usize) → fossic EventId.
    /// Grows in lock-step with inserts; `usize_to_event_id[n]` is the EventId
    /// for the vector inserted with id `n`.
    pub(crate) usize_to_event_id: Vec<EventId>,
    /// Optional stream-id mapping for stream-pattern filtering in `query()`.
    ///
    /// CP-D2-2: `SimilaritySearchProvider::index` does not receive `stream_id`,
    /// so this map is only populated via `HnswProvider::index_with_stream_id`.
    /// Events indexed via the trait-only path will not match any stream pattern
    /// filter (they are excluded from filtered results, not included).
    pub(crate) event_id_to_stream_id: HashMap<EventId, String>,
    /// Monotonically incrementing counter. Each insert uses `next_id` as the
    /// hnsw_rs DataId, then increments. Stays in sync with
    /// `usize_to_event_id.len()`.
    pub(crate) next_id: usize,
}

impl HnswInner {
    fn new(cfg: &HnswConfig) -> Self {
        HnswInner {
            index: HnswIndex::new(cfg),
            usize_to_event_id: Vec::new(),
            event_id_to_stream_id: HashMap::new(),
            next_id: 0,
        }
    }
}

// ── HnswProvider ─────────────────────────────────────────────────────────────

/// HNSW-backed implementation of `fossic::SimilaritySearchProvider`.
///
/// ## Construction
/// ```rust,ignore
/// use fossic::{OpenOptions, Store};
/// use fossic_similarity_hnsw::{HnswConfig, HnswProvider};
/// use std::sync::Arc;
///
/// let config = HnswConfig { dimensions: 1024, ..HnswConfig::default() };
/// let provider = Arc::new(HnswProvider::new("/path/to/store.db", config)?);
/// let store = Store::open("/path/to/store.db", OpenOptions {
///     similarity_provider: Some(provider),
///     ..Default::default()
/// })?;
/// ```
///
/// ## Stream-pattern filtering
/// The `SimilaritySearchProvider::index` trait method does not carry `stream_id`,
/// so events indexed via that path cannot be filtered by stream pattern. Use
/// [`HnswProvider::index_with_stream_id`] directly when stream-pattern filtering
/// is required (CP-D2-2).
pub struct HnswProvider {
    pub(crate) config: HnswConfig,
    /// `<parent_of_store_db>/hnsw/` directory.
    pub(crate) index_dir: PathBuf,
    pub(crate) inner: Mutex<Option<HnswInner>>,
    /// Lazy-initialized on first system-event emission (v1.7.2).
    pub(crate) system_writer: Mutex<Option<SystemStreamWriter>>,
}

impl HnswProvider {
    /// Create a new provider pointing at `store_db_path`.
    ///
    /// The HNSW index directory is created at `<parent_of_store_db>/hnsw/`
    /// if it does not already exist. No index is built until the first
    /// [`SimilaritySearchProvider::index`] call.
    pub fn new(
        store_db_path: impl Into<PathBuf>,
        config: HnswConfig,
    ) -> Result<Self, HnswError> {
        if config.dimensions == 0 {
            return Err(HnswError::InvalidDimensions { expected: 1, got: 0 });
        }

        let db_path = store_db_path.into();
        let index_dir = db_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("hnsw");

        std::fs::create_dir_all(&index_dir)?;

        Ok(HnswProvider {
            config,
            index_dir,
            inner: Mutex::new(None),
            system_writer: Mutex::new(None),
        })
    }

    /// Path prefix for hnsw_rs file_dump: `<index_dir>/index` produces
    /// `<index_dir>/index.hnsw.data` and `<index_dir>/index.hnsw.graph`.
    #[allow(dead_code)]
    pub(crate) fn index_basename(&self) -> String {
        "index".to_string()
    }

    #[allow(dead_code)]
    pub(crate) fn mappings_bin_path(&self) -> PathBuf {
        self.index_dir.join("mappings.bin")
    }

    /// Number of vectors currently in the index.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .as_ref()
            .map(|i| i.index.nb_points())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Index an event alongside its `stream_id` for stream-pattern filtering.
    ///
    /// Prefer this over the trait's `index` method when `SimilarityQuery`s
    /// may carry a `stream_pattern`. Events indexed without `stream_id` will
    /// be excluded from filtered queries (CP-D2-2).
    pub fn index_with_stream_id(
        &self,
        event_id: EventId,
        stream_id: &str,
        embedding: &[f32],
    ) -> Result<(), HnswError> {
        let expected = self.config.dimensions;
        if embedding.len() != expected {
            return Err(HnswError::InvalidDimensions { expected, got: embedding.len() });
        }
        let mut guard = self.inner.lock();
        let inner = guard.get_or_insert_with(|| HnswInner::new(&self.config));
        let id = inner.next_id;
        inner.index.insert(embedding, id);
        inner.usize_to_event_id.push(event_id);
        inner.event_id_to_stream_id.insert(event_id, stream_id.to_string());
        inner.next_id += 1;
        Ok(())
    }

    /// Remove a vector from the index.
    ///
    /// hnsw_rs does not expose a point-deletion API (HNSW graph mutation on
    /// delete is expensive). This method is a no-op that returns an error.
    /// Full deletion support is deferred to v2 if needed.
    pub fn remove(&self, _event_id: EventId) -> Result<(), HnswError> {
        Err(HnswError::Hnsw(
            "remove is not supported in v1; hnsw_rs does not expose point deletion".to_string(),
        ))
    }

    pub(crate) fn ensure_system_writer(&self) {
        let mut guard = self.system_writer.lock();
        if guard.is_none() {
            if let Some(db_dir) = self.index_dir.parent() {
                let db_path = db_dir.join("store.db");
                *guard = SystemStreamWriter::new(&db_path);
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn emit_system_event(&self, event_type: &str, payload: &serde_json::Value) {
        self.ensure_system_writer();
        let indexed_tags = serde_json::json!({ "event_class": "hnsw" });
        if let Some(ref mut w) = *self.system_writer.lock() {
            w.emit(event_type, payload, Some(&indexed_tags));
        }
    }
}

// ── SimilaritySearchProvider impl ────────────────────────────────────────────

impl SimilaritySearchProvider for HnswProvider {
    fn index(&self, event_id: EventId, embedding: &[f32]) -> Result<(), Error> {
        let expected = self.config.dimensions;
        if embedding.len() != expected {
            return Err(HnswError::InvalidDimensions { expected, got: embedding.len() }.into());
        }
        let mut guard = self.inner.lock();
        let inner = guard.get_or_insert_with(|| HnswInner::new(&self.config));
        let id = inner.next_id;
        inner.index.insert(embedding, id);
        inner.usize_to_event_id.push(event_id);
        // stream_id not available via trait signature — see CP-D2-2 and
        // index_with_stream_id for stream-pattern-filterable indexing.
        inner.next_id += 1;
        Ok(())
    }

    fn query(&self, q: SimilarityQuery) -> Result<Vec<SimilarityHit>, Error> {
        let expected = self.config.dimensions;
        if q.embedding.len() != expected {
            return Err(HnswError::InvalidDimensions { expected, got: q.embedding.len() }.into());
        }
        if q.k == 0 {
            return Ok(vec![]);
        }

        let guard = self.inner.lock();
        let inner = match guard.as_ref() {
            None => return Ok(vec![]),
            Some(i) => i,
        };

        if inner.index.nb_points() == 0 {
            return Ok(vec![]);
        }

        let internal_k = if q.stream_pattern.is_some() {
            q.k.saturating_mul(self.config.stream_filter_fudge_factor).max(q.k)
        } else {
            q.k
        };

        let neighbours = inner.index.search(&q.embedding, internal_k, self.config.ef_search);

        let mut hits: Vec<SimilarityHit> = Vec::with_capacity(q.k);
        for n in neighbours {
            let id = n.d_id;
            let event_id = match inner.usize_to_event_id.get(id) {
                Some(&eid) => eid,
                None => continue,
            };

            if let Some(ref pattern) = q.stream_pattern {
                match inner.event_id_to_stream_id.get(&event_id) {
                    Some(sid) if fossic::glob::matches(pattern, sid) => {}
                    _ => continue, // no stream_id registered or pattern mismatch
                }
            }

            hits.push(SimilarityHit { event_id, score: n.distance });
            if hits.len() >= q.k {
                break;
            }
        }

        Ok(hits)
    }
}
