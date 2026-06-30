---
pass: v1.0.0y
version: v1.0.0y
date: 2026-06-16
summary: TD-008 — move subscribe glob/exact seed queries from write connection to read pool
---

# Blast Radius — Pass v1.0.0y

## Files

### Modified
- `src/store.rs` — two `self.lock()?` calls in `Store::subscribe` replaced with `self.read_conn()?` (glob seed query + exact-stream cursor seed)

---

## Changes

### `src/store.rs` — `Store::subscribe` seed queries

Both the glob seed path (`MAX(version) GROUP BY stream_id`) and the exact-stream seed path (`MAX(version) WHERE stream_id = ?`) are pure reads. They previously ran on the write connection, causing unnecessary contention with concurrent appends during subscription setup.

Both now use `self.read_conn()` (pool connection). `ReadGuard` derefs to `&Connection`; the query logic is unchanged.

---

## Tech debt / polish debt

TD-008 resolved. No new items.

---

## Public API changes

None.

---

## Living report updates

TECH_DEBT.md: TD-008 status → resolved (pass-1.0.0y).
