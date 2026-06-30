---
pass: v1.2.1
version: v1.2.1
date: 2026-06-21
prior-commit: e765866
summary: ReducerStateLarge emission via SystemStreamWriter + StateAdaptive policy enablement
---

# Blast Radius — Pass v1.2.1

## Files

### Created
- `docs/aseptic/blast-radius/pass-1.2.1.md` — this file
- `docs/aseptic/pass-complete/pass-1.2.1.md` — pass report

### Modified
- `src/store.rs` — StateMonitor struct; two new StoreInner fields; timed apply loops in
  read_state / read_state_bytes; update_state_monitor + maybe_emit_state_large helpers;
  StateAdaptive branch in maybe_auto_snapshot; SystemStreamWriter import at crate level
- `src/types.rs` — OpenOptions::reducer_state_large_threshold_bytes field + Default value
- `src/reducers.rs` — StateAdaptive branch in validate_snapshot_policy: Err(NotImplemented) → Ok(())
- `fossic-py/src/types.rs` — manual OpenOptions struct literal: added reducer_state_large_threshold_bytes: 1_048_576
- `tests/snapshot_policy.rs` — 5 new tests; policy_not_implemented_adaptive renamed + inverted
- `CHANGELOG.md` — v1.2.1 section added
- `Cargo.toml` — version bumped: 1.2.0 → 1.2.1

---

## Changes

### `src/store.rs`

**`StateMonitor` struct (crate-private, before StoreInner):**
- `state_sizes: Vec<usize>` — rolling buffer, max 32 entries, per-event apply result size
- `last_emission_us: i64` — timestamp of last ReducerStateLarge emission (0 = never)
- `apply_costs_us: Vec<u64>` — rolling buffer, max 32 entries, per-event apply wall-clock cost
- Methods: `push_state_size`, `push_apply_cost`, `mean_state_size`, `avg_apply_cost_us`
- `Default::default()` initializes with `Vec::with_capacity(32)` and `last_emission_us = 0`

**New StoreInner fields (Phase 6+7+8 anchor zone):**
- `reducer_system_writer: parking_lot::Mutex<Option<SystemStreamWriter>>` — lazy-initialized;
  `None` until first ReducerStateLarge emission. Separate from dispatcher's writer (different
  thread ownership). Owns its own SQLite WAL connection.
- `state_monitors: parking_lot::Mutex<HashMap<(String, String), StateMonitor>>` — keyed by
  `(stream_id, branch)`; populated inside the read_state apply loop.

**`read_state` / `read_state_bytes` apply loops** — each iteration now:
1. Records `t0 = crate::schema::now_us()`
2. Calls `reducer.apply_bytes`
3. Computes `cost_us = (now_us() - t0).max(0) as u64`
4. Calls `self.update_state_monitor(stream_id, branch, state_bytes.len(), cost_us)`
Post-loop: `self.maybe_emit_state_large(stream_id, branch)?` before `maybe_auto_snapshot`.

**`update_state_monitor` (private)** — locks `state_monitors`, gets-or-inserts the entry,
pushes size and cost to rolling buffers (capped at 32).

**`maybe_emit_state_large` (private):**
1. Early-exit if `threshold == usize::MAX`
2. Lock `state_monitors`, check `mean_state_size() > threshold`
3. Throttle: return if `now - last_emission_us < 60_000_000` (60 seconds in µs)
4. Update `last_emission_us`, release monitor lock
5. Lock `reducer_system_writer`; lazy-init if `None` via `SystemStreamWriter::new(&self.inner.path)`
6. Call `writer.emit("ReducerStateLarge", &payload, None)`; errors silently dropped

**`maybe_auto_snapshot` — StateAdaptive branch:**
- Accumulates events in `snapshot_counters` (same map as EveryNEvents)
- Guards on `accumulated >= min_events_between` (returns early if below)
- Reads `avg_apply_cost_us` from state_monitors (0 if no monitor yet)
- Fires snapshot when `accumulated * avg_cost > target_replay_cost_us`
- Resets counter to 0 on fire (same as EveryNEvents)

### `src/types.rs`

`OpenOptions::reducer_state_large_threshold_bytes: usize` — default `1_048_576` (1 MiB).
Set to `usize::MAX` to disable entirely. Checked in `maybe_emit_state_large` before any
monitor lookup.

### `src/reducers.rs`

`validate_snapshot_policy` — `StateAdaptive { .. }` arm changed from
`Err(Error::NotImplemented { .. })` to `Ok(())`. Now accepted at registration time.

### `fossic-py/src/types.rs`

Manual `OpenOptions` struct literal in `TryFrom<&PyOpenOptions>` updated with the new field:
`reducer_state_large_threshold_bytes: 1_048_576`.

---

## Public API changes

**New `OpenOptions` field:** `reducer_state_large_threshold_bytes: usize` (default 1 MiB).

**`SnapshotPolicy::StateAdaptive`** — no longer returns `NotImplemented` at registration.
Existing callers using `StateAdaptive` will now find it operational (previously it was
rejected at registration, so no callsite could have depended on the error).

**Breaking changes:** None. `OpenOptions` gains a field but its `Default` impl provides the
value; callers using `Default::default()` or struct update syntax are unaffected. Callers
with manual struct literals must add the field — fossic-py updated in this pass.

---

## Threading model

Two `SystemStreamWriter` instances in steady state:
1. Dispatcher's writer (stack-local on the dispatch thread, initialized in `start_dispatcher`)
2. Reducer's writer (`reducer_system_writer` on StoreInner, lazy Mutex)

Both write to `_fossic/system` on separate SQLite connections in WAL mode. WAL allows
concurrent writers without conflict. Reducer's writer is initialized on first ReducerStateLarge
event; `None` in the zero-emission case.

Cursor ownership invariant (SR-04) is preserved: `maybe_emit_state_large` and
`maybe_auto_snapshot` neither read nor write subscription cursors.

---

## CCE deduplication note (test discovery)

`append` derives the event CCE id from `event_type + type_version + causation_id + payload`.
Test reducers that use identical payloads for all events (e.g., `Null`) will store only one
event due to `INSERT OR IGNORE` on the id primary key. Test helpers `append_slow` and
`append_grow` use `{"seq": N}` payloads to ensure each event has a distinct CCE id.

---

## Tech debt / polish debt

**No new entries this pass.**

---

## Adjacent project notifications

- **fossic-py**: `reducer_state_large_threshold_bytes` added to manual struct literal;
  not yet bridged to Python API surface (future pass).
- **fossic-node**: no change.
- **fossic-tauri**: no change.
- **Downstream consumers** (cerebra, lumaweave, policy-scout, ai-stack): no impact —
  `OpenOptions::Default` provides the new field; existing callers unaffected.
