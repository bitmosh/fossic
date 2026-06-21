---
pass: v1.2.0
version: v1.2.0
date: 2026-06-20
prior-commit: 93ac4c4
summary: SnapshotPolicy enum + EveryNEvents auto-snapshot wiring in ReducerRegistry and Store
---

# Blast Radius — Pass v1.2.0

## Files

### Created
- `tests/snapshot_policy.rs` — 7 tests covering policy validation and EveryNEvents behavior
- `docs/aseptic/blast-radius/pass-1.2.0.md` — this file
- `docs/aseptic/pass-complete/pass-1.2.0.md` — pass report

### Modified
- `src/reducers.rs` — `ReducerEntry.policy: SnapshotPolicy` field; `register_with_policy`,
  `register_dyn_with_policy`, `find_arc_with_policy` added; `register` and `find_arc` delegate
  to the new _with_policy variants; `validate_snapshot_policy` standalone function added
- `Cargo.toml` — version bumped: `1.1.0` → `1.2.0`

### Already committed in prior Track 1+2 passes (reference only — not in this commit)
- `src/types.rs` — `SnapshotPolicy` enum + `Default` impl added (committed in 995ae97)
- `src/error.rs` — `Error::SnapshotPolicyInvalid(String)` added (committed in 995ae97)
- `src/store.rs` — `StoreInner::snapshot_counters`, `register_reducer_with_policy`,
  `register_dyn_reducer_with_policy`, `get_reducer_with_policy`, `maybe_auto_snapshot`,
  `read_state`/`read_state_bytes` wired to call `maybe_auto_snapshot` (committed in 995ae97)
- `src/lib.rs` — `SnapshotPolicy` added to public re-exports (committed in 995ae97)
- `CHANGELOG.md` — v1.2.0 section added (committed in ca27c72)
- `Cargo.toml` — `[[test]] snapshot_policy` entry added (committed in ca27c72)

---

## Changes

### `src/reducers.rs`

**`ReducerEntry.policy: SnapshotPolicy`** — new `pub(crate)` field. All entries carry their
policy; the registry stores it alongside the reducer Arc.

**`validate_snapshot_policy(policy: &SnapshotPolicy) -> Result<(), Error>`** — standalone
`pub(crate)` function. Validates at registration time:
- `Manual` → `Ok`
- `EveryNEvents(0)` → `Err(SnapshotPolicyInvalid("EveryNEvents requires N >= 1"))`
- `EveryNEvents(_)` → `Ok`
- `EveryNSeconds(_)` → `Err(NotImplemented { feature: "SnapshotPolicy::EveryNSeconds …" })`
- `StateAdaptive { .. }` → `Err(NotImplemented { feature: "SnapshotPolicy::StateAdaptive …" })`

**`register_with_policy`** — validates, checks ambiguity, pushes `ReducerEntry` with policy.
Existing `register` delegates to this with `SnapshotPolicy::Manual`.

**`find_arc_with_policy`** — returns `Option<(Arc<dyn BoxedReducer>, SnapshotPolicy)>`.
Existing `find_arc` delegates to this and discards the policy.

**`register_dyn_with_policy`** / **`register_dyn_with_policy`** — same pattern for
`DynReducer` bridges.

---

## Public API changes

**New types:** `SnapshotPolicy` (enum, `Debug + Clone`) — already committed in 995ae97.

**New error variant:** `Error::SnapshotPolicyInvalid(String)` — already committed in 995ae97.

**New Store methods:**
- `Store::register_reducer_with_policy<R: Reducer>(pattern, reducer, policy) -> Result<(), Error>`
- `Store::register_dyn_reducer_with_policy(pattern, reducer, policy) -> Result<(), Error>`

**Breaking changes:** None. All `ReducerEntry` construction is internal; existing callers of
`register_reducer` and `register_dyn_reducer` are unaffected.

---

## Cursor ownership invariant (SR-04)

`dispatch_post_commit` in `subscriptions.rs` remains the ONLY path that advances subscription
cursors. `maybe_auto_snapshot` calls `take_snapshot` synchronously in-band with `read_state` —
it does not touch subscriptions or cursors.

---

## Tech debt / polish debt

**No new entries this pass.**

The two dead-code warnings on `CursorInner` and `TruncationCursor::encode/decode` (inherited
from v1.1.0) remain. They activate when the remaining bounded read methods ship.

---

## Adjacent project notifications

- **fossic-py**: no API surface visible across the FFI boundary; `SnapshotPolicy` not yet
  bridged to Python (future pass).
- **fossic-node**: no change.
- **fossic-tauri**: no change.
- **Downstream consumers** (cerebra, lumaweave, policy-scout, ai-stack): no impact — the new
  API is additive; callers using `register_reducer` are unaffected.
