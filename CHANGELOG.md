# Changelog

All notable changes to fossic are documented here.
Format: semantic version sections, newest first. Each section links to the pass report.

---

## v1.6.0 — 2026-06-21 — Phase 1 close: Bounded Resource API

### Phase 1 closed

Phase 1 ships the OOM safety net and the foundational primitives that Phases 2–5 depend on. All existing v1.0 API call sites continue to work unchanged — the new bounded surface is additive only.

Shipped across v1.1.0 through v1.1.9:

- **v1.1.0** — Foundation types (`ReadOutcome`, `TruncationCursor`, `SamplingMode`, `BudgetKind`), `OpenOptions` defaults, dispatch channel observability, `parking_lot` direct dep
- **v1.1.1** — `SystemStreamWriter` abstraction; `SubscriptionDegraded` emit refactored through it
- **v1.1.2** — `read_range_bounded` + `read_by_correlation_bounded` with cursor resume
- **v1.1.3** — `walk_causation_bounded` with three sampling modes (Rust-side BFS replaces recursive CTE)
- **v1.1.4** — `aggregate_bounded` with Clone-snapshot finalize
- **v1.1.5** — Streaming iterators (`read_range_iter`, `read_by_correlation_iter`, `walk_causation_iter`) with pool-release invariant
- **v1.1.6** — Python binding surface (`fossic-py`)
- **v1.1.7** — Node.js binding surface (`fossic-node`)
- **v1.1.8** — Tauri IPC bounded commands (`fossic-tauri`)
- **v1.1.9** — Documentation pass

Track 2 shipped Phases 6, 7, and 8 in parallel (v1.2.0–v1.5.0). Version v1.6.0 post-dates both tracks.

Next: Phase 2 — Hardware-Aware Defaults; Phase 3 — Pressure-Aware Substrate.

---

## v1.1.9 — 2026-06-21 — Documentation pass

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.9.md`

### Changed

- `README.md` — new `## Bounded reads and streaming iterators` section (API selection table, `ReadOutcome` enum, `TruncationCursor` usage, streaming iterators, `SamplingMode` table, `aggregate_bounded` cursor note); new `## Observability` section (`dispatch_channel_pressure`, `dispatch_channel_high_water_mark`)
- `fossic-py/README.md` — new `## Bounded reads and streaming iterators` section (Python syntax, `ReadOutcome` properties, `TruncationCursor`, `SamplingMode`, streaming iterators, bounded method signatures, CP-FOSSIC-3 workaround note)
- `fossic-node/README.md` — new `## Bounded reads and streaming iterators` section (TypeScript syntax, `ReadOutcome` discriminated union, `TruncationCursor`, `SamplingMode`, streaming iterators, bounded method signatures, `defaultMaxResults`/`defaultMaxBytes` OpenOptions note)
- `crates/fossic-tauri/README.md` — 7 new bounded commands added to IPC table; new `## Bounded read commands` section (`ReadOutcome` JSON shape, cursor resumption, `SamplingMode` JSON, `fossic_aggregate_bounded`, streaming limitation note)
- `docs/implement/FOSSIC_V1_SPEC.md` — §4.1 updated: `OpenOptions` budget defaults, bounded methods + streaming iterators in `impl Store`, observability methods, canonical type defs (`ReadOutcome`, `TruncationReason`, `TruncationCursor`, `SamplingMode`, `ReadQuery`); §10.1–10.3 updated with bounded variant notes; §18 Phase Roadmap added (Phase 1 arc table, Phases 2–5 descriptions)

---

## v1.1.8 — 2026-06-21 — Tauri IPC bounded read commands

