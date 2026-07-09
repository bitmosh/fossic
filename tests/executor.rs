// SPDX-License-Identifier: Apache-2.0
use fossic::{Append, BacklogTask, OpenOptions, Store, TaskKind, TaskPriority};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
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

fn open_tmp_fast_quiescence() -> Store {
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_path_buf();
    std::mem::forget(file);
    Store::open(
        path,
        OpenOptions {
            executor_quiescence_window_ms: 50,
            ..OpenOptions::default()
        },
    )
    .unwrap()
}

/// A panicking Custom task must not kill the executor — subsequent tasks run normally.
#[test]
fn panic_in_custom_task_does_not_kill_executor() {
    let store = open_tmp_fast_quiescence();

    // Schedule a panicking task (earlier deadline → runs first).
    store.schedule_task(BacklogTask {
        priority: TaskPriority::Normal,
        deadline_us: 1,
        persist_on_drop: false,
        kind: TaskKind::Custom(Arc::new(|| panic!("deliberate test panic — SR-10 A-6"))),
        recurring_interval: None,
    });

    // Schedule a follow-up that sets a flag (later deadline → runs second).
    let ran = Arc::new(AtomicBool::new(false));
    let ran2 = Arc::clone(&ran);
    store.schedule_task(BacklogTask {
        priority: TaskPriority::Normal,
        deadline_us: 2,
        persist_on_drop: false,
        kind: TaskKind::Custom(Arc::new(move || ran2.store(true, Ordering::SeqCst))),
        recurring_interval: None,
    });

    // Allow up to 3 s; executor wakes every 500 ms + 50 ms quiescence window per task.
    let start = std::time::Instant::now();
    while !ran.load(Ordering::SeqCst) {
        if start.elapsed().as_secs() >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert!(
        ran.load(Ordering::SeqCst),
        "follow-up Custom task should execute after panicking task; executor must survive panic",
    );
}

/// Each panicking task is counted; the executor keeps running across multiple panics.
#[test]
fn multiple_panics_in_custom_tasks_all_isolated() {
    let store = open_tmp_fast_quiescence();

    for i in 1..=3u32 {
        store.schedule_task(BacklogTask {
            priority: TaskPriority::Normal,
            deadline_us: i as i64,
            persist_on_drop: false,
            kind: TaskKind::Custom(Arc::new(move || panic!("panic {i}"))),
            recurring_interval: None,
        });
    }

    let count = Arc::new(AtomicU32::new(0));
    let count2 = Arc::clone(&count);
    store.schedule_task(BacklogTask {
        priority: TaskPriority::Normal,
        deadline_us: 4,
        persist_on_drop: false,
        kind: TaskKind::Custom(Arc::new(move || {
            count2.fetch_add(1, Ordering::SeqCst);
        })),
        recurring_interval: None,
    });

    let start = std::time::Instant::now();
    while count.load(Ordering::SeqCst) == 0 {
        if start.elapsed().as_secs() >= 5 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "counter task must execute after three panicking tasks",
    );
}

/// With a short grace timeout, drop still returns (no hang).
#[test]
fn executor_short_grace_closes_within_timeout() {
    let store = tmp_store(OpenOptions {
        background_executor_grace_timeout_ms: 2_000,
        ..OpenOptions::default()
    });

    store
        .declare_stream("executor/grace", "test", None)
        .unwrap();
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
