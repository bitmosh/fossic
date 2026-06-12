---
pass: 5
version: v0.5.0
date: "(retroactive estimate, not verified)"
summary: Snapshots and reducers — Rust core, take_snapshot, read_state, read_state_at_version
---

# Blast Radius — Pass 5 (v0.5.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic/src/snapshots.rs` — snapshot write, read, gc logic
- `fossic/src/reducers.rs` — reducer registry, glob_matches, pattern_specificity,
  state fold, BoxedReducer trait (pub(crate))

### Modified
- `fossic/src/store.rs` — register_reducer, take_snapshot, read_state,
  read_state_at_version, snapshot_info, gc_orphaned_snapshots methods
- `fossic/src/types.rs` — SnapshotInfo type added
- `fossic/src/schema.rs` — snapshots table added

### Created
- `fossic/tests/reducers.rs` — reducer registration and read_state tests
- `fossic/tests/snapshots.rs` — snapshot write/read/gc tests

---

## Public APIs

### Added
- `Store::register_reducer(pattern: &str, reducer: Box<dyn Reducer>)` — Rust trait-based
- `Store::take_snapshot(stream_id, branch) -> Result<SnapshotInfo>`
- `Store::read_state(stream_id, branch) -> Result<...>` — generic over state type
- `Store::read_state_at_version(stream_id, branch, version) -> Result<...>`
- `Store::snapshot_info(stream_id, branch, reducer_name) -> Result<Option<SnapshotInfo>>`
- `Store::gc_orphaned_snapshots() -> Result<usize>`
- `SnapshotInfo { stream_id, branch, version, reducer_name, ... }`

---

## Schema changes

- `snapshots` table created with columns: stream_id, branch, version, reducer_name,
  reducer_version, state_schema_version, state_blob, created_at
- Indexes on (stream_id, branch, reducer_name, version DESC)

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

- Snapshots are a read-path optimization only. `read_state` with no snapshot produces
  identical output to `read_state` with a snapshot (spec invariant §16.4).
- Pattern-based reducer registration: one reducer matches one stream pattern. If two
  patterns both match a stream_id with equal specificity, `ReducerPatternAmbiguousError`
  is raised.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: TD-004 (SimilaritySearchProvider absent from code) originates from this pass's
scope — the spec promised an extension point that was never added. Retroactively assigned
to this pass.*
