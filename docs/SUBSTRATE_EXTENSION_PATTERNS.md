# Substrate Extension Patterns

How to build correctly on top of fossic when writing a sibling crate, a third-party MCP server that embeds fossic, or any code that extends the substrate with new capabilities.

Audience: authors of sibling crates (`fossic-similarity-hnsw`, future `fossic-*` crates), third-party integrators, anyone adding a new substrate subsystem. Companion to `docs/SUBSTRATE_GOTCHAS.md` — that doc tells consumers what bites them; this doc tells extenders how to build correctly.

Every claim below is source-verified against the current tree. File:line citations are included so you can check the live code rather than trusting this document at version drift.

*Last updated: 2026-06-21. Substrate version: v1.7.1.*

---

## 1. SystemStreamWriter: Lazy-Mutex Pattern

### When to use

When your extension needs to write events to `_fossic/system`. All diagnostic, lifecycle, and telemetry events produced by substrate subsystems go here.

### The pattern

```rust
use fossic::SystemStreamWriter;
use parking_lot::Mutex;
use std::path::PathBuf;

pub struct MyExtension {
    index_dir: PathBuf,
    // Never open this connection at construction time.
    // Open it lazily on the first emission so construction never fails
    // due to a missing or locked db file.
    system_writer: Mutex<Option<SystemStreamWriter>>,
}

impl MyExtension {
    pub fn new(store_db_path: &std::path::Path) -> Self {
        MyExtension {
            index_dir: store_db_path.parent().unwrap().join("my_extension"),
            system_writer: Mutex::new(None),
        }
    }

    fn emit_system_event(&self, event_type: &str, payload: &serde_json::Value) {
        let mut guard = self.system_writer.lock();
        if guard.is_none() {
            // Reconstruct the store db path from index_dir's parent.
            if let Some(db_dir) = self.index_dir.parent() {
                *guard = SystemStreamWriter::new(&db_dir.join("store.db"));
            }
        }
        if let Some(ref mut w) = *guard {
            w.emit(event_type, payload, None);
        }
    }
}
```

### API surface (verified at v1.7.1)

```rust
// src/system_stream.rs:17,24,46
pub struct SystemStreamWriter { ... }

impl SystemStreamWriter {
    pub fn new(db_path: &Path) -> Option<Self>     // None on connection failure
    pub fn emit(
        &mut self,
        event_type: &str,
        payload: &serde_json::Value,
        indexed_tags: Option<&serde_json::Value>,  // None = no tag index
    )
}
```

`new` returns `Option<Self>` — connection failure is logged and swallowed, not propagated. The caller must tolerate `None`. `emit` is `&mut self` because SQLite connections are not `Sync`; hold the Mutex across the call.

The `indexed_tags` field is a JSON object whose keys become indexed columns in the `events` table. Use it if you want to query system events by subsystem:

```rust
let tags = serde_json::json!({ "event_class": "hnsw" });
writer.emit("HnswIndexSaved", &payload, Some(&tags));
```

### Five live instances in current substrate

| Field / variable | Location | Event types emitted |
|---|---|---|
| `start_dispatcher` local `sys_writer` | `store.rs:2133` | `SubscriptionDegraded` |
| `StoreInner.reducer_system_writer` | `store.rs:152` | `ReducerStateLarge` |
| `StoreInner.project_registry_writer` | `store.rs:161` | `ProjectRegistered`, `RelayHeartbeat` |
| `bg_thread_loop` local `sys_writer` | `executor.rs:228` | `DeferredTaskDropped` |
| `HnswProvider.system_writer` | `provider.rs:136` | HNSW lifecycle events (v1.7.2+) |

The first instance (dispatcher) is created eagerly when the dispatch thread starts; the rest are lazy — `None` until first emission. The lazy shape is preferred for extension code: it doesn't couple construction success to the DB being reachable at that instant.

### What not to do

Do not share one writer across threads without a Mutex. `SystemStreamWriter` wraps a `rusqlite::Connection`, which is not `Sync`. The Mutex<Option<...>> struct field is the correct unit of ownership for anything that might emit from multiple threads or from a background task.

---

## 2. Background Task Scheduling via TaskKind::Custom

### When to use

When your extension needs deferred or recurring background work: saving an HNSW index, running periodic maintenance, emitting telemetry after a quiescent window.

### The pattern

