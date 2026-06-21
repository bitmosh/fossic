use std::{
    collections::HashMap,
    io::{Read as IoRead, Write as IoWrite},
    path::PathBuf,
};

use fossic::{Error, EventId, SimilarityHit, SimilarityQuery, SimilaritySearchProvider, SystemStreamWriter};
use hnsw_rs::{
    anndists::dist::distances::{DistCosine, DistDot, DistL2},
    hnsw::Hnsw,
    hnswio::HnswIo,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{
    config::{DistanceMetric, HnswConfig},
    error::HnswError,
};

// ── Mappings persistence ──────────────────────────────────────────────────────

const MAPPINGS_VERSION: u8 = 0x01;

/// Wire format for the mappings.bin file.
/// Preceded by a single version byte (MAPPINGS_VERSION) at file offset 0.
#[derive(Serialize, Deserialize)]
struct MappingsFile {
    /// Parallel to the hnsw_rs DataId sequence. `usize_to_event_id[n]` is the
    /// fossic EventId for the vector inserted with hnsw_rs DataId `n`.
    usize_to_event_id: Vec<EventId>,
    /// Stream-id map for stream-pattern filtering. Only populated via
    /// `index_with_stream_id`; events indexed via trait path are absent (CP-D2-2).
    event_id_to_stream_id: HashMap<EventId, String>,
}

// ── HnswIndex ─────────────────────────────────────────────────────────────────

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
                16,
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

    /// Save graph + data via hnsw_rs file_dump.
    ///
    /// hnsw_rs produces TWO files: `{basename}.hnsw.data` and
    /// `{basename}.hnsw.graph`. The brief specified a single `index.bin`;
    /// this is a deviation — hnsw_rs's native format is two-file and there
    /// is no single-file option. The basename `"index"` is used, producing
    /// `index.hnsw.data` + `index.hnsw.graph` inside `index_dir`.
    ///
    /// `file_dump` overwrites existing files when mmap is not active
    /// (the default), so repeated saves replace rather than accumulate.
    fn dump(&self, index_dir: &std::path::Path, basename: &str) -> Result<(), HnswError> {
        use hnsw_rs::api::AnnT;
        let res = match self {
            HnswIndex::Cosine(h) => h.file_dump(index_dir, basename),
            HnswIndex::Euclidean(h) => h.file_dump(index_dir, basename),
            HnswIndex::InnerProduct(h) => h.file_dump(index_dir, basename),
        };
        res.map(|_| ())
            .map_err(|e| HnswError::IndexCorrupted(e.to_string()))
    }
}

// ── HnswInner ─────────────────────────────────────────────────────────────────

pub(crate) struct HnswInner {
    pub(crate) index: HnswIndex,
    pub(crate) usize_to_event_id: Vec<EventId>,
    /// CP-D2-2: only populated via `index_with_stream_id`; events indexed via
    /// trait path are absent and excluded from stream-filtered queries.
    pub(crate) event_id_to_stream_id: HashMap<EventId, String>,
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
/// ## Persistence
/// Call [`HnswProvider::save_to_disk`] to persist the index. The index
/// directory `<parent_of_store_db>/hnsw/` is created at construction time.
/// Existing files are loaded automatically if present at construction time.
///
/// ## Stream-pattern filtering
/// Use [`HnswProvider::index_with_stream_id`] when stream-pattern filtering
/// is required. Events indexed via the trait `index` method have no stream_id
/// registered and are excluded from filtered queries (CP-D2-2).
pub struct HnswProvider {
    pub(crate) config: HnswConfig,
    pub(crate) index_dir: PathBuf,
    pub(crate) inner: Mutex<Option<HnswInner>>,
    pub(crate) system_writer: Mutex<Option<SystemStreamWriter>>,
}

impl HnswProvider {
    /// Create (or reopen) a provider for the store at `store_db_path`.
    ///
    /// The index directory `<parent>/hnsw/` is created if it does not exist.
    /// If `index.hnsw.data`, `index.hnsw.graph`, and `mappings.bin` are all
    /// present, the index is loaded immediately; otherwise it starts empty.
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

        let provider = HnswProvider {
            config,
            index_dir,
            inner: Mutex::new(None),
            system_writer: Mutex::new(None),
        };

        provider.try_load_or_init()?;
        Ok(provider)
    }

    // ── File paths ────────────────────────────────────────────────────────────

    /// Basename passed to hnsw_rs `file_dump` / `HnswIo::new`.
    /// Produces `<index_dir>/index.hnsw.data` + `<index_dir>/index.hnsw.graph`.
    pub(crate) fn index_basename(&self) -> String {
        "index".to_string()
    }

    pub(crate) fn index_data_path(&self) -> PathBuf {
        self.index_dir.join("index.hnsw.data")
    }

