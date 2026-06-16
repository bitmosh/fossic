---
pass: v1.0.0w
version: v1.0.0w
sha: pending
date: 2026-06-16
summary: Phase 6b — crossbeam-channel read connection pool; concurrent reads no longer block each other or the write path
---

# Blast Radius — Pass v1.0.0w

## Files

### Modified
- `src/store.rs` — `StoreInner` gains two pool channel fields; `ReadGuard` struct added; `Store::read_conn()` helper added; ~16 pure-read methods switched from `self.lock()` to `self.read_conn()`; `compute_state_bytes` private helper switched; `take_snapshot` first acquisition switched (read), second stays write
- `src/types.rs` — `OpenOptions::read_pool_size: usize` added (default 4)
- `src/error.rs` — `Error::PoolExhausted { pool_size, timeout_ms }` variant added

### Created
- `tests/read_pool.rs` — new integration test file
- `docs/aseptic/blast-radius/pass-1.0.0w.md` — this file
- `docs/aseptic/pass-complete/pass-1.0.0w.md` — pass report

### Drift (discovered during implementation)
- `fossic-py/src/types.rs` — `TryFrom<&PyOpenOptions> for OpenOptions` had a struct literal missing `read_pool_size`; added `read_pool_size: 4` to match Python default

---

## Changes

### `src/error.rs` — `Error::PoolExhausted`

New variant returned when `recv_timeout(30s)` finds no available read connection.

### `src/types.rs` — `OpenOptions::read_pool_size`

New field, default 4. Determines how many read connections are opened at store startup.
Setting to 1 effectively restores single-connection read behaviour.

### `src/store.rs` — connection pool

**`ReadGuard`** struct: holds `Option<Connection>` + `Sender<Connection>`. `Deref` to `&Connection`. `Drop` returns the connection to the pool via `send()`. Option wrapping prevents double-return if `drop` is called after the connection is already consumed (should not happen in practice).

**`StoreInner`** additions: `read_pool_tx: Sender<Connection>` and `read_pool_rx: Receiver<Connection>`. Both stored so the guard can return connections via the sender and new acquisitions via the receiver.

**Pool init in `Store::open`**: opens `read_pool_size` connections, each configured with `PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 30000; PRAGMA query_only = ON`. `query_only = ON` is a safety net — if a pool connection is accidentally passed to a write function, SQLite returns SQLITE_READONLY rather than corrupting state.

**`Store::read_conn()`**: private helper that calls `recv_timeout(30s)` and wraps the result in `ReadGuard`. Returns `Error::PoolExhausted` on timeout.

**Methods switched to `read_conn()` (16):**
- `streams`, `stream_exists`
- `read_range`, `read_one`, `read_batch`, `read_by_external_id`, `read_by_correlation`, `walk_causation`, `aggregate`
- `get_cursor`, `list_branches`, `resolve_chain` (DB path only; cache hit path unchanged)
- `snapshot_info`, `get_snapshot_state`
- `compute_state_bytes` (private; called by all `read_state*` variants)
- `take_snapshot` (first lock only — reads events+snapshot; second lock remains write)

**Methods kept on `self.lock()` (write conn):**
- All appends, `declare_stream`, `register_upcaster`, `purge_event`, `set_cursor`, all branch mutations, `gc_orphaned_snapshots`, `write_snapshot_state`
- `subscribe` seed query (reads but infrequent setup; not worth the complexity)
- `take_snapshot` second lock (writes the snapshot row)

---

## Tech debt / polish debt

### TD-001 — `take_snapshot` dual-acquisition TOCTOU
`take_snapshot` acquires a read connection (events + snapshot read), releases it, computes state, then acquires the write connection to persist. A concurrent append between the two acquisitions could add events that are not included in the snapshot. This was already true with two `self.lock()` calls; Phase 6b doesn't make it worse, but the window is now wider (read conn ≠ write conn, so they don't serialise). Fix: restructure `take_snapshot` to acquire write lock for the full read+compute+write cycle, or use SQLite `BEGIN IMMEDIATE` for the read phase. Deferred — snapshots are idempotent; a slightly stale snapshot is not data loss.

### TD-002 — `subscribe` seed query on write conn
The seed query that determines starting cursors for glob subscriptions runs on the write connection (`self.lock()`). It's a pure read and could use `read_conn()`. Not a correctness issue — it's infrequent. Deferred.

### PD-001 — `PoolExhausted` not covered by integration tests
Testing exhaustion requires either a very short configurable timeout (not yet in OpenOptions) or a 30-second wait. Deferred. If `PoolExhausted` needs to be testable, add `OpenOptions::read_pool_timeout_ms` in a future polish pass.

---

## Public API changes

- `OpenOptions::read_pool_size: usize` — new field (existing `OpenOptions::default()` callers unaffected; field is in a non-exhaustive struct only by convention, not attribute, so callers using struct-update syntax `..Default::default()` are fine)
- `Error::PoolExhausted { pool_size, timeout_ms }` — new variant; match-exhaustive callers must add an arm

---

## Adjacent project notifications

- **Lattica / fossic-tauri**: `Error::PoolExhausted` is a new variant. Tauri command error handling (`FossicTauriError::from(Error)`) should handle it — check `crates/fossic-tauri/src/error.rs` if it pattern-matches on `Error` variants. (It likely uses a catch-all, so no break expected.)
- **fossic-py**: `Error` is opaque to Python callers (exposed as exception string). No change needed.

---

## Living report updates

None.
