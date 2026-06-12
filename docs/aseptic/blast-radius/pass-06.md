---
pass: 6
version: v0.6.0
date: "(retroactive estimate, not verified)"
summary: fossic-py — initial PyO3 Python binding; Python mirror of Rust API
---

# Blast Radius — Pass 6 (v0.6.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic-py/` — entire crate directory
- `fossic-py/Cargo.toml` — pyo3 0.29, fossic dependency
- `fossic-py/src/lib.rs` — PyO3 module entry point, type registrations
- `fossic-py/src/store.rs` — Store PyO3 class, all method bindings
- `fossic-py/src/types.rs` — Append, ReadQuery, StoredEvent, EventId, BranchInfo, etc.
- `fossic-py/src/errors.rs` — Python exception hierarchy (FossicError tree)
- `fossic-py/python/fossic/__init__.py` — Python wrapper Store class, SubscriptionHandle
- `fossic-py/python/fossic/_worker.py` — SubscriptionWorker thread
- `fossic-py/python/fossic/_fossic.pyi` — type stubs (retroactive estimate)
- `fossic-py/tests/conftest.py` — shared test fixtures (declared_store, tmp_store)
- `fossic-py/tests/test_append_read.py` — basic binding tests
- `fossic-py/tests/test_branches.py` — branch tests
- `.github/workflows/ci.yml` — Python CI job (retroactive estimate — may have been
  added in an earlier or later pass)

---

## Public APIs

### Added
- Full Python mirror of the Rust API surface via PyO3:
  `Store.open`, `Store.declare_stream`, `Store.append`, `Store.read_range`,
  `Store.read_one`, `Store.subscribe`, `Store.create_branch`, `Store.promote_branch`,
  `Store.mark_branch_dead_end`, `Store.list_branches`, `Store.resolve_chain`,
  `Store.register_reducer` (pure Python, no snapshot caching at this stage),
  `Store.read_state` (pure Python full replay), `Store.take_snapshot`,
  `Store.snapshot_info`, `Store.gc_orphaned_snapshots`
- Python exception hierarchy: `FossicError`, `StreamNotDeclaredError`,
  `BranchNotFoundError`, `BranchLifecycleError`, `PurgeConfirmationError`,
  `EventNotFoundError`, and others

---

## Schema changes

None — Python binding uses same SQLite database as Rust core.

---

## Configuration changes

None.

---

## Dependency changes

In `fossic-py/Cargo.toml`:
- Added: `pyo3 = "0.29"` — Python extension module support
- Added: `fossic` (path dependency) — the core crate

---

## Behavior changes

- Python `read_state` implemented as a pure-Python full-event replay (no snapshot
  caching via Rust DynReducer path — DynReducer not yet public). Correct output;
  does not scale for high-event-count streams.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
