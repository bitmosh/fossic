use fossic::{
    BacklogTask, EventId, OpenOptions, SimilarityQuery, SimilaritySearchProvider, Store, TaskKind,
    TaskPriority,
};
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

fn make_provider_in(dir: &TempDir, dims: usize) -> Arc<HnswProvider> {
    let db_path = dir.path().join("store.db");
    let config = HnswConfig::default().with_dimensions(dims);
    Arc::new(HnswProvider::new(&db_path, config).unwrap())
}

/// Creates a provider + Store sharing the same db_path. The Store uses a 50ms
/// quiescence window so tests can drain the executor within ~700ms.
fn make_store_and_provider(dims: usize) -> (Store, Arc<HnswProvider>, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("store.db");
    let config = HnswConfig::default().with_dimensions(dims);
    let provider = Arc::new(HnswProvider::new(&db_path, config).unwrap());
    let store = Store::open(
        &db_path,
        OpenOptions {
            executor_quiescence_window_ms: 50,
            background_executor_grace_timeout_ms: 1_000,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    (store, provider, dir)
}

fn event_id(n: u8) -> EventId {
    let mut bytes = [0u8; 32];
    bytes[0] = n;
    EventId::from_bytes(bytes)
}

fn event_id_u16(n: u16) -> EventId {
    let mut bytes = [0u8; 32];
    let be = n.to_be_bytes();
    bytes[0] = be[0];
    bytes[1] = be[1];
    EventId::from_bytes(bytes)
}

fn random_unit_vec(dims: usize, seed: u32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..dims)
        .map(|i| {
            let x = seed
                .wrapping_mul(1664525)
                .wrapping_add(1013904223)
                .wrapping_add(i as u32);
            (x as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect();
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    v.iter_mut().for_each(|x| *x /= norm);
    v
}

// ── Basic operation (carried over from v1.7.1 — must still pass) ──────────────

#[test]
fn empty_index_query_returns_empty() {
    let (p, _dir) = make_provider(4);
    let q = SimilarityQuery {
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        k: 5,
        stream_pattern: None,
    };
    assert!(p.query(q).unwrap().is_empty());
}

#[test]
fn index_and_query_roundtrip() {
    let (p, _dir) = make_provider(4);
    let eid = event_id(1);
    p.index(eid, &[1.0, 0.0, 0.0, 0.0]).unwrap();
    let hits = p
        .query(SimilarityQuery {
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            k: 1,
            stream_pattern: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].event_id, eid);
}

#[test]
fn index_wrong_dims_returns_error() {
    let (p, _dir) = make_provider(4);
    assert!(p.index(event_id(1), &[1.0, 0.0]).is_err());
}

#[test]
fn query_wrong_dims_returns_error() {
    let (p, _dir) = make_provider(4);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    assert!(p
        .query(SimilarityQuery {
            embedding: vec![1.0, 0.0],
            k: 1,
            stream_pattern: None,
        })
        .is_err());
}

#[test]
fn zero_k_returns_empty() {
    let (p, _dir) = make_provider(4);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    assert!(p
        .query(SimilarityQuery {
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            k: 0,
            stream_pattern: None,
        })
        .unwrap()
        .is_empty());
}

#[test]
fn remove_returns_unsupported_error() {
    let (p, _dir) = make_provider(4);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    assert!(p.remove(event_id(1)).is_err());
}

// ── Persistence: round-trip ───────────────────────────────────────────────────

/// 1000 vectors across 5 streams; save; reload fresh provider; query returns
/// the same top-k and stream filtering still works correctly.
#[test]
fn persistence_round_trip_with_stream_filter() {
    const DIMS: usize = 16;
    const STREAMS: &[&str] = &["alpha", "beta", "gamma", "delta", "epsilon"];
    const PER_STREAM: usize = 200;

    let dir = tempfile::tempdir().unwrap();

    // Build and save.
    {
        let p = make_provider_in(&dir, DIMS);
        let mut n: u16 = 0;
        for (si, stream) in STREAMS.iter().enumerate() {
            for vi in 0..PER_STREAM {
                let seed = (si * 100 + vi) as u32 + 1;
                p.index_with_stream_id(event_id_u16(n), stream, &random_unit_vec(DIMS, seed))
                    .unwrap();
                n += 1;
            }
        }
        assert_eq!(p.len(), 1000);
        p.save_to_disk().unwrap();
    }

    // Reload and query.
    let p2 = make_provider_in(&dir, DIMS);
    assert_eq!(p2.len(), 1000);

    // Unfiltered query should return k results.
    let q_emb = random_unit_vec(DIMS, 9999);
    let hits = p2
        .query(SimilarityQuery {
            embedding: q_emb.clone(),
            k: 10,
            stream_pattern: None,
        })
        .unwrap();
    assert_eq!(hits.len(), 10);

    // Stream-filtered query: results must all belong to "alpha" (IDs 0..200, stream index 0).
    let alpha_ids: std::collections::HashSet<EventId> = (0u16..200).map(event_id_u16).collect();
    let filtered = p2
        .query(SimilarityQuery {
            embedding: q_emb,
            k: 5,
            stream_pattern: Some("alpha".to_string()),
        })
        .unwrap();
    assert!(!filtered.is_empty());
    for hit in &filtered {
        assert!(
            alpha_ids.contains(&hit.event_id),
            "hit belongs to wrong stream after reload"
        );
    }
}

// ── Persistence: empty index ──────────────────────────────────────────────────

#[test]
fn save_and_load_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    {
        let p = make_provider_in(&dir, 4);
        assert_eq!(p.len(), 0);
        p.save_to_disk().unwrap();
    }
    let p2 = make_provider_in(&dir, 4);
    assert_eq!(p2.len(), 0);
    // Queries still work on empty reloaded index.
    let hits = p2
        .query(SimilarityQuery {
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            k: 5,
            stream_pattern: None,
        })
        .unwrap();
    assert!(hits.is_empty());
}

// ── Persistence: corrupt index files ─────────────────────────────────────────

/// Truncate index.hnsw.data to 100 bytes — load should fail with IndexCorrupted,
/// but the provider recovers to an empty, operational index.
#[test]
fn corrupt_index_data_file_recovers_to_empty() {
    const DIMS: usize = 8;
    let dir = tempfile::tempdir().unwrap();

    // Build and save a real index.
    {
        let p = make_provider_in(&dir, DIMS);
        for i in 0u8..10 {
            p.index(event_id(i), &random_unit_vec(DIMS, i as u32 + 1))
                .unwrap();
        }
        p.save_to_disk().unwrap();
    }

    // Truncate the data file.
    let data_path = dir.path().join("hnsw/index.hnsw.data");
    std::fs::write(&data_path, &[0u8; 100]).unwrap();

    // Reload — must not panic, must be operational.
    let p2 = make_provider_in(&dir, DIMS);
    assert_eq!(p2.len(), 0, "corrupted index should recover to empty");

    // Can still index and query after recovery.
    p2.index(event_id(99), &random_unit_vec(DIMS, 999)).unwrap();
    assert_eq!(p2.len(), 1);
}

/// Replace mappings.bin version byte 0x01 with 0xFF — MappingsVersionMismatch
/// is treated as corruption; provider recovers to empty.
#[test]
fn corrupt_mappings_version_byte_recovers_to_empty() {
    const DIMS: usize = 8;
    let dir = tempfile::tempdir().unwrap();

    {
        let p = make_provider_in(&dir, DIMS);
        for i in 0u8..5 {
            p.index(event_id(i), &random_unit_vec(DIMS, i as u32 + 1))
                .unwrap();
        }
        p.save_to_disk().unwrap();
    }

    // Overwrite version byte.
    let mappings_path = dir.path().join("hnsw/mappings.bin");
    let mut data = std::fs::read(&mappings_path).unwrap();
    data[0] = 0xFF;
    std::fs::write(&mappings_path, &data).unwrap();

    let p2 = make_provider_in(&dir, DIMS);
    assert_eq!(
        p2.len(),
        0,
        "invalid mappings version should recover to empty"
    );
}

// ── Persistence: partial-save cleanup ────────────────────────────────────────

/// Simulate a mid-save failure where the graph files are written but
/// mappings.bin cannot be created.
///
/// We pre-create `hnsw/mappings.bin` as a **directory** so that
/// `File::create(mappings_bin_path)` fails with "Is a directory" while
/// `file_dump` (which writes `index.hnsw.data` / `index.hnsw.graph` into
/// the hnsw dir) still succeeds. After the failure, both graph files must
/// be cleaned up — no partial save left on disk.
#[test]
fn partial_save_cleans_up_all_files() {
    const DIMS: usize = 4;
    let dir = tempfile::tempdir().unwrap();
    let hnsw_dir = dir.path().join("hnsw");

    // Pre-create hnsw/ dir with mappings.bin as a directory.
    // index_files_exist() returns false (graph files absent), so the provider
    // starts with an empty index and the directory is left in place.
    std::fs::create_dir_all(&hnsw_dir).unwrap();
    std::fs::create_dir(hnsw_dir.join("mappings.bin")).unwrap();

    let p = make_provider_in(&dir, DIMS);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();

    // save_to_disk: graph dump succeeds, then File::create(mappings.bin) fails.
    let result = p.save_to_disk();
    assert!(
        result.is_err(),
        "save should fail when mappings.bin cannot be created"
    );

    // Graph files must be cleaned up.
    assert!(
        !hnsw_dir.join("index.hnsw.data").exists(),
        "index.hnsw.data should be removed after partial save failure"
    );
    assert!(
        !hnsw_dir.join("index.hnsw.graph").exists(),
        "index.hnsw.graph should be removed after partial save failure"
    );
}

// ── Stream-pattern filtering (v1.7.1 regression) ─────────────────────────────

#[test]
fn stream_pattern_glob_matches_multiple_streams() {
    const DIMS: usize = 8;
    let (p, _dir) = make_provider(DIMS);
    p.index_with_stream_id(event_id(1), "events/user", &random_unit_vec(DIMS, 1))
        .unwrap();
    p.index_with_stream_id(event_id(2), "events/system", &random_unit_vec(DIMS, 2))
        .unwrap();
    p.index_with_stream_id(event_id(3), "metrics/host", &random_unit_vec(DIMS, 3))
        .unwrap();
    let hits = p
        .query(SimilarityQuery {
            embedding: random_unit_vec(DIMS, 42),
            k: 5,
            stream_pattern: Some("events/*".to_string()),
        })
        .unwrap();
    assert!(!hits.is_empty());
    for hit in &hits {
        assert_ne!(hit.event_id, event_id(3));
    }
}

#[test]
fn stream_filter_excludes_all_returns_empty() {
    const DIMS: usize = 4;
    let (p, _dir) = make_provider(DIMS);
    p.index_with_stream_id(event_id(1), "other", &[1.0, 0.0, 0.0, 0.0])
        .unwrap();
    p.index_with_stream_id(event_id(2), "other", &[0.0, 1.0, 0.0, 0.0])
        .unwrap();
    let hits = p
        .query(SimilarityQuery {
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            k: 2,
            stream_pattern: Some("target".to_string()),
        })
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn trait_indexed_events_excluded_from_stream_filter() {
    const DIMS: usize = 4;
    let (p, _dir) = make_provider(DIMS);
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    p.index_with_stream_id(event_id(2), "stream/a", &[0.9, 0.1, 0.0, 0.0])
        .unwrap();
    let hits = p
        .query(SimilarityQuery {
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            k: 2,
            stream_pattern: Some("stream/*".to_string()),
        })
        .unwrap();
    for hit in &hits {
        assert_ne!(hit.event_id, event_id(1));
    }
    assert!(hits.iter().any(|h| h.event_id == event_id(2)));
}

// ── Background scheduling (v1.7.3) ───────────────────────────────────────────

/// Dirty index + schedule_save → task executes in the next quiescent window,
/// files appear on disk, dirty cleared.
#[test]
fn schedule_save_fires_when_dirty() {
    const DIMS: usize = 4;
    let (store, p, dir) = make_store_and_provider(DIMS);
    let hnsw_dir = dir.path().join("hnsw");

    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    assert!(p.is_dirty());

    HnswProvider::schedule_save(Arc::clone(&p), &store, TaskPriority::Low);
    assert!(p.is_save_pending());

    // Give the bg thread one full poll tick (500ms) + quiescence window (50ms) + margin.
    std::thread::sleep(std::time::Duration::from_millis(700));

    assert!(!p.is_dirty(), "dirty should be cleared after save");
    assert!(
        !p.is_save_pending(),
        "save_pending should be cleared after closure runs"
    );
    assert!(
        hnsw_dir.join("index.hnsw.data").exists(),
        "graph data file must exist after save"
    );
    assert!(
        hnsw_dir.join("index.hnsw.graph").exists(),
        "graph file must exist after save"
    );
    assert!(
        hnsw_dir.join("mappings.bin").exists(),
        "mappings file must exist after save"
    );
    drop(store);
}

/// schedule_save is a no-op when dirty=false — no task is queued.
#[test]
fn schedule_save_noop_when_not_dirty() {
    const DIMS: usize = 4;
    let (store, p, _dir) = make_store_and_provider(DIMS);

    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    p.save_to_disk().unwrap(); // clears dirty

    assert!(!p.is_dirty());
    HnswProvider::schedule_save(Arc::clone(&p), &store, TaskPriority::Low);
    // save_pending must remain false — no task was queued
    assert!(
        !p.is_save_pending(),
        "schedule_save must be a no-op when dirty=false"
    );
    drop(store);
}

/// 1000 index+schedule_save calls in a hot loop: only ONE task is queued
/// (storm-scheduling prevention via optimistic save_pending stamp).
#[test]
fn schedule_save_storm_prevention() {
    const DIMS: usize = 4;
    let (store, p, dir) = make_store_and_provider(DIMS);
    let hnsw_dir = dir.path().join("hnsw");

    // Hot loop: index + schedule_save 1000 times.
    for i in 0u8..=255 {
        p.index(event_id(i), &[i as f32 / 255.0, 0.0, 0.0, 0.0])
            .unwrap();
        HnswProvider::schedule_save(Arc::clone(&p), &store, TaskPriority::Low);
    }
    // Only the first schedule_save queues a task; remaining 255 are no-ops.
    assert!(
        p.is_save_pending(),
        "save_pending must be true after first schedule"
    );

    // Wait for one task execution (700ms is one bg-thread tick).
    std::thread::sleep(std::time::Duration::from_millis(700));

    assert!(!p.is_dirty(), "save must have completed");
    assert!(!p.is_save_pending());
    assert!(hnsw_dir.join("index.hnsw.data").exists());
    drop(store);
}

/// A Low-priority save yields to a Normal-priority custom task when both are
/// queued: Normal runs in tick 1, Low runs in tick 2.
#[test]
fn schedule_save_low_priority_yields_to_normal() {
    use std::sync::atomic::{AtomicU32, Ordering as AO};
    const DIMS: usize = 4;
    let (store, p, dir) = make_store_and_provider(DIMS);
    let hnsw_dir = dir.path().join("hnsw");

    // Use a sequence counter to record execution order.
    let seq = Arc::new(AtomicU32::new(0));
    let normal_seq = Arc::new(AtomicU32::new(0));
    let seq_clone = Arc::clone(&seq);
    let normal_seq_clone = Arc::clone(&normal_seq);

    // Schedule Low-priority save first.
    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    HnswProvider::schedule_save(Arc::clone(&p), &store, TaskPriority::Low);

    // Schedule Normal-priority task second.
    store.schedule_task(BacklogTask {
        priority: TaskPriority::Normal,
        deadline_us: 0, // already past — runs immediately when quiescent
        persist_on_drop: false,
        kind: TaskKind::Custom(Arc::new(move || {
            normal_seq_clone.store(seq_clone.fetch_add(1, AO::SeqCst) + 1, AO::SeqCst);
        })),
        recurring_interval: None,
    });

    // After 1 tick: Normal should have run (higher priority), Low should not.
    std::thread::sleep(std::time::Duration::from_millis(700));
    let n_seq = normal_seq.load(AO::SeqCst);
    assert!(n_seq > 0, "Normal task must have run within the first tick");
    // dirty=true means Low (save) hasn't run yet
    assert!(
        p.is_dirty(),
        "Low-priority save should not have run yet after tick 1"
    );

    // After 2 ticks: Low (save) should have run.
    std::thread::sleep(std::time::Duration::from_millis(700));
    assert!(!p.is_dirty(), "Low-priority save must complete by tick 2");
    assert!(hnsw_dir.join("index.hnsw.data").exists());
    drop(store);
}

/// Drop the last Arc<HnswProvider> after schedule_save — the Weak in the
/// closure fails to upgrade. No panic, no error, no files written.
///
/// This documents the "in-memory state is lost" semantics for callers who
/// drop the provider before the quiescent window. The save closure is a no-op.
#[test]
fn schedule_save_drop_provider_before_quiescence_noop() {
    const DIMS: usize = 4;
    let (store, p, dir) = make_store_and_provider(DIMS);
    let hnsw_dir = dir.path().join("hnsw");

    p.index(event_id(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();
    assert!(p.is_dirty());

    // schedule_save captures Weak<HnswProvider>; the Arc<> passed in is the
    // only strong reference (strong_count drops to 1 after schedule_save returns).
    HnswProvider::schedule_save(Arc::clone(&p), &store, TaskPriority::Low);

    // Drop the last strong reference.
    drop(p);
    // Now strong_count = 0; provider is dropped. Weak::upgrade will return None.

    // Let the bg thread run the closure.
    std::thread::sleep(std::time::Duration::from_millis(700));

    // No panic, no files written (closure was a no-op due to failed upgrade).
    assert!(
        !hnsw_dir.join("index.hnsw.data").exists(),
        "no graph files after dropped-provider save"
    );
    assert!(!hnsw_dir.join("index.hnsw.graph").exists());
    drop(store);
}
