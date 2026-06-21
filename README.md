# fossic

Local-first event sourcing library with content-addressed event identity.

Events are identified by a deterministic hash of `(event_type, type_version, causation_id, CCE(payload))`. Two identical events at the same causal position produce the same ID, giving idempotent append semantics without a distributed coordinator.

Storage is a single SQLite file with WAL mode. No daemon, no separate server.

## Crates in this workspace

| Crate | Path | Purpose |
|---|---|---|
| `fossic` | `.` | Rust core library |
| `fossic-py` | `fossic-py/` | PyO3 Python bindings |
| `fossic-node` | `fossic-node/` | napi-rs Node.js bindings |
| `fossic-tauri` | `crates/fossic-tauri/` | Tauri 2 IPC companion crate |

## Quick start (Rust)

```rust
use fossic::{Store, OpenOptions, Append};

let store = Store::open("store.db", OpenOptions::default())?;
store.declare_stream("my-app/events", "my-app", None)?;

let event_id = store.append(Append {
    stream_id: "my-app/events".into(),
    branch: "main".into(),
    event_type: "ThingHappened".into(),
    type_version: 1,
    payload: serde_json::json!({"key": "value"}),
    ..Default::default()
})?;
```

## Bounded reads and streaming iterators

### When to use which API

| API | Use when |
|---|---|
| `read_range`, `walk_causation`, etc. | Stream is bounded in practice (small, known size) or you need all results at once |
| `read_range_bounded`, `walk_causation_bounded`, etc. | Stream may be large or unbounded; you need OOM safety, pagination, or time-bounded queries |
| `read_range_iter`, `walk_causation_iter`, etc. | You process events one at a time (streaming ETL, aggregation, display) and don't need resume semantics |

The bounded and iter APIs are additive — existing unbounded call sites continue to work unchanged.

### ReadOutcome

Bounded reads return a `ReadOutcome<T>` enum:

```rust
pub enum ReadOutcome<T> {
    Complete(T),
    Truncated {
        data: T,
        cursor: Option<TruncationCursor>,
        reason: TruncationReason,   // ResultCount | ByteSize
    },
}
```

Check the variant before processing:

```rust
use fossic::{ReadQuery, SamplingMode, WalkDirection};

let outcome = store.read_range_bounded(
    ReadQuery::stream("cerebra/lattice/session_42"),
    Some(1000),   // max_results
    None,         // max_bytes (None = no byte limit)
    None,         // cursor (None = start from beginning)
)?;

match outcome {
    ReadOutcome::Complete(events) => {
        // Fewer than 1000 events — all data is here.
        process_all(events);
    }
    ReadOutcome::Truncated { data, cursor, reason } => {
        // 1000 events returned; more may remain.
        process_page(data);
        if let Some(c) = cursor {
            // Resume from where we left off.
            let next_page = store.read_range_bounded(
                ReadQuery::stream("cerebra/lattice/session_42"),
                Some(1000),
                None,
                Some(c),
            )?;
        }
    }
}
```

### TruncationCursor

Cursors are opaque bytes — do not interpret or construct them manually. Serialize with `cursor.as_bytes()` / `cursor.into_bytes()` for persistence; reconstruct with `TruncationCursor::from_bytes(bytes)`. A cursor from one query mode (range) is not valid for another (correlation); passing a mismatched cursor returns an error.

Store-level defaults apply when per-call limits are absent:

```rust
let store = Store::open("store.db", OpenOptions {
    default_max_results: Some(10_000),
    default_max_bytes: Some(50 * 1024 * 1024), // 50 MB
    ..Default::default()
})?;

// Per-call limit (500) takes precedence over default (10 000).
let page = store.read_range_bounded(ReadQuery::stream("s"), Some(500), None, None)?;

// No per-call limit — store default (10 000) applies.
let page = store.read_range_bounded(ReadQuery::stream("s"), None, None, None)?;
```

### Streaming iterators

Iterators implement `Iterator<Item = Result<StoredEvent>>` and `FusedIterator`. Each call to `next()` fetches a batch of 100 events from the store, releases the read-pool connection, and returns one event. **The connection is never held across a yield.** A pool of size 1 can serve concurrent readers while an iterator is live.

```rust
for event in store.read_range_iter(ReadQuery::stream("cerebra/lattice/session_42")) {
    let event = event?; // each item is Result<StoredEvent>
    process(event);
}

// Causation walk with sampling control:
for event in store.walk_causation_iter(
    root_id,
    WalkDirection::Forward,
    100,                         // max_depth
    SamplingMode::BreadthFirst { max_per_level: 50 },
) {
    let event = event?;
    // ...
}
```

Iterators do not support resume (cursor resumption). For resumable streaming, use `read_range_bounded` in a loop.

### SamplingMode

`walk_causation_bounded` and `walk_causation_iter` accept a `SamplingMode`:

