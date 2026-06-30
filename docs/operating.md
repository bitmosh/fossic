# fossic — Operating Guide

**Version:** v1.8.1  
**Audience:** developers integrating fossic into a project, or running its test suite.

---

## Opening a store

```rust
use fossic::{Store, OpenOptions};

// Default options: WAL mode, 4 read connections, no encryption.
let store = Store::open("store.db", OpenOptions::default())?;

// All parent directories are created automatically under CreateIfMissing.
let store = Store::open("/data/my-app/events.db", OpenOptions::default())?;

// Require the file to already exist.
let store = Store::open("existing.db", OpenOptions {
    on_first_open: fossic::FirstOpenPolicy::RequireExisting,
    ..Default::default()
})?;
```

`Store` is `Clone + Send + Sync`. Clone it cheaply and share across threads — all concurrency is handled internally.

---

## Declaring streams

Streams must be declared before the first append. Undeclared appends return `Error::StreamNotDeclared`.

```rust
store.declare_stream("cerebra/lattice/session", "cerebra", Some("Lattice session events"))?;
store.declare_stream("policy-scout/audit", "policy-scout", None)?;

// Check existence
let exists = store.stream_exists("cerebra/lattice/session")?;

// List all
let streams: Vec<StreamInfo> = store.streams()?;
```

---

## Appending events

```rust
use fossic::Append;

// Single event
let event_id = store.append(Append {
    stream_id: "cerebra/lattice/session".into(),
    event_type: "InferenceStarted".into(),
    type_version: 1,
    payload: serde_json::json!({
        "model": "claude-opus-4",
        "session_id": "sess_abc123",
    }),
    causation_id: None,
    correlation_id: None,
    ..Default::default()
})?;

// Batch (single SQLite transaction)
let ids: Vec<EventId> = store.append_batch(&[
    Append { stream_id: "…".into(), event_type: "A".into(), .. Default::default() },
    Append { stream_id: "…".into(), event_type: "B".into(), .. Default::default() },
])?;

// Conditional append (compare-and-swap on stream version)
let id_opt = store.append_if(
    Append { stream_id: "my/stream".into(), .. Default::default() },
    |conn| {
        let v: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), -1) FROM events
             WHERE stream_id = ?1 AND branch = ?2",
            rusqlite::params!["my/stream", "main"],
            |r| r.get(0),
        )?;
        Ok(v == expected_version)
    },
)?;
// id_opt is None if the condition returned false (no write, no side effects).
```

**Payload discriminators.** Include a per-event-unique field if multiple events could have identical `(event_type, type_version, causation_id, payload)` — identical payloads under the same causation produce the same ID and silently collide. See `docs/gotchas.md` §1.

---

## Reading events

```rust
use fossic::ReadQuery;

// Range read — all events on stream "main" branch
let events = store.read_range(ReadQuery::stream("cerebra/lattice/session"))?;

// With bounds
let events = store.read_range(ReadQuery {
    stream_id: "cerebra/lattice/session".into(),
    branch: "main".into(),
    from_version: Some(10),
    to_version: Some(20),
    limit: Some(100),
    event_type_filter: Some("InferenceStarted".into()),
})?;

// Single event by ID
let event = store.read_one(event_id)?;

// By external ID
let event = store.read_by_external_id("my/stream", "ext-id-abc")?;

// Batch fetch by multiple IDs
let events = store.read_batch(&[id1, id2, id3])?; // keep batches ≤ 4096
```

Payloads are msgpack bytes. Decode with:

```rust
let payload: serde_json::Value = event.deserialize_payload_json()?;
// or typed:
let payload: MyPayload = event.deserialize_payload()?;
```

---

## Bounded reads (OOM-safe)

For streams that may be large, use the bounded variants which return `ReadOutcome<T>`:

```rust
use fossic::{ReadOutcome, TruncationCursor};

let outcome = store.read_range_bounded(
    ReadQuery::stream("cerebra/lattice/session"),
    Some(1_000),  // max events
    None,         // max bytes (None = no byte limit)
    None,         // cursor (None = start from beginning)
)?;

match outcome {
    ReadOutcome::Complete(events) => { /* all events returned */ }
    ReadOutcome::Truncated { data, cursor, reason } => {
        process(data);
        // Resume from cursor:
        if let Some(c) = cursor {
            let next = store.read_range_bounded(
                ReadQuery::stream("cerebra/lattice/session"),
                Some(1_000), None, Some(c)
            )?;
        }
    }
}
```

Or use streaming iterators (no cursor management needed; releases read connection between yields):

```rust
for result in store.read_range_iter(ReadQuery::stream("cerebra/lattice/session")) {
    let event = result?;
    process(event);
}
```

---

## Cross-stream queries

