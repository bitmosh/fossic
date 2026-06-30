# Blast Radius — pass-1.7.3 — Background indexing via TaskKind::Custom

**Version:** v1.7.3
**Date:** 2026-06-21
**Scope:** `src/store.rs` (+1 method), `crates/fossic-similarity-hnsw` (provider + tests), `docs/SUBSTRATE_EXTENSION_PATTERNS.md`

---

## Files changed

| File | Change |
|---|---|
| `src/store.rs` | Added `pub fn schedule_task(&self, task: BacklogTask)` |
| `crates/fossic-similarity-hnsw/src/provider.rs` | `dirty` + `save_pending` fields, `schedule_save`, `is_dirty`, `is_save_pending`, `save_to_disk` restructured |
| `crates/fossic-similarity-hnsw/tests/integration.rs` | 5 new scheduling tests (19 total) |
| `docs/SUBSTRATE_EXTENSION_PATTERNS.md` | §2 + §5 updated, CP-D2-3 + HNSW CPs added, version → v1.7.3 |
| `Cargo.toml` (root + 4 crates) | 1.7.2 → 1.7.3 |
| `fossic-py/pyproject.toml` | 1.7.2 → 1.7.3 |
| `CHANGELOG.md` | v1.7.3 section added |

---

## What changed in store.rs

One new public method:
```rust
pub fn schedule_task(&self, task: BacklogTask) {
    if let Some(ref exec) = *self.inner.background_executor.lock() {
        exec.schedule(task);
    }
}
```
No other changes. `background_executor: parking_lot::Mutex<Option<BackgroundExecutor>>` was already there; this is just a thin public façade.

---

## What changed in provider.rs

### New fields on HnswProvider
- `dirty: AtomicBool` — false at construction; set by `index()` and `index_with_stream_id()`; cleared by successful `save_to_disk()`
- `save_pending: AtomicBool` — false at construction; set at `schedule_save` time; cleared at the start of the save closure

### New public API
- `is_dirty() -> bool`
- `is_save_pending() -> bool`
- `schedule_save(provider: Arc<Self>, store: &Store, priority: TaskPriority)` — static-method style (not `&self`) so it can capture `Weak<HnswProvider>` in the closure

### Modified: save_to_disk
Restructured from early-return pattern to a single exit point. `dirty.store(false)` is called once, after any successful save (empty or non-empty). The failure paths (`return Err(e)`) still bypass it, keeping `dirty=true` on failure.

### schedule_save closure semantics
```
schedule_save(provider, store, priority):
  if !dirty: return  // no-op
  if save_pending.swap(true): return  // storm prevention (§3 discipline)
  weak = Arc::downgrade(&provider)
  store.schedule_task(Custom(|| {
    if weak.upgrade() fails: return  // provider dropped; state lost
    save_pending = false  // allow future schedules
    if dirty: save_to_disk()  // save_to_disk clears dirty on success
  }))
```

---

## Invariants preserved

- **v1.7.2 atomic save contract**: unchanged — any save failure cleans up all three files
- **CP-D2-2**: unchanged — trait `index` path still has no `stream_id`
- **No background-scheduled saves in v1.7.2**: v1.7.3 adds scheduling but does NOT call `schedule_save` automatically — callers opt in explicitly
- **dirty=false does not prevent direct save_to_disk calls** — direct calls always attempt to save; dirty is only checked in `schedule_save`

---

## What was NOT changed

- `HnswConfig`: no new fields
- `HnswError`: no new variants
- Load-on-construction path: unchanged
- v1.7.2 persistence tests: all 14 still passing unchanged
- Core fossic crate: only `store.rs` touched, no structural changes

---

## Deviation from brief: CP-D2-3

Brief: `schedule_save(executor: &BackgroundExecutor, priority: TaskPriority)`
Actual: `schedule_save(provider: Arc<Self>, store: &Store, priority: TaskPriority)`

Cause: `BackgroundExecutor::spawn` is `pub(crate)`. External crates cannot construct a `BackgroundExecutor`. `Store::schedule_task` is added as the public scheduling surface.

Resolution path: CP-D2-3 — anticipated v2 opens `BackgroundExecutor` or an equivalent handle directly.

---

## Test coverage

19 integration tests, 288 workspace tests, 0 failures.

New tests specifically for this pass:
- `schedule_save_fires_when_dirty`
- `schedule_save_noop_when_not_dirty`
- `schedule_save_storm_prevention`
- `schedule_save_low_priority_yields_to_normal`
- `schedule_save_drop_provider_before_quiescence_noop`

Timing: scheduling tests use `executor_quiescence_window_ms: 50` + 700ms sleep (1 bg-thread tick). Priority-ordering test uses 1400ms (2 ticks). All deterministic under normal CI load.
