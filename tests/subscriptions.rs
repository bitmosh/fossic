// SPDX-License-Identifier: Apache-2.0
use fossic::{
    Append, OpenOptions, ReadQuery, Store, StoredEvent, SubscribeQuery, SubscriptionHandler,
    SubscriptionMode,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

// ── Test helpers ──────────────────────────────────────────────────────────────

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

fn decl(store: &Store, id: &str) {
    store.declare_stream(id, "test", None).unwrap();
}

fn ev(stream_id: &str) -> Append {
    Append {
        stream_id: stream_id.to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"x": 1}),
        ..Default::default()
    }
}

/// Like `ev()` but every call produces a unique CCE event ID via a counter.
/// Use this whenever you need multiple distinct events (loops, cross-stream filtering, etc.)
fn unique_ev(stream_id: &str) -> Append {
    use std::sync::atomic::AtomicU64;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    Append {
        stream_id: stream_id.to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"seq": seq}),
        ..Default::default()
    }
}

/// Poll `f` every 10 ms until it returns true or `timeout_ms` elapses.
fn wait_for<F: Fn() -> bool>(f: F, timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    loop {
        if f() {
            return true;
        }
        if start.elapsed().as_millis() > timeout_ms as u128 {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Wraps a `Fn(&StoredEvent)` as a `SubscriptionHandler`.
struct FnHandler<F: Fn(&StoredEvent) + Send + Sync + 'static>(F);

impl<F: Fn(&StoredEvent) + Send + Sync + 'static> SubscriptionHandler for FnHandler<F> {
    fn on_event(&self, event: &StoredEvent) {
        (self.0)(event)
    }
}

// ── Synchronous mode ──────────────────────────────────────────────────────────

#[test]
fn sync_fires_before_append_returns() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/s");

    let fired = Arc::new(AtomicBool::new(false));
    let fired2 = Arc::clone(&fired);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/s"),
            SubscriptionMode::Synchronous,
            FnHandler(move |_| fired2.store(true, Ordering::Release)),
        )
        .unwrap();

    store.append(ev("test/s")).unwrap();

    // Must be true immediately after append returns — no waiting.
    assert!(
        fired.load(Ordering::Acquire),
        "Synchronous handler must fire before store.append() returns"
    );
}

#[test]
fn sync_panic_does_not_abort_append() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/s");

    let handle = store
        .subscribe(
            SubscribeQuery::stream("test/s"),
            SubscriptionMode::Synchronous,
            FnHandler(|_| panic!("intentional panic in sync handler")),
        )
        .unwrap();

    // Append must succeed even though the handler panics.
    let id = store.append(ev("test/s")).unwrap();
    assert_ne!(id.to_hex(), "");

    // The subscription is marked degraded after the panic.
    assert!(
        handle.is_degraded(),
        "subscription must be degraded after panic"
    );
}

#[test]
fn sync_degraded_stops_delivery() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/s");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let handle = store
        .subscribe(
            SubscribeQuery::stream("test/s"),
            SubscriptionMode::Synchronous,
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
                panic!("panic every time");
            }),
        )
        .unwrap();

    store.append(ev("test/s")).unwrap(); // panics → degraded
    assert!(handle.is_degraded());

    // Subsequent appends should NOT invoke the handler again.
    store.append(ev("test/s")).unwrap();
    store.append(ev("test/s")).unwrap();

    assert_eq!(
        count.load(Ordering::Relaxed),
        1,
        "degraded subscriber must not receive further events"
    );
}

#[test]
fn sync_multiple_subscribers_all_fire() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/s");

    let c1 = Arc::new(AtomicBool::new(false));
    let c2 = Arc::new(AtomicBool::new(false));

    let c1c = Arc::clone(&c1);
    let c2c = Arc::clone(&c2);

    let _h1 = store
        .subscribe(
            SubscribeQuery::stream("test/s"),
            SubscriptionMode::Synchronous,
            FnHandler(move |_| c1c.store(true, Ordering::Release)),
        )
        .unwrap();

    let _h2 = store
        .subscribe(
            SubscribeQuery::stream("test/s"),
            SubscriptionMode::Synchronous,
            FnHandler(move |_| c2c.store(true, Ordering::Release)),
        )
        .unwrap();

    store.append(ev("test/s")).unwrap();

    assert!(c1.load(Ordering::Acquire), "subscriber 1 should fire");
    assert!(c2.load(Ordering::Acquire), "subscriber 2 should fire");
}