```rust
use fossic::WalkDirection;

// All events caused by a root event
let children = store.walk_causation(root_id, WalkDirection::Forward, 10)?;

// All events sharing a correlation ID
let related = store.read_by_correlation(correlation_id)?;

// Bounded BFS walk with sampling
use fossic::SamplingMode;
let outcome = store.walk_causation_bounded(
    root_id,
    WalkDirection::Forward,
    20,                                           // max_depth
    SamplingMode::BreadthFirst { max_per_level: 50 },
    Some(500),                                    // max_results
    None,                                         // max_bytes
    None,                                         // resume cursor
)?;
```

---

## Subscriptions

```rust
use fossic::{SubscribeQuery, SubscriptionMode};

struct MyHandler;
impl fossic::SubscriptionHandler for MyHandler {
    fn on_event(&self, event: &fossic::StoredEvent) {
        // process event
    }
}

// PostCommit: non-blocking, dedicated handler thread, bounded queue
let handle = store.subscribe(
    SubscribeQuery::stream("cerebra/lattice/session"),
    SubscriptionMode::PostCommit { queue_size: 500 },
    MyHandler,
)?;

// Synchronous: fires while write lock is held, before append() returns
let handle = store.subscribe(
    SubscribeQuery::stream("my/stream"),
    SubscriptionMode::Synchronous,
    MyHandler,
)?;

// Drop handle to unsubscribe
drop(handle);

// Check if degraded (queue overflow or handler panic)
if handle.is_degraded() {
    // re-subscribe from last known version
}

// Glob subscriptions
let handle = store.subscribe(
    SubscribeQuery {
        stream_pattern: "cerebra/**".into(),
        branch: "main".into(),
        include_system: false,
    },
    SubscriptionMode::PostCommit { queue_size: 1000 },
    MyHandler,
)?;

// Monitor system events (degradation signals, etc.)
let sys_handle = store.subscribe(
    SubscribeQuery {
        stream_pattern: "_fossic/system".into(),
        branch: "main".into(),
        include_system: true,  // required — system events filtered by default
    },
    SubscriptionMode::PostCommit { queue_size: 64 },
    SystemMonitor,
)?;
```

---

## Reducers and state

```rust
use serde::{Deserialize, Serialize};
use fossic::Reducer;

#[derive(Serialize, Deserialize, Clone)]
struct SessionState {
    inference_count: u64,
    last_model: Option<String>,
}

struct SessionReducer;
impl Reducer for SessionReducer {
    type State = SessionState;
    type Event = serde_json::Value;

    const NAME: &'static str = "session_reducer";
    const VERSION: u32 = 1;
    const STATE_SCHEMA_VERSION: u32 = 1;

    fn initial_state(&self) -> SessionState {
        SessionState { inference_count: 0, last_model: None }
    }

    fn apply(&self, mut state: SessionState, event: &serde_json::Value) -> SessionState {
        if let Some("InferenceStarted") = event["event_type"].as_str() {
            state.inference_count += 1;
            state.last_model = event["model"].as_str().map(String::from);
        }
        state
    }
}

// Register against a stream pattern
store.register_reducer("cerebra/lattice/*", SessionReducer)?;

// Or with an automatic snapshot policy
use fossic::SnapshotPolicy;
store.register_reducer_with_policy(
    "cerebra/lattice/*",
    SessionReducer,
    SnapshotPolicy::EveryNEvents(500),
)?;

// Read current state
let state: SessionState = store.read_state("cerebra/lattice/session", "main")?;

// Read state at a specific version
let state_at_10: SessionState =
    store.read_state_at_version("cerebra/lattice/session", "main", 10)?;

// Manual snapshot
let info = store.take_snapshot("cerebra/lattice/session", "main")?;

// GC snapshots for unregistered reducers
let deleted = store.gc_orphaned_snapshots()?;
```

---

## Branches

```rust
use fossic::CreateBranch;

// Create a branch diverging at version 42 on main
store.create_branch(&CreateBranch {
    stream_id: "cerebra/lattice/session".into(),
    branch_id: "what-if-b".into(),
    parent_id: "main".into(),
    parent_version: 42,
    description: Some("Counterfactual: strategy B taken at decision point".into()),
    alternatives: None,
})?;

// Append to the branch
store.append(Append {
    stream_id: "cerebra/lattice/session".into(),
    branch: "what-if-b".into(),
    event_type: "StrategyBSelected".into(),
    ..Default::default()
})?;

// Read from branch
let events = store.read_range(ReadQuery {
    stream_id: "cerebra/lattice/session".into(),
    branch: "what-if-b".into(),
    ..ReadQuery::stream("cerebra/lattice/session")
})?;

// Lifecycle management
store.promote_branch("cerebra/lattice/session", "what-if-b", Some("Selected as primary"))?;
store.mark_branch_dead_end("cerebra/lattice/session", "what-if-b", Some("Strategy rejected"))?;
```

