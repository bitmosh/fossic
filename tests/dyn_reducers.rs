use fossic::{Append, DynReducer, Error, OpenOptions, Store};
use tempfile::tempdir;

// A simple counter reducer implemented as a DynReducer.
// State: msgpack-encoded { "count": u64 }
struct CounterDynReducer;

impl DynReducer for CounterDynReducer {
    fn name(&self) -> &str {
        "counter-dyn"
    }
    fn version(&self) -> u32 {
        1
    }
    fn state_schema_version(&self) -> u32 {
        1
    }
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error> {
        rmp_serde::to_vec_named(&serde_json::json!({"count": 0})).map_err(Error::MsgpackEncode)
    }
    fn apply_bytes(&self, state_bytes: &[u8], _event_payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut state: serde_json::Value =
            rmp_serde::from_slice(state_bytes).map_err(Error::MsgpackDecode)?;
        let count = state["count"].as_u64().unwrap_or(0) + 1;
        state["count"] = serde_json::json!(count);
        rmp_serde::to_vec_named(&state).map_err(Error::MsgpackEncode)
    }
}

fn open_tmp() -> (Store, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.fossic");
    let store = Store::open(&path, OpenOptions::default()).unwrap();
    (store, dir)
}

// Each call produces a unique CCE hash by including a distinct sequence number.
fn append_event(store: &Store, stream_id: &str, seq: u32) {
    store
        .append(Append {
            stream_id: stream_id.to_string(),
            event_type: "test.event".to_string(),
            payload: serde_json::json!({"seq": seq}),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn register_dyn_reducer_and_read_state() {
    let (store, _dir) = open_tmp();
    store
        .register_dyn_reducer("dyn/**", Box::new(CounterDynReducer))
        .unwrap();
    store.declare_stream("dyn/counter", "test", None).unwrap();

    for i in 0..3 {
        append_event(&store, "dyn/counter", i);
    }

    let state: serde_json::Value = store.read_state("dyn/counter", "main").unwrap();
    assert_eq!(state["count"], 3);
}

#[test]
fn read_state_bytes_returns_raw_msgpack() {
    let (store, _dir) = open_tmp();
    store
        .register_dyn_reducer("dyn/**", Box::new(CounterDynReducer))
        .unwrap();
    store.declare_stream("dyn/raw", "test", None).unwrap();
    append_event(&store, "dyn/raw", 0);

    let bytes = store.read_state_bytes("dyn/raw", "main").unwrap();
    let value: serde_json::Value = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(value["count"], 1);
}

#[test]
fn read_state_at_version_with_reducer_name() {
    let (store, _dir) = open_tmp();
    store
        .register_dyn_reducer("dyn/**", Box::new(CounterDynReducer))
        .unwrap();
    store.declare_stream("dyn/versioned", "test", None).unwrap();

    for i in 0..5 {
        append_event(&store, "dyn/versioned", i);
    }

    // Events are 0-indexed (first event = v0). to_version=2 includes v0,v1,v2 = 3 events.
    let state: serde_json::Value = store
        .read_state_at_version_with_reducer("dyn/versioned", "main", 2, "counter-dyn")
        .unwrap();
    assert_eq!(state["count"], 3);
}

#[test]
fn reducer_not_found_by_name_error() {
    let (store, _dir) = open_tmp();
    let result = store.read_state_at_version_with_reducer::<serde_json::Value>(
        "any/stream",
        "main",
        1,
        "nonexistent",
    );
    assert!(matches!(result, Err(Error::ReducerNotFoundByName { .. })));
}

#[test]
fn dyn_and_static_reducers_coexist() {
    use fossic::Reducer;

    #[derive(Clone)]
    struct StaticCounter;

    impl Reducer for StaticCounter {
        type State = serde_json::Value;
        type Event = serde_json::Value;
        const NAME: &'static str = "static-counter";
        const VERSION: u32 = 1;
        const STATE_SCHEMA_VERSION: u32 = 1;
        fn initial_state(&self) -> Self::State {
            serde_json::json!({"count": 0})
        }
        fn apply(&self, mut state: Self::State, _event: &Self::Event) -> Self::State {
            let count = state["count"].as_u64().unwrap_or(0) + 1;
            state["count"] = serde_json::json!(count);
            state
        }
    }

    // Use single-star patterns to avoid the conservative ** overlap check.
    let (store, _dir) = open_tmp();
    store.register_reducer("static/*", StaticCounter).unwrap();
    store
        .register_dyn_reducer("dyn/*", Box::new(CounterDynReducer))
        .unwrap();

    store.declare_stream("static/events", "test", None).unwrap();
    store.declare_stream("dyn/events", "test", None).unwrap();

    // Use distinct seq values to avoid CCE dedup across streams (CCE hashes payload+type, not stream_id).
    append_event(&store, "static/events", 100);
    append_event(&store, "dyn/events", 200);

    let s1: serde_json::Value = store.read_state("static/events", "main").unwrap();
    let s2: serde_json::Value = store.read_state("dyn/events", "main").unwrap();

    assert_eq!(s1["count"], 1);
    assert_eq!(s2["count"], 1);
}

#[test]
fn take_snapshot_works_with_dyn_reducer() {
    let (store, _dir) = open_tmp();
    store
        .register_dyn_reducer("dyn/**", Box::new(CounterDynReducer))
        .unwrap();
    store.declare_stream("dyn/snap", "test", None).unwrap();

    for i in 0..4 {
        append_event(&store, "dyn/snap", i);
    }

    store.take_snapshot("dyn/snap", "main").unwrap();

    append_event(&store, "dyn/snap", 4);

    let state: serde_json::Value = store.read_state("dyn/snap", "main").unwrap();
    assert_eq!(state["count"], 5);
}
