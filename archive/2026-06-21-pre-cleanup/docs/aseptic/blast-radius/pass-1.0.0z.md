---
pass: v1.0.0z
version: v1.0.0z
date: 2026-06-16
summary: PD-009 — OpenOptions::read_pool_timeout_ms + PoolExhausted integration test
---

# Blast Radius — Pass v1.0.0z

## Files

### Modified
- `Cargo.toml` — new `test-helpers = []` feature flag
- `src/types.rs` — `OpenOptions::read_pool_timeout_ms: u64` field added (default 30_000)
- `src/store.rs` — `read_conn()` now reads timeout from `options.read_pool_timeout_ms`; `Store::_test_hold_read_conn(hold_ms)` added under `#[cfg(feature = "test-helpers")]`
- `fossic-py/src/types.rs` — `TryFrom<&PyOpenOptions> for OpenOptions` struct literal: `read_pool_timeout_ms: 30_000` added
- `tests/read_pool.rs` — `open_tmp_with_pool_and_timeout` helper added; `pool_exhausted_returns_error` test added; `Error` added to imports

---

## Changes

### `Cargo.toml` — `test-helpers` feature

New feature flag `test-helpers = []`. Gates `Store::_test_hold_read_conn`. Integration tests run with `--all-features` (already the canonical `just test-rust` invocation) so the test picks it up automatically.

### `src/types.rs` — `OpenOptions::read_pool_timeout_ms`

New field, default 30_000ms. Matches the hardcoded value that was previously in `read_conn()`. Existing callers using `..Default::default()` are unaffected.

### `src/store.rs` — `read_conn()` + `_test_hold_read_conn`

`read_conn()` now reads `self.inner.options.read_pool_timeout_ms` instead of the hardcoded 30_000. `PoolExhausted.timeout_ms` is set to the same value so error messages are accurate when a non-default timeout is configured.

`_test_hold_read_conn(hold_ms)`: acquires a read connection and sleeps `hold_ms` milliseconds, then drops the guard (returning the connection). Only compiled under `#[cfg(feature = "test-helpers")]`. Not in production builds.

### `fossic-py/src/types.rs` — drift fix

`TryFrom<&PyOpenOptions> for OpenOptions` struct literal was missing `read_pool_timeout_ms`. Added `read_pool_timeout_ms: 30_000`.

### `tests/read_pool.rs` — `pool_exhausted_returns_error`

Opens a `pool_size: 1, read_pool_timeout_ms: 50` store. Spawns a thread that calls `_test_hold_read_conn(200)` (holds the one connection for 200ms). Main thread sleeps 5ms to let the thread acquire, then calls `read_range` → asserts `Err(Error::PoolExhausted { pool_size: 1, timeout_ms: 50 })`.

---

## Tech debt / polish debt

PD-009 resolved. No new items.

---

## Public API changes

- `OpenOptions::read_pool_timeout_ms: u64` — new field; existing `..Default::default()` callers unaffected
- `test-helpers` feature — new; not part of `default` features; exposes `Store::_test_hold_read_conn`

---

## Adjacent project notifications

- **fossic-py**: struct literal updated inline; no Python-visible API change (`OpenOptions` is not yet exposed to Python callers directly)
- **fossic-tauri**: `OpenOptions` struct-literal callers must add `read_pool_timeout_ms` if any; likely uses `..Default::default()` → unaffected

---

## Living report updates

POLISH_DEBT.md: PD-009 status → resolved (pass-1.0.0z).