```rust
use fossic::{BackgroundExecutor, BacklogTask, TaskKind, TaskPriority};
use std::{sync::Arc, time::Duration};

fn schedule_save(executor: &BackgroundExecutor, provider: Arc<MyProvider>) {
    executor.schedule(BacklogTask {
        priority: TaskPriority::Low,
        deadline_us: fossic_now_us() + 5_000_000, // 5s from now
        persist_on_drop: false,    // Custom tasks: always false
        kind: TaskKind::Custom(Arc::new(move || {
            // Capture Arc<MyProvider> at scheduling time.
            // The closure must be Fn (not FnOnce) — it may be re-queued
            // for recurring tasks.
            if let Err(e) = provider.save_to_disk() {
                eprintln!("[WARN] save_to_disk failed: {e}");
            }
        })),
        recurring_interval: Some(Duration::from_secs(30)), // None for one-shot
    });
}
```

### API surface (verified at v1.7.1)

```rust
// src/executor.rs:44
pub enum TaskKind {
    GcOrphanSnapshots,
    TakeSnapshot { stream_id: String, branch: String },
    Custom(std::sync::Arc<dyn Fn() + Send + Sync + 'static>),
}

// src/executor.rs:59,188
pub struct BacklogTask {
    pub priority: TaskPriority,
    pub deadline_us: i64,         // micros since Unix epoch
    pub persist_on_drop: bool,
    pub kind: TaskKind,
    pub recurring_interval: Option<Duration>,
}

impl BackgroundExecutor {
    pub fn schedule(&self, task: BacklogTask) { ... }
    // spawn is pub(crate) — sibling crates do not call it
}
```

### Why Arc<dyn Fn()> not Box<dyn FnOnce(&Store)>

The brief for `TaskKind::Custom` originally specified `Box<dyn FnOnce(&Store)>`. The implementation uses `Arc<dyn Fn() + Send + Sync>` instead, for two reasons:

1. **Circular module dependency.** `BackgroundExecutor` lives in `executor.rs`. `Store` lives in `store.rs`. `store.rs` imports from `executor.rs`. If `executor.rs` imported `Store` from `store.rs`, there would be a mutual dependency. The closure captures its context (including an `Arc<Store>` if needed) at scheduling time, so `executor.rs` never needs to see `Store`.

2. **`TaskKind` derives `Clone`**, which `Box<dyn FnOnce>` cannot satisfy. `Arc<dyn Fn()>` is `Clone`.

Deviation recorded as **CP-D2-1**.

### Capture Arc<Store> when you need substrate access

If your closure needs to call store methods (e.g., to read events before saving), capture `Arc<Store>` at scheduling time:

```rust
let store_clone = Arc::clone(&store);
let provider_clone = Arc::clone(&provider);
executor.schedule(BacklogTask {
    kind: TaskKind::Custom(Arc::new(move || {
        let events = store_clone.read_range(...).unwrap_or_default();
        provider_clone.process(events);
    })),
    ..
});
```

See **CP-EXECUTOR-FUTURE** (footer) for the anticipated future in which this pattern is replaced by a formal `Arc<Store>` parameter on the task signature.

### Quiescence semantics

The background thread polls every 500ms (`executor.rs:231`). It only executes tasks when the store is quiescent — no writes and no subscription dispatches within the configured window (default 2s). This means:

- **Custom tasks are not real-time.** From schedule to execution is at minimum one 500ms sleep, then however long until the next quiescent window.
- **Recurring tasks run at most once per quiescent window,** not once per `recurring_interval`. If the store is never quiescent for 60 seconds, a task with `recurring_interval: Some(Duration::from_secs(10))` fires once in that period.
- `persist_on_drop: true` on a Custom task has no effect at shutdown — the executor emits `DeferredTaskDropped` and drops it. Set `persist_on_drop: false` for all Custom tasks.

---

## 3. Optimistic Timestamp at Schedule Time

### The problem

When a quiescence-gated recurring scheduler evaluates "should I schedule a task now?", there is a gap between the evaluation and the executor's actual run. A write burst that keeps the store non-quiescent for several seconds can cause the same check to fire many times before any task executes, queuing duplicate tasks.

### The pattern

Update a "last scheduled at" timestamp **at schedule time**, not at execution time. The next evaluation check sees the update and skips scheduling:

