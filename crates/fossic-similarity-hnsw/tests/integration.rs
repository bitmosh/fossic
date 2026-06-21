use fossic::{EventId, SimilarityQuery, SimilaritySearchProvider};
use fossic_similarity_hnsw::{HnswConfig, HnswProvider};
use std::sync::Arc;
use tempfile::TempDir;

fn make_provider(dims: usize) -> (Arc<HnswProvider>, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let config = HnswConfig::default().with_dimensions(dims);
    let provider = Arc::new(HnswProvider::new(&db_path, config).unwrap());
    (provider, dir)
}

fn event_id(n: u8) -> EventId {
    let mut bytes = [0u8; 32];
    bytes[0] = n;
    EventId::from_bytes(bytes)
}

fn random_unit_vec(dims: usize, seed: u32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..dims)
        .map(|i| {
            // deterministic pseudo-random via LCG
            let x = seed.wrapping_mul(1664525).wrapping_add(1013904223).wrapping_add(i as u32);
            (x as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect();
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    v.iter_mut().for_each(|x| *x /= norm);
    v
}

// ── Basic operation ───────────────────────────────────────────────────────────

#[test]
fn empty_index_query_returns_empty() {
    let (p, _dir) = make_provider(4);
    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        k: 5,
        stream_pattern: None,
    };
    let hits = p.query(q).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn index_and_query_roundtrip() {
    let (p, _dir) = make_provider(4);
    let eid = event_id(1);
    p.index(eid, &[1.0, 0.0, 0.0, 0.0]).unwrap();
    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        k: 1,
        stream_pattern: None,
    };
    let hits = p.query(q).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].event_id, eid);
}

#[test]
fn len_increments_with_inserts() {
    let (p, _dir) = make_provider(4);
    assert_eq!(p.len(), 0);
    assert!(p.is_empty());
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    assert_eq!(p.len(), 1);
    p.index(event_id(2), &[0.0, 1.0, 0.0, 0.0]).unwrap();
    assert_eq!(p.len(), 2);
}

// ── Dimension validation ──────────────────────────────────────────────────────

#[test]
fn index_wrong_dims_returns_error() {
    let (p, _dir) = make_provider(4);
    let result = p.index(event_id(1), &[1.0, 0.0]); // 2 instead of 4
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("InvalidDimensions") || msg.contains("dimensions") || msg.contains("2"),
        "unexpected error message: {}", msg);
}

#[test]
fn query_wrong_dims_returns_error() {
    let (p, _dir) = make_provider(4);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0], // 2 instead of 4
        k: 1,
        stream_pattern: None,
    };
    let result = p.query(q);
    assert!(result.is_err());
}

#[test]
fn zero_k_returns_empty() {
    let (p, _dir) = make_provider(4);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        k: 0,
        stream_pattern: None,
    };
    let hits = p.query(q).unwrap();
    assert!(hits.is_empty());
}

// ── Stream-pattern filtering ──────────────────────────────────────────────────

/// 100 vectors across 5 streams; query with a stream_pattern returns only
/// events from the matching stream.
#[test]
fn stream_pattern_filters_correctly() {
    const DIMS: usize = 16;
    const PER_STREAM: usize = 20;
    let streams = ["alpha", "beta", "gamma", "delta", "epsilon"];

    let (p, _dir) = make_provider(DIMS);

    // index 20 vectors per stream
    let mut n: u8 = 0;
    for (si, stream) in streams.iter().enumerate() {
        for vi in 0..PER_STREAM {
            let eid = event_id(n);
            let seed = (si * 100 + vi) as u32 + 1;
            let emb = random_unit_vec(DIMS, seed);
            p.index_with_stream_id(eid, stream, &emb).unwrap();
            n += 1;
        }
    }

    assert_eq!(p.len(), 100);

    // query targeting "alpha" only
    let query_emb = random_unit_vec(DIMS, 999);
    let q = SimilarityQuery {
        embedding: query_emb,
        k: 10,
        stream_pattern: Some("alpha".to_string()),
    };
    let hits = p.query(q).unwrap();
    assert!(!hits.is_empty(), "expected some results from alpha stream");
    assert!(hits.len() <= 10);

    // all returned events must belong to alpha
    let alpha_ids: std::collections::HashSet<EventId> = (0u8..20)
        .map(event_id)
        .collect();
    for hit in &hits {
        assert!(alpha_ids.contains(&hit.event_id),
            "hit {:?} does not belong to alpha stream", hit.event_id.to_hex());
    }
}

/// Glob pattern — wildcard matches multiple streams.
#[test]
fn stream_pattern_glob_matches_multiple_streams() {
    const DIMS: usize = 8;
    let (p, _dir) = make_provider(DIMS);

    // two streams: "events/user" and "events/system"
    p.index_with_stream_id(event_id(1), "events/user",   &random_unit_vec(DIMS, 1)).unwrap();
    p.index_with_stream_id(event_id(2), "events/system", &random_unit_vec(DIMS, 2)).unwrap();
    p.index_with_stream_id(event_id(3), "metrics/host",  &random_unit_vec(DIMS, 3)).unwrap();

    let q = SimilarityQuery {
        embedding: random_unit_vec(DIMS, 42),
        k: 5,
        stream_pattern: Some("events/*".to_string()),
    };
    let hits = p.query(q).unwrap();
    assert!(!hits.is_empty());
    // must not contain event_id(3) (metrics/host)
    for hit in &hits {
        assert_ne!(hit.event_id, event_id(3), "metrics/host slipped through events/* filter");
    }
}

/// When fudge_factor × k candidates are all outside the stream, result is empty.
#[test]
fn stream_filter_excludes_all_returns_empty() {
    const DIMS: usize = 4;
    let (p, _dir) = make_provider(DIMS);

    // index 2 events on "other" stream, query for "target" — no results expected
    p.index_with_stream_id(event_id(1), "other", &[1.0, 0.0, 0.0, 0.0]).unwrap();
    p.index_with_stream_id(event_id(2), "other", &[0.0, 1.0, 0.0, 0.0]).unwrap();

    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        k: 2,
        stream_pattern: Some("target".to_string()),
    };
    let hits = p.query(q).unwrap();
    assert!(hits.is_empty(), "expected empty result when no events match stream_pattern");
}

/// Events indexed via the trait `index` method (no stream_id) are excluded
/// from stream-pattern filtered queries (CP-D2-2).
#[test]
fn trait_indexed_events_excluded_from_stream_filter() {
    const DIMS: usize = 4;
    let (p, _dir) = make_provider(DIMS);

    // two events: one via trait (no stream_id), one via index_with_stream_id
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap(); // no stream_id
    p.index_with_stream_id(event_id(2), "stream/a", &[0.9, 0.1, 0.0, 0.0]).unwrap();

    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        k: 2,
        stream_pattern: Some("stream/*".to_string()),
    };
    let hits = p.query(q).unwrap();
    // event_id(1) has no stream_id and must be excluded
    for hit in &hits {
        assert_ne!(hit.event_id, event_id(1),
            "trait-indexed event (no stream_id) should be excluded from stream-filtered query");
    }
    // event_id(2) should appear
    assert!(hits.iter().any(|h| h.event_id == event_id(2)),
        "stream-registered event should appear in filtered result");
}

// ── Inherent methods ──────────────────────────────────────────────────────────

#[test]
fn remove_returns_unsupported_error() {
    let (p, _dir) = make_provider(4);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    let result = p.remove(event_id(1));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("not supported") || msg.contains("deletion"),
        "unexpected error: {}", msg);
}
