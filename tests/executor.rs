use fossic::{Append, OpenOptions, Store};
use tempfile::NamedTempFile;

fn tmp_store(opts: OpenOptions) -> Store {
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_path_buf();
    std::mem::forget(file);
    Store::open(path, opts).unwrap()
}

fn ping(i: u32) -> Append {
    Append {
        stream_id: "executor/test".into(),
        event_type: "Ping".into(),
        payload: serde_json::json!({ "i": i }),
        ..Append::default()
    }
}

/// Store opens and drops without hanging.
#[test]
fn executor_lifecycle_no_hang() {
    let store = tmp_store(OpenOptions::default());
    store.declare_stream("executor/test", "test", None).unwrap();

    for i in 0..5u32 {
        store.append(ping(i)).unwrap();
    }

    let start = std::time::Instant::now();
    drop(store);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "store drop took {:?} — executor did not stop promptly",
        elapsed,
    );
}

/// With a short grace timeout, drop still returns (no hang).
#[test]
fn executor_short_grace_closes_within_timeout() {
    let store = tmp_store(OpenOptions {
        background_executor_grace_timeout_ms: 2_000,
        ..OpenOptions::default()
    });

    store.declare_stream("executor/grace", "test", None).unwrap();
    store
        .append(Append {
            stream_id: "executor/grace".into(),
            event_type: "Probe".into(),
            payload: serde_json::json!({}),
            ..Append::default()
        })
        .unwrap();

    let start = std::time::Instant::now();
    drop(store);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 4,
        "store drop with 2s grace took {:?}",
        elapsed,
    );
}
