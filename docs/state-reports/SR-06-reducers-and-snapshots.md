# SR-06 — Reducers and Snapshot System

**Series:** Fossic State Reports · Document 6 of 9
**Covers:** `src/reducers.rs`, `src/snapshots.rs`, `fossic-py/src/store.rs` (reducer/snapshot methods)
**Related:** SR-03 (event read path, upcasters), SR-04 (subscription cursor system), SR-07 (cross-stream aggregate queries)

---

## 1. Design Philosophy

Fossic reducers are stateless objects. The reducer struct itself carries no mutable fields and holds no state between calls. All accumulated state lives in `ReducerState`, a separately typed value that the store serializes to the `snapshots` table as msgpack bytes and deserializes on demand.

This separation is load-bearing:

- **Reducer objects are freely clonable and `Send + Sync + 'static`.** They can be placed in `Arc`, cloned across threads, and held for the lifetime of the store without any concern for mutation or lifecycle.
- **State accumulation is always reproducible from events.** Because `apply` is a pure function, you can replay every event from version 0 and arrive at the same state. No stored state is canonical; it is always a cache of a deterministic computation.
- **Snapshot caching is opt-in and explicit.** There is no background snapshot thread. You call `take_snapshot` when you decide the cost of full replay is too high. This keeps the hot append path free of snapshot overhead.
- **`State: Default` defines the zero state.** The fold starts from `State::default()` when no snapshot exists. There is no separate "initial state" factory.

The reducer system is the fold abstraction: given a stream of events in version order, produce a current-state value by applying a pure function once per event. It is equivalent to a left fold:

```
state = events.fold(State::default(), |s, e| reducer.apply(s, e))
```

The snapshot system is the memoization layer on top: instead of always folding from version 0, find the most recent snapshot and fold only the delta events since it.

---

## 2. Reducer Trait (Rust)

```rust
pub trait Reducer: Send + Sync + 'static {
    type State: ReducerState;

    const NAME: &'static str;
    const VERSION: u32;
    const STATE_SCHEMA_VERSION: u32;

    fn apply(state: &mut Self::State, event: &StoredEvent);
}

pub trait ReducerState: Default + Serialize + DeserializeOwned + Send + 'static {}
```

### 2.1 NAME

A string constant that is the snapshot lookup key. It must be unique across all reducers registered on a given store. If two distinct reducer types share the same `NAME`, they will overwrite each other's snapshots. The convention is to use kebab-case descriptive names:

```rust
const NAME: &'static str = "posture-reducer";
const NAME: &'static str = "session-signal-aggregator";
```

`NAME` is stored in the `snapshots.reducer_name` column. `snapshot_info(stream_id, branch, reducer_name)` uses it for lookup.

### 2.2 VERSION

The code version of the reducer logic. Increment when the `apply` function changes in a way that would produce different state from the same event sequence (backward-incompatible logic change).

`VERSION` is stored in `snapshots.reducer_version` for audit purposes. It does NOT affect snapshot lookup — only `NAME` and `STATE_SCHEMA_VERSION` determine whether an existing snapshot is eligible to be used.

If you change `apply` logic and do not change `VERSION`, old snapshots will be used as starting points and you will get inconsistent state (events before the snapshot were folded with old logic, events after are folded with new logic). The correct procedure for a logic change is:

1. Increment `VERSION`.
2. Call `gc_orphaned_snapshots` to discard snapshots made with the old logic (optional, but prevents stale starts).
3. Rebuild state via full replay or take a fresh snapshot.

### 2.3 STATE_SCHEMA_VERSION

The serialization schema version of `State`. Increment when the fields or structure of `ReducerState` change in a way that makes old msgpack bytes incompatible (added required fields, removed fields, renamed fields).

This is the key field for snapshot eligibility. `find_latest_snapshot` includes `AND state_schema_version = ?4` in its query. A snapshot written with `state_schema_version = 1` will never be returned when the reducer has `STATE_SCHEMA_VERSION = 2`. The old snapshots accumulate until `gc_orphaned_snapshots` is called.

When incrementing `STATE_SCHEMA_VERSION`, also increment `VERSION` (the logic to handle new state fields is new logic).

### 2.4 apply

```rust
fn apply(state: &mut Self::State, event: &StoredEvent);
```