**fossic-tauri** gains bounded read commands with cursor resumption.

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.8.md`

### New commands (fossic-tauri)
- `fossic_read_range_bounded` / `fossic_read_range_from_cursor`
- `fossic_read_by_correlation_bounded` / `fossic_read_by_correlation_from_cursor`
- `fossic_walk_causation_bounded` / `fossic_walk_causation_from_cursor`
- `fossic_aggregate_bounded`

### New types (fossic-tauri)
- `SerializedReadOutcome` — `{ kind, results, reason?, next_cursor? }` (reason/next_cursor omitted on complete)
- `TruncationCursor` serialized as base64 string over IPC (vs. `Buffer` in the Node binding)

### Notes
- Streaming (push over Tauri event channel) is deferred to v1.2.x; use cursor pagination as substitute
- `fossic_aggregate_bounded` truncation cursor is always null — fold-resume not yet supported by the `Aggregate` trait (deferred to v1.2.x)
- All existing commands unchanged

---

## v1.5.0 — 2026-06-21 — Track 2 close: fossic core substrate-complete

**Pass report:** `docs/aseptic/blast-radius/pass-1.5.0.md`

Track 2 closes. Phases 6, 7, and 8 are fully shipped. The fossic substrate is
complete for local-first event sourcing with background execution, quiescence-gated
snapshot policies, and federated project discovery primitives.

**Track 2 arc:**
- v1.2.0 — `EveryNEvents` snapshot policy (Phase 6 open)
- v1.2.1 — `ReducerStateLarge` diagnostics + `StateAdaptive` policy
- v1.2.2 — `auto_gc_orphans`, Phase 6 close
- v1.3.0 — `BackgroundExecutor` + `QuiescenceMonitor` scaffold (Phase 7 open)
- v1.3.1 — `EveryNSeconds` enforcement + recurring background GC, Phase 7 close
- v1.4.0 — `ProjectRegistered` + `RelayHeartbeat` emit primitives (Phase 8 open)
- v1.4.1 — Project registration docs pass
- v1.5.0 — Track 2 close (this version)

No new API or behavior changes. Version bump marks the substrate-complete milestone.

---

## v1.4.1 — 2026-06-21 — Documentation: project registration for federated deployments

**Pass report:** `docs/aseptic/blast-radius/pass-1.4.1.md`

### Changed

- `README.md` — new `## Project Registration (for federated deployments)` section:
  manual registration spec table, `RelayConfig` heartbeat example, indexed_tags note,
  forward-link to §15 (fossic-coordinator) and §9.4 (event schema).
- `docs/implement/FOSSIC_V1_SPEC.md §9.4` — `ProjectRegistered` and `RelayHeartbeat`
  added to the `_fossic/system` event type table with trigger, payload fields, and
  `indexed_tags` schema. No new section added — the existing event-type table is the
  canonical home. A full federation protocol section is deferred as CP-T2-2 (future
  fossic-coordinator crate work).

---

## v1.4.0 — 2026-06-21 — Phase 8: Hub Coordinator Preparation

**Pass report:** `docs/aseptic/blast-radius/pass-1.4.0.md`

### Added

- `Store::emit_project_registered(source_store, local_store_path, subscribe_pattern,
  project_description)` — emits a `ProjectRegistered` event to `_fossic/system`.
  Call on relay agent startup and on first hub-direct write to announce this
  project's local store and relay pattern.
- `Store::emit_relay_heartbeat(source_store, last_event_version, queue_lag,
  uptime_us)` — emits a `RelayHeartbeat` event to `_fossic/system`. Call from a
  heartbeat thread at the configured interval.
- `src/registry.rs` — substrate-side emit-only helpers for both event types.
  Receives a `&mut SystemStreamWriter`; no `Store`/`StoreInner` dependency.
- `StoreInner::project_registry_writer` — lazy `Mutex<Option<SystemStreamWriter>>`
  for the registry writer; same pattern as `reducer_system_writer`. Dedicated
  connection so relay threads never contend with dispatcher or reducer writers.
- Both system events carry `indexed_tags = {"source_store": "<name>"}` for future
  coordinator filtering.
- **Python (`fossic-py`):** `Store.emit_project_registered` and
  `Store.emit_relay_heartbeat` bindings. `RelayConfig.heartbeat_interval_s`
  (default 5.0 s) and `RelayConfig.project_description`. `RelayAgent` spawns a
  daemon heartbeat thread and calls `emit_project_registered` on startup.

---

## v1.3.1 — 2026-06-21 — Phase 6/7 integration: EveryNSeconds + recurring GC

**Pass report:** `docs/aseptic/blast-radius/pass-1.3.1.md`

### Added

- `BacklogTask::recurring_interval: Option<Duration>` — when `Some(d)`, the executor
  re-queues the task with `deadline = now + d` after each execution. Tasks in the heap
  at shutdown are drained and handled (persist_on_drop/log) without re-queuing.
- `Store::schedule_background_snapshot` (private) — enqueues a `TakeSnapshot` task at
  `TaskPriority::Normal` without a recurring interval.
- `StoreInner::last_snapshot_us: parking_lot::RwLock<HashMap<(String, String), i64>>` —
  per-`(stream_id, branch)` timestamp of the most recent snapshot schedule; updated
  optimistically at schedule time to prevent storm-scheduling in busy `read_state` loops.
