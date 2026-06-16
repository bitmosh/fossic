# Pass Report — v1.0.0w

**Date:** 2026-06-16
**Version:** v1.0.0w
**Type:** feature (Phase 6b — crossbeam-channel read connection pool)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| `Error::PoolExhausted` variant | DONE | `src/error.rs` |
| `OpenOptions::read_pool_size` (default 4) | DONE | `src/types.rs` |
| `ReadGuard` struct + `Deref` + `Drop` | DONE | `src/store.rs` |
| Pool fields in `StoreInner` | DONE | `src/store.rs` |
| Pool init in `Store::open` with `query_only = ON` | DONE | `src/store.rs` |
| `Store::read_conn()` helper | DONE | `src/store.rs` |
| 16 read methods switched to `read_conn()` | DONE | `src/store.rs` (streams, stream_exists, read_range, read_one, read_batch, read_by_external_id, read_by_correlation, walk_causation, aggregate, get_cursor, list_branches, resolve_chain, snapshot_info, get_snapshot_state, compute_state_bytes, take_snapshot first lock) |
| `tests/read_pool.rs` (4 tests) | DONE | New file |

---

## 2. Test results

178 tests, 0 failures. Full workspace clean.

New tests in `tests/read_pool.rs`:
- `read_does_not_block_when_write_mutex_held` — read succeeds while write mutex is held in another thread
- `concurrent_reads_all_complete` — pool_size threads read simultaneously, all complete
- `read_pool_size_one_is_valid` — pool of 1 works for sequential reads
- `pool_connections_are_query_only` — reads and writes both work correctly (pool for reads, write mutex for writes)

---

## 3. Files touched

Reference: `docs/aseptic/blast-radius/pass-1.0.0w.md`

Summary:
- Modified: 4 files (`src/store.rs`, `src/types.rs`, `src/error.rs`, `fossic-py/src/types.rs`)
- Created: 3 files (`tests/read_pool.rs`, blast-radius, pass-complete)

Drift: `fossic-py/src/types.rs` had a struct literal missing `read_pool_size`. Fixed inline.

---

## 4. API changes

- `OpenOptions::read_pool_size: usize` — new public field; existing callers using `..Default::default()` are unaffected; struct-literal callers in other crates must add the field (fossic-py fixed this pass)
- `Error::PoolExhausted { pool_size, timeout_ms }` — new variant; match-exhaustive callers must add an arm
- `Store::read_conn()` — private; not public API

---

## 5. Tech debt / polish debt registered

| ID | Description | Severity |
|---|---|---|
| TD-001 | `take_snapshot` dual-acquisition TOCTOU | Low (snapshots are idempotent) |
| TD-002 | `subscribe` seed query uses write conn | Cosmetic |
| PD-001 | `PoolExhausted` not covered by integration tests | Low |

---

## 6. Adjacent project impact

- **Lattica / fossic-tauri**: `Error::PoolExhausted` is a new variant in the error enum. `FossicTauriError` likely uses a catch-all; verify if any match-exhaustive code exists.
- **fossic-py**: Fixed inline this pass (`read_pool_size: 4` in struct literal).

---

## 7. PASS COMPLETE message

```
── PASS COMPLETE · Phase 6b (read pool) · fossic · v1.0.0w ──
Commit: pending

crossbeam-channel read connection pool. Pure-read methods (read_range, aggregate,
walk_causation, read_batch, etc.) now draw from a pool of N connections (default 4)
and never contend with the write mutex or each other.

Highlights:
· ReadGuard RAII struct — returns conn to pool on drop
· Pool init with PRAGMA query_only = ON as write-accident guard
· 16 read methods switched; write path (append, append_if, etc.) untouched
· OpenOptions::read_pool_size (default 4); Error::PoolExhausted added
· Drift fixed: fossic-py OpenOptions struct literal missing read_pool_size

Tech debt registered: TD-001 (take_snapshot dual-acquisition), TD-002 (subscribe seed query on write conn), PD-001 (PoolExhausted not integration-tested)

Tests: 178 passed, 0 failed (4 new in tests/read_pool.rs)
Branch: clean
```
