# SR-09 — Python Bindings Deep-Dive

**Series:** Fossic State Reports · Document 9 of 9
**Covers:** `fossic-py/src/` — the PyO3 bridge between the Rust core and the Python API surface.
**Companion docs:** SR-03 (event lifecycle), SR-04 (subscriptions), SR-06 (reducers), SR-08 (schema evolution and errors).

---

## 1. Module Architecture

The Python package has two layers:

**`_fossic`** — the native extension module, compiled from Rust via PyO3. Contains every class, free function, and exception type. The leading underscore marks it as an internal implementation detail; callers should not import from it directly.

**`fossic`** — the public Python package. Re-exports everything from `_fossic` and adds pure-Python convenience wrappers — most notably the context manager and iteration protocol for subscriptions.

The native module is registered in `fossic-py/src/lib.rs`:

```rust
#[pymodule]
fn _fossic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Classes
    m.add_class::<PyStore>()?;
    m.add_class::<PyEventId>()?;
    m.add_class::<PyStoredEvent>()?;
    m.add_class::<PyAppend>()?;
    m.add_class::<PyReadQuery>()?;
    m.add_class::<PyOpenOptions>()?;
    m.add_class::<PyStreamInfo>()?;
    m.add_class::<PyBranchInfo>()?;
    m.add_class::<PyBranchSegment>()?;
    m.add_class::<PyCreateBranch>()?;
    m.add_class::<PySnapshotInfo>()?;
    m.add_class::<PySubscriptionMode>()?;
    m.add_class::<PyAggregateQuery>()?;
    m.add_class::<PyRawSubscriptionHandle>()?;

    // Exception hierarchy
    register_errors(m)?;

    // CCE encoding (testing / tooling)
    m.add_function(wrap_pyfunction!(cce_encode_value, m)?)?;
    m.add_function(wrap_pyfunction!(cce_encode_bytes_raw, m)?)?;
    m.add_function(wrap_pyfunction!(cce_encode_f64_bits, m)?)?;
    m.add_function(wrap_pyfunction!(compute_event_id, m)?)?;

    Ok(())
}
```

Source files in `fossic-py/src/`:

| File | Contents |
|------|----------|
| `lib.rs` | Module registration |
| `store.rs` | `PyStore` — all `Store` methods; `PyTransform`, `PyUpcaster`, `PyDynReducer`, `CollectAll` |
| `types.rs` | All data-transfer classes (`PyEventId`, `PyStoredEvent`, `PyAppend`, etc.) and `json_to_py`/`py_to_json` helpers |
| `subscriptions.rs` | `PyQueueHandler`, `PyRawSubscriptionHandle` |
| `errors.rs` | Exception class declarations and `to_py_err` mapping |
| `cce.rs` | CCE encoding functions exposed as free functions |

---

## 2. Store.open — Path Expansion

```python
from fossic import Store, OpenOptions

store = Store.open("/absolute/path/to/store.db")
store = Store.open("~/Projects/lattica/hub.db")   # tilde expanded automatically
store = Store.open("~/data/hub.db", options=OpenOptions(read_pool_size=8))
```

The Python `open` static method calls `shellexpand::tilde(path)` on the Rust side before forwarding to `fossic::Store::open`. This is one of the few places where the Python binding adds behavior absent from the Rust API — the Rust `Store::open` receives a raw path and does no tilde expansion.

The `expanded` path is a `Cow<str>` returned by `shellexpand::tilde`; `.as_ref()` is used to get a `&str` for `Store::open`.

Full constructor with all options:

```python
from fossic import Store, OpenOptions, EncryptionMode, CheckpointMode, FirstOpenPolicy

opts = OpenOptions(
    encryption=EncryptionMode.PLAINTEXT,              # v1: only PLAINTEXT is fully implemented
    checkpoint_mode=CheckpointMode.AUTO,              # v1: AUTO only; MANUAL is a reserved shape
    first_open_policy=FirstOpenPolicy.CREATE_IF_MISSING,  # or MUST_EXIST
    read_pool_size=4,                                 # number of concurrent read connections
    read_pool_timeout_ms=30_000,                      # 30 seconds before PoolExhausted
)
store = Store.open("~/data/hub.db", options=opts)
```

`FirstOpenPolicy.MUST_EXIST` returns `StoreNotFoundError` if the file does not exist. `CREATE_IF_MISSING` (default) creates the file and runs schema migration if it is new.

---

## 3. The FFI Bridge — json_to_py and py_to_json

These two functions are used at every boundary where data crosses between Rust and Python. Understanding them is essential for predicting type conversions and error behavior.

### json_to_py — Rust → Python

```rust
pub fn json_to_py<'py>(py: Python<'py>, v: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    let s = serde_json::to_string(v).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("json serialize: {e}"))
    })?;
    py.import("json")?.call_method1("loads", (s.as_str(),))
}
```

Route: `serde_json::Value` → JSON string → `json.loads` → Python object.

Type mapping:

