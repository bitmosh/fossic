use fossic::{Append, CreateBranch, Error, OpenOptions, Reducer, Store};
use serde::{Deserialize, Serialize};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Test reducer ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Default)]
struct CountState {
    count: u64,
    sum: i64,
}

#[derive(Deserialize)]
struct CountEvent {
    value: i64,
}

struct CountReducer;

impl Reducer for CountReducer {
    type State = CountState;
    type Event = CountEvent;

    const NAME: &'static str = "count_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        CountState::default()
    }

    fn apply(&self, mut state: Self::State, event: &Self::Event) -> Self::State {
        state.count += 1;
        state.sum += event.value;
        state
    }
}

fn append_count_events(store: &Store, stream_id: &str, values: &[i64]) {
    for v in values {
        store
            .append(Append {
                stream_id: stream_id.to_string(),
                event_type: "Count".to_string(),
                payload: serde_json::json!({"value": v}),
                ..Default::default()
            })
            .unwrap();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn take_snapshot_stores_state() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();
    append_count_events(&store, "test/s", &[1, 2, 3]);

    let info = store.take_snapshot("test/s", "main").unwrap();
    assert_eq!(info.stream_id, "test/s");
    assert_eq!(info.branch, "main");
    assert_eq!(info.version, 2); // 3 events at versions 0, 1, 2
    assert_eq!(info.reducer_name, "count_reducer");
    assert_eq!(info.state_schema_version, 1);
}

#[test]
fn snapshot_info_returns_metadata() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();
    append_count_events(&store, "test/s", &[10]);

    store.take_snapshot("test/s", "main").unwrap();
    let info = store
        .snapshot_info("test/s", "main", "count_reducer")
        .unwrap();
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.reducer_name, "count_reducer");
    assert_eq!(info.version, 0);
}

#[test]
fn snapshot_info_none_when_no_snapshot() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();

    let info = store
        .snapshot_info("test/s", "main", "nonexistent")
        .unwrap();
    assert!(info.is_none());
}

#[test]
fn take_snapshot_no_events_fails() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();

    let result = store.take_snapshot("test/s", "main");
    assert!(
        matches!(result, Err(Error::NoEventsToSnapshot { .. })),
        "expected NoEventsToSnapshot, got {:?}",
        result
    );
}

#[test]
fn take_snapshot_no_reducer_fails() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    append_count_events(&store, "test/s", &[1]);

    let result = store.take_snapshot("test/s", "main");
    assert!(matches!(result, Err(Error::ReducerNotFound { .. })));
}

#[test]
fn read_state_with_snapshot_is_equivalent_to_without() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();

    // Append events in two batches with a snapshot in between.
    append_count_events(&store, "test/s", &[1, 2, 3]);
    store.take_snapshot("test/s", "main").unwrap();
    append_count_events(&store, "test/s", &[4, 5]);

    let state: CountState = store.read_state("test/s", "main").unwrap();
    assert_eq!(state.count, 5);
    assert_eq!(state.sum, 15);
}

#[test]
fn gc_no_active_reducers_deletes_all_snapshots() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();
    append_count_events(&store, "test/s", &[1, 2]);
    store.take_snapshot("test/s", "main").unwrap();

    // Open a fresh store with no reducers registered — GC should delete all snapshots.
    let dir2 = tempfile::tempdir().unwrap();
    let store2 = Store::open(dir2.path().join("t.db"), OpenOptions::default()).unwrap();
    store2.declare_stream("test/s", "t", None).unwrap();
    store2.register_reducer("test/s", CountReducer).unwrap();
    append_count_events(&store2, "test/s", &[1]);
    store2.take_snapshot("test/s", "main").unwrap();

    // Unregister by opening a store with different reducer (simulated via a new store with no reducers).
    let dir3 = tempfile::tempdir().unwrap();
    let path3 = dir3.path().join("t.db");
    // Re-open dir2's DB without registering any reducers.
    let store_empty = Store::open(dir2.path().join("t.db"), OpenOptions::default()).unwrap();
    let deleted = store_empty.gc_orphaned_snapshots().unwrap();
    assert_eq!(deleted, 1, "should delete the one snapshot for count_reducer");
    let _ = path3; // silence unused warning
}

#[test]
fn gc_keeps_snapshots_for_active_reducer() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();
    append_count_events(&store, "test/s", &[1, 2, 3]);
    store.take_snapshot("test/s", "main").unwrap();

    // GC with the reducer still registered — should delete nothing.
    let deleted = store.gc_orphaned_snapshots().unwrap();
    assert_eq!(deleted, 0);
}

#[test]
fn take_snapshot_idempotent_when_no_new_events() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", CountReducer).unwrap();
    append_count_events(&store, "test/s", &[42]);

    let info1 = store.take_snapshot("test/s", "main").unwrap();
    let info2 = store.take_snapshot("test/s", "main").unwrap();
    // Second call: no new events, returns the same snapshot version.
    assert_eq!(info1.version, info2.version);
}

#[test]
fn snapshot_on_branch() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/**", CountReducer).unwrap();
    append_count_events(&store, "test/s", &[1, 2]);

    store
        .create_branch(&CreateBranch {
            stream_id: "test/s".to_string(),
            branch_id: "b1".to_string(),
            parent_id: "main".to_string(),
            parent_version: 1,
            description: None,
            alternatives: None,
        })
        .unwrap();

    // Append to branch.
    store
        .append(Append {
            stream_id: "test/s".to_string(),
            branch: "b1".to_string(),
            event_type: "Count".to_string(),
            payload: serde_json::json!({"value": 10}),
            ..Default::default()
        })
        .unwrap();

    let info = store.take_snapshot("test/s", "b1").unwrap();
    assert_eq!(info.branch, "b1");
    assert_eq!(info.version, 0); // branch has 1 event at version 0
}