- `StoreInner::store_open_us: i64` — Store open timestamp used as the first-snapshot
  fallback when no entry exists in `last_snapshot_us`.

### Changed

- `SnapshotPolicy::EveryNSeconds` — now live. `validate_snapshot_policy` accepts
  `N >= 1`; `N = 0` returns `SnapshotPolicyInvalid`. Was previously `NotImplemented`.
- `maybe_auto_snapshot` — added `EveryNSeconds(N)` arm: checks `last_snapshot_us`
  (fallback: `store_open_us`), marks the window optimistically, calls
  `schedule_background_snapshot`. Snapshot executes during the next quiescent window.
- `StoreOps::bg_take_snapshot` — fully implemented on `StoreInner` (was
  `NotImplemented` placeholder). Replicates `Store::take_snapshot` logic using raw
  fields; updates `last_snapshot_us` after a successful write.
- `TaskKind` — derives `Clone` (required for recurring re-push).
- `execute_task` — now takes `&BacklogTask` (was `BacklogTask`); re-push is handled
  in the caller (`bg_thread_loop`) after execution returns.
- `Store::open` — when `auto_gc_orphans=true`, schedules an initial
  `GcOrphanSnapshots` task with `recurring_interval=Some(Duration::from_secs(3600))`
  at `TaskPriority::Low`.
- `snapshots.rs` CP-T2-1 marker — updated to "RESOLVED (v1.3.1)".
- `tests/snapshot_policy.rs` — `policy_not_implemented_seconds` renamed to
  `policy_every_n_seconds_accepted`; new `policy_every_n_seconds_zero_rejected` test.

### Test count

318 passing (was 317 in v1.3.0; +1: `policy_every_n_seconds_zero_rejected`).

---

## v1.3.0 — 2026-06-21 — Phase 7: BackgroundExecutor + QuiescenceMonitor

**Pass report:** `docs/aseptic/blast-radius/pass-1.3.0.md`

### Added

- `src/executor.rs` (new module) — crate-private Background Executor implementation.
  - `StoreOps` trait — capability surface the executor needs from the store
    (`bg_gc_orphaned_snapshots`, `bg_take_snapshot`). Implemented on `StoreInner`; executor
    holds a `Weak<dyn StoreOps>` so it never keeps the store alive.
  - `TaskPriority` enum — `Low=0`, `Normal=1`, `High=2` (derives `Ord`).
  - `TaskKind` enum — `GcOrphanSnapshots`, `TakeSnapshot { stream_id, branch }`.
  - `BacklogTask` struct — priority + deadline_us + persist_on_drop + kind. Implements `Ord`
    for `BinaryHeap<BacklogTask>`: higher priority first; equal priority → earlier deadline first.
  - `QuiescenceMonitor` — two `AtomicI64`: `last_write_us` and `last_subscription_dispatch_us`.
    Both initialised to `now_us()` at construction. Methods: `note_write()`, `note_dispatch()`,
    `is_quiescent(window_us) -> bool`.
  - `BackgroundExecutor` — spawns the `fossic-bg` thread at `Store::open` time.
    `schedule()` enqueues a `BacklogTask`. `impl Drop` signals the stop-flag and
    waits `grace_timeout` for the thread via a `crossbeam_channel::bounded::<()>(1)` done
    channel; times out and detaches (does not kill) on expiry.
  - `bg_thread_loop` — 500ms poll interval. Stop-path drains remaining tasks: emits
    `DeferredTaskDropped` system events for `persist_on_drop=true` tasks (lazy-opening a third
    `SystemStreamWriter` connection), logs and drops others. Normal-path: drain channel into
    `BinaryHeap`, quiescence gate, pop and execute one task per loop.
  - Unit tests: heap ordering (high-before-low, earlier-deadline first), QuiescenceMonitor
    (not quiescent immediately after write or dispatch).
- `OpenOptions::background_executor_grace_timeout_ms: u64` — grace period in milliseconds
  before executor is detached at store close. Default: 10,000ms.
- `OpenOptions::executor_quiescence_window_ms: u64` — minimum quiet window (both write and
  dispatch idle) before executor runs a task. Default: 2,000ms.
- `StoreInner::quiescence: Arc<QuiescenceMonitor>` — shared with the dispatcher thread.
- `StoreInner::background_executor: parking_lot::Mutex<Option<BackgroundExecutor>>` —
  `Mutex` required because `JoinHandle` is `!Sync`. Initialised to `None` then set after
  `Arc<StoreInner>` construction so `Weak` downgrade is possible.