Pure mutation. `state` is mutated in place. No return value. No I/O, no side effects, no panics that escape. Called once per event in ascending version order.

The `StoredEvent` received in `apply` has already had upcasters applied (if any are registered). The payload is msgpack bytes. The reducer is responsible for decoding:

```rust
fn apply(state: &mut Self::State, event: &StoredEvent) {
    match event.event_type.as_str() {
        "LockdownActivated" => {
            let payload: serde_json::Value = rmp_serde::from_slice(&event.payload)
                .expect("payload must be valid msgpack");
            state.locked_down = true;
            state.reason = payload["reason"].as_str().map(String::from);
        }
        _ => {}
    }
}
```

### 2.5 ReducerState bound

```rust
pub trait ReducerState: Default + Serialize + DeserializeOwned + Send + 'static {}
```

- `Default`: the zero state (before any events). `State::default()` must represent the correct initial state.
- `Serialize + DeserializeOwned`: for msgpack round-tripping via `rmp_serde`. Use `serde::Serialize` and `serde::Deserialize` derives.
- `Send + 'static`: so the store can pass state across threads when needed.

A minimal implementation:

```rust
#[derive(Default, Serialize, Deserialize)]
struct PostureState {
    locked_down: bool,
    reason: Option<String>,
    activated_at_us: Option<i64>,
}

impl ReducerState for PostureState {}
```

---

## 3. DynReducer Trait (Python and Foreign Bridges)

When a language binding needs to register a reducer without compile-time type information, it implements `DynReducer`:

```rust
pub trait DynReducer: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> u32;
    fn state_schema_version(&self) -> u32;
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error>;
    fn apply_bytes(&self, state_bytes: &[u8], event_payload: &[u8]) -> Result<Vec<u8>, Error>;
}
```

`initial_state_bytes` returns the msgpack encoding of the initial state (equivalent to `State::default()` serialized). `apply_bytes` receives the current state as msgpack bytes and the event payload as msgpack bytes, and must return the new state as msgpack bytes.

### 3.1 Type Erasure Wrappers

The store works internally with `BoxedReducer`:

```rust
pub(crate) type BoxedReducer = Box<dyn ErasedReducer>;
```

where `ErasedReducer` is the object-safe interface derived from `Reducer` via the `ErasedReducer` blanket impl. The concrete flow:

- Rust `Reducer<State = S>` → `ErasedReducer` (blanket impl, calls `S::default()` for initial state)
- `Box<dyn DynReducer>` → `DynReducerAdapter` (newtype that implements `ErasedReducer` by delegating to `initial_state_bytes`/`apply_bytes`)
- Both paths produce a `BoxedReducer` stored in `ReducerRegistry`

The caller never sees `ErasedReducer` or `DynReducerAdapter`. The store API exposes:

```rust
store.register_reducer(pattern, my_rust_reducer)?;      // -> BoxedReducer via ErasedReducer
store.register_dyn_reducer(pattern, my_dyn_reducer)?;   // -> BoxedReducer via DynReducerAdapter
```

---

## 4. Pattern-Based Registry

Reducers are not registered per stream. They are registered per stream pattern:

```rust
store.register_reducer("cerebra/agent-trace/*", CerebraSessionReducer)?;
store.register_dyn_reducer("policy-scout/**", py_reducer_adapter)?;
```

All reads of state (`read_state`, `read_state_at_version`) and all snapshot operations (`take_snapshot`, `gc_orphaned_snapshots`) resolve the correct reducer by matching `stream_id` against registered patterns.

### 4.1 Pattern Matching

Uses the same glob system as subscriptions (see SR-04):

- `*` — matches exactly one path segment (no `/`).
- `**` — matches zero or more path segments.
- Literal segments match exactly.

Pattern `cerebra/agent-trace/*` matches `cerebra/agent-trace/sess-abc123` (three segments, last is wildcard). Pattern `policy-scout/**` matches `policy-scout`, `policy-scout/posture`, `policy-scout/audit/session-xyz`.

### 4.2 Specificity Score

```rust
fn specificity_score(pattern: &str) -> usize {
    pattern.split('/').take_while(|seg| *seg != "*" && *seg != "**").count()
}
```

The specificity score is the count of leading literal (non-wildcard) segments. Examples:

| Pattern | Score |
|---|---|
| `cerebra/agent-trace/*` | 2 |
| `cerebra/**` | 1 |
| `*/agent-trace` | 0 |
| `**` | 0 |
| `policy-scout/posture` | 2 |

When multiple patterns match a stream, the one with the highest score wins. This allows a general catch-all (`**`, score 0) alongside specific overrides (`cerebra/agent-trace/*`, score 2).

### 4.3 ReducerPatternAmbiguous

At registration time, the new pattern is checked against all existing patterns for potential ambiguity. Two patterns are ambiguous if:

1. They have the same specificity score, AND
2. They *could* match the same stream (their patterns overlap)

The overlap check (`patterns_may_overlap`) uses a recursive descent that tests whether two patterns can produce the same matched stream. The check is conservative — it flags patterns that might overlap even if no actual stream with that name exists yet.

```rust
Error::ReducerPatternAmbiguous { a: "cerebra/*".into(), b: "*/agent-trace".into() }
```

Example triggering case: `cerebra/*` (score 1) and `*/agent-trace` (score 1) both match `cerebra/agent-trace`. Even though neither is declared yet to handle that stream, both could, so registration of the second fails.

This error is returned from `register_reducer` / `register_dyn_reducer` at registration time, not at query time. It prevents the "which reducer wins" ambiguity from surfacing at read time.

### 4.4 ReducerNotFound and ReducerNotFoundByName

`Error::ReducerNotFound { stream_id }` — no registered pattern matches the given `stream_id`. Returned by `read_state`, `take_snapshot`.

`Error::ReducerNotFoundByName { name }` — looking up by name (e.g. for `gc_orphaned_snapshots` internal bookkeeping) finds no reducer with that name. Not typically surfaced to callers directly.

---

## 5. Snapshot Schema

```sql
CREATE TABLE snapshots (
    stream_id            TEXT    NOT NULL,
    branch               TEXT    NOT NULL DEFAULT 'main',
    version              INTEGER NOT NULL,
    reducer_name         TEXT    NOT NULL,
    reducer_version      INTEGER NOT NULL DEFAULT 1,
    state_schema_version INTEGER NOT NULL DEFAULT 1,
    state_blob           BLOB    NOT NULL,
    created_at           INTEGER NOT NULL,
    PRIMARY KEY (stream_id, branch, reducer_name, state_schema_version, version)
);

CREATE INDEX idx_snapshots_lookup
    ON snapshots(stream_id, branch, reducer_name, state_schema_version, version DESC);
```

### 5.1 Column details

- **stream_id / branch**: the stream this snapshot covers.
- **version**: the event version the snapshot was taken at. A snapshot at version N represents the state after folding all events from 0 to N inclusive.
- **reducer_name**: the `Reducer::NAME` constant. The snapshot lookup key.
- **reducer_version**: the `Reducer::VERSION` at time of snapshot. Stored for audit; not used in lookup queries.
- **state_schema_version**: the `Reducer::STATE_SCHEMA_VERSION` at time of snapshot. **Used in lookup queries.** A snapshot written with schema version 1 will never be returned when the reducer declares version 2.
- **state_blob**: the msgpack-encoded state. Deserializes to `Reducer::State` (Rust) or a Python dict (Python bindings).
- **created_at**: microseconds since Unix epoch when the snapshot was written.

### 5.2 Primary Key Design

The PK is `(stream_id, branch, reducer_name, state_schema_version, version)`. Including `state_schema_version` in the PK means multiple snapshots for the same stream at the same version can coexist if they were taken by reducers with different schema versions. This is the correct behavior during a schema migration: the old snapshot (schema 1) is not overwritten when a new one (schema 2) is written.

### 5.3 Index for Latest Lookup

The index is `version DESC` so that `ORDER BY version DESC LIMIT 1` returns the most recent snapshot in the first row without a full scan. Without this index, a stream with thousands of snapshots would require scanning all of them.

---

## 6. find_latest_snapshot

```rust
pub(crate) fn find_latest_snapshot(
    conn: &Connection,
    stream_id: &str,
    branch: &str,
    reducer_name: &str,
    state_schema_version: u32,
    max_version: Option<u64>,
) -> Result<Option<(u64, Vec<u8>)>, Error>
```

Returns `Option<(version, state_blob)>`.

