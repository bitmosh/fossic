use fossic::{Append, Error, OpenOptions, ReadQuery, Reducer, SnapshotPolicy, Store};
use serde::{Deserialize, Serialize};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Test reducer ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
struct SumState {
    count: u64,
    total: i64,
}

#[derive(Deserialize)]
struct AddEvent {
    value: i64,
}

struct SumReducer;

impl Reducer for SumReducer {
    type State = SumState;
    type Event = AddEvent;

    const NAME: &'static str = "sum_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        SumState::default()
    }

    fn apply(&self, mut state: Self::State, event: &Self::Event) -> Self::State {
        state.count += 1;
        state.total += event.value;
        state
    }
}

fn append_add(store: &Store, stream_id: &str, value: i64) {
    store
        .append(Append {
            stream_id: stream_id.to_string(),
            event_type: "Add".to_string(),
            payload: serde_json::json!({"value": value}),
            ..Default::default()
        })
        .unwrap();
}

// ── Policy validation ─────────────────────────────────────────────────────────

#[test]
fn policy_invalid_zero() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result =
        store.register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(0));
    assert!(
        matches!(result, Err(Error::SnapshotPolicyInvalid(_))),
        "expected SnapshotPolicyInvalid, got {result:?}",
    );
}

#[test]
fn policy_every_n_seconds_accepted() {
    // v1.3.1: EveryNSeconds is live — schedules snapshots via BackgroundExecutor.
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result =
        store.register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNSeconds(60));
    assert!(
        result.is_ok(),
        "EveryNSeconds(60) should be accepted, got {result:?}"
    );
}

#[test]
fn policy_every_n_seconds_zero_rejected() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result =
        store.register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNSeconds(0));
    assert!(
        matches!(result, Err(Error::SnapshotPolicyInvalid(_))),
        "expected SnapshotPolicyInvalid for N=0, got {result:?}",
    );
}

#[test]
fn state_adaptive_policy_accepted() {
    // v1.2.1: StateAdaptive is now a live policy, no longer NotImplemented.
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    let result = store.register_reducer_with_policy(
        "s1",
        SumReducer,
        SnapshotPolicy::StateAdaptive {
            target_replay_cost_us: 100_000,
            min_events_between: 10,
        },
    );
    assert!(
        result.is_ok(),
        "StateAdaptive should be accepted, got {result:?}"
    );
}

// ── EveryNEvents behavior ─────────────────────────────────────────────────────

#[test]
fn every_n_events_no_snapshot_below_threshold() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store
        .register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(3))
        .unwrap();

    append_add(&store, "s1", 1);
    append_add(&store, "s1", 2);

    let _: SumState = store.read_state("s1", "main").unwrap();
    let snap = store.snapshot_info("s1", "main", "sum_reducer").unwrap();
    assert!(snap.is_none(), "no snapshot expected before threshold");
}

#[test]
fn every_n_events_snapshot_fires_at_threshold() {
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store
        .register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(3))
        .unwrap();

    append_add(&store, "s1", 1);
    append_add(&store, "s1", 2);
    append_add(&store, "s1", 3);

    let state: SumState = store.read_state("s1", "main").unwrap();
    assert_eq!(state.count, 3);
    assert_eq!(state.total, 6);

    let snap = store.snapshot_info("s1", "main", "sum_reducer").unwrap();
    assert!(snap.is_some(), "snapshot expected after threshold");
    assert_eq!(snap.unwrap().version, 2); // 3 events at versions 0, 1, 2
}

#[test]
fn every_n_events_counter_resets_after_snapshot() {
    // EveryNEvents(3): first read_state at 3 events fires a snapshot (v2);
    // second read_state at 3 new events fires another (v5).
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store
        .register_reducer_with_policy("s1", SumReducer, SnapshotPolicy::EveryNEvents(3))
        .unwrap();

    for v in [1i64, 2, 3] {
        append_add(&store, "s1", v);
    }
    let _: SumState = store.read_state("s1", "main").unwrap();
    let snap1 = store
        .snapshot_info("s1", "main", "sum_reducer")
        .unwrap()
        .expect("first snapshot expected after 3 events");
    assert_eq!(snap1.version, 2);

    for v in [4i64, 5, 6] {
        append_add(&store, "s1", v);
    }
    let state: SumState = store.read_state("s1", "main").unwrap();
    assert_eq!(state.count, 6);
    assert_eq!(state.total, 21);

    let snap2 = store
        .snapshot_info("s1", "main", "sum_reducer")
        .unwrap()
        .expect("second snapshot expected after counter reset");
    assert_eq!(snap2.version, 5);
}

#[test]
fn manual_policy_never_auto_snapshots() {
    // Default register_reducer uses Manual — no snapshot should fire automatically.
    let (store, _dir) = open_tmp();
    store.declare_stream("s1", "test", None).unwrap();
    store.register_reducer("s1", SumReducer).unwrap();

    for v in [1i64, 2, 3, 4, 5] {
        append_add(&store, "s1", v);
    }

    let state: SumState = store.read_state("s1", "main").unwrap();
    assert_eq!(state.count, 5);

    let snap = store.snapshot_info("s1", "main", "sum_reducer").unwrap();
    assert!(snap.is_none(), "Manual policy must not auto-snapshot");
}

// ── StateAdaptive behavior ────────────────────────────────────────────────────

// A reducer that sleeps 1ms per apply, giving predictable timing for StateAdaptive.
#[derive(Serialize, Deserialize, Clone, Default)]
struct SlowState {
    count: u64,
}