- `impl StoreOps for StoreInner` — wires `bg_gc_orphaned_snapshots` to the existing
  `gc_orphaned_snapshots_impl` path; `bg_take_snapshot` returns `NotImplemented` (placeholder
  for v1.3.1 EveryNSeconds).
- `quiescence.note_write()` called after every successful `append`, `append_batch`,
  `append_if` (non-conditional: `append_if` only notes when a write actually happened).
- `quiescence.note_dispatch()` called inside `start_dispatcher` after each post-commit
  dispatch round.
- `fossic-py/src/types.rs` manual `OpenOptions` literal — two new fields with defaults.
- `tests/executor.rs` — two new tests: `executor_lifecycle_no_hang`,
  `executor_short_grace_closes_within_timeout`.
- `Cargo.toml` — `[[test]] name = "executor"`.

### Changed

- `Store::open` — creates `QuiescenceMonitor` before `start_dispatcher`, passes it to the
  dispatcher, and after `Arc<StoreInner>` construction coerces to `Weak<dyn StoreOps>` and
  spawns `BackgroundExecutor`.
- `start_dispatcher` signature — added `quiescence: Arc<QuiescenceMonitor>` parameter.

### Test count

317 passing (was 286 in v1.2.2; +31: 4 executor unit tests, 2 executor integration tests,
25 additional tests added by linter passes between v1.2.2 and v1.3.0).

---



## v1.2.2 — 2026-06-21 — auto_gc_orphans: drop-time GC fallback (Phase 6 close)

**Pass report:** `docs/aseptic/blast-radius/pass-1.2.2.md`

### Added

- `OpenOptions::auto_gc_orphans: bool` — when `true`, `gc_orphaned_snapshots` is called at
  store drop time (when the last `Store` clone is dropped), purging snapshots whose reducer is
  no longer registered. Default: `false`. Drop-time GC fires only on the last clone (guarded by
  `Arc::strong_count == 1`). Phase 7 (v1.3.1) supplements this with background-scheduled GC
  via `BackgroundExecutor`; this drop-time call is retained as final-shutdown cleanup even when
  Phase 7 is present.
- `impl Drop for Store` — wires the `auto_gc_orphans` flag; errors are silently dropped (GC is
  best-effort; callers who need a count can call `gc_orphaned_snapshots` explicitly).
- CP-T2-1 marker in `src/snapshots.rs` — Phase 7 integration point for the GC scheduler.
- `fossic-py/src/types.rs` — manual `OpenOptions` struct literal updated with
  `auto_gc_orphans: false`.
- `tests/snapshots.rs` — 3 new tests: `auto_gc_orphans_flag_off_no_gc_on_drop`,
  `auto_gc_orphans_flag_on_gc_fires_on_drop`, `auto_gc_orphans_only_fires_on_last_clone_drop`.

---

## v1.2.1 — 2026-06-21 — ReducerStateLarge emission + StateAdaptive policy

**Pass report:** `docs/aseptic/blast-radius/pass-1.2.1.md`

### Added

- `OpenOptions::reducer_state_large_threshold_bytes: usize` — rolling-mean state-size threshold
  (bytes) above which `ReducerStateLarge` is emitted to `_fossic/system`. Computed over the last
  32 `apply_bytes` results per `(stream_id, branch)`. Emission throttled to once per 60 seconds.
  Default: 1 MiB (1_048_576). Set to `usize::MAX` to disable.
- `StateMonitor` struct (crate-private) — rolling buffer of last 32 state sizes and apply costs
  per `(stream_id, branch)`. Methods: `mean_state_size()`, `avg_apply_cost_us()`.
- `StoreInner::reducer_system_writer: parking_lot::Mutex<Option<SystemStreamWriter>>` — lazy-
  initialized system-stream writer for reducer-side emissions. Separate from the dispatcher's
  writer; owns its own SQLite connection. Initialized on first `ReducerStateLarge` event.
- `StoreInner::state_monitors: parking_lot::Mutex<HashMap<(String, String), StateMonitor>>` —
  per-`(stream_id, branch)` rolling monitor; populated inside the `read_state` apply loop.
- `Store::update_state_monitor` (private) — called per-event in the apply loop; updates rolling
  state-size and apply-cost buffers.
- `Store::maybe_emit_state_large` (private) — checks mean vs. threshold, enforces 60-second
  throttle, lazy-inits writer, emits `ReducerStateLarge` to `_fossic/system`.
