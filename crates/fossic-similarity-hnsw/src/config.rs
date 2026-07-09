// SPDX-License-Identifier: Apache-2.0
/// Distance metric used to compare embeddings in the HNSW index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    InnerProduct,
}

/// Configuration for an `HnswProvider` instance.
///
/// All fields have sensible defaults except `dimensions`, which must be set
/// explicitly to match the embedding model's output size.
///
/// # Example
/// ```rust,ignore
/// use fossic_similarity_hnsw::HnswConfig;
/// // mxbai-embed-large produces 1024-dim embeddings
/// let config = HnswConfig { dimensions: 1024, ..HnswConfig::default() };
/// ```
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// Maximum number of vectors the index can hold. This is a capacity hint;
    /// hnsw_rs will resize if exceeded, but pre-allocating avoids reallocs.
    /// Default: 100_000.
    pub max_elements: usize,
    /// Dimensionality of each embedding vector. **Required — no default.**
    /// Must match the embedding model output exactly; mismatches return `InvalidDimensions`.
    pub dimensions: usize,
    /// Number of candidate neighbors examined during index construction.
    /// Higher values improve recall at the cost of index build time.
    /// Default: 200.
    pub ef_construction: usize,
    /// HNSW graph degree — neighbours stored per node at each layer.
    /// Values 8–64; default 16 is a good trade-off for dense corpora.
    pub m: usize,
    /// Number of candidates examined during search.
    /// Higher values improve recall at the cost of query latency.
    /// Default: 50.
    pub ef_search: usize,
    /// Distance metric. Default: `Cosine` (appropriate for normalized embeddings).
    pub distance: DistanceMetric,
    /// Multiplier applied to `k` before querying the HNSW index when
    /// `SimilarityQuery::stream_pattern` is set. The raw candidate set is
    /// expanded by this factor before stream-pattern filtering, then truncated
    /// to the requested `k`. A larger fudge factor improves recall when the
    /// filtered stream represents a small fraction of the index.
    /// Default: 2.
    pub stream_filter_fudge_factor: usize,
}

impl Default for HnswConfig {
    fn default() -> Self {
        HnswConfig {
            max_elements: 100_000,
            dimensions: 0,
            ef_construction: 200,
            m: 16,
            ef_search: 50,
            distance: DistanceMetric::Cosine,
            stream_filter_fudge_factor: 2,
        }
    }
}

impl HnswConfig {
    /// Set the embedding dimensionality. Required before constructing a provider.
    pub fn with_dimensions(mut self, dims: usize) -> Self {
        self.dimensions = dims;
        self
    }
}
