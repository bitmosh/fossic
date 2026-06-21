# Pass Report — v1.2.0

**Date:** 2026-06-20
**Version:** v1.2.0
**Type:** feat — Phase 6 (SnapshotPolicy: EveryNEvents registration and wiring)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| `SnapshotPolicy` enum (Manual, EveryNEvents, EveryNSeconds, StateAdaptive) | DONE | `src/types.rs` (committed in 995ae97) |
| `Error::SnapshotPolicyInvalid(String)` | DONE | `src/error.rs` (committed in 995ae97) |
| `SnapshotPolicy` re-exported from crate root | DONE | `src/lib.rs` (committed in 995ae97) |
| `ReducerEntry.policy` field + `validate_snapshot_policy` | DONE | `src/reducers.rs` |
| `register_with_policy` / `register_dyn_with_policy` | DONE | `src/reducers.rs` |
| `find_arc_with_policy` | DONE | `src/reducers.rs` |
| `StoreInner::snapshot_counters` + `maybe_auto_snapshot` | DONE | `src/store.rs` (committed in 995ae97) |
| `Store::register_reducer_with_policy` / `register_dyn_reducer_with_policy` | DONE | `src/store.rs` (committed in 995ae97) |
| `read_state` + `read_state_bytes` wired to `maybe_auto_snapshot` | DONE | `src/store.rs` (committed in 995ae97) |
| `tests/snapshot_policy.rs` — 7 tests | DONE | all pass |
| EveryNSeconds stub (NotImplemented at registration) | DONE | Phase 7 dependency noted |
| StateAdaptive stub (NotImplemented at registration) | DONE | v1.2.1 dependency noted |

---

## 2. Test results

265 tests, 0 failures. 7 new snapshot_policy tests added.

---

## 3. Files touched

Reference: `docs/aseptic/blast-radius/pass-1.2.0.md`

- Modified in 93ac4c4: `src/reducers.rs`, `tests/snapshot_policy.rs` (new)
- Previously committed (995ae97, ca27c72): `src/types.rs`, `src/error.rs`, `src/store.rs`,
  `src/lib.rs`, `CHANGELOG.md`, `Cargo.toml` (`[[test]]` entry)
- This commit: `Cargo.toml` (version bump), blast-radius, pass-complete

---

## 4. API changes

**New public types:**
- `SnapshotPolicy` — `Manual | EveryNEvents(u32) | EveryNSeconds(u32) | StateAdaptive { .. }`

**New error variant:**
- `Error::SnapshotPolicyInvalid(String)` — registration-time validation failure

**New Store methods:**
- `Store::register_reducer_with_policy<R: Reducer>(pattern, reducer, SnapshotPolicy) -> Result<(), Error>`
- `Store::register_dyn_reducer_with_policy(pattern, Box<dyn DynReducer>, SnapshotPolicy) -> Result<(), Error>`

**Breaking changes:** None. Existing `register_reducer` and `register_dyn_reducer` callers are unaffected.

---

## 5. Tech debt / polish debt

No new entries. Existing `CursorInner` / `TruncationCursor` dead-code warnings carry over from v1.1.0 until bounded-read call sites land.

---

## 6. PASS COMPLETE message

```
── PASS COMPLETE · v1.2.0 · 2026-06-20 ──────────────────────

Title: Automatic Snapshots via EveryNEvents Policy
Summary: SnapshotPolicy enum ships with Manual and EveryNEvents variants; registering a reducer with EveryNEvents(N) causes read_state to auto-snapshot after every N cumulative events applied, with the counter persisted in-process and reset on each snapshot.
Project: fossic

Highlights:
· register_reducer_with_policy(pattern, reducer, SnapshotPolicy::EveryNEvents(N)) now accepted; N=0 returns SnapshotPolicyInvalid at registration time
· read_state and read_state_bytes accumulate applied-event counts per (stream_id, branch) and fire take_snapshot when threshold is met; at-version historical variants are deliberately excluded
· EveryNSeconds and StateAdaptive stub at registration with NotImplemented, unblocking future phases without cluttering the call surface
· Manual policy (default) preserves existing behavior — zero change for callers using register_reducer

Learnings:
· Snapshot counter lives on StoreInner rather than ReducerRegistry because registry has no stream_id/branch context at lookup time; co-locating it avoids a parameter leak through the registry API

Commit: [PENDING_SHA]
Tests: 265 passed · 0 failed · 0 skipped
Branch: clean
```