- `SnapshotPolicy::StateAdaptive` now live — enabled in `validate_snapshot_policy` (previously
  returned `NotImplemented`). Logic in `maybe_auto_snapshot`: fires when
  `accumulated_events × avg_apply_cost_us > target_replay_cost_us` AND
  `accumulated >= min_events_between`; counter resets same as `EveryNEvents`.
- `fossic-py/src/types.rs` — manual `OpenOptions` struct literal updated with
  `reducer_state_large_threshold_bytes: 1_048_576`.
- `tests/snapshot_policy.rs` — 4 new tests: `state_adaptive_policy_accepted`,
  `state_adaptive_triggers_snapshot`, `state_adaptive_respects_min_events_between`,
  `state_large_emits_to_system_stream`, `state_large_throttled`. Previous
  `policy_not_implemented_adaptive` renamed and inverted.

### Changed

- `Store::read_state` and `Store::read_state_bytes` apply loops now time each `apply_bytes`
  call (`now_us()` delta) and call `update_state_monitor`; `maybe_emit_state_large` is called
  after the loop, before `maybe_auto_snapshot`.

### Architecture

Two `SystemStreamWriter` instances in steady state: dispatcher's (held on dispatch thread) and
reducer's (lazy Mutex on StoreInner). Both write to `_fossic/system`; WAL handles concurrent
writes from separate connections without contention.

---

## v1.2.0 — 2026-06-20 — SnapshotPolicy: EveryNEvents registration and wiring

**Pass report:** `docs/aseptic/blast-radius/pass-1.2.0.md`

### Added

- `SnapshotPolicy` — public enum: `Manual` (default), `EveryNEvents(u32)`, `EveryNSeconds(u32)`,
  `StateAdaptive { target_replay_cost_us, min_events_between }`. Re-exported from crate root.
- `Error::SnapshotPolicyInvalid(String)` — returned when a policy is structurally invalid
  (e.g. `EveryNEvents(0)`).
- `ReducerRegistry::register_with_policy` / `register_dyn_with_policy` — register a reducer
  with an explicit `SnapshotPolicy`; existing `register` / `register_dyn` delegate to these
  with `Manual`.
- `ReducerRegistry::find_arc_with_policy` — returns `Option<(Arc<dyn BoxedReducer>, SnapshotPolicy)>`;
  existing `find_arc` delegates to this.
- `reducers::validate_snapshot_policy(policy) -> Result<(), Error>` — standalone validation
  function; called at registration time.
- `Store::register_reducer_with_policy<R: Reducer>` — public surface for typed reducers.
- `Store::register_dyn_reducer_with_policy` — public surface for `DynReducer` bridges.
- `StoreInner::snapshot_counters: parking_lot::RwLock<HashMap<(String, String), u32>>` — per-
  `(stream_id, branch)` accumulating event counter; resets to 0 when a snapshot fires.
- `Store::read_state` and `Store::read_state_bytes` wired to call `maybe_auto_snapshot` after
  folding. Historical variants (`read_state_at_version`, `read_state_bytes_at_version`) are
  intentionally not wired (historical reads must not advance the snapshot cadence counter).
- `tests/snapshot_policy.rs` — 7 tests covering policy validation and `EveryNEvents` behavior.

### Not yet implemented

- `EveryNSeconds` and `StateAdaptive` return `Error::NotImplemented` at registration time.
  `EveryNSeconds` requires the Phase 7 background executor (v1.3.x);
  `StateAdaptive` requires v1.2.1 state-size monitoring.

---

## v1.1.7 — 2026-06-21 — Node binding surface: bounded reads + streaming async iterables

**fossic-node** gains the bounded read and streaming iterator surface introduced in v1.1.3–v1.1.5.

### New types (fossic-node)
- `ReadOutcome` — TypeScript discriminated union; `kind: 'complete' | 'truncated'`, `results`, `reason`, `nextCursor`
- `TruncationCursor` — opaque class; `.toBytes()` → `Buffer`, static `.fromBytes(buf: Buffer)`
- `SamplingMode` — namespace with constructor functions `.exhaustive()`, `.breadthFirst(maxPerLevel)`, `.adaptive(targetCount)`
- `FossicRangeIter`, `FossicCorrelationIter`, `FossicCausationIter` — `AsyncIterable<StoredEvent>`; `for await` works directly