// ── PostCommit mode ───────────────────────────────────────────────────────────

#[test]
fn post_commit_fires_after_append() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/pc");

    let fired = Arc::new(AtomicBool::new(false));
    let fired2 = Arc::clone(&fired);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/pc"),
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| fired2.store(true, Ordering::Release)),
        )
        .unwrap();

    store.append(ev("test/pc")).unwrap();

    assert!(
        wait_for(|| fired.load(Ordering::Acquire), 1000),
        "PostCommit handler must fire within 1 second"
    );
}

#[test]
fn post_commit_receives_correct_event_fields() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/fields");

    let received = Arc::new(Mutex::new(None::<(String, String, String, u64)>));
    let recv2 = Arc::clone(&received);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/fields"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |e| {
                let mut guard = recv2.lock().unwrap();
                *guard = Some((
                    e.stream_id.clone(),
                    e.branch.clone(),
                    e.event_type.clone(),
                    e.version,
                ));
            }),
        )
        .unwrap();

    store.append(ev("test/fields")).unwrap();

    assert!(wait_for(|| received.lock().unwrap().is_some(), 1000));

    let guard = received.lock().unwrap();
    let (sid, branch, etype, version) = guard.as_ref().unwrap();
    assert_eq!(sid, "test/fields");
    assert_eq!(branch, "main");
    assert_eq!(etype, "TestEvent");
    assert_eq!(*version, 0);
}

#[test]
fn post_commit_queue_overflow_marks_degraded() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/overflow");

    // Lock that the handler tries to acquire: held during test, released at end.
    // This guarantees the handler never finishes processing event 0, so the
    // bounded queue (size 3) fills deterministically and overflows.
    let lock = Arc::new(std::sync::Mutex::new(()));
    let guard = lock.lock().unwrap();
    let lock2 = Arc::clone(&lock);

    let handle = store
        .subscribe(
            SubscribeQuery::stream("test/overflow"),
            SubscriptionMode::PostCommit { queue_size: 3 },
            FnHandler(move |_| {
                let _guard2 = lock2.lock().unwrap(); // blocks while test holds guard
            }),
        )
        .unwrap();

    // Append enough events to overflow the queue. Each call uses unique_ev so
    // the CCE content-hash differs per event (ev() would produce duplicates → no-ops).
    for _ in 0..10 {
        store.append(unique_ev("test/overflow")).unwrap();
    }

    let result = wait_for(|| handle.is_degraded(), 2000);
    drop(guard); // release handler threads
    assert!(result, "subscription must be degraded after queue overflow");
}

#[test]
fn post_commit_overflow_writes_system_event() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/degraded-sys");

    let lock = Arc::new(std::sync::Mutex::new(()));
    let guard = lock.lock().unwrap();
    let lock2 = Arc::clone(&lock);

    let handle = store
        .subscribe(
            SubscribeQuery::stream("test/degraded-sys"),
            SubscriptionMode::PostCommit { queue_size: 2 },
            FnHandler(move |_| {
                let _guard2 = lock2.lock().unwrap();
            }),
        )
        .unwrap();

    for _ in 0..8 {
        store.append(unique_ev("test/degraded-sys")).unwrap();
    }

    let degraded = wait_for(|| handle.is_degraded(), 2000);
    drop(guard); // release handler threads
    assert!(degraded);

    // Give the dispatcher time to write the SubscriptionDegraded event.
    std::thread::sleep(std::time::Duration::from_millis(200));

    let sys_events = store
        .read_range(ReadQuery::stream("_fossic/system"))
        .unwrap();
    let degraded_count = sys_events
        .iter()
        .filter(|e| e.event_type == "SubscriptionDegraded")
        .count();
    assert!(
        degraded_count >= 1,
        "SubscriptionDegraded event must be written to _fossic/system; got {degraded_count}"
    );
}