Two SQL variants are used depending on whether `max_version` is provided:

**Without max_version** (used by `read_state`):

```sql
SELECT version, state_blob FROM snapshots
WHERE stream_id = ?1 AND branch = ?2
  AND reducer_name = ?3 AND state_schema_version = ?4
ORDER BY version DESC LIMIT 1
```

**With max_version** (used by `read_state_at_version`):

```sql
SELECT version, state_blob FROM snapshots
WHERE stream_id = ?1 AND branch = ?2
  AND reducer_name = ?3 AND state_schema_version = ?4
  AND version <= ?5
ORDER BY version DESC LIMIT 1
```

The `max_version` bound ensures that a future snapshot (version > requested version) does not contaminate a historical query. For example, if you ask for state at version 50 and a snapshot exists at version 100, the version 100 snapshot is ignored, and the query falls back to either an older snapshot or a full replay from version 0.

---

## 7. write_snapshot

```rust
pub(crate) fn write_snapshot(
    conn: &Connection,
    stream_id: &str,
    branch: &str,
    version: u64,
    reducer_name: &str,
    reducer_version: u32,
    state_schema_version: u32,
    state_blob: &[u8],
) -> Result<SnapshotInfo, Error>
```

SQL:

```sql
INSERT OR REPLACE INTO snapshots
(stream_id, branch, version, reducer_name, reducer_version,
 state_schema_version, state_blob, created_at)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
```

The `OR REPLACE` clause means: if a row with the same primary key `(stream_id, branch, reducer_name, state_schema_version, version)` already exists, replace it. This makes repeated `take_snapshot` calls at the same version idempotent — the second call overwrites the first with an identical result (since the computation is deterministic).

Returns `SnapshotInfo`:

```rust
pub struct SnapshotInfo {
    pub stream_id: String,
    pub branch: String,
    pub version: u64,
    pub reducer_name: String,
    pub reducer_version: u32,
    pub state_schema_version: u32,
    pub created_at: i64,
}
```

`created_at` is `now_us()` at write time — microseconds since Unix epoch.

---

## 8. take_snapshot — The Full Flow

```rust
store.take_snapshot(stream_id, branch)?; // -> SnapshotInfo
```

Step-by-step:

**Step 1: Resolve reducer**
Look up the registered reducer for `stream_id` via pattern matching. Returns `ReducerNotFound` if no pattern matches.

**Step 2: Acquire read connection, find latest snapshot**
`conn = acquire_read()?`  
`find_latest_snapshot(conn, stream_id, branch, reducer.name(), reducer.state_schema_version(), None)`  
→ `Option<(u64, Vec<u8>)>`

If found: the starting state is deserialized from `state_blob`. `start_version = snapshot_version + 1`.  
If not found: the starting state is `initial_state_bytes()` (the serialized `State::default()`). `start_version = 0`.

The `ReadGuard` drops here; the read connection returns to the pool.

**Step 3: Acquire read connection, read delta events**
`read_range(stream_id, branch, from_version=start_version)`

Returns all events from `start_version` through the latest version on the stream. If there are no events (the stream is empty or has no events after the last snapshot), returns `Error::NoEventsToSnapshot`.

The read connection returns to the pool.

**Step 4: Fold events**
For each event in version order:
- Deserialize current state from msgpack bytes.
- Call `reducer.apply_bytes(state_bytes, event.payload)`.
- Update state bytes.

This is the potentially expensive step for long streams. No lock is held during folding.

**Step 5: Acquire write connection, write snapshot**
`conn = acquire_write()?`  
`write_snapshot(conn, stream_id, branch, latest_event.version, ...)`

Returns `SnapshotInfo`.

### 8.1 TD-001: Race Window

Between step 2 (reading the last snapshot) and step 5 (writing the new snapshot), there is a window where a concurrent `take_snapshot` call on the same `(stream_id, branch)` can run. Both will:

1. Read the same "last snapshot" in step 2.
2. Read the same delta events in step 3.
3. Fold independently in step 4.
4. Write snapshots in step 5 — the second write wins (OR REPLACE).

The result is still correct: both produces an identical deterministic state (same events folded through the same pure function). The problem is wasted work — both threads fold the same events. There is no data corruption.