```rust
fn maybe_schedule_save(&self) {
    let now_us = fossic_now_us();
    let key = self.index_dir.clone();
    let window_us = self.config.save_interval_secs as i64 * 1_000_000;

    let last = {
        let map = self.last_save_us.read();
        *map.get(&key).unwrap_or(&0)
    };

    if now_us - last >= window_us {
        // Write the optimistic update BEFORE scheduling.
        {
            let mut map = self.last_save_us.write();
            map.insert(key, now_us);  // <-- optimistic stamp
        }
        self.schedule_save_to_disk();
    }
}
```

### Where this pattern lives in current substrate

`SnapshotPolicy::EveryNSeconds` at `store.rs:1669–1685`:

```rust
// store.rs:1677–1683
if now - last >= window_us {
    // Optimistic update prevents storm-scheduling between
    // the schedule call and the executor's next quiescent window.
    {
        let mut map = self.inner.last_snapshot_us.write();
        map.insert(key, now);
    }
    self.schedule_background_snapshot(stream_id, branch);
}
```

The comment at `store.rs:1678` is load-bearing: without the optimistic update, every `read_state` call during a write burst re-evaluates the "has the window passed?" check, sees that no snapshot has been taken yet (the executor hasn't run), and schedules again.

For the HNSW provider, the same pattern will apply for scheduled `save_to_disk` calls in v1.7.2: stamp `last_save_us` at schedule time, not in the save closure.

### Recurring task re-queue

The background executor itself uses the analogous pattern for re-queuing recurring tasks: at task completion, it stamps the next deadline as `now_us() + interval.as_micros()` (`executor.rs:285`). This is execution-time stamping, which is correct for re-queuing (not storm-prevention), because the next deadline must be computed after the previous execution finishes.

---

## 4. Clean Shutdown via Arc::strong_count + Weak<dyn Trait>

### The composition

Extensions that hold background threads referencing substrate internals need a clean shutdown story. The substrate uses three mechanisms that compose without explicit coordination:

**1. `Weak<dyn StoreOps>` in the executor**

`BackgroundExecutor::spawn` receives a `Weak<dyn StoreOps>` (`executor.rs:153`). Each task dispatched by the bg thread upgrades the Weak first:

```rust
// executor.rs:276–279
if let Some(ops) = store_ops.upgrade() {
    execute_task(&*ops, &task);
}
// upgrade() returning None means the store has been dropped — skip.
```

When the store drops, the Weak upgrade fails and tasks are silently skipped. The executor doesn't need to know that the store has gone away — it discovers it naturally on the next poll.

**2. `stop_flag: Arc<AtomicBool>` in BackgroundExecutor::drop**

`BackgroundExecutor::drop` sets the stop flag and waits up to `grace_timeout` for the bg thread to exit (`executor.rs:194–212`). If the grace period expires, the thread is detached (not killed) and will exit naturally on its next 500ms wake when it sees the flag.

**3. `Arc::strong_count == 1` at Store::drop**

`Store::drop` uses `strong_count == 1` to detect that this is the last handle:

```rust
// store.rs:181–187
impl Drop for Store {
    fn drop(&mut self) {
        if self.inner.options.auto_gc_orphans && Arc::strong_count(&self.inner) == 1 {
            let _ = self.gc_orphaned_snapshots();
        }
    }
}
```

The check avoids double-execution when multiple `Store` clones exist — only the last one drops. Extension code can use the same pattern for finalisation that should run exactly once at store close.

### Applying this to a sibling crate

A sibling crate that holds a background thread with an Arc back to its own state should follow the same shape:

```rust
pub struct MyProvider {
    inner: Arc<MyProviderInner>,
}

impl Drop for MyProvider {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            // This is the last reference — run cleanup.
            // The bg thread is a Weak to self.inner, so it
            // will see a failed upgrade on its next poll.
        }
    }
}
```

The `BackgroundExecutor` that runs the provider's tasks holds a Weak to the store, not to the provider. The provider holds its own Arc<MyProviderInner>. These are independent reference counts — neither one keeps the other alive past its natural lifetime.

---

## 5. StoreOps: Status as Extension Trait

### Current status (v1.7.1)

`StoreOps` is **`pub(crate)`** at `executor.rs:21`:

```rust
// executor.rs:21
pub(crate) trait StoreOps: Send + Sync + 'static {
    fn bg_gc_orphaned_snapshots(&self) -> Result<usize, Error>;
    fn bg_take_snapshot(&self, stream_id: &str, branch: &str) -> Result<SnapshotInfo, Error>;
}
```

`BackgroundExecutor::spawn` takes `Weak<dyn StoreOps>`. `StoreOps` is implemented on `StoreInner` in `store.rs:1838`. Sibling crates cannot implement `StoreOps` or call `spawn` directly — `spawn` is `pub(crate)` as well.

### v1 workaround for sibling crates that need Store access

Sibling crate tasks that need to call store methods must capture an `Arc<Store>` in the `TaskKind::Custom` closure environment at scheduling time:

```rust
// fossic-similarity-hnsw will do this for the v1.7.3 background indexing pass
let store_weak = Arc::downgrade(&store);
executor.schedule(BacklogTask {
    kind: TaskKind::Custom(Arc::new(move || {
        if let Some(store) = store_weak.upgrade() {
            // Call store methods here.
        }
    })),
    ..
});
```

Using `Weak<Store>` (not `Arc<Store>`) is recommended so the task doesn't artificially extend the store's lifetime. The upgrade check is the same natural guard the built-in executor uses with `Weak<dyn StoreOps>`.

Recorded as **CP-EXECUTOR-FUTURE**: the anticipated D5/D6 formalization adds `StoreOps` to the public surface and gives sibling crates a first-class handle, removing the need for this workaround.

### What StoreOps provides today

The two methods on `StoreOps` are substrate-internal:

- `bg_gc_orphaned_snapshots` — Phase 7 GC
- `bg_take_snapshot` — Phase 6 snapshot scheduling

These are not useful to sibling crates directly. The workaround above (`Arc<Store>`) is the correct path for any sibling task that wants to read or write events.

---

## 6. Cross-Language Binding Constraints

### napi-rs: Option<Buffer> passthrough for class fields in objects

napi-rs distinguishes between `#[napi]` classes (reference-counted JS objects) and `#[napi(object)]` plain-data structs. A `#[napi(object)]` struct cannot contain a field of a `#[napi]` class type. Attempting it produces a compile error at the napi-rs codegen step.

The concrete instance in `fossic-node`: `TruncationCursorJs` is a `#[napi]` class. `ReadOutcomeJs` is `#[napi(object)]`. `ReadOutcomeJs` needs to return an optional cursor. The solution:

```rust
// fossic-node/src/types.rs:376
#[napi(object)]
pub struct ReadOutcomeJs {
    pub kind: String,
    pub results: Vec<StoredEventJs>,
    pub reason: Option<String>,
    pub next_cursor: Option<Buffer>,  // serialized bytes, not TruncationCursorJs
}
```

The JS layer (`index.js`) wraps the raw `Buffer` back into a `TruncationCursorJs` instance before returning to callers. The Rust side serializes the cursor to bytes; the JS side reconstructs the class. The Rust struct never holds the class directly.

**General rule for napi-rs binding code:** if a `#[napi(object)]` struct needs to carry a Rust type that is also a `#[napi]` class, use `Option<Buffer>` (or raw bytes) as the transport type and reconstruct the class in the JS or TypeScript wrapper layer.

Established at v1.1.7 (`fossic-node`).

### PyO3: `__next__ -> Option<T>` as natural StopIteration

PyO3 implements the Python iterator protocol through `__next__`. When `__next__` returns `Ok(None)`, PyO3 raises `StopIteration` automatically — no explicit `Err(PyStopIteration::...)` needed for normal exhaustion:

```rust
// fossic-py/src/store.rs:722 (ReadRangeIter)
fn __next__(&mut self) -> PyResult<Option<PyStoredEvent>> {
    match self.iter.next() {
        Some(event) => Ok(Some(PyStoredEvent::from(event))),
        None => Ok(None),  // PyO3 raises StopIteration for you
    }
}
```

Explicit `Err(PyStopIteration)` is used only when the termination condition is not "iterator exhausted" but "channel closed" (subscription iterators in `fossic-py/src/subscriptions.rs:72`). For read iterators, always use `Ok(None)`.

This maps cleanly to Python's `for event in store.read_range_iter(...)` — no user-visible boilerplate.

Established at v1.1.6 (`fossic-py`).

### BigInt for u64 in napi-rs

JavaScript `number` is a 64-bit float and cannot exactly represent all `u64` values. napi-rs maps `u64` to `BigInt` in `#[napi]` contexts. In `#[napi(object)]` structs, fields that carry version numbers, timestamps, or other u64 values should use `BigInt`:

```rust
// fossic-node/src/types.rs:66
pub struct StoredEventJs {
    pub version: BigInt,     // not u64
    pub timestamp_us: i64,   // fits in i64; plain number on the JS side
    ..
}
```

The `get_u64()` extraction method on `BigInt` returns `(lossless: bool, value: u64)`. Check `lossless` in inputs if overflow is possible; callers controlling their own input can skip the check.

---

## 7. Documentation Discipline for Substrate Work

### Source-verify before writing

Every claim in substrate documentation should trace to a specific file and line that exists in the current tree. Docs written from memory or from a prior session's context drift silently as code evolves.

The working procedure:

1. Load the relevant source files (Read tool or grep) before writing the section.
2. Check that the types, method signatures, and field names you're about to document still exist and still have the visibility you claim.
3. Include file:line citations in the doc so future readers — and future agents — can verify rather than trust.

### Why this matters: multi-session conversational drift

Over a long project, a conversational agent builds a working mental model of the codebase. That model is accurate at build time but diverges as code evolves and context windows roll over. Without source-verification before writing, a docs pass can confidently describe:

- Methods that were renamed
- Structs with different field shapes than documented
- API surface that is still `pub(crate)` but described as `pub`
- Visibility that was opened in a prior pass but the doc references the pre-open state

The authoring pass for `SUBSTRATE_GOTCHAS.md` caught three concrete drifts: a method renamed between a prior session's note and the current code, a visibility label that was `pub(crate)` not `pub`, and a struct field whose type had changed. All three were caught by the loading sequence before a word was written. None would have been caught by running tests.

### The SUBSTRATE_GOTCHAS.md standard

`docs/SUBSTRATE_GOTCHAS.md` is the reference example for this discipline. Each entry:

- Opens with the exact behavior, not an assertion about intent
- Includes source citations the reader can verify
- Shows the mitigation as working code, not prose
- Omits everything that can be inferred from the code directly

Apply the same standard to any new substrate documentation. If a claim cannot be grounded in a file:line, it belongs in a `## Notes` block at the end, clearly labeled as uncited.

---

## Known Gaps and Open Watchlist

The patterns above describe what exists at v1.7.1. The following are known limitations or anticipated evolution, recorded as CP (Contract Point) markers per the project convention.

**CP-D2-1 — TaskKind::Custom closure signature**
Current: `Arc<dyn Fn() + Send + Sync + 'static>`. Brief-specified: `Box<dyn FnOnce(&Store)>`. The deviation is intentional (circular module dep avoidance, Clone requirement). The anticipated v2 signature adds a `&Store`-equivalent parameter via the `StoreOps` formalization. Sibling crate code using the current closure-capture workaround will need to be updated when that lands.

**CP-D2-2 — SimilaritySearchProvider::index lacks stream_id**
The trait signature at v1.7.1 is `index(&self, event_id: EventId, embedding: &[f32]) -> Result<(), Error>`. No `stream_id` parameter. Events indexed via the trait path cannot be filtered by stream pattern in `query()`. The `HnswProvider::index_with_stream_id` inherent method is the v1 workaround. A v2 trait would carry `stream_id` as a parameter. Any crate implementing `SimilaritySearchProvider` directly should document which indexing path callers are expected to use.

**CP-FOSSIC-OBSERVABILITY-RESET — dispatch_channel_high_water_mark has no reset method**
`StoreInner.dispatch_channel_high_water_mark: Arc<AtomicUsize>` (`store.rs:143`) tracks the peak depth ever observed in the post-commit dispatch channel. The value is exposed via `Store::dispatch_channel_high_water_mark()` but there is no reset method. Applications that want per-window peak tracking must snapshot and diff externally. A `reset_dispatch_channel_high_water_mark()` method is the anticipated fix.

**CP-FOSSIC-PY-VERSION-STRING — fossic-py pip version label mismatch**
The version string visible via `pip show fossic` may diverge from the Cargo.toml version during periods where the binding crates are bumped as a group. This is a cosmetic issue in dev environments; it does not affect runtime behavior. Tracked for resolution when the Python packaging pipeline is formalized.

---

### CP Marker Convention

CP markers (`CP-<NAME>`) are short labels used to cross-reference known deviations, open design questions, and deferred-until-vN decisions across code comments, blast-radius docs, and this document. They are not bug IDs — they don't have formal state transitions. The convention is: if code deviates from a stated intent or a known open question is load-bearing for future decisions, file a CP and reference it wherever the deviation or question surfaces. This makes it possible to grep for all instances of a concern across the whole tree.
