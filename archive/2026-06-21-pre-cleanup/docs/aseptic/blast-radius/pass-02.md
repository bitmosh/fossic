---
pass: 2
version: v0.2.0
sha: adfa3de
date: 2026-06-12
summary: Branches, snapshots, glob pattern reducers
---

# Blast Radius — Pass 2 (v0.2.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content completely rewritten — original bootstrap estimate described subscriptions/WAL
> (which are v0.3.0). Actual commit adfa3de added branches, snapshots, reducers, glob.

## Files

### Created

8 files in commit adfa3de:

- `src/branches.rs` — branch lifecycle: create, promote, mark_dead_end, list, resolve_chain
- `src/glob.rs` — glob pattern matching utilities for reducer stream patterns
- `src/reducers.rs` — `Reducer` trait, `BoxedReducer`, reducer registry, pattern-matched dispatch
- `src/snapshots.rs` — snapshot write, read, gc; snapshot-aware `read_state` path
- `tests/branches.rs` — branch lifecycle tests, isolation tests, resolve_chain tests
- `tests/glob.rs` — glob pattern matching property tests
- `tests/reducers.rs` — reducer registration, `read_state` round-trip tests
- `tests/snapshots.rs` — snapshot write/restore/gc tests, equivalence invariant tests

---

## Public APIs

### Added

**Branching:**
- `Store::create_branch(b: CreateBranch) -> Result<()>` — fork a new branch from a parent at a given version
- `Store::promote_branch(stream_id, branch_id, reason: Option<&str>) -> Result<()>` — mark a branch as the new main trunk
- `Store::mark_branch_dead_end(stream_id, branch_id, reason: Option<&str>) -> Result<()>` — close an experimental branch
- `Store::list_branches(stream_id) -> Result<Vec<BranchInfo>>` — list explicitly-created branches; implicit `main` trunk is NOT included
- `Store::resolve_chain(stream_id, branch_id) -> Result<Vec<BranchSegment>>` — full parent chain from branch to root
- `CreateBranch { stream_id, branch_id, parent_id, parent_version, description }` — branch creation descriptor
- `BranchInfo { id, stream_id, parent_id, parent_version, description, created_at, lifecycle, closed_at, closed_reason }` — branch metadata; NOTE: field is `.id` not `.branch_id`, `.lifecycle` not `.status` (PD-003 origin)
- `BranchSegment { branch_id, from_version, to_version }` — chain segment

**Snapshots + Reducers:**
- `Store::register_reducer(pattern: &str, reducer: Box<dyn Reducer>) -> Result<()>` — register a Rust-trait reducer against a glob-matched stream pattern
- `Store::take_snapshot(stream_id, branch) -> Result<SnapshotInfo>` — write current reducer state to snapshot table
- `Store::read_state(stream_id, branch) -> Result<Value>` — apply reducer from most recent snapshot + remaining events
- `Store::read_state_at_version(stream_id, branch, version) -> Result<Value>` — read state pinned to a specific version
- `Store::snapshot_info(stream_id, branch, reducer_name) -> Result<Option<SnapshotInfo>>` — query snapshot metadata
- `Store::gc_orphaned_snapshots() -> Result<usize>` — remove snapshots for streams/branches with no events
- `SnapshotInfo { stream_id, branch, version, reducer_name, reducer_version, state_schema_version, created_at }` — snapshot metadata
- `Reducer` trait — `name() -> &str`, `version() -> u32`, `state_schema_version() -> u32`, `initial_state() -> Value`, `apply(state: Value, event_payload: &Value) -> Value`

---

## Schema changes

- `branches` table created — columns: `id TEXT`, `stream_id TEXT`, `parent_id TEXT`, `parent_version INTEGER`, `description TEXT`, `created_at INTEGER`, `lifecycle TEXT DEFAULT 'active'`, `closed_at INTEGER`, `closed_reason TEXT`, `alternatives TEXT`; indexes on (stream_id), (stream_id, lifecycle)
- `snapshots` table created — columns: `stream_id TEXT`, `branch TEXT`, `version INTEGER`, `reducer_name TEXT`, `reducer_version INTEGER`, `state_schema_version INTEGER`, `state_blob BLOB`, `created_at INTEGER`; composite index on (stream_id, branch, reducer_name, version DESC)

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

- `read_range` with `branch="exp"` returns only events committed to that branch — NOT the parent chain. Branch read isolation is per-branch, not cumulative.
- `list_branches` returns empty list for a stream with no explicitly-created diverged branches. The implicit `main` trunk never appears in results.
- Snapshots are a read-path optimization only. `read_state` with no snapshot is semantically identical to `read_state` with a snapshot (spec invariant §16.4).
- Pattern-based reducer dispatch: if two patterns match a stream with equal specificity, `ReducerPatternAmbiguousError` is raised at `read_state` time.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: PD-003 (BranchInfo field naming: `.id` vs `.branch_id`, `.lifecycle` vs `.status`) originates here. The naming is consistent in Rust but differs from what some tests initially expected.*