**Why this is acceptable:** The alternative is to hold the write lock for steps 2–5 (the entire fold computation). For a stream with 100,000 events and a complex reducer, this could block all writes for seconds. The race window accepts duplicated CPU work in exchange for keeping the write path unblocked.

**Mitigation patterns:**
- Call `take_snapshot` from a single background thread. If only one goroutine/thread owns snapshot responsibility, the race never triggers.
- The `OR REPLACE` ensures the snapshot table is always consistent even if both writes land.

---

## 9. read_state — Snapshot + Delta Fold

```rust
// Rust
let state_bytes = store.read_state_bytes(stream_id, branch)?;
let state: MyState = rmp_serde::from_slice(&state_bytes)?;

// Python
state = store.read_state(stream_id, branch)  # returns dict
```

Full algorithm:

1. **Resolve reducer** — pattern match `stream_id` → `BoxedReducer`.
2. **Find latest snapshot** — `find_latest_snapshot(None)`.
3. **Determine start version**:
   - Snapshot found: `start_version = snapshot.version + 1`, initial bytes = `snapshot.state_blob`.
   - No snapshot: `start_version = 0`, initial bytes = `reducer.initial_state_bytes()`.
4. **Read delta** — `read_range(stream_id, branch, from_version=start_version)`. Empty result is fine (state is the snapshot itself, or default if no snapshot and no events).
5. **Fold** — for each event: `state_bytes = reducer.apply_bytes(state_bytes, event.payload)?`.
6. **Return** — the final `state_bytes` (msgpack). Python layer decodes to dict via `rmp_serde → serde_json → json.loads`.

### 9.1 read_state_at_version

```rust
store.read_state_bytes_at_version(stream_id, branch, version: u64)?;
// Python: store.read_state_at_version(stream_id, branch, version)
```

Same as `read_state` with two differences:
- `find_latest_snapshot` is called with `max_version = version` — ensures no snapshot beyond the target version is used.
- `read_range` is called with `to_version = version` — ensures no event beyond the target version is folded.

Returns the state as it would be immediately after the event at exactly `version` was applied.

### 9.2 snapshot_info

```rust
store.snapshot_info(stream_id, branch, reducer_name)?; // -> Option<SnapshotInfo>
```

SQL:

```sql
SELECT version, reducer_version, state_schema_version, created_at FROM snapshots
WHERE stream_id = ?1 AND branch = ?2 AND reducer_name = ?3
ORDER BY version DESC LIMIT 1
```

Note: this query does NOT filter by `state_schema_version`. It returns the most recent snapshot regardless of schema version. Useful for diagnostics ("when was the last snapshot taken and at what version?") without needing to know the schema version.

---

## 10. gc_orphaned_snapshots

```rust
store.gc_orphaned_snapshots()?; // -> usize (rows deleted)
```

Deletes all snapshot rows whose `(reducer_name, state_schema_version)` combination is not present in the currently registered reducer set.

Algorithm:

1. Collect `active: Vec<(String, u32)>` — the `(name, state_schema_version)` for each registered reducer.
2. If `active` is empty: `DELETE FROM snapshots` — all snapshots are orphaned.
3. Otherwise:
   - `SELECT DISTINCT reducer_name, state_schema_version FROM snapshots` — the existing pairs.
   - For each pair not in `active`: `DELETE FROM snapshots WHERE reducer_name = ?1 AND state_schema_version = ?2`.
4. Return total rows deleted.

### 10.1 When to Call

- After bumping `STATE_SCHEMA_VERSION` on a reducer: old schema snapshots are no longer returned by `find_latest_snapshot` (they fail the `state_schema_version` filter), but they still occupy space. `gc_orphaned_snapshots` reclaims that space.
- After removing a reducer registration entirely: snapshots for that reducer name accumulate forever if not GC'd.
- Do NOT call before registering any reducers — the empty `active` set causes all snapshots to be deleted.

### 10.2 Example

Before GC: reducer `posture-reducer` was schema version 1, now bumped to version 2.

```sql
-- Before GC
snapshots table contains:
  (stream_id="policy-scout/posture", reducer_name="posture-reducer", state_schema_version=1, ...)  -- old
  (stream_id="policy-scout/posture", reducer_name="posture-reducer", state_schema_version=2, ...)  -- new

-- After gc_orphaned_snapshots (posture-reducer v2 is registered, v1 is not):
-- The v1 row is deleted.
-- Returns 1 (rows deleted).
```

