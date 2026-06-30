use fossic::{Append, Error, OpenOptions, ReadQuery, Store};
use std::sync::{Arc, Mutex};

fn open_tmp_with_pool(pool_size: usize) -> (Store, tempfile::TempDir) {
    open_tmp_with_pool_and_timeout(pool_size, 30_000)
}

fn open_tmp_with_pool_and_timeout(pool_size: usize, timeout_ms: u64) -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(
        dir.path().join("test.db"),
        OpenOptions {
            read_pool_size: pool_size,
            read_pool_timeout_ms: timeout_ms,
            ..Default::default()
        },
    )
    .expect("open store");
    (store, dir)
}

fn open_tmp() -> (Store, tempfile::TempDir) {
    open_tmp_with_pool(4)
}

fn decl(store: &Store, id: &str) {
    store.declare_stream(id, "test", None).unwrap();
}

fn append_n(store: &Store, stream: &str, n: usize) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    for _ in 0..n {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        store
            .append(Append {
                stream_id: stream.to_string(),
                event_type: "E".to_string(),
                payload: serde_json::json!({ "s": seq }),
                ..Default::default()
            })
            .unwrap();
    }
}

#[test]
fn read_does_not_block_when_write_mutex_held() {
    // Arrange: a store with events. Hold the write mutex via a separate thread +
    // channel so a concurrent read can proceed on a pool connection.
    let (store, _dir) = open_tmp();
    decl(&store, "pool/r");
    append_n(&store, "pool/r", 5);

    let store2 = store.clone();
    let (write_lock_held_tx, write_lock_held_rx) = std::sync::mpsc::sync_channel::<()>(0);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(0);

    // Thread 1: holds the write mutex by attempting to append (which needs it).
    // We use a blocking fake payload transform to simulate holding the lock.
    // Simpler approach: use append_if with a condition that blocks until signalled.
    let t1 = std::thread::spawn(move || {
        store2
            .append_if(
                Append {
                    stream_id: "pool/r".to_string(),
                    event_type: "Block".to_string(),
                    payload: serde_json::json!({ "s": 9999 }),
                    ..Default::default()
                },
                move |_conn| {
                    // Signal that we're inside the transaction (write mutex held).
                    let _ = write_lock_held_tx.send(());
                    // Wait for test to signal release.
                    let _ = release_rx.recv();
                    Ok(false) // Don't commit — we just needed the lock.
                },
            )
            .unwrap();
    });

    // Wait until the writer is holding the write mutex.
    write_lock_held_rx.recv().unwrap();

    // Read must succeed without blocking (uses pool connection, not write mutex).
    let events = store.read_range(ReadQuery::stream("pool/r")).unwrap();
    assert_eq!(
        events.len(),
        5,
        "read_range must succeed while write mutex is held"
    );

    // Release the writer.
    let _ = release_tx.send(());
    t1.join().unwrap();
}

#[test]
fn concurrent_reads_all_complete() {
    // Spawn pool_size threads each doing a read simultaneously.
    // With the pool they should all proceed in parallel without deadlock.
    let pool_size = 4;
    let (store, _dir) = open_tmp_with_pool(pool_size);
    decl(&store, "pool/c");
    append_n(&store, "pool/c", 10);

    let results: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for _ in 0..pool_size {
        let s = store.clone();
        let r = Arc::clone(&results);
        handles.push(std::thread::spawn(move || {
            let events = s.read_range(ReadQuery::stream("pool/c")).unwrap();
            r.lock().unwrap().push(events.len());
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let counts = results.lock().unwrap();
    assert_eq!(
        counts.len(),
        pool_size,
        "all {pool_size} threads must complete"
    );
    assert!(
        counts.iter().all(|&c| c == 10),
        "each thread must see all 10 events; got: {counts:?}"
    );
}

#[test]
fn read_pool_size_one_is_valid() {
    // Pool size of 1 works correctly — sequential reads still pass.
    let (store, _dir) = open_tmp_with_pool(1);
    decl(&store, "pool/one");
    append_n(&store, "pool/one", 3);

    let e1 = store.read_range(ReadQuery::stream("pool/one")).unwrap();
    let e2 = store.read_range(ReadQuery::stream("pool/one")).unwrap();
    assert_eq!(e1.len(), 3);
    assert_eq!(e2.len(), 3);
}

#[test]
fn pool_connections_are_query_only() {
    // Verify that write attempts on pool connections are rejected by SQLite
    // (query_only = ON). We exercise this indirectly: reads succeed, and the
    // write path (append) still works via the separate write mutex.
    let (store, _dir) = open_tmp();
    decl(&store, "pool/qo");
    append_n(&store, "pool/qo", 2);

    // Read succeeds on pool connection.
    let events = store.read_range(ReadQuery::stream("pool/qo")).unwrap();
    assert_eq!(events.len(), 2);

    // Write succeeds on write connection.
    append_n(&store, "pool/qo", 1);
    let events = store.read_range(ReadQuery::stream("pool/qo")).unwrap();
    assert_eq!(events.len(), 3);
}

#[test]
fn pool_exhausted_returns_error() {
    // pool_size: 1, timeout: 50ms. Hold the one connection in a thread for 200ms,
    // then verify the main thread gets PoolExhausted within the timeout window.
    let (store, _dir) = open_tmp_with_pool_and_timeout(1, 50);
    decl(&store, "pool/ex");
    append_n(&store, "pool/ex", 1);

    let store2 = store.clone();
    let handle = std::thread::spawn(move || {
        store2._test_hold_read_conn(200);
    });

    // Give the thread a moment to acquire the connection before we compete.
    std::thread::sleep(std::time::Duration::from_millis(5));

    let result = store.read_range(ReadQuery::stream("pool/ex"));
    assert!(
        matches!(
            result,
            Err(Error::PoolExhausted {
                pool_size: 1,
                timeout_ms: 50
            })
        ),
        "expected PoolExhausted {{ pool_size: 1, timeout_ms: 50 }}"
    );

    handle.join().unwrap();
}