| serde_json | Python |
|-----------|--------|
| `Value::Null` | `None` |
| `Value::Bool(b)` | `bool` |
| `Value::Number(n)` | `int` (if integer), `float` (if fractional) |
| `Value::String(s)` | `str` |
| `Value::Array(v)` | `list` |
| `Value::Object(m)` | `dict` |

No custom types, no special handling for bytes or datetimes. JSON is the intermediate format purely because PyO3 doesn't ship a built-in msgpack-to-Python conversion — this approach uses only Python stdlib and avoids a third-party FFI dependency.

### py_to_json — Python → Rust

```rust
pub fn py_to_json(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    let s: String = py
        .import("json")?
        .call_method1("dumps", (obj,))?
        .extract()?;
    serde_json::from_str(&s).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("json deserialize: {e}"))
    })
}
```

Route: Python object → `json.dumps` → JSON string → `serde_json::from_str` → `serde_json::Value`.

**Failure modes and what to do:**

| Python object | `json.dumps` behavior | Recommendation |
|--------------|----------------------|----------------|
| `int` > `i64::MAX` | dumps succeeds; CCE encoding fails with `StorageError` (U64Overflow) | Keep integers within i64 range |
| `float('nan')` | `ValueError` by default (JSON disallows NaN) | Avoid NaN in payloads; if needed, represent as `null` or a string |
| `bytes` | `TypeError` | Base64-encode to `str`, or use a separate out-of-band channel |
| `datetime` | `TypeError` | Serialize to ISO 8601 string or a Unix timestamp integer |
| Custom class | `TypeError` | Implement `__json__` / provide a serializable dict |

The double-serialization (JSON string as intermediate in both directions) has modest CPU cost for typical event payloads (< 100 KB). For bulk ingest of very large payloads, this cost can accumulate — batching with `append_batch` amortizes it.

### payload_to_py — msgpack bytes → Python

For `StoredEvent.payload`, a helper `payload_to_py` is used at read time:

```rust
pub fn payload_to_py<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let v: serde_json::Value = rmp_serde::from_slice(bytes).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("msgpack decode error: {e}"))
    })?;
    json_to_py(py, &v)
}
```

Route: `msgpack bytes` → `serde_json::Value` (via `rmp_serde::from_slice`) → Python dict (via `json_to_py`). So the full round-trip for reading a stored event payload is: SQLite BLOB → msgpack bytes → serde_json::Value → JSON string → `json.loads` → Python dict.

---

## 4. Type System — Complete Class Reference

### EventId

```python
from fossic import EventId

eid = EventId.from_hex("a3f2c1d0" * 8)  # 64 lowercase hex chars
eid = EventId.from_hex(stored_event.id.hex())  # round-trip

eid.hex()        # -> str  (64 lowercase hex chars)
eid.as_bytes()   # -> bytes (32 raw bytes)
str(eid)         # -> eid.hex()
repr(eid)        # -> "EventId(<hex>)"
hash(eid)        # usable in sets and as dict keys
eid == other_eid # equality: compares raw bytes
```

Internally `PyEventId { inner: fossic::EventId }` — a newtype over 32 bytes. The `__hash__` is computed via Python's `DefaultHasher` on the raw bytes — this is not the same as the blake3 hash used for identity derivation; it is solely for Python container use.

`from_hex` validates that the input is exactly 64 hexadecimal characters. Returns `InvalidEventIdError` on invalid input.

`as_bytes()` returns a `bytes` object via `PyBytes::new(py, self.inner.as_bytes())` — a copy of the 32 raw bytes.

### StoredEvent

```python
event.id              # EventId
event.stream_id       # str
event.branch          # str (usually "main")
event.version         # int  (u64 in Rust → Python int)
event.timestamp_us    # int  (i64 in Rust → Python int, microseconds since Unix epoch)
event.causation_id    # Optional[EventId]
event.correlation_id  # Optional[EventId]
event.event_type      # str
event.type_version    # int  (u32 in Rust → Python int)
event.payload         # dict  ← decoded from msgpack via JSON (see §3)
event.external_id     # Optional[str]
event.indexed_tags    # Optional[dict]  ← decoded from JSON TEXT
```

`timestamp_us` is a raw integer — divide by 1,000,000 for seconds, or use `datetime.fromtimestamp(event.timestamp_us / 1e6, tz=timezone.utc)`.

`type_version` reflects the **stored** version, not the upcasted version. If upcasters ran at read time, the payload bytes are updated but `type_version` is unchanged. The event `id` is also unchanged — it reflects the original stored encoding.

### Append

```python
from fossic import Append

a = Append(
    stream_id="cerebra/agent-trace/sess-abc",  # required
    event_type="StepStarted",                  # required
    payload={"step_id": "step-1"},             # required, must be JSON-serializable dict
    branch="main",                             # default
    type_version=1,                            # default
    causation_id=None,                         # Optional[EventId]
    correlation_id=None,                       # Optional[EventId]
    external_id=None,                          # Optional[str]
    indexed_tags=None,                         # Optional[dict] — must be a flat object if provided
)
event_id = store.append(a)  # -> EventId
```

