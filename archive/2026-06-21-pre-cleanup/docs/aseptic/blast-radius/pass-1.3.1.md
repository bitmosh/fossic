# Blast Radius — v1.3.1

**pass:** v1.3.1 — Phase 6/7 integration: EveryNSeconds + recurring GC
**prior-commit:** 4c73e0f
**date:** 2026-06-21

## Files changed

### Modified files
- `src/executor.rs` — `TaskKind` derives `Clone`; `BacklogTask::recurring_interval:
  Option<Duration>` added; `execute_task` takes `&BacklogTask`; `bg_thread_loop`
  re-queues recurring tasks after execution
- `src/reducers.rs` — `EveryNSeconds(0)` → `SnapshotPolicyInvalid`;
  `EveryNSeconds(_)` → `Ok(())`
- `src/snapshots.rs` — CP-T2-1 comment updated to RESOLVED
- `src/store.rs` — `StoreInner`: `last_snapshot_us` + `store_open_us` fields;
  `Store::open`: initialise both, schedule recurring `GcOrphanSnapshots` when
  `auto_gc_orphans=true`; `maybe_auto_snapshot`: `EveryNSeconds` arm;
  `schedule_background_snapshot` helper; `bg_take_snapshot` fully implemented
- `tests/snapshot_policy.rs` — renamed + new zero-rejection test
- `CHANGELOG.md` — v1.3.1 entry
- `Cargo.toml` — v1.3.0 → v1.3.1

### New files
- `docs/aseptic/blast-radius/pass-1.3.1.md` — this file
- `docs/aseptic/pass-complete/pass-1.3.1.md` — pass-complete record

## Risk surface

**Optimistic last_snapshot_us update.** Window is marked immediately at schedule
time, not at execution time. If the snapshot fails in the background, the window
is not retried until the next `N` seconds elapse. This is intentional: retrying
on every `read_state` call after a failure would storm-schedule.

**bg_take_snapshot uses read pool.** Acquires a read-pool connection under the
`Weak<dyn StoreOps>` upgrade. Pool size respects `OpenOptions::read_pool_size`.
If pool is exhausted (all connections busy), returns `PoolExhausted` — logged as
WARN, snapshot skipped.

**Recurring GC deadline.** Initial task has `deadline_us = store_open_us`, which
is already in the past. The quiescence gate still governs — it will only run after
the first quiet window post-open, not immediately.

**Re-push after upgrade None.** If `store_ops.upgrade()` returns `None` (store
dropped), the task is still re-queued with the new deadline. On the next loop
iteration, `stop_flag` will be true (since the store drop triggers executor drop),
so the recurring task is drained and dropped cleanly.

**fossic-py not drifted.** No new `OpenOptions` fields in v1.3.1; fossic-py
drift fix for v1.3.0 fields was committed in v1.3.0 commit 1 (`2233cd7`).

## Test coverage

- `policy_every_n_seconds_accepted` — EveryNSeconds(60) registers without error
- `policy_every_n_seconds_zero_rejected` — EveryNSeconds(0) returns SnapshotPolicyInvalid
- All 316 prior tests continue to pass

## Out of scope

`TaskPriority::High` remains unused (reserved for future escalation).
`EveryNSeconds` quiescence window integration test (actual timed background
execution) deferred — wall-clock test would require sleep ≥ 2s and is fragile.
