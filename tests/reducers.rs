// SPDX-License-Identifier: Apache-2.0
use fossic::{Append, Error, EventId, OpenOptions, Reducer, Store};
use serde::{Deserialize, Serialize};

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Store::open(dir.path().join("test.db"), OpenOptions::default()).expect("open store");
    (store, dir)
}

// ── Test reducers ─────────────────────────────────────────────────────────────

/// Counts events and accumulates a sum field.
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

/// Concatenates string payloads.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
struct CatState {
    items: Vec<String>,
}

#[derive(Deserialize)]
struct CatEvent {
    item: String,
}

struct CatReducer;

impl Reducer for CatReducer {
    type State = CatState;
    type Event = CatEvent;

    const NAME: &'static str = "cat_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        CatState::default()
    }

    fn apply(&self, mut state: Self::State, event: &Self::Event) -> Self::State {
        state.items.push(event.item.clone());
        state
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

fn append_add_ret(store: &Store, stream_id: &str, value: i64) -> EventId {
    store
        .append(Append {
            stream_id: stream_id.to_string(),
            event_type: "Add".to_string(),
            payload: serde_json::json!({"value": value}),
            ..Default::default()
        })
        .unwrap()
}

// ── register_reducer ─────────────────────────────────────────────────────────

#[test]
fn register_reducer_exact_pattern() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("policy-scout/audit", "t", None)
        .unwrap();
    store
        .register_reducer("policy-scout/audit", SumReducer)
        .unwrap();
    append_add(&store, "policy-scout/audit", 5);

    let state: SumState = store.read_state("policy-scout/audit", "main").unwrap();
    assert_eq!(state.count, 1);
    assert_eq!(state.total, 5);
}

#[test]
fn register_reducer_wildcard_star() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/lattice/abc", "t", None)
        .unwrap();
    store
        .declare_stream("cerebra/lattice/def", "t", None)
        .unwrap();
    store
        .register_reducer("cerebra/lattice/*", SumReducer)
        .unwrap();

    append_add(&store, "cerebra/lattice/abc", 3);
    append_add(&store, "cerebra/lattice/def", 7);

    let s1: SumState = store.read_state("cerebra/lattice/abc", "main").unwrap();
    let s2: SumState = store.read_state("cerebra/lattice/def", "main").unwrap();
    assert_eq!(s1.total, 3);
    assert_eq!(s2.total, 7);
}

#[test]
fn register_reducer_wildcard_double_star() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/lattice/abc", "t", None)
        .unwrap();
    store
        .declare_stream("cerebra/lattice/abc/sub", "t", None)
        .unwrap();
    store.register_reducer("cerebra/**", SumReducer).unwrap();

    append_add(&store, "cerebra/lattice/abc", 1);
    append_add(&store, "cerebra/lattice/abc/sub", 2);

    let s1: SumState = store.read_state("cerebra/lattice/abc", "main").unwrap();
    let s2: SumState = store.read_state("cerebra/lattice/abc/sub", "main").unwrap();
    assert_eq!(s1.total, 1);
    assert_eq!(s2.total, 2);
}

#[test]
fn register_reducer_double_star_also_matches_single_segment() {
    let (store, _dir) = open_tmp();
    store.declare_stream("cerebra/lattice", "t", None).unwrap();
    store.register_reducer("cerebra/**", SumReducer).unwrap();
    append_add(&store, "cerebra/lattice", 99);
    let s: SumState = store.read_state("cerebra/lattice", "main").unwrap();
    assert_eq!(s.total, 99);
}

#[test]
fn read_state_no_reducer_fails() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    append_add(&store, "test/s", 1);

    let result = store.read_state::<SumState>("test/s", "main");
    assert!(
        matches!(result, Err(Error::ReducerNotFound { .. })),
        "expected ReducerNotFound"
    );
}

#[test]
fn ambiguous_patterns_same_specificity_fails() {
    let (store, _dir) = open_tmp();
    store
        .register_reducer("cerebra/lattice/*", SumReducer)
        .unwrap();

    // Same specificity (2 literal segments) — ambiguous
    let result = store.register_reducer("cerebra/lattice/*", CatReducer);
    assert!(
        matches!(result, Err(Error::ReducerPatternAmbiguous { .. })),
        "expected ReducerPatternAmbiguous, got {:?}",
        result
    );
}

#[test]
fn more_specific_pattern_does_not_conflict() {
    let (store, _dir) = open_tmp();
    store
        .declare_stream("cerebra/lattice/abc", "t", None)
        .unwrap();
    store
        .declare_stream("cerebra/lattice/special", "t", None)
        .unwrap();

    // "cerebra/lattice/special" is more specific (3 literals) than "cerebra/lattice/*" (2).
    store
        .register_reducer("cerebra/lattice/*", SumReducer)
        .unwrap();
    store
        .register_reducer("cerebra/lattice/special", CatReducer)
        .unwrap();

    append_add(&store, "cerebra/lattice/abc", 5);
    store
        .append(Append {
            stream_id: "cerebra/lattice/special".to_string(),
            event_type: "Cat".to_string(),
            payload: serde_json::json!({"item": "hello"}),
            ..Default::default()
        })
        .unwrap();

    // "abc" matches the less-specific pattern.
    let sum_state: SumState = store.read_state("cerebra/lattice/abc", "main").unwrap();
    assert_eq!(sum_state.total, 5);

    // "special" matches the more-specific pattern.
    let cat_state: CatState = store.read_state("cerebra/lattice/special", "main").unwrap();
    assert_eq!(cat_state.items, vec!["hello"]);
}

