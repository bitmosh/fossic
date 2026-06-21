---
pass: v1.2.2
version: v1.2.2
date: 2026-06-21
prior-commit: d3e2dcc
summary: auto_gc_orphans flag — drop-time GC fallback; closes Phase 6
---

# Blast Radius — Pass v1.2.2

## Files

### Created
- `docs/aseptic/blast-radius/pass-1.2.2.md` — this file
- `docs/aseptic/pass-complete/pass-1.2.2.md` — pass report

### Modified
- `src/types.rs` — `OpenOptions::auto_gc_orphans: bool` field + Default value (`false`)
- `src/store.rs` — `impl Drop for Store` wiring drop-time GC behind `auto_gc_orphans` flag
- `src/snapshots.rs` — CP-T2-1 continuity-point marker above `gc_orphaned_snapshots_impl`
- `fossic-py/src/types.rs` — manual `OpenOptions` struct literal: added `auto_gc_orphans: false`
- `tests/snapshots.rs` — 3 new tests: flag-off, flag-on, last-clone-only
- `CHANGELOG.md` — v1.2.2 section added
- `Cargo.toml` — version bumped: 1.2.1 → 1.2.2

---

## Changes

### `src/types.rs`

**`OpenOptions::auto_gc_orphans: bool`** — new field, default `false`.

When `true`, `gc_orphaned_snapshots` is called at store drop time. Drop-time GC fires
only when the last `Store` clone is dropped (guarded by `Arc::strong_count(&self.inner) == 1`
in `impl Drop for Store`). Errors are silently discarded; callers who need a count call
`gc_orphaned_snapshots` explicitly.

Phase 7 (v1.3.1) supplements this with recurring background-scheduled GC via
`BackgroundExecutor`; the drop-time call is retained as final-shutdown cleanup even when
Phase 7 is present.

### `src/store.rs`

**`impl Drop for Store`** — new `Drop` implementation. Guard: checks
`self.inner.options.auto_gc_orphans && Arc::strong_count(&self.inner) == 1`. If both
conditions hold, delegates to the existing `gc_orphaned_snapshots()` public method (which
acquires the write connection, reads the active reducer registry, and deletes orphaned
snapshot rows). Errors discarded via `let _ = ...`.

Placed between the `Store` struct definition and `impl Store` for locality.

### `src/snapshots.rs`

**CP-T2-1 marker** — continuity-point comment inserted above `gc_orphaned_snapshots_impl`:

```
// CP-T2-1: When Phase 7 (Background Executor) lands, supplement
// this drop-time GC with quiescent-window scheduling via
// BackgroundExecutor::schedule(GcOrphanSnapshots, TaskPriority::Low).
// Keep this drop-time call as final-shutdown cleanup.
```

### `fossic-py/src/types.rs`

Manual `OpenOptions` struct literal in `TryFrom<&PyOpenOptions>` updated with:
`auto_gc_orphans: false`.

---

## Test coverage (`tests/snapshots.rs` — 3 new tests)

**`auto_gc_orphans_flag_off_no_gc_on_drop`** — creates a snapshot with `CountReducer`
registered, then drops a `Store` opened without that reducer and `auto_gc_orphans: false`.
Verifies the snapshot survives.

**`auto_gc_orphans_flag_on_gc_fires_on_drop`** — same setup but `auto_gc_orphans: true`.
Verifies the orphaned snapshot is removed after drop.

**`auto_gc_orphans_only_fires_on_last_clone_drop`** — creates two `Store` clones with
`auto_gc_orphans: true`. Drops clone A (strong_count 2 → 1 in B, so our A's Drop sees
count == 2 before decrement). Confirms snapshot survives. Drops clone B (last reference).
Confirms snapshot is now removed.

---

## Public API changes

**New `OpenOptions` field:** `auto_gc_orphans: bool` (default `false`).
Callers using `Default::default()` or struct update syntax are unaffected.
Manual struct literals must add the field — fossic-py updated in this pass.

**New runtime behavior:** When `auto_gc_orphans: true`, the last drop of a `Store` triggers
a synchronous GC of orphaned snapshots. This is a blocking write to the SQLite DB at drop
time. For stores with large snapshot tables and many orphaned rows, this can introduce a
pause at shutdown. Callers who need predictable drop latency should leave the flag `false`
and call `gc_orphaned_snapshots()` explicitly at a controlled point.

**Breaking changes:** None.

---

## Phase 6 close

v1.2.2 closes Phase 6 (Snapshot Policies). The Phase 6 surface is now complete:
- v1.2.0: SnapshotPolicy enum, EveryNEvents wiring, snapshot_counters
- v1.2.1: ReducerStateLarge emission, StateAdaptive policy enablement
- v1.2.2: auto_gc_orphans drop-time GC fallback

v1.3.0 opens Phase 7 (Background Executor + Quiescent Operations).

---

## Threading model

`impl Drop for Store` runs on the thread that drops the last `Store` clone. GC acquires the
write connection (`Mutex<Connection>`) synchronously. No other `Store` clone is alive at this
point (strong_count == 1 guard), so no contention with the write mutex is possible.

`gc_orphaned_snapshots` also acquires a read on `reducers: RwLock<ReducerRegistry>`. Since
all other clones have been dropped, no concurrent reader or writer of the registry can exist
through this store's Arc. Lock acquisition is therefore always uncontested at drop time.

---

## Tech debt / polish debt

**No new entries this pass.**

---

## Adjacent project notifications

- **fossic-py**: `auto_gc_orphans` added to manual struct literal; not yet bridged to Python
  API surface (future pass — low priority, flag is opt-in and defaults false).
- **fossic-node**: no change.
- **fossic-tauri**: no change.
- **Downstream consumers**: no impact — `OpenOptions::Default` provides the new field.
