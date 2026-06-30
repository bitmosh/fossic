# Pass Report — v1.0.0y

**Date:** 2026-06-16
**Version:** v1.0.0y
**Type:** cosmetic (TD-008 — subscribe seed queries off write connection)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| Both `self.lock()` calls in `Store::subscribe` → `self.read_conn()` | DONE | `src/store.rs` lines 380, 397 |

---

## 2. Test results

228 tests, 0 failures. Full workspace clean.

---

## 3. Files touched

- Modified: 1 file (`src/store.rs`)
- Created: 2 files (blast-radius, pass-complete)

---

## 4. API changes

None.

---

## 5. Tech debt / polish debt resolved

TD-008 — `subscribe` glob seed query on write connection. Resolved.

---

## 6. PASS COMPLETE message

```
── PASS COMPLETE · TD-008 (subscribe seed on read_conn) · fossic · v1.0.0y ──
Commit: pending

Both seed queries in Store::subscribe (glob MAX(version) GROUP BY stream_id
and exact-stream MAX(version)) moved from self.lock() to self.read_conn().
Pure reads; no write connection contention during subscription setup.

TD-008 resolved.

Tests: 228 passed, 0 failed
Branch: clean
```