#[derive(Deserialize)]
struct SlowEvent {
    #[allow(dead_code)]
    seq: u32,
}

struct SlowReducer;

impl Reducer for SlowReducer {
    type State = SlowState;
    type Event = SlowEvent;

    const NAME: &'static str = "slow_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        SlowState::default()
    }

    fn apply(&self, mut state: Self::State, _event: &Self::Event) -> Self::State {
        std::thread::sleep(std::time::Duration::from_millis(1));
        state.count += 1;
        state
    }
}

fn append_slow(store: &Store, stream_id: &str, seq: u32) {
    store
        .append(Append {
            stream_id: stream_id.to_string(),
            event_type: "Tick".to_string(),
            payload: serde_json::json!({ "seq": seq }),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn state_adaptive_triggers_snapshot() {
    // SlowReducer costs ~1ms per apply. target_replay_cost_us = 500 (0.5ms),
    // min_events_between = 1 → snapshot fires after first read_state.
    let (store, _dir) = open_tmp();
    store.declare_stream("slow", "test", None).unwrap();
    store
        .register_reducer_with_policy(
            "slow",
            SlowReducer,
            SnapshotPolicy::StateAdaptive {
                target_replay_cost_us: 500,
                min_events_between: 1,
            },
        )
        .unwrap();

    append_slow(&store, "slow", 0);
    let state: SlowState = store.read_state("slow", "main").unwrap();
    assert_eq!(state.count, 1);

    let snap = store.snapshot_info("slow", "main", "slow_reducer").unwrap();
    assert!(
        snap.is_some(),
        "StateAdaptive should have triggered a snapshot"
    );
}

#[test]
fn state_adaptive_respects_min_events_between() {
    // min_events_between = 10: no snapshot after only 3 events.
    let (store, _dir) = open_tmp();
    store.declare_stream("slow2", "test", None).unwrap();
    store
        .register_reducer_with_policy(
            "slow2",
            SlowReducer,
            SnapshotPolicy::StateAdaptive {
                target_replay_cost_us: 500,
                min_events_between: 10,
            },
        )
        .unwrap();

    for i in 0..3u32 {
        append_slow(&store, "slow2", i);
    }
    let _: SlowState = store.read_state("slow2", "main").unwrap();

    let snap = store
        .snapshot_info("slow2", "main", "slow_reducer")
        .unwrap();
    assert!(
        snap.is_none(),
        "StateAdaptive must not snapshot before min_events_between is reached"
    );
}

// ── ReducerStateLarge behavior ────────────────────────────────────────────────

// Reducer that appends 50 bytes to a Vec on each apply.
#[derive(Serialize, Deserialize, Clone, Default)]
struct BigState {
    data: Vec<u8>,
}

#[derive(Deserialize)]
struct GrowEvent {
    #[allow(dead_code)]
    seq: u32,
}

struct BigReducer;

impl Reducer for BigReducer {
    type State = BigState;
    type Event = GrowEvent;

    const NAME: &'static str = "big_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        BigState::default()
    }

    fn apply(&self, mut state: Self::State, _event: &Self::Event) -> Self::State {
        state.data.extend(vec![0u8; 50]);
        state
    }
}

fn append_grow(store: &Store, stream_id: &str, seq: u32) {
    store
        .append(Append {
            stream_id: stream_id.to_string(),
            event_type: "Grow".to_string(),
            payload: serde_json::json!({ "seq": seq }),
            ..Default::default()
        })
        .unwrap();
}

fn open_tmp_with_threshold(threshold: usize) -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(
        dir.path().join("test.db"),
        OpenOptions {
            reducer_state_large_threshold_bytes: threshold,
            ..Default::default()
        },
    )
    .expect("open store");
    (store, dir)
}

#[test]
fn state_large_emits_to_system_stream() {
    // threshold = 100 bytes; BigReducer grows by 50 bytes/event.
    // After 3 events the state is ~150 bytes (msgpack), mean > 100 → ReducerStateLarge emitted.
    let (store, _dir) = open_tmp_with_threshold(100);
    store.declare_stream("big", "test", None).unwrap();
    store.register_reducer("big", BigReducer).unwrap();

    for i in 0..3u32 {
        append_grow(&store, "big", i);
    }
    let _: BigState = store.read_state("big", "main").unwrap();

    let events = store
        .read_range(ReadQuery {
            stream_id: "_fossic/system".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: Some("ReducerStateLarge".to_string()),
        })
        .unwrap();
    assert!(
        !events.is_empty(),
        "expected ReducerStateLarge event in _fossic/system"
    );
}

#[test]
fn state_large_throttled() {
    // Two rapid read_state calls → only one ReducerStateLarge within the 60-second window.
    let (store, _dir) = open_tmp_with_threshold(100);
    store.declare_stream("big2", "test", None).unwrap();
    store.register_reducer("big2", BigReducer).unwrap();

    for i in 0..5u32 {
        append_grow(&store, "big2", i);
    }
    let _: BigState = store.read_state("big2", "main").unwrap();
    let _: BigState = store.read_state("big2", "main").unwrap();

    let events = store
        .read_range(ReadQuery {
            stream_id: "_fossic/system".to_string(),
            branch: "main".to_string(),
            from_version: None,
            to_version: None,
            limit: None,
            event_type_filter: Some("ReducerStateLarge".to_string()),
        })
        .unwrap();
    assert_eq!(
        events.len(),
        1,
        "throttle must suppress the second ReducerStateLarge within 60 seconds"
    );
}
