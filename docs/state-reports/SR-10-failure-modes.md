# SR-10 — Substrate Failure Mode Reconnaissance

**Version:** v1.8.0 (`Cargo.toml` version field)  
**Date:** 2026-06-21  
**Branch:** track2-p678  
**Scope:** Pure reconnaissance — no code changes. All citations verified against on-disk source.

---

## PART A — Verified Findings

Twelve failure modes confirmed against live source. Each entry lists the **property** (what the substrate actually does), **why it bites** (the observable consequence), and **evidence** (file:line of the load-bearing code).

---

### A-1. CCE hash excludes stream_id and branch — identical payloads on different streams collide

**Property.** `derive_event_id` hashes `(event_type, type_version, causation_id, payload)`. Stream_id and branch are not inputs.

```
src/cce.rs:133–145
src/append.rs:54–59   (call site in append_impl)
```

**Consequence.** Two events with identical `(event_type, type_version, causation_id, payload)` but different `stream_id` values produce the same BLAKE3 event_id. The `INSERT OR IGNORE` in `append_impl` (`src/append.rs:80–99`) silently ignores the second insert. The caller receives the original event_id with no error. The second event's version slot is consumed (gap in the sequence), the event is not stored, and subscribers never receive it.

**Why it bites.** Consumer vocabularies that share event type names across streams must artificially differentiate them via `causation_id` or payload fields, or they silently lose writes at the storage layer. The idempotence design is deliberate for deduplication across retry scenarios; the cross-stream collision is a corollary that bites integrators who don't expect it. Documented in `docs/SUBSTRATE_GOTCHAS.md` § CCE Identity.

**Observation surface.** No error, no log entry, no system event. Caller sees a successful return with the same event_id as the original event.

---

### A-2. Dispatch channel is unbounded — unconstrained memory growth under slow dispatch

**Property.** The post-commit dispatch channel is created with `crossbeam_channel::unbounded::<StoredEvent>()`.

```
src/store.rs:241–242   (channel creation)
src/store.rs:397–401   (send site in append)
src/store.rs:440–445   (send site in append_batch)
src/store.rs:507–511   (send site in append_if)
```

**Consequence.** When the dispatcher thread is slower than the write path — whether because `dispatch_post_commit` is iterating many subscribers, one subscriber's queue is full and its try_send is stalling, or the OS scheduler is not giving the dispatcher thread time — the channel grows without bound. There is no capacity limit, no drop-oldest policy, and no back-pressure signal propagated to the write path. A sustained write burst against a slow subscriber set can exhaust heap memory.

**Observation surface.** `Store::dispatch_channel_pressure()` returns the current queue depth (`src/store.rs:1545–1547`). `Store::dispatch_channel_high_water_mark()` returns the peak since store open (`src/store.rs:1552–1554`). Both are read-only metrics — no automatic trigger exists.

**No write-path throttle exists.** The only available response is manual: monitor pressure, reduce write rate, or increase subscriber queue sizes to drain the channel faster.

---

### A-3. PostCommit subscriber overflow → permanent degradation, no recovery path

**Property.** When a PostCommit subscriber's bounded channel is full, `try_send` returns `TrySendError::Full`. The subscriber is immediately marked permanently degraded.

```
src/subscriptions.rs:268–270   (Full → degraded.store(true))
src/system_stream.rs:98–112    (SubscriptionDegraded payload schema)
src/store.rs:2151–2163         (dispatcher emits SubscriptionDegraded via sys_writer)
```