#[test]
fn drop_handle_stops_delivery() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/drop");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let handle = store
        .subscribe(
            SubscribeQuery::stream("test/drop"),
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    store.append(ev("test/drop")).unwrap();
    assert!(wait_for(|| count.load(Ordering::Relaxed) == 1, 1000));

    drop(handle); // Unsubscribe.
    std::thread::sleep(std::time::Duration::from_millis(50));

    let before = count.load(Ordering::Relaxed);
    store.append(ev("test/drop")).unwrap();
    store.append(ev("test/drop")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert_eq!(
        count.load(Ordering::Relaxed),
        before,
        "no more events should arrive after handle is dropped"
    );
}

// ── Filtering ─────────────────────────────────────────────────────────────────

#[test]
fn subscribe_filters_by_stream() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/a");
    decl(&store, "test/b");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/a"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    store.append(unique_ev("test/b")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "subscriber on test/a should not receive events for test/b"
    );

    store.append(unique_ev("test/a")).unwrap();
    assert!(wait_for(|| count.load(Ordering::Relaxed) == 1, 2000));
}

#[test]
fn subscribe_filters_by_branch() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/br");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery {
                stream_pattern: "test/br".to_string(),
                branch: "main".to_string(),
                include_system: false,
            },
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    // Append to a different branch.
    store
        .append(Append {
            stream_id: "test/br".to_string(),
            branch: "other".to_string(),
            event_type: "TestEvent".to_string(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "subscriber on main should not receive events for branch 'other'"
    );
}

#[test]
fn multiple_subscribers_same_stream_both_receive() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/multi");

    let c1 = Arc::new(AtomicU64::new(0));
    let c2 = Arc::new(AtomicU64::new(0));
    let c1c = Arc::clone(&c1);
    let c2c = Arc::clone(&c2);

    let _h1 = store
        .subscribe(
            SubscribeQuery::stream("test/multi"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                c1c.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    let _h2 = store
        .subscribe(
            SubscribeQuery::stream("test/multi"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                c2c.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    store.append(ev("test/multi")).unwrap();

    assert!(wait_for(|| c1.load(Ordering::Relaxed) == 1, 1000));
    assert!(wait_for(|| c2.load(Ordering::Relaxed) == 1, 1000));
}

#[test]
fn sync_and_post_commit_both_receive() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/mixed");

    let sync_fired = Arc::new(AtomicBool::new(false));
    let pc_fired = Arc::new(AtomicBool::new(false));
    let sync2 = Arc::clone(&sync_fired);
    let pc2 = Arc::clone(&pc_fired);

    let _hs = store
        .subscribe(
            SubscribeQuery::stream("test/mixed"),
            SubscriptionMode::Synchronous,
            FnHandler(move |_| sync2.store(true, Ordering::Release)),
        )
        .unwrap();

    let _hp = store
        .subscribe(
            SubscribeQuery::stream("test/mixed"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| pc2.store(true, Ordering::Release)),
        )
        .unwrap();

    store.append(ev("test/mixed")).unwrap();

    // Sync fires synchronously before append returns.
    assert!(sync_fired.load(Ordering::Acquire));
    // PostCommit fires asynchronously.
    assert!(wait_for(|| pc_fired.load(Ordering::Acquire), 1000));
}

#[test]
fn idempotent_append_not_redispatched() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/idemp");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/idemp"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    let a = ev("test/idemp");
    // Build an identical event (same event_type, type_version, no causation_id, same payload).
    let a2 = Append {
        stream_id: "test/idemp".to_string(),
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({"x": 1}),
        ..Default::default()
    };

    store.append(a).unwrap();
    assert!(wait_for(|| count.load(Ordering::Relaxed) == 1, 1000));

    // Second append with identical content should be deduplicated.
    store.append(a2).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert_eq!(
        count.load(Ordering::Relaxed),
        1,
        "duplicate append should not dispatch again"
    );
}

#[test]
fn system_stream_events_not_dispatched_to_user_subscribers() {
    let (store, _dir) = open_tmp();
    decl(&store, "test/user");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/user"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    // Appending directly to _fossic/system (it's pre-declared) must not dispatch
    // to subscribers watching user streams.
    store
        .append(Append {
            stream_id: "_fossic/system".to_string(),
            event_type: "SomeSystemEvent".to_string(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "_fossic/system events must not reach user-stream subscribers"
    );
}

#[test]
fn subscribe_to_nonexistent_stream_ok_and_later_receives() {
    let (store, _dir) = open_tmp();

    let fired = Arc::new(AtomicBool::new(false));
    let fired2 = Arc::clone(&fired);

    // Subscribe before declaring or appending anything.
    let _handle = store
        .subscribe(
            SubscribeQuery::stream("test/late"),
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| fired2.store(true, Ordering::Release)),
        )
        .unwrap();

    // Now declare and append.
    decl(&store, "test/late");
    store.append(ev("test/late")).unwrap();

    assert!(
        wait_for(|| fired.load(Ordering::Acquire), 1000),
        "subscriber registered before stream declaration must still receive events"
    );
}

// ── Glob pattern subscriptions ────────────────────────────────────────────────

#[test]
fn glob_star_receives_multiple_matching_streams() {
    let (store, _dir) = open_tmp();
    decl(&store, "glob/a");
    decl(&store, "glob/b");
    decl(&store, "other/c");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery {
                stream_pattern: "glob/*".to_string(),
                branch: "main".to_string(),
                include_system: false,
            },
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    store.append(unique_ev("glob/a")).unwrap();
    store.append(unique_ev("glob/b")).unwrap();
    store.append(unique_ev("other/c")).unwrap(); // must NOT match

    assert!(
        wait_for(|| count.load(Ordering::Relaxed) >= 2, 2000),
        "glob/* subscription must receive events from both glob/a and glob/b"
    );
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(
        count.load(Ordering::Relaxed),
        2,
        "glob/* must not receive events for other/c"
    );
}

#[test]
fn glob_double_star_receives_all_matching_streams() {
    let (store, _dir) = open_tmp();
    decl(&store, "ns/sub/a");
    decl(&store, "ns/sub/b");
    decl(&store, "ns/other");
    decl(&store, "different/x");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery {
                stream_pattern: "ns/**".to_string(),
                branch: "main".to_string(),
                include_system: false,
            },
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    store.append(unique_ev("ns/sub/a")).unwrap();
    store.append(unique_ev("ns/sub/b")).unwrap();
    store.append(unique_ev("ns/other")).unwrap();
    store.append(unique_ev("different/x")).unwrap(); // must NOT match

    assert!(
        wait_for(|| count.load(Ordering::Relaxed) >= 3, 2000),
        "ns/** must receive events from all ns/ subtree streams"
    );
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(
        count.load(Ordering::Relaxed),
        3,
        "ns/** must not receive events for different/x"
    );
}

#[test]
fn include_system_false_blocks_system_stream_on_glob() {
    let (store, _dir) = open_tmp();
    decl(&store, "any/stream");

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery {
                stream_pattern: "**".to_string(),
                branch: "main".to_string(),
                include_system: false,
            },
            SubscriptionMode::PostCommit { queue_size: 8 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    store
        .append(Append {
            stream_id: "_fossic/system".to_string(),
            event_type: "SysEvent".to_string(),
            payload: serde_json::json!({}),
            ..Default::default()
        })
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "include_system: false must suppress _fossic/system even when ** pattern is used"
    );
}

#[test]
fn glob_sub_does_not_replay_historical_events() {
    // Events appended BEFORE the glob subscription is created must NOT be delivered.
    // Events appended AFTER must be delivered.
    let (store, _dir) = open_tmp();
    decl(&store, "hist/a");
    decl(&store, "hist/b");

    // Pre-existing history (before subscription).
    store.append(unique_ev("hist/a")).unwrap();
    store.append(unique_ev("hist/a")).unwrap();
    store.append(unique_ev("hist/b")).unwrap();

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let received2 = Arc::clone(&received);

    let _handle = store
        .subscribe(
            SubscribeQuery {
                stream_pattern: "hist/*".to_string(),
                branch: "main".to_string(),
                include_system: false,
            },
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |e| {
                received2.lock().unwrap().push(e.stream_id.clone());
            }),
        )
        .unwrap();

    // Brief pause — no replayed events should arrive.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(
        received.lock().unwrap().len(),
        0,
        "glob sub must not replay events committed before subscription"
    );

    // New events after subscription must arrive.
    store.append(unique_ev("hist/a")).unwrap();
    store.append(unique_ev("hist/b")).unwrap();

    assert!(
        wait_for(|| received.lock().unwrap().len() >= 2, 2000),
        "glob sub must receive events appended after subscription"
    );
    assert_eq!(
        received.lock().unwrap().len(),
        2,
        "glob sub must receive exactly the 2 new events"
    );
}

#[test]
fn glob_sub_receives_events_on_new_stream_created_after_subscription() {
    // A stream that didn't exist at subscription time must be delivered when
    // its first event arrives (stream_cursors defaults to -1 for unknown streams).
    let (store, _dir) = open_tmp();
    decl(&store, "ns/existing");

    // Pre-existing event on known stream.
    store.append(unique_ev("ns/existing")).unwrap();

    let count = Arc::new(AtomicU64::new(0));
    let count2 = Arc::clone(&count);

    let _handle = store
        .subscribe(
            SubscribeQuery {
                stream_pattern: "ns/**".to_string(),
                branch: "main".to_string(),
                include_system: false,
            },
            SubscriptionMode::PostCommit { queue_size: 16 },
            FnHandler(move |_| {
                count2.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap();

    // Pause — the pre-existing event must NOT be replayed.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "pre-existing event must not replay after glob sub creation"
    );

    // Create a brand-new stream and append to it.
    decl(&store, "ns/new");
    store.append(unique_ev("ns/new")).unwrap();

    assert!(
        wait_for(|| count.load(Ordering::Relaxed) >= 1, 2000),
        "glob sub must receive first event on stream created after subscription"
    );
}

// ── SR-10 A-5: SubscriptionDegraded emitted for sync panics ──────────────────

struct PanicHandler;
impl SubscriptionHandler for PanicHandler {
    fn on_event(&self, _event: &StoredEvent) {
        panic!("deliberate sync subscriber panic — SR-10 A-5");
    }
}

struct NormalCountHandler {
    count: Arc<AtomicU64>,
}
impl SubscriptionHandler for NormalCountHandler {
    fn on_event(&self, _event: &StoredEvent) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn sync_panic_subscriber_marked_degraded() {
    let (store, _dir) = open_tmp();
    decl(&store, "panic/deg");

    let handle = store
        .subscribe(
            SubscribeQuery::stream("panic/deg"),
            SubscriptionMode::Synchronous,
            PanicHandler,
        )
        .unwrap();

    store.append(unique_ev("panic/deg")).unwrap();

    assert!(
        handle.is_degraded(),
        "sync-panicking subscriber must be degraded after append"
    );
}

#[test]
fn sync_panic_does_not_block_other_subscribers() {
    let (store, _dir) = open_tmp();
    decl(&store, "panic/other");

    let _panic_handle = store
        .subscribe(
            SubscribeQuery::stream("panic/other"),
            SubscriptionMode::Synchronous,
            PanicHandler,
        )
        .unwrap();

    let count = Arc::new(AtomicU64::new(0));
    let _normal_handle = store
        .subscribe(
            SubscribeQuery::stream("panic/other"),
            SubscriptionMode::Synchronous,
            NormalCountHandler {
                count: Arc::clone(&count),
            },
        )
        .unwrap();

    store.append(unique_ev("panic/other")).unwrap();

    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "normal sync subscriber must still receive events even when a peer panics",
    );
}

#[test]
fn sync_panic_emits_subscription_degraded_event() {
    let (store, _dir) = open_tmp();
    decl(&store, "panic/sysevent");

    let _handle = store
        .subscribe(
            SubscribeQuery::stream("panic/sysevent"),
            SubscriptionMode::Synchronous,
            PanicHandler,
        )
        .unwrap();

    store.append(unique_ev("panic/sysevent")).unwrap();

    // The SubscriptionDegraded event is written by the dispatcher thread after
    // the write lock is released — poll until it appears in _fossic/system.
    let found = wait_for(
        || {
            store
                .read_range(ReadQuery {
                    stream_id: "_fossic/system".into(),
                    branch: "main".into(),
                    event_type_filter: Some("SubscriptionDegraded".into()),
                    from_version: None,
                    to_version: None,
                    limit: None,
                })
                .map(|v| !v.is_empty())
                .unwrap_or(false)
        },
        2000,
    );

    assert!(
        found,
        "SubscriptionDegraded must be written to _fossic/system when a sync subscriber panics",
    );
}