// ── read_state ────────────────────────────────────────────────────────────────

#[test]
fn read_state_accumulates_multiple_events() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", SumReducer).unwrap();

    for v in 1..=5i64 {
        append_add(&store, "test/s", v);
    }

    let state: SumState = store.read_state("test/s", "main").unwrap();
    assert_eq!(state.count, 5);
    assert_eq!(state.total, 15); // 1+2+3+4+5
}

#[test]
fn read_state_empty_stream_returns_initial_state() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", SumReducer).unwrap();

    let state: SumState = store.read_state("test/s", "main").unwrap();
    assert_eq!(state.count, 0);
    assert_eq!(state.total, 0);
}

// ── read_state_at_version ─────────────────────────────────────────────────────

#[test]
fn read_state_at_version_partial() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", SumReducer).unwrap();

    for v in 1..=10i64 {
        append_add(&store, "test/s", v);
    }

    // Versions 0..=4 → values 1..=5 → sum=15
    let state: SumState = store.read_state_at_version("test/s", "main", 4).unwrap();
    assert_eq!(state.count, 5);
    assert_eq!(state.total, 15);
}

#[test]
fn read_state_at_version_zero() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", SumReducer).unwrap();

    for v in 1..=5i64 {
        append_add(&store, "test/s", v);
    }

    // Version 0 only: one event with value=1
    let state: SumState = store.read_state_at_version("test/s", "main", 0).unwrap();
    assert_eq!(state.count, 1);
    assert_eq!(state.total, 1);
}

#[test]
fn read_state_at_version_equals_full_read_at_last_version() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", SumReducer).unwrap();

    for v in 1..=5i64 {
        append_add(&store, "test/s", v);
    }

    let full: SumState = store.read_state("test/s", "main").unwrap();
    let at_v4: SumState = store.read_state_at_version("test/s", "main", 4).unwrap();
    assert_eq!(full, at_v4);
}

// ── snapshot-assisted read ────────────────────────────────────────────────────

#[test]
fn read_state_after_snapshot_uses_snapshot() {
    let (store, _dir) = open_tmp();
    store.declare_stream("test/s", "t", None).unwrap();
    store.register_reducer("test/s", SumReducer).unwrap();

    // Append 3 events and take a snapshot.
    for v in 1..=3i64 {
        append_add(&store, "test/s", v);
    }
    store.take_snapshot("test/s", "main").unwrap();

    // Append 2 more events.
    for v in 4..=5i64 {
        append_add(&store, "test/s", v);
    }

    let state: SumState = store.read_state("test/s", "main").unwrap();
    assert_eq!(state.count, 5);
    assert_eq!(state.total, 15);
}

// ── Panic isolation tests (SR-10 A-11) ────────────────────────────────────────

/// A reducer that panics on every apply call.
struct PanickingReducer;

impl Reducer for PanickingReducer {
    type State = SumState;
    type Event = AddEvent;

    const NAME: &'static str = "panicking_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> Self::State {
        SumState::default()
    }

    fn apply(&self, _state: Self::State, _event: &Self::Event) -> Self::State {
        panic!("deliberate test panic — SR-10 A-11")
    }
}

#[test]
fn panicking_reducer_returns_error_not_unwind() {
    let (store, _dir) = open_tmp();
    store.declare_stream("panic/stream", "test", None).unwrap();
    store
        .register_reducer("panic/stream", PanickingReducer)
        .unwrap();

    append_add(&store, "panic/stream", 42);

    let result: Result<SumState, Error> = store.read_state("panic/stream", "main");
    match result {
        Err(Error::ReducerPanicked {
            reducer_name,
            panic_message,
            ..
        }) => {
            assert_eq!(reducer_name, "panicking_reducer");
            assert!(
                panic_message.contains("deliberate test panic"),
                "panic_message should contain the original message, got: {panic_message}",
            );
        }
        other => panic!("expected ReducerPanicked, got: {other:?}"),
    }
}

#[test]
fn panicking_reducer_error_includes_event_id() {
    let (store, _dir) = open_tmp();
    store.declare_stream("panic/eventid", "test", None).unwrap();
    store
        .register_reducer("panic/eventid", PanickingReducer)
        .unwrap();

    let event_id = append_add_ret(&store, "panic/eventid", 1);

    let result: Result<SumState, Error> = store.read_state("panic/eventid", "main");
    match result {
        Err(Error::ReducerPanicked { event_id_hex, .. }) => {
            let expected_hex: String = event_id
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(
                event_id_hex, expected_hex,
                "event_id_hex must match the appended event's id"
            );
        }
        other => panic!("expected ReducerPanicked, got: {other:?}"),
    }
}

#[test]
fn non_panicking_reducer_unchanged() {
    let (store, _dir) = open_tmp();
    store.declare_stream("sum/stable", "test", None).unwrap();
    store.register_reducer("sum/stable", SumReducer).unwrap();

    for v in 1..=5i64 {
        append_add(&store, "sum/stable", v);
    }

    let state: SumState = store.read_state("sum/stable", "main").unwrap();
    assert_eq!(state.count, 5);
    assert_eq!(state.total, 15);
}