`payload` conversion path: Python dict → `py_to_json` → `serde_json::Value`. The Rust side then CCE-encodes this value for ID derivation and msgpack-encodes it for storage.

`indexed_tags` must be a dict whose keys are alphanumeric + underscore only (enforced on the Rust side, returns `InvalidIndexedTags` on violation).

### ReadQuery

```python
from fossic import ReadQuery

q = ReadQuery(
    stream_id="cerebra/agent-trace/sess-abc",  # required
    branch="main",            # default "main"
    from_version=None,        # Optional[int], inclusive lower bound
    to_version=None,          # Optional[int], inclusive upper bound
    limit=None,               # Optional[int], max events to return
    event_type_filter=None,   # Optional[str], exact match on event_type column
)
events = store.read_range(q)  # -> list[StoredEvent]
```

`from_version` defaults to 0; `to_version` defaults to `i64::MAX`; `limit` defaults to `i64::MAX`. All results are ordered by version ascending.

### OpenOptions

```python
from fossic import OpenOptions, EncryptionMode, CheckpointMode, FirstOpenPolicy

opts = OpenOptions(
    encryption=EncryptionMode.PLAINTEXT,
    checkpoint_mode=CheckpointMode.AUTO,
    first_open_policy=FirstOpenPolicy.CREATE_IF_MISSING,
    read_pool_size=4,
    read_pool_timeout_ms=30_000,
)
```

**EncryptionMode** variants: `PLAINTEXT` (fully implemented), `OS_KEYRING` (schema present, crypto-shredding NotImplemented in v1), `ENV_VAR(key_name: str)` (same status). In v1, only `PLAINTEXT` should be used.

