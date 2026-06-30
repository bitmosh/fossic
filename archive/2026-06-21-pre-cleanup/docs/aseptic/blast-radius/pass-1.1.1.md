---
pass: v1.1.1
version: v1.1.1
date: 2026-06-20
prior-commit: 995ae97
summary: SystemStreamWriter abstraction — dedicated system-stream connection; Phase 1 anchor comments
---

# Blast Radius — Pass v1.1.1

## Files

### Created
- `src/system_stream.rs` — `SystemStreamWriter` struct; `new`, `emit`, `emit_subscription_degraded`
- `docs/aseptic/blast-radius/pass-1.1.1.md` — this file

### Modified
- `src/store.rs` — `start_dispatcher` rewritten to use `SystemStreamWriter`; `write_degraded_event` removed; `TransactionBehavior` + `now_us` imports removed
- `src/lib.rs` — `mod system_stream;` added
- `src/types.rs` — `// ── PHASE 1 TYPES ──` anchor comment added above bounded-read types section
- `src/error.rs` — `// ── PHASE 1 ERRORS ──` anchor comment added above `ReadBudgetExceeded`

---

## Changes

### `src/system_stream.rs` (new)

`SystemStreamWriter` owns a dedicated SQLite connection opened at dispatcher-thread startup — separate from the store write mutex and read pool. This prevents system-event writes from contending with user appends.

**`new(db_path: &Path) -> Option<Self>`** — opens connection with WAL + 30s busy_timeout. Returns `None` on failure (WARN to stderr); callers tolerate absence.

**`emit(&mut self, event_type, payload, indexed_tags)`** — derives CCE event id internally via `derive_event_id`; writes to `_fossic/system` / `main`; silently drops errors (best-effort delivery). Never exposes event_id to callers.

**`emit_subscription_degraded(&mut self, sub_id, stream_id, branch, dropped_version)`** — constructs the same `serde_json::json!` payload with identical field set as the removed `write_degraded_event`. CCE-derived event id is byte-identical.

### `src/store.rs`

`start_dispatcher` no longer opens a raw `Connection` or calls `write_degraded_event`. It constructs a `SystemStreamWriter::new(&db_path)` once at thread start and calls `writer.emit_subscription_degraded(...)` for each degraded subscription id. The writer is held exclusively by the dispatcher thread — no sharing, no locking.

Removed: `write_degraded_event` function (~55 lines). Removed imports: `TransactionBehavior`, `now_us` (both were only used by the removed function).

### `src/lib.rs`

`mod system_stream;` added. The module is `pub(crate)` — no new public API surface.

### `src/types.rs`

Phase 1 anchor comment inserted above the bounded-read types block:
```
// ── PHASE 1 TYPES ─────────────────────────────────────────────────────────────
// All Phase 1 (Bounded Resource API) types live below this marker.
```

### `src/error.rs`

Phase 1 anchor comment inserted above `ReadBudgetExceeded`:
```
// ── PHASE 1 ERRORS ────────────────────────────────────────────────────────────
// All Phase 1 (Bounded Resource API) error variants live below this marker.
```

---

## Public API changes

**None.** `SystemStreamWriter` is `pub(crate)`. No change to `Store`, `Error`, or any exported type.

---

## Regression spec

`post_commit_overflow_writes_system_event` and all 20 other subscription tests pass without modification. 246/246 total.

---

## Tech debt / polish debt

**No new entries this pass.**

---

## Adjacent project notifications

No FFI-visible changes. fossic-py, fossic-node, fossic-tauri unaffected.
