use crate::{error::Error, types::EventId};

/// A similarity search backend. Implementations index events and answer k-NN queries.
///
/// v1 note: No implementation ships with fossic v1; this trait is a stub.
/// Wire a custom provider via `OpenOptions::similarity_provider`.
pub trait SimilaritySearchProvider: Send + Sync + 'static {
    /// Index a newly appended event.
    fn index(&self, event_id: EventId, embedding: &[f32]) -> Result<(), Error>;
    /// Run a k-nearest-neighbour query.
    fn query(&self, q: SimilarityQuery) -> Result<Vec<SimilarityHit>, Error>;
}

/// Parameters for a k-NN similarity search.
pub struct SimilarityQuery {
    pub embedding: Vec<f32>,
    pub k: usize,
    pub stream_pattern: Option<String>,
}

/// A single hit returned by a similarity query.
pub struct SimilarityHit {
    pub event_id: EventId,
    pub score: f32,
}