### New methods on `Store` (fossic-node)
- `readRangeBounded(query, maxResults?, maxBytes?, cursor?)` → `Promise<ReadOutcome>`
- `readByCorrelationBounded(correlationId, maxResults?, maxBytes?, cursor?)` → `Promise<ReadOutcome>`
- `walkCausationBounded(start, direction, maxDepth?, sampling?, maxResults?, maxBytes?, cursor?)` → `Promise<ReadOutcome>`
- `readRangeIter(query)` → `FossicRangeIter`
- `readByCorrelationIter(correlationId)` → `FossicCorrelationIter`
- `walkCausationIter(start, direction, maxDepth?, sampling?)` → `FossicCausationIter`

### OpenOptions additions (fossic-node)
- `defaultMaxResults?: number` — store-level result budget applied when per-call budget is absent
- `defaultMaxBytes?: number` — store-level byte budget; CP-FOSSIC-3 fix from the Python pass, not repeated here

### Notes
- Pool connections are released before each async yield — same invariant as v1.1.5
- Wrong-type cursors (e.g. range cursor passed to a correlation query) raise `FossicError` at the Rust boundary
- `Option<TruncationCursorJs>` cannot be embedded in `#[napi(object)]`; Buffer passthrough with JS-layer wrapping keeps the cursor type opaque

---

## v1.1.6 — 2026-06-21 — Python binding surface: bounded reads + streaming iterators

**fossic-py** gains the bounded read and streaming iterator surface introduced in v1.1.3–v1.1.5.

### New types (fossic-py)
- `ReadOutcome` — tagged-union class; `.is_truncated`, `.complete`, `.results`, `.reason`, `.next_cursor`
- `TruncationCursor` — opaque; `.to_bytes()` / classmethod `.from_bytes(b)`
- `SamplingMode` — static constructors `.exhaustive()`, `.breadth_first(max_per_level=N)`, `.adaptive(target_count=N)`
- `RangeIter`, `CorrelationIter`, `CausationIter` — Python iterators backed by Rust batch-fetch iterators

### New methods on `Store` (fossic-py)
- `read_range_bounded(query, max_results, max_bytes, cursor)` → `ReadOutcome`
- `read_by_correlation_bounded(correlation_id, max_results, max_bytes, cursor)` → `ReadOutcome`
- `walk_causation_bounded(start, direction, max_depth, sampling, max_results, max_bytes, cursor)` → `ReadOutcome`
- `read_range_iter(query)` → `RangeIter`
- `read_by_correlation_iter(correlation_id)` → `CorrelationIter`
- `walk_causation_iter(start, direction, max_depth, sampling)` → `CausationIter`

### Notes
- `ReadOutcome.next_cursor` is `None` for `aggregate_bounded` (cursor is `Option` at Rust level; Python surface mirrors this exactly)
- `PyOpenOptions` does not yet expose `default_max_results` / `default_max_bytes` — two test cases are explicitly skipped
- Streaming iterators release the pool connection before each Python yield — same invariant as v1.1.5

### Test coverage
`fossic-py/tests/test_bounded.py` — 20 tests, parity with `tests/bounded_foundation.rs` and `tests/bounded_reads.rs`

---

## v1.1.5 — 2026-06-21 — Bounded Resource API: streaming iterators

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.5.md`

### Added

- `Store::read_range_iter(query: ReadQuery) -> RangeIter` — streaming iterator over
  `read_range`. Fetches events in internal batches of 100; pool connection acquired and
  released per batch, never held across `Iterator::next` yield points.
- `Store::read_by_correlation_iter(correlation_id: EventId) -> CorrelationIter` — streaming
  iterator over `read_by_correlation`. Same batch model.
- `Store::walk_causation_iter(start, direction, max_depth, sampling) -> CausationIter` —
  streaming iterator over the BFS causation graph. Same batch model.
- All three types implement `Iterator<Item = Result<StoredEvent, Error>>` and
  `FusedIterator` — safe to call `next()` after `None`.
- `tests/streaming_iters.rs` — 14 tests: empty/non-empty paths for all three iterators,
  fused-after-exhaustion, cross-batch-boundary continuity (105 events), pool-release
  invariant (pool_size=1 + concurrent reader confirms connection is returned before yield).

### Changed

- `WalkDirection` derives `Debug, Clone, Copy, PartialEq, Eq` (previously no derives).
  Additive change; no call-site impact.
- `ReadQuery` derives `Clone` (previously no derives). Additive.

### No aggregate_iter

`aggregate` is fold-shaped and doesn't fit iterator semantics. The `restore()` gap documented
in v1.1.4 also means fold-resume isn't ready. Deferred to v1.2.x.

### Pool invariant

The pool connection is acquired inside `fetch_batch()`, which returns before `next()` yields.
The pool is never held across a yield boundary. Confirmed by the
`iterator_releases_pool_connection_between_yields` test (pool_size=1; concurrent reader
succeeds in bounded time).

---

## v1.1.4 — 2026-06-21 — Bounded Resource API: aggregate_bounded with Clone-snapshot finalize

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.4.md`

