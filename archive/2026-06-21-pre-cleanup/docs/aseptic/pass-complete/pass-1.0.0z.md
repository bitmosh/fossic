# Pass Report — v1.0.0z

**Date:** 2026-06-16
**Version:** v1.0.0z
**Type:** polish (PD-009 — PoolExhausted integration test + configurable timeout)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| `test-helpers` feature flag in `Cargo.toml` | DONE | Gates `_test_hold_read_conn` |
| `OpenOptions::read_pool_timeout_ms: u64` (default 30_000) | DONE | `src/types.rs` |
| `read_conn()` reads from `options.read_pool_timeout_ms` | DONE | `src/store.rs` |
| `Store::_test_hold_read_conn(hold_ms)` under `test-helpers` | DONE | `src/store.rs` |
| `fossic-py` struct literal drift fix | DONE | `fossic-py/src/types.rs` |
| `pool_exhausted_returns_error` integration test | DONE | `tests/read_pool.rs` |

---

## 2. Test results

231 tests, 0 failures (`--all-features`). `pool_exhausted_returns_error` passes.

---

## 3. Files touched

Reference: `docs/aseptic/blast-radius/pass-1.0.0z.md`

- Modified: 5 files (`Cargo.toml`, `src/types.rs`, `src/store.rs`, `fossic-py/src/types.rs`, `tests/read_pool.rs`)
- Created: 2 files (blast-radius, pass-complete)

Drift: `fossic-py/src/types.rs` struct literal missing `read_pool_timeout_ms`. Fixed inline.

---

## 4. API changes

- `OpenOptions::read_pool_timeout_ms: u64` — new public field; existing `..Default::default()` callers unaffected
- `test-helpers` feature — new; not enabled by default; exposes `Store::_test_hold_read_conn`

---

## 5. Tech debt / polish debt resolved

PD-009 — `PoolExhausted` not covered by integration tests. Resolved.

---

## 6. PASS COMPLETE message

```
── PASS COMPLETE · PD-009 (PoolExhausted test + configurable timeout) · fossic · v1.0.0z ──
Commit: pending

OpenOptions::read_pool_timeout_ms (default 30_000ms). read_conn() now reads
from options instead of hardcoded 30s. New test-helpers feature exposes
Store::_test_hold_read_conn for integration tests. pool_exhausted_returns_error:
pool_size 1, timeout 50ms, conn held 200ms → PoolExhausted verified.

Drift fixed: fossic-py struct literal missing read_pool_timeout_ms.
PD-009 resolved.

Tests: 231 passed, 0 failed (--all-features)
Branch: clean
```
