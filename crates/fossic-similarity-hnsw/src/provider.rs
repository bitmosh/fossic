use std::{path::PathBuf, sync::Arc};

use fossic::{Error, EventId, SimilarityHit, SimilarityQuery, SimilaritySearchProvider, SystemStreamWriter};
use parking_lot::Mutex;

use crate::{config::HnswConfig, error::HnswError};

// ── HnswInner ─────────────────────────────────────────────────────────────────

/// Mutable core — wrapped in `Mutex<Option<HnswInner>>`.
/// `Option` is `None` until the index is first built or loaded (v1.7.1).
pub(crate) struct HnswInner {
    // Populated in v1.7.1 when hnsw_rs integration lands.
    // Placeholder to establish the struct shape.
    _placeholder: (),
}

// ── HnswProvider ─────────────────────────────────────────────────────────────

/// HNSW-backed implementation of `fossic::SimilaritySearchProvider`.
///
/// Construct with [`HnswProvider::new`], then pass to `OpenOptions::similarity_provider`.
///
/// ```rust,ignore
/// use fossic::{OpenOptions, Store};
/// use fossic_similarity_hnsw::{HnswConfig, HnswProvider};
///
/// let config = HnswConfig { dimensions: 1024, ..HnswConfig::default() };
/// let provider = HnswProvider::new("/path/to/store.db", config)?;
/// let store = Store::open("/path/to/store.db", OpenOptions {
///     similarity_provider: Some(std::sync::Arc::new(provider)),
///     ..Default::default()
/// })?;
/// ```
pub struct HnswProvider {
    pub(crate) config: HnswConfig,
    /// `<store_db_path>/../hnsw/` directory.
    pub(crate) index_dir: PathBuf,
    pub(crate) inner: Mutex<Option<HnswInner>>,
    /// Lazy-initialized on first system-event emission.
    /// Separate connection from any store connection — never contends.
    pub(crate) system_writer: Mutex<Option<SystemStreamWriter>>,
}

impl HnswProvider {
    /// Create a new provider.
    ///
    /// `store_db_path` is the path to the fossic `store.db` file.
    /// The HNSW index directory is created at `<parent_of_store_db>/hnsw/`
    /// if it does not already exist.
    ///
    /// No index is built yet — that happens in [`Self::open`] (v1.7.2).
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

    pub(crate) fn index_bin_path(&self) -> PathBuf {
        self.index_dir.join("index.bin")
    }

    pub(crate) fn mappings_bin_path(&self) -> PathBuf {
        self.index_dir.join("mappings.bin")
    }

    /// Lazy-init the SystemStreamWriter. No-op if already open.
    pub(crate) fn ensure_system_writer(&self) {
        let mut guard = self.system_writer.lock();
        if guard.is_none() {
            // Construct writer from the db path (parent of index_dir).
            if let Some(db_dir) = self.index_dir.parent() {
                let db_path = db_dir.join("store.db");
                *guard = SystemStreamWriter::new(&db_path);
            }
        }
    }

    fn emit_system_event(&self, event_type: &str, payload: &serde_json::Value) {
        self.ensure_system_writer();
        let indexed_tags = serde_json::json!({ "event_class": "hnsw" });
        if let Some(ref mut w) = *self.system_writer.lock() {
            w.emit(event_type, payload, Some(&indexed_tags));
        }
    }

    /// Number of indexed vectors. Returns 0 if the index is not yet built.
    pub fn len(&self) -> usize {
        // Populated in v1.7.1.
        let guard = self.inner.lock();
        if guard.is_none() { 0 } else { 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── SimilaritySearchProvider impl ────────────────────────────────────────────

impl SimilaritySearchProvider for HnswProvider {
    fn index(&self, _event_id: EventId, _embedding: &[f32]) -> Result<(), Error> {
        // Full implementation in v1.7.1.
        // For now: validate dimensions if inner is initialized, otherwise no-op.
        let expected = self.config.dimensions;
        if _embedding.len() != expected {
            return Err(HnswError::InvalidDimensions {
                expected,
                got: _embedding.len(),
            }
            .into());
        }
        Ok(())
    }

    fn query(&self, q: SimilarityQuery) -> Result<Vec<SimilarityHit>, Error> {
        // Full implementation in v1.7.1.
        // For now: validate dimensions and return empty (index not yet built).
        let expected = self.config.dimensions;
        if q.embedding.len() != expected {
            return Err(HnswError::InvalidDimensions {
                expected,
                got: q.embedding.len(),
            }
            .into());
        }
        if q.k == 0 {
            return Ok(vec![]);
        }
        Ok(vec![])
    }
}