---

## 11. Python Reducer Protocol

The Python binding for reducer registration (`PyStore.register_reducer`) wraps a Python object into `PyDynReducer`, which bridges to `DynReducer`. The Python object must implement:

```python
class MyReducer:
    name: str = "my-reducer"           # Attribute (or property)
    version: int = 1                   # Attribute (or property)
    state_schema_version: int = 1      # Attribute (or property)

    def initial_state(self) -> dict:
        """Return the initial state dict."""
        return {"count": 0, "last_event_type": None}

    def apply(self, state: dict, event_payload: dict) -> dict:
        """Return the new state dict."""
        return {
            "count": state["count"] + 1,
            "last_event_type": event_payload.get("type"),
        }
```

### 11.1 Registration

```python
store.register_reducer("cerebra/agent-trace/*", MyReducer())
```

At registration time, the Python bridge reads `name`, `version`, `state_schema_version` via `getattr`. Missing attributes raise `PyAttributeError`.

### 11.2 Type Mapping Path

The Python bridge uses a double-serialization path through JSON:

**initial_state_bytes:**
1. Call `reducer.initial_state()` → Python dict.
2. Serialize to JSON string via `json.dumps`.
3. Parse JSON string to `serde_json::Value`.
4. Serialize to msgpack via `rmp_serde::to_vec_named`.

**apply_bytes(state_bytes, event_payload):**
1. Deserialize `state_bytes` (msgpack) → `serde_json::Value`.
2. Serialize to JSON string, parse via `json.loads` → Python dict.
3. Deserialize `event_payload` (msgpack) → `serde_json::Value`.
4. Serialize to JSON string, parse via `json.loads` → Python dict.
5. Call `reducer.apply(state_dict, event_dict)` → Python dict.
6. Serialize result via `json.dumps` → JSON string → `serde_json::Value` → msgpack.

The double serialization (msgpack → JSON → Python → JSON → msgpack) is the cost of crossing the language boundary. For high-frequency reducers on long streams, this overhead is measurable.

### 11.3 Limitation: take_snapshot Does Not Work with Python Reducers

The Rust `store.take_snapshot(stream_id, branch)` method resolves the reducer via `ReducerRegistry`, which holds `BoxedReducer` (Rust-side type-erased reducers). Python reducers registered via `store.register_reducer(pattern, py_obj)` are registered in a separate Python-side registry and are not visible to the Rust `ReducerRegistry`.

Consequence: calling `store.take_snapshot(stream_id, branch)` for a stream whose only registered reducer is a Python reducer returns `ReducerNotFound`.

**Workaround:** Python consumers with large streams must either:

1. Implement snapshot caching externally (store state in another file or stream).
2. Minimize the number of events per stream (keep streams short enough that full replay is acceptable).
3. Write a Rust reducer and expose it via PyO3 (not trivial, but provides full snapshot support).

This limitation is tracked as a known gap.

---

## 12. Practical Patterns

### Pattern: Register and read state

```python
class SessionReducer:
    name = "session-state"
    version = 1
    state_schema_version = 1

    def initial_state(self):
        return {"step_count": 0, "last_signal": None, "clutch": False}

    def apply(self, state, payload):
        state = dict(state)  # avoid mutating the input dict
        match payload.get("__type"):
            case "StepStarted":
                state["step_count"] += 1
            case "ClutchDecisionMade":
                state["clutch"] = payload.get("clutch_active", False)
            case "SignalEvaluated":
                state["last_signal"] = payload.get("signal_name")
        return state

store.register_reducer("cerebra/agent-trace/*", SessionReducer())
state = store.read_state("cerebra/agent-trace/sess-abc123", "main")
print(f"Steps: {state['step_count']}, Clutch: {state['clutch']}")
```

### Pattern: Historical state query

```python
# What was the session state after exactly 10 events?
state_at_10 = store.read_state_at_version("cerebra/agent-trace/sess-abc123", "main", 9)
# version is 0-indexed; version 9 = 10th event
```

### Pattern: Snapshot check before full read