**CheckpointMode** variants: `AUTO` (SQLite's built-in WAL auto-checkpoint; default and only v1 implementation), `MANUAL` (API shape reserved but not implemented).

**FirstOpenPolicy** variants: `CREATE_IF_MISSING` (creates file + schema if absent), `MUST_EXIST` (returns `StoreNotFoundError` if file is not found).

### StreamInfo

```python
infos = store.streams()  # -> list[StreamInfo]
info = infos[0]

info.id           # str  ← NOTE: the field is .id, NOT .stream_id
info.declared_by  # str
info.declared_at  # int (microseconds since epoch)
info.description  # Optional[str]
```

The field name `.id` (not `.stream_id`) is a frequent source of bugs in backfill code. Always use `info.id` when iterating `store.streams()`.

### BranchInfo

```python
branches = store.list_branches("cerebra/decisions")  # -> list[BranchInfo]
b = branches[0]

b.id              # str — the branch_id
b.stream_id       # str
b.parent_id       # str — "main" or another branch ID
b.parent_version  # int — fork point version
b.description     # Optional[str]
b.created_at      # int (microseconds)
b.lifecycle       # str — "ephemeral" | "promoted" | "dead_end"
b.closed_at       # Optional[int]
b.closed_reason   # Optional[str]
b.alternatives    # Optional[list[str]]
```

### BranchSegment

```python
chain = store.resolve_chain("cerebra/decisions", "repair-1")  # -> list[BranchSegment]
seg = chain[0]

seg.branch_id   # str
seg.to_version  # Optional[int] — None means "read to the current tip"
```

`to_version` is `Some(parent_version)` for all segments except the leaf. The leaf segment has `to_version = None`. Use this to paginate reads across the chain:

```python
for seg in chain:
    events = store.read_range(ReadQuery(
        stream_id=stream_id,
        branch=seg.branch_id,
        to_version=seg.to_version,
    ))
    process(events)
```

### CreateBranch

```python
from fossic import CreateBranch

cb = CreateBranch(
    stream_id="cerebra/decisions",
    branch_id="repair-attempt-1",
    parent_id="main",          # "main" or an existing branch ID
    parent_version=42,         # fork point — inclusive
    description="Speculative repair for bug-123",  # Optional[str]
    alternatives=["repair-attempt-2"],              # Optional[list[str]]
)
store.create_branch(cb)
```

### SnapshotInfo

```python
info = store.snapshot_info("policy-scout/posture", "main", "posture-reducer")
# -> Optional[SnapshotInfo]

if info:
    info.stream_id             # str
    info.branch                # str
    info.version               # int — event version the snapshot was taken at
    info.reducer_name          # str
    info.reducer_version       # int
    info.state_schema_version  # int
    info.created_at            # int (microseconds)
```

### SubscriptionMode

```python
from fossic import SubscriptionMode

mode = SubscriptionMode.post_commit(queue_size=1024)  # default when mode=None
mode = SubscriptionMode.synchronous()
```

`post_commit` creates a bounded dispatch channel of `queue_size` events. When the channel is full and `try_send` fails, the subscription is marked degraded. See SR-04 for full degradation behavior.

`synchronous` fires the callback inside the write lock, before commit. Use only for very fast callbacks where in-transaction consistency is required.

### AggregateQuery

```python
from fossic import AggregateQuery

q = AggregateQuery(
    stream_pattern="cerebra/**",    # required — glob pattern
    branch="main",                  # default "main"
    event_type_filter=None,         # Optional[str] — exact match
    from_timestamp_us=None,         # Optional[int] — inclusive lower bound
    to_timestamp_us=None,           # Optional[int] — inclusive upper bound
    indexed_tags_filter=None,       # Optional[dict] — key=value filter pushed to SQL
)
events = store.aggregate(q)  # -> list[StoredEvent]
```

`indexed_tags_filter` is a Python dict. Each key-value pair becomes a `json_extract(indexed_tags, '$.key') = ?` SQL predicate. Keys must be alphanumeric + underscore. The stream_pattern applies a SQLite `GLOB` pre-filter and a Rust glob post-filter — see SR-07 for the discrepancy between SQLite GLOB and fossic glob semantics.

---

## 5. Subscription API in Detail

### RawSubscriptionHandle (low-level)

`PyRawSubscriptionHandle` is the Rust-side class exposed to Python. It holds:
- `rx: cc::Receiver<StoredEvent>` — receives events from the `PyQueueHandler`.
- `handle: Option<SubscriptionHandle>` — RAII handle; dropping it calls `registry.unsubscribe`.

```python
raw = store.subscribe(
    stream_pattern="cerebra/**",
    branch="main",
    mode=None,            # defaults to PostCommit(queue_size=1024)
    include_system=False,
)

event = raw._wait_for_next_event(timeout_secs=1.0)
# Returns: StoredEvent | None (timeout) | raises StopIteration (channel closed)

raw.is_degraded()  # -> bool
raw.unsubscribe()  # explicit drop of SubscriptionHandle
```

`_wait_for_next_event` releases the GIL during the blocking `recv_timeout` call:

```rust
fn _wait_for_next_event(&self, py: Python<'_>, timeout_secs: f64) -> PyResult<Option<PyStoredEvent>> {
    let rx = self.rx.clone();
    let timeout = Duration::from_secs_f64(timeout_secs);
    let result = py.detach(|| rx.recv_timeout(timeout));
    match result {
        Ok(event) => Ok(Some(PyStoredEvent::from(event))),
        Err(cc::RecvTimeoutError::Timeout) => Ok(None),
        Err(cc::RecvTimeoutError::Disconnected) => {
            Err(pyo3::exceptions::PyStopIteration::new_err("subscription closed"))
        }
    }
}
```

`py.detach` is the PyO3 mechanism for releasing the GIL. The Rust `recv_timeout` blocks without holding the GIL, so other Python threads can run freely during the wait.

`unsubscribe()` calls `self.handle.take()`, which drops the `SubscriptionHandle`. Dropping the handle calls `registry.unsubscribe(id)`, which closes the sender half of the crossbeam channel. The next call to `_wait_for_next_event` then returns `Disconnected` → `StopIteration`.

### PyQueueHandler — the Rust dispatch bridge

When `store.subscribe(...)` is called from Python, the bridge creates an unbounded crossbeam channel and a `PyQueueHandler`:

```rust
let (tx, rx) = cc::unbounded::<fossic::StoredEvent>();
let handler = PyQueueHandler::new(tx);
```

`PyQueueHandler` implements `SubscriptionHandler`:

```rust
impl SubscriptionHandler for PyQueueHandler {
    fn on_event(&self, event: &StoredEvent) {
        let _ = self.tx.send(event.clone());  // best-effort; silent drop if channel closed
    }
}
```

**Important:** The crossbeam channel is `unbounded`. The queue_size in `SubscriptionMode::PostCommit { queue_size }` controls the internal Rust dispatcher-to-handler channel (for degradation tracking), not this Python-side channel. The Python-side channel is unbounded and will not cause degradation by itself — but it will buffer events in memory if the Python consumer is slow. Monitor memory usage when subscribing to high-volume streams.

The handler receives the Rust `SubscriptionHandle` and the `rx` end of the channel, returns `PyRawSubscriptionHandle::new(rx, handle)`.

### Python subscription context manager (public API)

The `fossic` public package wraps `RawSubscriptionHandle` in a higher-level API:

```python
with store.subscribe("cerebra/**") as sub:
    for event in sub:
        process(event)
# __exit__ calls raw.unsubscribe(), which closes the channel
# the for loop sees StopIteration and exits cleanly
```

Iteration protocol inside the context manager:
1. `__next__` → calls `raw._wait_for_next_event(timeout_secs=1.0)`.
2. `StoredEvent` returned → yield the event.
3. `None` returned (timeout) → loop and call `_wait_for_next_event` again.
4. `StopIteration` raised (channel closed) → propagate `StopIteration`, ending the for loop.

The 1.0 second timeout means the loop wakes up periodically even when no events arrive, which is important for detecting shutdown signals or checking `is_degraded()`.

---

## 6. PyDynReducer — Python Reducer Bridge

Python reducers are bridged into the Rust `DynReducer` trait via `PyDynReducer`:

```rust
struct PyDynReducer {
    name: String,
    version: u32,
    state_schema_version: u32,
    py_obj: Py<PyAny>,
}
```

### Registration

```python
class PostureReducer:
    name = "posture-reducer"
    version = 1
    state_schema_version = 1

    def initial_state(self):
        return {"lockdown": False, "lockdown_reason": None, "last_updated_us": None}

    def apply(self, state, event_payload):
        if event_payload.get("__type__") == "LockdownActivated":
            return {**state, "lockdown": True, "lockdown_reason": event_payload.get("reason")}
        if event_payload.get("__type__") == "LockdownDeactivated":
            return {**state, "lockdown": False, "lockdown_reason": None}
        return state

store.register_reducer("policy-scout/posture", PostureReducer())
```

At registration, the bridge reads `name`, `version`, `state_schema_version` as Python attributes and stores them as owned Rust `String`/`u32` values. The Python object is kept alive as `Py<PyAny>` (a GIL-independent reference).

### Call paths at read_state time

**`initial_state_bytes()`:**
```rust
Python::attach(|py| {
    let py_state = self.py_obj.call_method0(py, "initial_state")?;
    let json_v = py_to_json(py, py_state.bind(py))?;
    rmp_serde::to_vec_named(&json_v).map_err(fossic::Error::MsgpackEncode)
})
```
Python dict → `py_to_json` → `serde_json::Value` → `rmp_serde::to_vec_named` → msgpack bytes.

**`apply_bytes(state_bytes, event_payload)`:**
```rust
Python::attach(|py| {
    // Decode state
    let state_json: serde_json::Value = rmp_serde::from_slice(state_bytes)?;
    let py_state = json_to_py(py, &state_json)?;

    // Decode event payload
    let event_json: serde_json::Value = rmp_serde::from_slice(event_payload)?;
    let py_event = json_to_py(py, &event_json)?;

    // Call Python apply
    let new_state = self.py_obj.call_method1(py, "apply", (py_state, py_event))?;

    // Encode new state
    let new_json = py_to_json(py, new_state.bind(py))?;
    rmp_serde::to_vec_named(&new_json).map_err(fossic::Error::MsgpackEncode)
})
```

Per `apply` call: 2 msgpack decodes + 2 `json_to_py` calls + 1 Python call + 1 `py_to_json` + 1 msgpack encode. For high-event-count streams, this can be slow. Consider implementing the reducer in Rust if performance is critical.

### Known limitation — take_snapshot

`store.take_snapshot(stream_id, branch)` uses the internal Rust `ReducerRegistry`, which holds `BoxedReducer` (Rust-native reducers). Python DynReducers registered via `store.register_reducer(pattern, py_obj)` go into the same `ReducerRegistry` as `DynReducerAdapter(Box<dyn DynReducer>)`, but `take_snapshot` currently requires a native `Reducer` trait implementation.

**Consequence:** Calling `store.take_snapshot()` after registering only a Python reducer raises an error. Python consumers must implement their own snapshot mechanism externally, or use `read_state()` which does the full fold each time (potentially slow for long streams).

This is documented in the `take_snapshot` docstring in `store.rs` and noted in FOSSIC-PY-NOTES.md.

---

## 7. PyTransform and PyUpcaster

Both are thin wrappers around Python callables that bridge into Rust traits.

### PyTransform

```python
def strip_metadata(event_type: str, payload: dict) -> dict:
    """Remove internal metadata keys before CCE encoding."""
    return {k: v for k, v in payload.items() if not k.startswith("_")}

store.register_payload_transform("cerebra/**", strip_metadata)
```

Callable signature: `(event_type: str, payload: dict) -> dict`.

Rust bridge:
```rust
impl PayloadTransform for PyTransform {
    fn transform(&self, event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        Python::attach(|py| {
            // msgpack bytes → serde_json → Python dict
            let v: serde_json::Value = rmp_serde::from_slice(payload)?;
            let py_payload = json_to_py(py, &v)?;
            // call Python
            let result = self.callable.call1(py, (event_type, py_payload))?;
            // Python dict → serde_json → msgpack bytes
            let json_v = py_to_json(py, result.bind(py))?;
            rmp_serde::to_vec_named(&json_v).map_err(Error::MsgpackEncode)
        })
    }
}
```

Transforms fire **before CCE encoding** during append. The ID reflects the transformed payload; the stored msgpack bytes are the transformed payload. Transform errors abort the append.

### PyUpcaster

```python
def upgrade_v1_to_v2(payload: dict) -> dict:
    """Add 'agent_version' field missing from v1 events."""
    return {**payload, "agent_version": "unknown"}

store.register_upcaster("StepStarted", from_version=1, to_version=2, callable=upgrade_v1_to_v2)
```

Callable signature: `(payload: dict) -> dict`. No `event_type` parameter — upcasters are keyed by event_type at registration time.

Rust bridge:
```rust
impl Upcaster for PyUpcaster {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        Python::attach(|py| {
            let v: serde_json::Value = rmp_serde::from_slice(payload)?;
            let py_payload = json_to_py(py, &v)?;
            let result = self.callable.call1(py, (py_payload,))?;
            let json_v = py_to_json(py, result.bind(py))?;
            rmp_serde::to_vec_named(&json_v).map_err(Error::MsgpackEncode)
        })
    }
}
```

Upcasters fire at **read time** on every `read_range`, `read_one`, `read_batch`, `read_by_external_id` call. The stored event bytes are unchanged; only the `StoredEvent.payload` dict returned to Python reflects the upcasted bytes. See SR-08 for the full upcaster chain requirements (contiguous from/to versions, `UpcasterChainGap`).

Both `PyTransform` and `PyUpcaster` use `Python::attach` to acquire the GIL from whichever Rust thread is calling (write thread for transforms, read thread for upcasters).

---

## 8. Exception Hierarchy

All fossic Python exceptions inherit from `FossicError(Exception)`. Declared in `fossic-py/src/errors.rs` using `pyo3::create_exception!`:

```
FossicError                    ← catch-all for any fossic error
├── StreamNotDeclaredError     ← Error::StreamNotDeclared
├── InvalidStreamIdError       ← Error::InvalidStreamId
├── InvalidEventIdError        ← Error::InvalidEventId
├── StoreNotFoundError         ← Error::StoreNotFound
├── SchemaMismatchError        ← Error::SchemaMismatch
├── NotImplementedError        ← Error::NotImplemented  ⚠️ shadows builtins.NotImplementedError
├── BranchNotFoundError        ← Error::BranchNotFound
├── BranchLifecycleError       ← Error::BranchLifecycleError
├── InvalidBranchIdError       ← Error::InvalidBranchId
├── ReducerPatternAmbiguousError ← Error::ReducerPatternAmbiguous
├── ReducerNotFoundError       ← Error::ReducerNotFound
├── ReducerNotFoundByNameError ← Error::ReducerNotFoundByName
├── ReducerCallError           ← Error::ReducerError
├── NoEventsToSnapshotError    ← Error::NoEventsToSnapshot
├── PurgeConfirmationError     ← Error::PurgeConfirmationError
├── EventNotFoundError         ← Error::EventNotFound
├── UpcasterChainGapError      ← Error::UpcasterChainGap
└── StorageError               ← catch-all: Sqlite, MsgpackEncode, MsgpackDecode, Io, Internal, Cce, PoolExhausted
```

### The `to_py_err` mapping

```rust
pub fn to_py_err(e: FossicError) -> PyErr {
    let msg = e.to_string();
    match e {
        FossicError::StreamNotDeclared { .. }     => StreamNotDeclaredError::new_err(msg),
        FossicError::InvalidStreamId { .. }       => InvalidStreamIdError::new_err(msg),
        FossicError::InvalidEventId(_)            => InvalidEventIdError::new_err(msg),
        FossicError::StoreNotFound { .. }         => StoreNotFoundError::new_err(msg),
        FossicError::SchemaMismatch { .. }        => SchemaMismatchError::new_err(msg),
        FossicError::NotImplemented { .. }        => NotImplementedError::new_err(msg),
        FossicError::BranchNotFound { .. }        => BranchNotFoundError::new_err(msg),
        FossicError::BranchLifecycleError { .. }  => BranchLifecycleError::new_err(msg),
        FossicError::InvalidBranchId { .. }       => InvalidBranchIdError::new_err(msg),
        FossicError::ReducerPatternAmbiguous { .. } => ReducerPatternAmbiguousError::new_err(msg),
        FossicError::ReducerNotFound { .. }       => ReducerNotFoundError::new_err(msg),
        FossicError::ReducerNotFoundByName { .. } => ReducerNotFoundByNameError::new_err(msg),
        FossicError::ReducerError { .. }          => ReducerCallError::new_err(msg),
        FossicError::NoEventsToSnapshot { .. }    => NoEventsToSnapshotError::new_err(msg),
        FossicError::PurgeConfirmationError { .. } => PurgeConfirmationError::new_err(msg),
        FossicError::EventNotFound { .. }         => EventNotFoundError::new_err(msg),
        FossicError::UpcasterChainGap { .. }      => UpcasterChainGapError::new_err(msg),
        _ => StorageError::new_err(msg),  // Sqlite, Msgpack*, Io, Internal, Cce, PoolExhausted
    }
}
```

`CceError` variants (U64Overflow, DuplicateKeys, StringTooLarge) all map to `StorageError` via the wildcard arm — there is no dedicated Python exception for CCE errors.

`PoolExhausted` also maps to `StorageError`. Its message is: `"read pool exhausted: all {pool_size} connections busy after {timeout_ms}ms; increase OpenOptions::read_pool_size"`. Catch it by catching `StorageError` and inspecting the message string, or increase `read_pool_size` in `OpenOptions`.

### ⚠️ NotImplementedError shadow

`fossic.NotImplementedError` shadows Python's built-in `builtins.NotImplementedError`. This is a PyO3 constraint — `create_exception!` names the exception class directly. In any module that imports from fossic, `NotImplementedError` may refer to the fossic variant, not the built-in.

Safe usage patterns:

```python
import fossic

# Explicit fossic exception:
try:
    store.shred_stream("cerebra/posture", "test")
except fossic.NotImplementedError as e:
    print(f"not implemented: {e}")   # v1 stub

# Explicit builtin when needed:
import builtins
raise builtins.NotImplementedError("override me")

# Or use NotImplemented (the singleton, not the exception):
return NotImplemented  # for __eq__, __lt__, etc.
```

---

## 9. GIL Patterns

Understanding GIL interactions is critical for multi-threaded fossic consumers.

### Subscription wait — GIL released

```rust
let result = py.detach(|| rx.recv_timeout(timeout));
```

`py.detach` releases the GIL for the duration of the `recv_timeout` call. Other Python threads run freely while a subscription is waiting for events. This makes subscription iteration compatible with Python threading — a subscription loop in one thread does not starve other threads.

### Reducer and transform callbacks — GIL acquired

```rust
Python::attach(|py| { ... })
```

Used in `PyDynReducer::apply_bytes`, `PyDynReducer::initial_state_bytes`, `PyTransform::transform`, `PyUpcaster::upcast`. When called from a Rust thread that does not hold the GIL (such as the write thread during an append), `Python::attach` acquires the GIL.

**Consequence:** While a Python transform is executing inside `store.append()`, the Rust write thread holds the GIL. No other Python thread can run. Transform callbacks should be fast.

**Consequence:** While `apply_bytes` is executing inside `store.read_state()`, the Rust read thread holds the GIL. The GIL is acquired and released once per event in the fold sequence. For a stream with 10,000 events, this means 10,000 GIL acquisitions. If other threads need the GIL, they will be blocked during each acquisition window.

For high-event-count streams, consider:
1. Using `read_state_at_version` with periodic external checkpointing to keep the event count small.
2. Implementing the hot path as a Rust reducer (no GIL overhead).
3. Running the consumer in a dedicated process to avoid contention.

### Multiple subscribers from multiple threads

```python
import threading

def watch_cerebra():
    with store.subscribe("cerebra/**") as sub:
        for event in sub:
            handle(event)

def watch_policy():
    with store.subscribe("policy-scout/**") as sub:
        for event in sub:
            handle(event)

t1 = threading.Thread(target=watch_cerebra, daemon=True)
t2 = threading.Thread(target=watch_policy, daemon=True)
t1.start(); t2.start()
```

This is safe. Each subscription holds its own `PyRawSubscriptionHandle` with its own crossbeam channel. Both threads release the GIL during `_wait_for_next_event`, so they don't block each other while waiting.

---

## 10. CCE Functions (Testing / Tooling)

Four free functions are exposed for conformance testing and tooling. Production application code almost never needs these.

```python
from fossic import cce_encode_value, cce_encode_bytes_raw, cce_encode_f64_bits, compute_event_id

# Encode any JSON-serializable Python value to CCE bytes
cce_bytes = cce_encode_value({"foo": "bar"})          # -> bytes
cce_bytes = cce_encode_value(42)                       # -> bytes (INT tag + i64 LE)
cce_bytes = cce_encode_value(None)                     # -> bytes (NULL tag only)

# Encode raw bytes as CCE BYTES type (tag 0x05 + u64 length + data)
cce_bytes = cce_encode_bytes_raw(b"\x01\x02\x03")     # -> bytes

# Encode an f64 by IEEE 754 bit pattern, with CCE canonicalization
# bits_hex: 16-char lowercase hex of the big-endian u64 bit pattern
cce_bytes = cce_encode_f64_bits("3ff0000000000000")   # 1.0 -> bytes (tag 0x03 + LE bytes)
cce_bytes = cce_encode_f64_bits("8000000000000000")   # -0.0 -> same as +0.0 after canonicalization
cce_bytes = cce_encode_f64_bits("7ff8000000000000")   # quiet NaN

# Compute event ID without appending
event_id = compute_event_id(
    event_type="UserCreated",
    payload={"user_id": "abc"},
    type_version=1,        # default
    causation_id=None,     # default
)  # -> EventId
```

`compute_event_id` calls the same derivation that `store.append()` uses internally. The returned `EventId` is byte-identical to what the store would assign for the same inputs. Useful for pre-computing IDs in tests, validating relay deduplication keys, or building external indices.

Note: `cce_encode_value` is the standalone encoder (no version prefix). `compute_event_id` includes the `fossic-cce-v1\0` prefix in the hash. See SR-01 for the full derivation formula.

---

## 11. Build and Distribution

The `fossic-py` crate uses `maturin` for building Python wheels. Target matrix:

| Platform | Architecture |
|----------|-------------|
| Linux (manylinux) | x86_64 |
| macOS | arm64 |
| macOS | x86_64 |
| Windows | x86_64 |

```bash
# Install from PyPI
pip install fossic

# Build from source (requires Rust toolchain + maturin)
cd fossic-py
maturin develop        # editable install in current venv
maturin build          # produce .whl in target/wheels/
```

The Python package name is `fossic`. The native extension is `_fossic`. After installation:

```python
import fossic           # public API — use this
import fossic._fossic   # raw native module — rarely needed; for introspection only
```

`fossic._fossic` exposes all PyO3 classes directly. If you need to inspect what methods a class supports, `dir(fossic._fossic.Store)` works, though the public `fossic.Store` re-export is identical.

---

## 12. Operational Patterns

### Pattern: relay with idempotent deduplication

```python
def relay_stream(src_store, dst_store, src_stream_id, dst_stream_id):
    """Relay all events from src to dst with deduplication via external_id."""
    events = src_store.read_range(ReadQuery(stream_id=src_stream_id, branch="main"))
    for event in events:
        existing = dst_store.read_by_external_id(dst_stream_id, event.id.hex())
        if existing:
            continue  # already relayed
        dst_store.append(Append(
            stream_id=dst_stream_id,
            event_type=event.event_type,
            payload=event.payload,
            type_version=event.type_version,
            causation_id=event.causation_id,
            correlation_id=event.correlation_id,
            external_id=event.id.hex(),  # source event ID as dedup key
            indexed_tags=event.indexed_tags,
        ))
```

### Pattern: subscribe with graceful shutdown

```python
import threading

stop_event = threading.Event()

def subscription_worker(store):
    with store.subscribe("cerebra/**") as sub:
        for event in sub:
            if stop_event.is_set():
                break
            process(event)
            if sub.is_degraded():
                logging.warning("subscription degraded — restarting")
                break  # restart in outer loop

# To shut down cleanly:
stop_event.set()
```

### Pattern: paginated read for large streams

```python
def read_all_events(store, stream_id, branch="main", chunk_size=1000):
    from_v = 0
    while True:
        events = store.read_range(ReadQuery(
            stream_id=stream_id,
            branch=branch,
            from_version=from_v,
            limit=chunk_size,
        ))
        if not events:
            break
        yield from events
        from_v = events[-1].version + 1
```

### Pattern: cross-stream aggregate with indexed_tags

```python
# Find all StepStarted events in the last hour with agent="claude"
import time

now_us = int(time.time() * 1_000_000)
one_hour_us = 3_600 * 1_000_000

events = store.aggregate(AggregateQuery(
    stream_pattern="cerebra/**",
    event_type_filter="StepStarted",
    from_timestamp_us=now_us - one_hour_us,
    indexed_tags_filter={"agent": "claude"},  # requires indexed_tags={"agent": "claude"} at append time
))
```

### Pattern: compute event ID before append

```python
from fossic import compute_event_id, EventId

# Pre-compute to build an external index entry before committing
eid = compute_event_id("UserCreated", {"user_id": "abc", "email": "x@y.com"})
print(f"Will be stored as: {eid.hex()}")

# Then append — store will produce the same ID
actual_id = store.append(Append(
    stream_id="users/profiles",
    event_type="UserCreated",
    payload={"user_id": "abc", "email": "x@y.com"},
))
assert actual_id == eid  # guaranteed if no payload transform is registered for this stream
```

Note: if a payload transform is registered for the stream, the transform runs before CCE encoding, and `actual_id` will differ from the pre-computed `eid`. `compute_event_id` does not apply transforms.

---

## 13. Summary — Key Invariants for Python Consumers

1. **StreamInfo field is `.id`, not `.stream_id`.** Every backfill loop that iterates `store.streams()` must use `info.id`.

2. **Payload is a dict at the Python boundary.** Internally it is msgpack; the conversion happens via JSON string. Objects not serializable by `json.dumps` will raise at append time.

3. **Python reducers cannot use `take_snapshot`.** Implement external snapshotting or use Rust reducers if snapshot caching matters.

4. **`fossic.NotImplementedError` shadows the built-in.** Use `builtins.NotImplementedError` for abstract method stubs in the same codebase.

5. **`StorageError` is the catch-all for infrastructure errors** including pool exhaustion, SQLite errors, msgpack errors, CCE errors, and internal errors. Inspect `.args[0]` for the error message when finer-grained handling is needed.

6. **GIL is held during transform and reducer callbacks.** Keep these fast; long transforms stall all Python threads.

7. **Subscription channel to Python is unbounded.** A slow consumer will buffer events in memory, not trigger degradation. Monitor memory when subscribing to high-volume streams.

8. **`_wait_for_next_event` releases the GIL.** Multiple subscription threads do not starve each other.

9. **Tilde expansion happens in Python bindings, not Rust core.** `Store.open("~/...")` works in Python; the equivalent Rust `Store::open("~/...")` does not expand the tilde.

10. **`compute_event_id` does not apply payload transforms.** If transforms are registered, pre-computed IDs will not match stored IDs.
