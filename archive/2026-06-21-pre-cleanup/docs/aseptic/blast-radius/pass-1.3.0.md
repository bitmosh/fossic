# Blast Radius — v1.3.0

**pass:** v1.3.0 — Phase 7: BackgroundExecutor + QuiescenceMonitor
**prior-commit:** d895b2b
**date:** 2026-06-21

## Files changed

### New files
- `src/executor.rs` — BackgroundExecutor, QuiescenceMonitor, StoreOps trait, task heap
- `tests/executor.rs` — lifecycle tests
- `docs/aseptic/blast-radius/pass-1.3.0.md` — this file
- `docs/aseptic/pass-complete/pass-1.3.0.md` — pass-complete record

### Modified files
- `src/lib.rs` — `mod executor;` added
- `src/types.rs` — `OpenOptions`: two new fields (`background_executor_grace_timeout_ms`,
  `executor_quiescence_window_ms`) with defaults 10_000 and 2_000
- `src/store.rs` — `StoreInner`: two new fields; `Store::open`: quiescence + executor
  spawn; `start_dispatcher`: quiescence param + `note_dispatch()` call; `append` /
  `append_batch` / `append_if`: `note_write()` after success
- `fossic-py/src/types.rs` — manual `OpenOptions` literal: two new fields with defaults
- `Cargo.toml` — `[[test]] name = "executor"` entry
- `CHANGELOG.md` — v1.3.0 entry updated from "Version bump only" to full content

## Risk surface

**Thread spawn at open time.** `fossic-bg` thread started in `Store::open`. If spawn fails,
a WARN is printed and the store opens without background execution (no user-visible error).

**Weak reference lifecycle.** Executor holds `Weak<dyn StoreOps>`. If upgrade returns None
(store dropped), task is silently skipped. No memory hazard.

**Grace timeout detach.** If the bg thread does not stop within `grace_timeout`, it is
detached (JoinHandle dropped), not killed. Thread eventually stops when it wakes from its
500ms sleep and sees stop_flag. No resource leak beyond one sleeping thread.

**Arc coercion.** `inner.clone()` (Arc<StoreInner>) coerced to `Arc<dyn StoreOps>` at
assignment site — stable Rust unsized coercion, verified by type-check.

**Drop order.** `background_executor` is the last field in `StoreInner`; dropped after
`conn`, `read_pool_rx`, etc. The bg thread opens its own connection (`SystemStreamWriter`)
at shutdown — does not use the store's write mutex, no deadlock risk.

**note_write on all appends.** Fires even for idempotent (duplicate) appends. Conservative
but not harmful — keeps quiescence window active, which delays background tasks slightly.

**fossic-py drift.** Two new fields added to manual struct literal in
`fossic-py/src/types.rs` with the same defaults as `OpenOptions::default()`.

## Test coverage

- `executor::tests::task_priority_high_before_low` — heap ordering
- `executor::tests::task_equal_priority_earlier_deadline_first` — deadline tiebreak
- `executor::tests::quiescence_not_quiescent_immediately_after_write`
- `executor::tests::quiescence_not_quiescent_immediately_after_dispatch`
- `tests/executor.rs::executor_lifecycle_no_hang` — open+append+drop within 5s
- `tests/executor.rs::executor_short_grace_closes_within_timeout` — 2s grace, drops in <4s

## Out of scope

`bg_take_snapshot` returns `NotImplemented` (placeholder for v1.3.1 EveryNSeconds).
`schedule()` method exists but is not yet called from store code — wired in v1.3.1.
CP-T2-1 (snapshots.rs GC scheduling) deferred to v1.3.1.
