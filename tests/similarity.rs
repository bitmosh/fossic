use fossic::{
    Error, EventId, OpenOptions, SimilarityHit, SimilarityQuery, SimilaritySearchProvider, Store,
};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

struct MockSimilarityProvider {
    hits: Mutex<Vec<SimilarityHit>>,
}

impl SimilaritySearchProvider for MockSimilarityProvider {
    fn index(&self, _event_id: EventId, _embedding: &[f32]) -> Result<(), Error> {
        Ok(())
    }
    fn query(&self, _q: SimilarityQuery) -> Result<Vec<SimilarityHit>, Error> {
        Ok(self.hits.lock().unwrap().drain(..).collect())
    }
}

fn open_with_provider(provider: Arc<dyn SimilaritySearchProvider>) -> (Store, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.fossic");
    let opts = OpenOptions {
        similarity_provider: Some(provider),
        ..OpenOptions::default()
    };
    let store = Store::open(&path, opts).unwrap();
    (store, dir)
}

#[test]
fn no_provider_returns_not_implemented() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.fossic");
    let store = Store::open(&path, OpenOptions::default()).unwrap();

    let result = store.similarity_query(SimilarityQuery {
        embedding: vec![0.1, 0.2],
        k: 5,
        stream_pattern: None,
    });
    assert!(matches!(result, Err(Error::NotImplemented { .. })));
}

#[test]
fn provider_query_is_called() {
    let provider = Arc::new(MockSimilarityProvider {
        hits: Mutex::new(vec![SimilarityHit {
            event_id: EventId::from_bytes([1u8; 32]),
            score: 0.95,
        }]),
    });
    let (store, _dir) = open_with_provider(provider);

    let hits = store
        .similarity_query(SimilarityQuery {
            embedding: vec![0.1, 0.2],
            k: 1,
            stream_pattern: None,
        })
        .unwrap();

    assert_eq!(hits.len(), 1);
    assert!((hits[0].score - 0.95).abs() < 1e-6);
}

#[test]
fn empty_result_ok() {
    let provider = Arc::new(MockSimilarityProvider {
        hits: Mutex::new(vec![]),
    });
    let (store, _dir) = open_with_provider(provider);

    let hits = store
        .similarity_query(SimilarityQuery {
            embedding: vec![],
            k: 10,
            stream_pattern: None,
        })
        .unwrap();

    assert!(hits.is_empty());
}