### Added

- `Store::aggregate_bounded<A: Aggregate + Clone>(query, agg, max_events_scanned, max_bytes)`
  — bounded aggregate variant. Folds events until `max_events_scanned` events have been
  processed or `max_bytes` of payload have accumulated. On truncation, clones the aggregator
  at the cut point and calls `finalize()` on the clone; returns `ReadOutcome<A::Output>`.
- `aggregate_bounded_impl` in `src/cross_stream.rs` — budget loop with at-least-one guarantee
  for byte budget (first event always folds even if its payload alone exceeds the ceiling).
  Result-count budget fires after `N` events have been folded and more remain.
- `tests/aggregate_bounded.rs` — 11 tests: complete/empty/truncated paths, at-least-one byte
  guarantee, partial-finalize correctness (Summator), store-default + per-call override,
  event_type_filter respected, count-beats-bytes priority, `cursor: None` on all Truncated results.

### Changed

- `ReadOutcome::Truncated.cursor` widened from `TruncationCursor` to `Option<TruncationCursor>`.
  Pageable reads (range, correlation, causation walk) continue to return `Some(cursor)`.
  `aggregate_bounded` returns `cursor: None` — fold-resume requires re-feeding partial state
  into a new aggregator instance, which `Aggregate` does not yet support. Deferred to v1.2.x.
  All in-tree call sites updated (construction wrapped in `Some`; resume loops drop the extra
  `Some(...)` wrapper since the extracted cursor is already `Option`).

### No resume cursor in v1.1

Aggregate resume requires a `restore(partial_output) -> Self` method or equivalent injection
point on the `Aggregate` trait. Not introduced here. Callers needing resume can re-run with a
`from_timestamp_us` offset, or use the unbounded `aggregate` if result-size bounding is not needed.

### Budget resolution

Per-call arg → `OpenOptions::default_max_results` / `default_max_bytes` → unbounded.
`default_max_results` is reused as the events-scanned default; aggregate truncation is on
input size (events read), not output size.

---

## v1.1.3 — 2026-06-21 — Bounded Resource API: walk_causation_bounded with sampling modes

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.3.md`

### Added

- `Store::walk_causation_bounded(start, direction, max_depth, sampling, max_results, max_bytes, resume)`
  — bounded BFS causation walk. Cuts at BFS level boundaries (whole levels always yielded);
  first level is always returned regardless of budget (at-least-one guarantee).
- `walk_causation_bounded_impl` in `src/cross_stream.rs` — Rust-side BFS loop (one SQL query
  per level) replacing the recursive CTE approach, enabling clean level-boundary cut points.
  `seen: HashSet<[u8;32]>` deduplicates within a call; resume initialises `seen` from the
  cursor frontier to prevent re-yielding via convergent paths.
- `bfs_expand_forward` / `bfs_expand_backward` / `expand_frontier` — one-level BFS helpers;
  `ORDER BY id ASC` throughout for deterministic ordering.
- `apply_bfs_sampling(events, sampling, max_depth)` — sampling truncation at level boundaries:
  `Exhaustive` = no truncation; `BreadthFirst { max_per_level }` = take first N by `id ASC`;
  `Adaptive { target_count }` = `max_per_level = max(1, target_count / max_depth)`.
- `CursorInner::Causation` corrected: `{ frontier: Vec<[u8;32]>, direction: u8, depth_consumed: u32 }`.
  Previous v1.1.0 design (`start_id, depth, last_seen_id`) was wrong for frontier-based BFS.
  No external call sites; safe to correct before any consumer exists.
- `tests/causation_bounded.rs` — 14 tests: forward/backward/both walks, result-count and
  byte-budget truncation, at-least-one guarantee, full pagination, max_depth, empty BFS,
  BreadthFirst and Adaptive sampling, store-level default fallback, cursor type mismatch,
  direction mismatch errors.

### Budget resolution

Per-call arg → `OpenOptions::default_max_results` / `default_max_bytes` → unbounded.
Resolution in the public `Store` method; impl receives already-resolved `Option<usize>` values.

---

## v1.1.2 — 2026-06-20 — Bounded Resource API: read_range_bounded + read_by_correlation_bounded

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.2.md`