---

## Consumer cursors

For consumers that process events sequentially and need to resume after restart:

```rust
// Save position
store.set_cursor("my-relay", "cerebra/lattice/session", "main", last_version)?;

// Resume from saved position
let resume_version = store.get_cursor("my-relay", "cerebra/lattice/session", "main")?;
let events = store.read_range(ReadQuery {
    stream_id: "cerebra/lattice/session".into(),
    branch: "main".into(),
    from_version: resume_version.map(|v| v + 1),
    ..ReadQuery::stream("cerebra/lattice/session")
})?;
```

---

## Upcasters (schema migration)

```rust
use fossic::Upcaster;

struct V1ToV2;
impl Upcaster for V1ToV2 {
    fn upcast(&self, payload: serde_json::Value) -> Result<serde_json::Value, fossic::Error> {
        // transform payload from version 1 to version 2 shape
        Ok(payload)
    }
}

store.register_upcaster("InferenceStarted", 1, 2, V1ToV2)?;
// All reads of InferenceStarted v1 events now transparently return v2 shape
```

---

## Deletion

```rust
// Purge a single event by ID (requires exact confirmation string)
store.purge_event(
    event_id,
    "I understand this breaks replay-from-zero",
    "reason for purge",
    "operator-id",
)?;

// Crypto-shred an entire stream (destroys the DEK; requires encryption enabled)
store.shred_stream("my/stream", "GDPR erasure request #123")?;
// Note: encryption is not implemented in v1; shred_stream is a no-op without it
```

---

## Observability

```rust
// Dispatch channel backlog
let pressure = store.dispatch_channel_pressure();
let peak = store.dispatch_channel_high_water_mark();

// Subscription queue depth and capacity
let depth = handle.queue_depth();     // None for Synchronous subscribers
let cap   = handle.queue_capacity();  // None for Synchronous subscribers

// Schedule a custom background task
use fossic::executor::{BacklogTask, TaskKind, TaskPriority};
store.schedule_task(BacklogTask {
    priority: TaskPriority::Low,
    deadline_us: 0,
    persist_on_drop: false,
    kind: TaskKind::Custom(std::sync::Arc::new(|| {
        // runs in fossic-bg thread during quiescent window
    })),
    recurring_interval: None,
});
```

---

## Running the test suite

Requires: Rust stable, Python 3.12+, Node.js 22, `just`.

```sh
# All three binding suites (Rust + Python + Node)
just test

# Single suite (faster during development)
just test-rust   # Rust workspace, all features
just test-py     # Python (maturin release build + pytest)
just test-node   # Node (npm build + vitest)
```

First run: ~2 minutes (Python venv setup, maturin release build, npm install).  
Subsequent runs: ~30 s (incremental compilation, cached deps).

Without `just`:

```sh
# Rust
cargo test --workspace --all-features

# Python (from repo root; creates .venv-test on first run)
python3 -m venv .venv-test
.venv-test/bin/pip install maturin pytest
cd fossic-py && ../.venv-test/bin/maturin develop --release
PYTHONPATH=fossic-py/python .venv-test/bin/pytest fossic-py/tests/ -v

# Node
cd fossic-node && npm install && npm run build && npm test
```

---

## OpenOptions reference

| Field | Default | Description |
|---|---|---|
| `encryption` | `Plaintext` | Encryption mode. `OsKeyring`/`EnvVar` return `NotImplemented` in v1. |
| `checkpoint_mode` | `Auto` | WAL checkpoint strategy. `Manual` returns `NotImplemented` in v1. |
| `on_first_open` | `CreateIfMissing` | Whether to create the store file if it doesn't exist. |
| `read_pool_size` | `4` | Number of read connections held in the pool. |
| `read_pool_timeout_ms` | `30000` | Pool acquisition timeout. `PoolExhausted` returned on timeout. |
| `default_max_results` | `None` | Store-wide event count ceiling for bounded reads. Per-call overrides this. |
| `default_max_bytes` | `None` | Store-wide byte ceiling for bounded reads. Per-call overrides this. |
| `reducer_state_large_threshold_bytes` | `1_048_576` (1 MiB) | Rolling-mean state threshold for `ReducerStateLarge` emission. Set to `usize::MAX` to disable. |
| `auto_gc_orphans` | `false` | GC orphaned snapshots at store drop time. |
| `background_executor_grace_timeout_ms` | `10_000` | Grace period for `fossic-bg` shutdown. |
| `executor_quiescence_window_ms` | `2_000` | Minimum quiet time before background tasks run. |