```python
def read_state_with_freshness_check(store, stream_id, branch, max_staleness_events=1000):
    info = store.snapshot_info(stream_id, branch, "session-state")
    if info is None:
        # No snapshot — full replay. Check if stream is long.
        events = store.read_range(ReadQuery(stream_id=stream_id, branch=branch))
        if len(events) > max_staleness_events:
            print(f"Warning: {len(events)} events, no snapshot — take one after this read")
    return store.read_state(stream_id, branch)
```

### Pattern: GC after schema bump

```rust
// In Rust: bump STATE_SCHEMA_VERSION from 1 to 2
impl Reducer for PostureReducer {
    const NAME: &'static str = "posture-reducer";
    const VERSION: u32 = 2;          // bumped
    const STATE_SCHEMA_VERSION: u32 = 2;  // bumped
    // ...
}

// After re-registering the reducer, GC old snapshots
let deleted = store.gc_orphaned_snapshots()?;
println!("Deleted {} stale snapshots", deleted);
```

### Pattern: Avoid gc_orphaned_snapshots before registration

```python
# WRONG: this deletes all snapshots
store.gc_orphaned_snapshots()       # called before register_reducer
store.register_reducer("**", r)    # now registered, but snapshots are gone

# CORRECT
store.register_reducer("**", r)
store.gc_orphaned_snapshots()       # now only non-registered snapshots are deleted
```

### Pattern: Concurrent read_state without snapshot thrashing

If multiple threads call `read_state` on the same stream concurrently without a snapshot, each thread does a full replay independently. This is correct but wasteful. The solution is to take a snapshot after a full replay:

```python
import threading

_snapshot_lock = threading.Lock()

def read_state_and_snapshot(store, stream_id, branch):
    state = store.read_state(stream_id, branch)
    with _snapshot_lock:
        # Only one thread takes a snapshot; others will benefit next time.
        # (Rust-only: Python reducers can't use take_snapshot)
        pass
    return state
```

---

## 13. Error Reference

| Error | When |
|---|---|
| `ReducerNotFound { stream_id }` | No registered pattern matches `stream_id` in `read_state`, `take_snapshot`. |
| `ReducerNotFoundByName { name }` | Looking up by reducer name finds nothing. |
| `ReducerPatternAmbiguous { a, b }` | Registration of a pattern that overlaps an existing same-specificity pattern. |
| `ReducerError { message }` | The `apply` function returned `Err`, or the Python callable raised an exception. |
| `NoEventsToSnapshot { stream_id, branch }` | `take_snapshot` called on a stream with no events since the last snapshot. |
| `MsgpackDecode(...)` | State blob or event payload fails msgpack deserialization (schema mismatch). |
| `MsgpackEncode(...)` | State fails msgpack serialization in Python bridge. |

The most common operational error is `MsgpackDecode` after bumping `STATE_SCHEMA_VERSION` without calling `gc_orphaned_snapshots`: the store finds a snapshot with the old schema and tries to deserialize it into the new `State` type, which fails if the field layout changed. Always GC after a schema version bump.

---

## 14. Design Decisions and Rationale

### Why patterns, not per-stream registration?

In a system with hundreds of streams (e.g. `cerebra/agent-trace/{session_id}` for each session), registering a reducer per stream would require knowing the stream IDs at startup. Pattern-based registration lets one reducer handle all `cerebra/agent-trace/*` streams without any stream-ID bookkeeping.

### Why stateless reducers?

Stateful reducers (where the reducer holds mutable state) require synchronization and have lifecycle implications (when to reset, how to clone). Stateless reducers compose trivially: you can register the same reducer type for different patterns without any shared-state concern.

### Why are Python reducers excluded from take_snapshot?

`take_snapshot` was designed for the Rust `ReducerRegistry`. Extending it to the Python side would require the Rust `take_snapshot` path to acquire the Python GIL mid-computation (breaking the GIL-free write path) or the Python layer to re-implement the entire fold-and-write loop. Neither was acceptable for v1. The gap is documented and deferred.

### Why INSERT OR REPLACE in write_snapshot?

The alternative (INSERT OR IGNORE) would silently ignore a second snapshot at the same version, which could mask a reducer version upgrade that produces the same version number but different state bytes. OR REPLACE ensures the latest computation always wins. Since the computation is deterministic (same events in same order), OR REPLACE and OR IGNORE produce the same state bytes anyway.