**Consequence.** Once degraded, the subscription is skipped for all future dispatches (`dispatch_post_commit` checks `entry.degraded.load()` at line 244). No drain-and-retry path exists. The caller must drop the `SubscriptionHandle` (which unregisters it, closing the handler thread's channel) and call `Store::subscribe` again with a fresh cursor seeded from the current stream tip. Events that arrived during the degraded interval are permanently lost from the subscriber's perspective — they are in the store, but the new subscription cursor starts from `MAX(version)` at subscribe time, not from the point of degradation.

**Recovery prerequisite.** The resubscription cursor gap is observable if the caller reads the stream directly after resubscription and compares versions. There is no built-in mechanism to detect or fill the gap.

**Observation surface.** `SubscriptionHandle::is_degraded()` polls the flag. A `SubscriptionDegraded` event is written to `_fossic/system` by the dispatcher thread's `SystemStreamWriter` (best-effort; see A-12 for contention risks).

---

### A-4. Synchronous subscriber fires while write lock is held — subscriber latency is append latency

**Property.** `dispatch_sync` is called inside the block that holds `Mutex<Connection>`, before the lock is released.

```
src/store.rs:379–393   (append: lock block, dispatch_sync inside)
src/store.rs:417–435   (append_batch: same structure)
src/store.rs:483–501   (append_if: same structure)
src/subscriptions.rs:178–216   (dispatch_sync iterates all Synchronous entries)
```

**Consequence.** Every synchronous subscriber's `on_event` call runs with the write connection's mutex locked. There is no timeout and no yield point inside `dispatch_sync`. A subscriber that does I/O (reads from the same store via a read pool connection, calls an external service, allocates heavily) adds directly to append latency for every writer sharing that store instance. Under contention, this can cause the write mutex to be held for unexpectedly long durations.

**Secondary risk.** A synchronous subscriber that attempts `Store::append` from within `on_event` will deadlock — it will try to acquire the write mutex that is already held by the calling append. This is not guarded against.

**Scope.** PostCommit subscribers are exempt: they receive events from the dispatcher thread after the write lock is released.

---

### A-5. Synchronous subscriber panic → degraded, no SubscriptionDegraded event emitted

**Property.** `dispatch_sync` catches panics via `std::panic::catch_unwind`.

```
src/subscriptions.rs:197–213   (catch_unwind + eprintln + degraded_flag.store)
```

**Consequence.** A panicking synchronous subscriber is marked degraded and receives an `eprintln!` log. No `SubscriptionDegraded` event is written to `_fossic/system`. This is asymmetric with PostCommit overflow (A-3), which does emit a system event.

**Observation gap.** A caller that relies on `_fossic/system` to monitor subscription health will miss sync-subscriber panics entirely. The only signal is `SubscriptionHandle::is_degraded()` — which requires the caller to poll actively.

**Why the asymmetry exists (inference).** Emitting a system event from `dispatch_sync` would require the dispatcher's `SystemStreamWriter` (which lives on a separate thread) or a new writer acquired while holding the write lock. Neither is straightforward without coupling the write path to the system stream write path. The asymmetry appears to be a pragmatic omission rather than an intentional design choice.

---

### A-6. Custom task panic kills fossic-bg permanently

**Property.** `execute_task` dispatches `TaskKind::Custom(f)` via direct invocation: `f()`. No `catch_unwind` wraps the call.

```
src/executor.rs:298–312   (execute_task, case Custom)
src/executor.rs:216–296   (bg_thread_loop: no panic handler around execute_task)
```

**Consequence.** A panicking custom closure unwinds through `bg_thread_loop`. The `done_tx` channel never receives its signal. On `BackgroundExecutor::Drop`, `done_rx.recv_timeout` returns `Disconnected` and the thread handle is joined. No replacement thread is spawned. All subsequent background tasks — `GcOrphanSnapshots`, `TakeSnapshot`, future `Custom` tasks — are silently discarded. `schedule()` calls succeed (the `task_tx` channel is unbounded and disconnected sends are silently ignored), so callers see no error.

**Scope.** Built-in tasks (`GcOrphanSnapshots`, `TakeSnapshot`) have explicit `eprintln!` on `Err(e)` but do not panic. Only user-supplied `Custom` closures are at risk.

**Detection.** No system event is emitted when fossic-bg exits abnormally. The only signal is a thread panic message on stderr (from Rust's default panic handler) and the absence of background task effects (orphan snapshots not GC'd, scheduled snapshots not taken).

---

### A-7. Background executor grace timeout → thread detachment, DropDeferred semantics may surprise

**Property.** `BackgroundExecutor::Drop` signals the stop flag, then waits `grace_timeout` via `done_rx.recv_timeout`. On `RecvTimeoutError::Timeout`, it logs a WARN and detaches — it does not kill the thread.

```
src/executor.rs:193–212   (Drop implementation)
src/executor.rs:233–261   (stop-flag handling in bg_thread_loop: emits DeferredTaskDropped then exits)
```

**Consequence.** The detached thread continues running until it wakes from its 500ms sleep cycle and observes the stop flag. It will drain the task heap, emit `DeferredTaskDropped` for `persist_on_drop` tasks via its lazily-initialized `SystemStreamWriter`, and exit cleanly. This means `DeferredTaskDropped` events may arrive in `_fossic/system` after the `Store` struct and `BackgroundExecutor` have been dropped.

**Why it bites.** A caller that reads `_fossic/system` immediately after `store.close()` to confirm dropped tasks may see an incomplete picture. The `[WARN fossic] fossic-bg did not stop within grace period` log appears before `DeferredTaskDropped` events are emitted, which can give a false impression that tasks were lost rather than deferred.

**Configuration.** `OpenOptions::background_executor_grace_timeout_ms`. Default observed: no explicit default documented in `OpenOptions`; the span from `Store::open` at `src/store.rs:275` uses this field directly.

---

### A-8. Read pool exhaustion → PoolExhausted error, no retry or backoff

**Property.** `Store::read_conn()` calls `recv_timeout(read_pool_timeout_ms)` on the pool receiver. On timeout: `Err(Error::PoolExhausted { pool_size, timeout_ms })`.

```
src/store.rs:1576–1587   (read_conn implementation)
src/store.rs:261–273     (pool initialization in Store::open)
```

**Consequence.** Every read-path method (`read_range`, `read_state`, `read_one`, `read_by_correlation`, `aggregate`, `subscribe`, `snapshot_info`, etc.) acquires from the pool. A `ReadGuard` holds the connection and returns it to the pool on `Drop` (`src/store.rs:57–63`). If `pool_size` connections are all held concurrently and a new read is attempted, the caller blocks for `read_pool_timeout_ms` milliseconds and then receives `PoolExhausted`. There is no retry, no backoff, no priority queue.

**Deadlock risk.** Holding a `ReadGuard` and calling any Store method that internally calls `read_conn()` on the same store with a pool size of 1 will deadlock for `read_pool_timeout_ms` ms, then return `PoolExhausted`. This is possible if a subscription handler (which may hold no lock) calls `store.read_range()` while the only pool connection is held by the outer `subscribe` call that is seeding the cursor.

**Configuration.** `OpenOptions::read_pool_size` (default not shown in this pass; set during `Store::open`), `OpenOptions::read_pool_timeout_ms`.

---

### A-9. aggregate_bounded returns cursor: None on truncation — no fold resume

**Property.** `aggregate_bounded_impl` returns `ReadOutcome::Truncated { cursor: None, ... }` when the event count or byte budget is exceeded.

```
src/cross_stream.rs:654–661   (truncation branch, cursor: None)
src/cross_stream.rs:558–561   (comment: "Deferred to v1.2.x")
```

**Consequence.** A caller that receives `ReadOutcome::Truncated` has no opaque cursor to pass for continuation. They know the fold was cut but cannot determine at which event it stopped or resume from that point. Paging through a large event set via `aggregate_bounded` is not possible. The caller's only options are: (a) use unbounded `aggregate_impl` (which may be rejected by `default_max_events` / `default_max_bytes` if those are set on the store), or (b) manually segment the query by `from_timestamp_us` / `to_timestamp_us`.

**Staleness of deferral comment.** The comment reads "Deferred to v1.2.x" but the current version is v1.8.0. The deferral has been superseded by six major versions without resolution.

---

### A-10. Snapshot race (TD-001) — two separate lock acquisitions allow concurrent append to slip through

**Property.** `Store::take_snapshot` reads events via `read_conn()` and then writes the snapshot via `lock()` (write mutex). These are two separate lock acquisitions with no coordination between them.

```
src/store.rs:1344–1405   (take_snapshot; comment "TD-001: two separate acquisitions")
src/store.rs:1346–1388   (read_conn block: finds snapshot, reads events, computes snap_ver)
src/store.rs:1394–1404   (lock block: writes snapshot)
```

**Consequence.** A concurrent `append()` that commits between the `read_conn` release and the `lock()` acquisition is not included in the snapshot. The snapshot records `snap_ver = last(events_at_read_time).version`. The next `read_state` call correctly reads from `snap_ver + 1` onward and will pick up the missed events — so final state correctness is preserved. However, the snapshot understates the stream head at creation time and reduces snapshot efficacy: the missed events must be replayed on every `read_state` call until a newer snapshot is taken.

**Why it bites in high-write workloads.** If the write rate is high enough that a new event arrives in every snapshot's creation window, snapshots are always slightly stale and accumulate less of a replay-reduction benefit than expected.

**Acknowledged in source.** The comment at `store.rs:1344` cites "blast-radius pass-1.0.0w" as prior context for the known limitation.

---

### A-11. Reducer apply panic propagates uncaught to caller

**Property.** `ErasedReducer::apply_bytes` deserializes state and event, calls `self.reducer.apply(state, &event)`, and serializes the result. No `catch_unwind` wraps the `apply` call.

```
src/reducers.rs:73–80   (ErasedReducer::apply_bytes: apply called directly)
src/store.rs:1244–1247  (read_state: apply_bytes in loop, no catch_unwind)
src/store.rs:1269–1270  (read_state_at_version: same)
src/store.rs:1282–1284  (read_state_bytes: same)
src/store.rs:1302–1303  (read_state_bytes_at_version: same)
src/store.rs:1329–1330  (read_state_at_version_with_reducer: same)
src/store.rs:1390–1391  (take_snapshot: same)
```

**Consequence.** A user-defined `Reducer::apply` implementation that panics (due to an assertion, index out of bounds, unwrap, or arithmetic overflow) unwinds through `apply_bytes`, through the fold loop in `read_state`, and kills the calling thread. Since `read_state` is typically called from application code, this propagates to the application thread rather than an isolated substrate thread.

**Scope.** The `Reducer` trait docs say "Must be a pure function: no I/O, no mutation of `self`, no randomness" but do not say "no panic." A panic in a pure function is plausible (assertion, type narrowing). DynReducer panics follow the same path through `DynReducerAdapter::apply_bytes`.

**Detection.** Rust's default panic handler logs to stderr. No `ReducerPanicked` system event exists.

---

### A-12. Four SystemStreamWriters share _fossic/system — IMMEDIATE transaction contention can silently drop events

**Property.** Four separate `SystemStreamWriter` instances write to `_fossic/system` via independent SQLite connections, each opening `IMMEDIATE` transactions for atomicity.

```
src/store.rs:2143       (dispatcher thread sys_writer — for SubscriptionDegraded)
src/store.rs:152, 1774  (reducer_system_writer — for ReducerStateLarge)
src/store.rs:161, 1514  (project_registry_writer — for ProjectRegistered, RelayHeartbeat)
src/executor.rs:228     (bg executor sys_writer — for DeferredTaskDropped, shutdown only)
```

`SystemStreamWriter::emit()` opens a transaction with `BEGIN IMMEDIATE`:

```
src/system_stream.rs:62–65   (transaction_with_behavior(TransactionBehavior::Immediate))
src/system_stream.rs:27–30   (PRAGMA busy_timeout = 30000)
```

On `Err(_)` from `transaction_with_behavior`, `emit()` silently returns:

```
src/system_stream.rs:62–65   (Err(_) => return)
```

**Consequence.** SQLite WAL mode serializes `IMMEDIATE` transactions through a single write lock. If two writers race (e.g., dispatcher emitting `SubscriptionDegraded` while the reducer writer emits `ReducerStateLarge`), one will wait up to `busy_timeout` (30 000 ms). If it times out — which is unlikely at 30 s but possible under extreme I/O pressure — `emit()` returns silently with the system event dropped. No error is surfaced; no retry occurs.

**Practical frequency.** Items 2 and 3 (`reducer_system_writer`, `project_registry_writer`) are protected by their own `parking_lot::Mutex`, so only one caller at a time holds each. The dispatcher thread holds item 1 exclusively. The background executor (item 4) is only active at shutdown. True concurrent contention between writers is rare under normal operation but structurally possible and not guarded beyond `busy_timeout`.

**Version assignment race.** The `MAX(version) + 1` query inside the transaction is safe against concurrent writers at the SQLite level (IMMEDIATE locks the file for the duration), but only if `begin immediate` succeeds. If it does not, the event is dropped with no version conflict — correctness is preserved at the cost of observability.

---

## PART B — Open Design Questions

These require explicit decisions, not just documentation.

---

### B-1. Should PostCommit degradation be recoverable without resubscription?

Currently: caller must drop `SubscriptionHandle` and resubscribe from current stream tip. Events received during the degraded interval are permanently undeliverable through the subscription path (they remain in the store and can be read directly).

**Option A — Accept as-is.** Document the drain-and-resubscribe pattern. Require consumers to implement gap detection by comparing the last-received version against `MAX(version)` on resubscription. No substrate changes needed.

**Option B — Add SubscriptionHandle::recover().** Drain the backing channel (discarding or buffering pending items), clear the degraded flag, and reset the cursor to the current stream tip. The window of lost events is the same as today, but the caller does not need to create a new handle or re-register the handler. Implementation requires write access to the subscription registry entry from the handle.

**Option C — Auto-recover with configurable queue.** When `TrySendError::Full` is encountered and the subscriber is not yet degraded, drop the oldest item in the channel and retry. Requires switching from `crossbeam_channel::bounded` to a ring-buffer sender. Introduces ordering ambiguity (oldest vs newest drop policy).

**Decision needed:** which recovery model is correct for fossic's contract.

---

### B-2. Should sync subscriber panics emit SubscriptionDegraded to _fossic/system?

Currently: sync panics produce `eprintln!` + degraded flag. PostCommit overflow produces the same flag plus a `SubscriptionDegraded` event. The observability model is asymmetric.

**Why emit:** consumers monitoring `_fossic/system` would see sync degradations alongside PostCommit degradations without additional polling.

**Why not emit:** emitting from `dispatch_sync` (while the write lock is held) requires a dedicated system writer call on the hot path. The dispatcher thread's `SystemStreamWriter` is not accessible from `dispatch_sync` (different thread). A new connection or a shared channel would be needed.

**Minimum viable fix:** after `dispatch_sync` returns (write lock released), check the degraded set against the pre-call set and emit `SubscriptionDegraded` from the dispatcher thread via the existing `sys_writer`. This adds one pass over subscribers per append for any degradation check — acceptable at low subscriber counts, potentially costly at high counts.

**Decision needed:** emit SubscriptionDegraded for sync panics, or document the asymmetry explicitly and leave as-is.

---

### B-3. Should Custom task panics be caught in the executor?

Currently: a panicking custom closure kills fossic-bg permanently. All background tasks are silently lost.

**Proposed fix:** wrap `execute_task` in `catch_unwind` inside `bg_thread_loop`. On `Err(panic_val)`, log to stderr and continue the loop. This isolates task panics to the task, not the thread.

**Risk:** `catch_unwind` requires the closure to be `UnwindSafe`. `TaskKind::Custom` is currently `Arc<dyn Fn() + Send + Sync + 'static>` — `Fn` is not `UnwindSafe` by default. Wrapping in `AssertUnwindSafe` is the pragmatic path, accepting that partially-mutated captured state is possible after a panic.

**Decision needed:** accept task-level panic isolation with `AssertUnwindSafe`, or impose `UnwindSafe` as a bound on `Custom` closures (breaking change to `TaskKind`).

---

### B-4. Should aggregate_bounded support resume cursors?

The "Deferred to v1.2.x" comment at `cross_stream.rs:558` is now three major versions stale (v1.8.0). The capability gap is real: a caller cannot page through a large event set via `aggregate_bounded`.

**What resume requires:** `Aggregate::finalize` consumes `self`. To resume, the caller would need a way to serialize intermediate fold state or re-feed it to a new aggregator instance. Options:

- **Checkpoint-based resume:** on truncation, serialize the aggregator's current state via a new `Aggregate::checkpoint()` method and encode it alongside the event position in the cursor. Resume deserializes the checkpoint into a fresh aggregator and folds from the cursor position. Requires a new trait bound (`Aggregate: Checkpoint` or similar).
- **Position-only cursor:** return the `timestamp_us` or event `id` of the last-folded event. Caller creates a new `Aggregate` from scratch and calls `aggregate_bounded` again with `from_timestamp_us` set to just after the last-folded event. This re-folds from initial state across each page — O(n²) total work but no new trait machinery.
- **Accept as-is:** document that `aggregate_bounded` is not pageable. Callers needing full-corpus aggregation use unbounded `aggregate` or segment by time/stream.

**Decision needed:** implement resume (and if so, which model), or formally close the deferral with a documented limitation.

---

### B-5. Should _fossic/* streams be subscribable with an explicit opt-in?

Currently: the dispatcher thread skips `event.stream_id.starts_with("_fossic/")` before fan-out (`store.rs:2147–2149`). System events are undeliverable via subscription; callers must poll `read_range` on `_fossic/system`.

**Why allow it:** consumers that want reactive monitoring of `SubscriptionDegraded`, `DeferredTaskDropped`, or `ReducerStateLarge` currently must implement their own polling loop. A `SubscribeQuery { include_system: true }` already exists as a field (`subscriptions.rs:26`); what's missing is the dispatcher honoring it.

**Why block it (current design):** allowing `_fossic/*` fan-out creates a feedback loop risk — a subscriber to `_fossic/system` that is itself slow can produce `SubscriptionDegraded` events, which are written to `_fossic/system`, which would be fanned to the same subscriber again. The current hard block prevents this class of cycle.

**Mitigation if opened:** the dispatcher could be modified to skip `SubscriptionDegraded` fan-out when the event originates from `_fossic/*` (second-level blocking), or the system stream itself could be excluded from triggering new `SubscriptionDegraded` events.

**Decision needed:** keep the hard block and require polling, or design a cycle-safe opt-in path.

---

### B-6. Should the TD-001 snapshot race be fixed or formally accepted?

The two-connection snapshot race (`store.rs:1344`) has been acknowledged in source since at least pass 1.0.0w. The choice has not been made explicit.

**Option A — Accept and document.** Add a note to `take_snapshot` docs that the snapshot may be slightly stale under concurrent writes. The correctness guarantee (correct final state on next `read_state`) already holds. Callers in high-write environments should expect stale snapshots and rely on the incremental replay to fill the gap.

**Option B — Serialized snapshot via write lock.** Hold the write lock for the entire snapshot: read events, fold state, and write the snapshot row all within a single `lock()` acquisition. This eliminates the race window. Cost: the write mutex is held for the duration of the fold (potentially slow for long event histories), blocking concurrent appends.

**Option C — Optimistic version lock.** Take snapshot with a read_conn as today, but in the write phase check that `MAX(version)` has not advanced beyond `snap_ver` (using `append_if`-style conditional INSERT). If it has, re-read the new events, fold, and retry. Maintains low write-lock hold time but may loop under sustained writes.

**Decision needed:** which correctness/performance trade-off is acceptable for v2.x.

---

*End of SR-10 — no code changes in this pass.*