### Added

- `Store::read_range_bounded(q, max_results, max_bytes, resume)` — bounded `read_range`
  variant. Stops at `max_results` events or `max_bytes` payload, whichever is first.
  Always returns at least one event (byte-budget at-least-one guarantee). Returns
  `ReadOutcome::Complete` when the budget is not hit.
- `Store::read_by_correlation_bounded(correlation_id, max_results, max_bytes, resume)` —
  bounded `read_by_correlation` variant. Uses `ORDER BY id ASC` (32-byte BLOB
  lexicographic) for deterministic resume; resume predicate is `id > last_seen_id`.
- `read_range_bounded_impl` in `src/read.rs` — row-by-row budget tracking; cursor
  encodes `next_version` for exact resume.
- `read_by_correlation_bounded_impl` in `src/cross_stream.rs` — same budget model;
  `(?2 IS NULL OR id > ?2)` resume clause so no-resume and resume share one SQL path.
- `tests/bounded_reads.rs` — 14 tests: truncation at count/bytes, full pagination,
  resume correctness, store-level default fallback, cursor type mismatch error.

### Fixed

- `CursorInner::Correlation::after_timestamp_us: i64` → `last_seen_id: [u8; 32]`.
  The v1.1.0 field name/type was wrong for the `id > last_seen_id` predicate.
  `CursorInner` is `pub(crate)` with no external call sites; safe to correct now.

### Budget resolution (both methods)

Per-call arg → `OpenOptions::default_max_results` / `default_max_bytes` → unbounded.
Resolution happens in the public `Store` method, not in the `*_bounded_impl` helper.

---

## v1.1.0 — 2026-06-20 — Bounded Resource API: Foundation Types

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.0.md`

### Added

- `ReadOutcome<T>` — discriminated enum for bounded read results: `Complete(T)` and
  `Truncated { data, cursor, reason }`. Structurally distinct from existing unbounded reads.
- `TruncationCursor` — opaque resume token. Internal msgpack encoding; three inner variants
  (`Range`, `Correlation`, `Causation`) corresponding to the three cross-stream read shapes.
  Public API: `from_bytes`, `into_bytes`, `as_bytes`.
- `TruncationReason` — `ResultCount` | `ByteSize`; indicates which budget was hit.
- `BudgetKind` — `ResultCount` | `ByteSize`; used in `Error::ReadBudgetExceeded`.
- `SamplingMode` — `Exhaustive` | `BreadthFirst { max_per_level }` | `Adaptive { target_count }`;
  controls graph-walk truncation strategy for the upcoming `walk_causation` bounded variant.
- `Error::ReadBudgetExceeded { budget: BudgetKind, limit: usize }` — returned when a bounded
  read exceeds its configured ceiling. Not yet raised by any production code path (v1.1.2+).
- `OpenOptions::default_max_results: Option<usize>` — store-level default result-count ceiling
  for bounded reads. `None` = no default (callers supply per-call budget).
- `OpenOptions::default_max_bytes: Option<usize>` — store-level default byte-size ceiling.
- `Store::dispatch_channel_pressure() -> usize` — current pending-event count in the
  post-commit dispatch channel. Live observable; useful for back-pressure detection.
- `Store::dispatch_channel_high_water_mark() -> usize` — historical peak channel depth since
  store open. Updated atomically at each `append` / `append_batch` send site.
- `StoreInner::dispatch_channel_high_water_mark: Arc<AtomicUsize>` — backing field for HWM.
- `tests/bounded_foundation.rs` — 15 tests covering all new types and observability methods.

### Changed

- `Cargo.toml` version bumped: `0.1.0` → `1.1.0`.

### Not yet raised

- `Error::ReadBudgetExceeded` — present in the error enum but no call site yet. Ships in v1.1.2
  when `read_range_bounded` and `read_by_correlation_bounded` are implemented.
- `TruncationCursor::encode` / `decode` — present but unused until bounded read methods ship.

---

## v1.0.0aa — 2026-06-17

Relay infrastructure shipped: `RelayConfig`, `RelayAgent`, `relay_append`, `run_relay` in
`fossic-py/fossic/relay.py`. See commit 42ca201.