| Mode | Behavior |
|---|---|
| `SamplingMode::Exhaustive` | Full BFS — returns every reachable node up to `max_depth`. Default. |
| `SamplingMode::BreadthFirst { max_per_level }` | BFS capped at `max_per_level` nodes per depth level. |
| `SamplingMode::Adaptive { target_count }` | Adjusts per-level cap dynamically to approach `target_count` total nodes. |

### aggregate_bounded

`aggregate_bounded` does not produce a cursor on truncation — fold-resume would require injecting partial aggregator state into a new instance, which the `Aggregate` trait does not yet support. If the result is truncated, the `cursor` field is always `None`. Full cursor-based resume for aggregates is v1.2.x work.

## Key concepts

- **Content-addressed IDs (CCE):** event identity is a deterministic function of content. Appending the same event twice returns the same ID and stores only one row.
- **Stream registry:** streams must be declared before append. Typos become errors at the point of mistake.
- **Subscription modes:** `Synchronous` (fires inside the write transaction) and `PostCommit` (fires on a dedicated thread after commit, with a bounded queue and degraded-state handling).
- **Branches:** lightweight pointer records — no event copying on branch creation.
- **Crypto-shredding:** per-stream DEKs allow GDPR-compliant deletion by destroying the key.

## Project Registration (for federated deployments)

Projects participating in a federated hub announce themselves by emitting
`ProjectRegistered` events to the `_fossic/system` stream. The canonical
`RelayAgent` in `fossic-py` does this automatically on startup. Projects
performing hub-direct writes (without a relay agent) should call
`store.emit_project_registered(...)` once before their first write.

```python
store.emit_project_registered(
    source_store="my-project",           # stable project identifier
    local_store_path="/path/to/store.db",
    subscribe_pattern="my-project/**",   # glob relayed to hub
    project_description="Human-readable description",
)
```

**Manual registration fields:**

| Field | Type | Description |
|---|---|---|
| `source_store` | `str` | Stable project identifier; used as hub stream namespace and `indexed_tags["source_store"]` on every relayed event. Changing this breaks hub stream names and causation routing. |
| `local_store_path` | `str` | Absolute path to this project's local fossic store file. |
| `subscribe_pattern` | `str` | Glob passed to `store.subscribe()` by the relay agent (e.g. `"cerebra/**"`). |
| `project_description` | `str` | Optional human-readable label; included in the event payload for coordinator display. |

`RelayAgent` also emits `RelayHeartbeat` events at a configurable interval
(default 5 s) so a hub coordinator can detect stalled relays:

```python
config = RelayConfig(
    local_store_path="/path/to/store.db",
    hub_store_path="/path/to/hub.db",
    source_prefix="my-project",
    subscribe_pattern="my-project/**",
    heartbeat_interval_s=5.0,         # default
    project_description="My project",
)
RelayAgent(config).run()
```

Both event types are written to `_fossic/system` with
`indexed_tags={"source_store":"<name>"}` so a future hub coordinator crate
(`fossic-coordinator`, see §15 of `docs/implement/FOSSIC_V1_SPEC.md`) can
efficiently filter by project. See §9.4 of the spec for the full event schema.

## Threading model

Fossic uses `std::thread` and `crossbeam-channel` — no async runtime required. See §14 of `docs/implement/FOSSIC_V1_SPEC.md` for the full threading model.

## Observability

The post-commit dispatch channel is the queue between the writer thread and subscription delivery. Under high write load, this queue can fill up. Two accessors let you monitor it:

```rust
// Current depth — how many events are queued but not yet delivered.
let depth = store.dispatch_channel_pressure();

// Historical peak since this Store instance was opened.
let peak = store.dispatch_channel_high_water_mark();
```

A rising `dispatch_channel_pressure()` without `SubscriptionDegraded` system events indicates back-pressure accumulating before the per-subscription queue. A nonzero `dispatch_channel_high_water_mark()` that never decreases is normal; a value that keeps climbing across restarts indicates a subscriber that processes slower than the write rate.

A Phase 3 PressureMonitor will automate back-pressure detection and adaptive queue management. Until then, monitor these signals from application instrumentation.

## Tests

```sh
just test
```

Runs Rust, Python, and Node binding tests and prints pass counts for each.
First run takes ~2 minutes (Python venv setup, maturin release build, npm install).
Subsequent runs are ~30 s (incremental compilation, cached deps).

For a single binding during development:

```sh
just test-rust   # Rust workspace (includes fossic-tauri integration tests)
just test-py     # Python (builds maturin extension, runs pytest)
just test-node   # Node (builds native module, runs vitest)
```

Without `just` installed, you can invoke each suite directly:

```sh
# Rust
cargo test --workspace --all-features

# Python (from repo root; .venv-test must exist with maturin + pytest)
cd fossic-py && ../.venv-test/bin/maturin develop --release
PYTHONPATH=fossic-py/python .venv-test/bin/pytest fossic-py/tests/ -v

# Node
cd fossic-node && npm install && npm run build && npm test
```

## License

MIT OR Apache-2.0
