---
pass: 4
version: v0.4.0
date: "(retroactive estimate, not verified)"
summary: Branches — create, promote, dead_end lifecycle; resolve_chain
---

# Blast Radius — Pass 4 (v0.4.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Modified
- `fossic/src/store.rs` — create_branch, promote_branch, mark_branch_dead_end,
  list_branches, resolve_chain methods added
- `fossic/src/types.rs` — CreateBranch, BranchInfo, BranchSegment types added
- `fossic/src/schema.rs` — branches table + indexes added

### Created
- `fossic/tests/branches.rs` — branch lifecycle tests

---

## Public APIs

### Added
- `Store::create_branch(b: CreateBranch) -> Result<()>`
- `Store::promote_branch(stream_id, branch_id, reason: Option<&str>) -> Result<()>`
- `Store::mark_branch_dead_end(stream_id, branch_id, reason: Option<&str>) -> Result<()>`
- `Store::list_branches(stream_id) -> Result<Vec<BranchInfo>>` — returns explicitly-created
  branches only; implicit `main` trunk is not stored and does not appear
- `Store::resolve_chain(stream_id, branch_id) -> Result<Vec<BranchSegment>>`
- `CreateBranch { stream_id, branch_id, parent_id, ... }`
- `BranchInfo { id, stream_id, lifecycle, ... }` — `.id` not `.branch_id`; `.lifecycle` not `.status`
- `BranchSegment { branch_id, ... }`
- `BranchNotFoundError`, `BranchLifecycleError`, `InvalidBranchIdError` — exception types
- `ReadQuery` — gains `branch: String` field (default "main")
- `Append` — gains `branch: String` field (default "main")

---

## Schema changes

- `branches` table created with columns: id, stream_id, parent_id, parent_version,
  description, created_at, lifecycle, closed_at, closed_reason, alternatives
- `idx_branches_stream`, `idx_branches_lifecycle` indexes created

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

- `read_range` with `branch="exp"` returns only events committed to that branch — NOT the
  parent chain. Branch read isolation is per-branch, not cumulative from parent.
- `list_branches` returns empty list for a stream that has no explicitly-created diverged
  branches. The implicit `main` trunk is never in the results.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