    pub(crate) fn index_graph_path(&self) -> PathBuf {
        self.index_dir.join("index.hnsw.graph")
    }

    pub(crate) fn mappings_bin_path(&self) -> PathBuf {
        self.index_dir.join("mappings.bin")
    }

    fn index_files_exist(&self) -> bool {
        self.index_data_path().exists()
            && self.index_graph_path().exists()
            && self.mappings_bin_path().exists()
    }

    // ── Init: load or start empty ─────────────────────────────────────────────

    fn try_load_or_init(&self) -> Result<(), HnswError> {
        let now_us = fossic_now_us();
        if self.index_files_exist() {
            match self.load_inner(now_us) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // Corrupt files — emit system event, start empty, continue.
                    self.emit_system_event(
                        "HnswIndexCorrupted",
                        &serde_json::json!({
                            "error_message": e.to_string(),
                            "attempted_path": self.index_dir.display().to_string(),
                            "timestamp_us": now_us,
                        }),
                    );
                    // Fall through to empty init below.
                }
            }
        }

        // Start empty.
        let inner = HnswInner::new(&self.config);
        let initial_capacity = self.config.max_elements;
        let dimensions = self.config.dimensions;
        *self.inner.lock() = Some(inner);
        self.emit_system_event(
            "HnswIndexBuilt",
            &serde_json::json!({
                "dimensions": dimensions,
                "initial_capacity": initial_capacity,
                "timestamp_us": now_us,
            }),
        );
        Ok(())
    }

    fn load_inner(&self, now_us: i64) -> Result<(), HnswError> {
        // Load HNSW graph.
        // SAFETY: HnswIo::load_hnsw returns Hnsw<'b, T, D> where 'b is bounded
        // by the HnswIo lifetime. This lifetime exists to support mmap mode, where
        // point data is memory-mapped from the file and the Hnsw holds slices into
        // the mapping. We use ReloadOptions::default() which has datamap: false —
        // no mmap. Without mmap all point data is copied into owned Vecs inside
        // Hnsw; the returned value holds no borrows into io. Transmuting 'b to
        // 'static is safe in the no-mmap case.
        let mut io = HnswIo::new(&self.index_dir, &self.index_basename());
        let index = match self.config.distance {
            DistanceMetric::Cosine => {
                let h = load_hnsw_catching_panics::<DistCosine>(&mut io)?;
                HnswIndex::Cosine(h)
            }
            DistanceMetric::Euclidean => {
                let h = load_hnsw_catching_panics::<DistL2>(&mut io)?;
                HnswIndex::Euclidean(h)
            }
            DistanceMetric::InnerProduct => {
                let h = load_hnsw_catching_panics::<DistDot>(&mut io)?;
                HnswIndex::InnerProduct(h)
            }
        };

        // Load mappings.
        let mappings = self.load_mappings()?;
        let vector_count = mappings.usize_to_event_id.len();
        let next_id = vector_count;

        let index_file_bytes = self.index_data_path().metadata().map(|m| m.len()).unwrap_or(0)
            + self.index_graph_path().metadata().map(|m| m.len()).unwrap_or(0);
        let mappings_file_bytes = self
            .mappings_bin_path()
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        let inner = HnswInner {
            index,
            usize_to_event_id: mappings.usize_to_event_id,
            event_id_to_stream_id: mappings.event_id_to_stream_id,
            next_id,
        };
        *self.inner.lock() = Some(inner);

        self.emit_system_event(
            "HnswIndexLoaded",
            &serde_json::json!({
                "dimensions": self.config.dimensions,
                "vector_count": vector_count,
                "index_file_bytes": index_file_bytes,
                "mappings_file_bytes": mappings_file_bytes,
                "timestamp_us": now_us,
            }),
        );
        Ok(())
    }

    // ── Mappings serialization ────────────────────────────────────────────────

    fn save_mappings(&self, inner: &HnswInner) -> Result<(), HnswError> {
        let wire = MappingsFile {
            usize_to_event_id: inner.usize_to_event_id.clone(),
            event_id_to_stream_id: inner.event_id_to_stream_id.clone(),
        };
        let encoded = rmp_serde::to_vec(&wire)?;

        let mut f = std::fs::File::create(self.mappings_bin_path())?;
        f.write_all(&[MAPPINGS_VERSION])?;
        f.write_all(&encoded)?;
        Ok(())
    }

    fn save_empty_mappings(&self) -> Result<(), HnswError> {
        let wire = MappingsFile {
            usize_to_event_id: Vec::new(),
            event_id_to_stream_id: HashMap::new(),
        };
        let encoded = rmp_serde::to_vec(&wire)?;
        let mut f = std::fs::File::create(self.mappings_bin_path())?;
        f.write_all(&[MAPPINGS_VERSION])?;
        f.write_all(&encoded)?;
        Ok(())
    }

    fn load_mappings(&self) -> Result<MappingsFile, HnswError> {
        let mut f = std::fs::File::open(self.mappings_bin_path())?;
        let mut version = [0u8; 1];
        f.read_exact(&mut version)?;
        if version[0] != MAPPINGS_VERSION {
            return Err(HnswError::MappingsVersionMismatch(version[0]));
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        let wire: MappingsFile = rmp_serde::from_slice(&buf)?;
        Ok(wire)
    }

    // ── Public persistence API ────────────────────────────────────────────────

    /// Persist the current index to disk.
    ///
    /// Writes two hnsw_rs files (`index.hnsw.data` + `index.hnsw.graph`) and
    /// `mappings.bin`. All three must succeed together — if any write fails,
    /// all three files are removed before the error is returned (no partial
    /// saves on disk).
    ///
    /// Note: hnsw_rs produces two graph files, not a single `index.bin`.
    /// This is a deviation from the v1.7.2 brief; hnsw_rs has no single-file
    /// format. Both files are treated as a unit for save/load and cleanup.
    pub fn save_to_disk(&self) -> Result<(), HnswError> {
        let guard = self.inner.lock();
        match guard.as_ref() {
            None => {
                // Not yet initialized — nothing to save.
                return Ok(());
            }
            Some(inner) => {
                if inner.next_id == 0 {
                    // Empty index — write valid empty mappings so load knows it's valid.
                    return self.save_empty_mappings();
                }
                // Save HNSW graph files.
                if let Err(e) = inner.index.dump(&self.index_dir, &self.index_basename()) {
                    self.cleanup_index_files();
                    return Err(e);
                }
                // Save mappings.
                if let Err(e) = self.save_mappings(inner) {
                    self.cleanup_index_files();
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Delete all three index files, ignoring errors (best-effort cleanup).
    fn cleanup_index_files(&self) {
        let _ = std::fs::remove_file(self.index_data_path());
        let _ = std::fs::remove_file(self.index_graph_path());
        let _ = std::fs::remove_file(self.mappings_bin_path());
    }

    // ── Inherent methods ──────────────────────────────────────────────────────

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
    /// hnsw_rs does not expose a point-deletion API. This always returns an error.
    /// Full deletion support is deferred to v2 if needed.
    pub fn remove(&self, _event_id: EventId) -> Result<(), HnswError> {
        Err(HnswError::Hnsw(
            "remove is not supported in v1; hnsw_rs does not expose point deletion".to_string(),
        ))
    }

    // ── System event emission ─────────────────────────────────────────────────

    fn emit_system_event(&self, event_type: &str, payload: &serde_json::Value) {
        let mut guard = self.system_writer.lock();
        if guard.is_none() {
            if let Some(db_dir) = self.index_dir.parent() {
                let db_path = db_dir.join("store.db");
                *guard = SystemStreamWriter::new(&db_path);
            }
        }
        let indexed_tags = serde_json::json!({ "event_class": "hnsw" });
        if let Some(ref mut w) = *guard {
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
                    _ => continue,
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Load an HNSW index from `io`, catching panics from corrupt files.
///
/// hnsw_rs uses `assert_eq!` internally for format validation — a corrupt
/// data file will panic rather than return an error. We catch those panics
/// here and convert them to `HnswError::IndexCorrupted`.
///
/// SAFETY: `io` was constructed with the default `ReloadOptions`, which
/// disables mmap (`datamap: false`). Without mmap all point data is copied
/// into owned Vecs; the returned `Hnsw` holds no borrows into `io`. The
/// `'b → 'static` transmute is valid in the no-mmap case.
fn load_hnsw_catching_panics<D>(io: &mut HnswIo) -> Result<Hnsw<'static, f32, D>, HnswError>
where
    D: hnsw_rs::anndists::dist::Distance<f32> + Default + Send + Sync,
{
    // SAFETY: see function doc above. Transmute happens inside the closure so
    // that no Hnsw<'b, ..> (lifetime tied to io) escapes the closure body.
    let load_result: std::thread::Result<Result<Hnsw<'static, f32, D>, _>> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            io.load_hnsw::<f32, D>().map(|h: Hnsw<'_, f32, D>| -> Hnsw<'static, f32, D> {
                unsafe { std::mem::transmute(h) }
            })
        }));
    match load_result {
        Ok(Ok(h)) => Ok(h),
        Ok(Err(e)) => Err(HnswError::IndexCorrupted(e.to_string())),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("panic in hnsw_rs load_hnsw");
            Err(HnswError::IndexCorrupted(msg.to_string()))
        }
    }
}

fn fossic_now_us() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}
