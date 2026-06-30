---
pass: 10.1
version: v0.10.1
date: 2026-06-12
summary: Add event_type_filter to ReadQuery across all four layers (Rust core, fossic-py, fossic-node, fossic-tauri)
---

# Blast Radius — Pass 10.1 (v0.10.1)

> Live-filed blast-radius artifact.

## Files

### Modified

- `src/types.rs` — `ReadQuery` struct: added `pub event_type_filter: Option<String>` field and
  `event_type_filter: None` to the `ReadQuery::stream()` constructor.

- `src/read.rs` — `read_range_impl`: SQL query extended with NULL-guard clause
  `AND (?6 IS NULL OR event_type = ?6)`; 6th param `q.event_type_filter` added to
  `params![]`. Sentinel pattern retained for from/to/limit; NULL-guard pattern used for
  event_type_filter (mirrors AggregateQuery).

- `src/store.rs` — Two internal `ReadQuery { ... }` struct literals (in `take_snapshot_impl`
  and the snapshot replay path) updated with `event_type_filter: None`.

- `tests/integration.rs` — Two existing struct literal tests updated with
  `event_type_filter: None`. Five new tests added: `read_range_event_type_filter_returns_matching`,
  `read_range_event_type_filter_no_match_returns_empty`,
  `read_range_event_type_filter_combined_with_from_version`,
  `read_range_event_type_filter_limit_applied_after_filter`,
  `read_range_event_type_filter_none_returns_all`.

- `fossic-py/src/types.rs` — `PyReadQuery` struct: added `event_type_filter: Option<String>`
  field, added `event_type_filter = None` to `#[pyo3(signature)]` and `fn new(...)` params,
  updated `From<&PyReadQuery> for ReadQuery` to pass through the field.

- `fossic-py/python/fossic/__init__.py` — Added docstring to `Store.read_range` documenting
  `event_type_filter` parameter.

- `fossic-py/tests/test_cross_stream.py` — Removed `@pytest.mark.xfail` from
  `test_read_range_event_type_filter`; corrected `event_type="Alpha"` →
  `event_type_filter="Alpha"` (the xfail was using the wrong field name).

- `fossic-node/src/types.rs` — `ReadQueryJs` struct: added `pub event_type_filter: Option<String>`
  field; `From<ReadQueryJs> for fossic::ReadQuery` updated to thread the field through.

- `fossic-node/index.d.ts` — Populated from empty. Full TypeScript declarations for all
  public types (`StoredEvent`, `Append`, `ReadQuery`, `StreamInfo`, `BranchInfo`,
  `SnapshotInfo`, `SnapshotState`, `OpenOptions`, `SubscribeQuery`, `CreateBranch`) and
  classes (`EventId`, `FossicSubscription`, `Store`). `ReadQuery.eventTypeFilter?: string | null`
  added. `FossicSubscription` includes `next()`, `[Symbol.asyncIterator]()`, and
  `[Symbol.asyncDispose]()` matching the JS wrapper in `index.js`.

- `fossic-node/__test__/cross-stream.spec.ts` — Four new tests added:
  `eventTypeFilter returns only matching events`, `eventTypeFilter with no match returns empty array`,
  `eventTypeFilter combined with fromVersion`, `eventTypeFilter null returns all events`.

- `crates/fossic-tauri/src/commands.rs` — `fossic_read_range` command: added
  `event_type_filter: Option<String>` parameter and `if let Some(f) = event_type_filter { q.event_type_filter = Some(f); }` threading.

- `docs/implement/FOSSIC_V1_SPEC.md` — §6 ReadQuery: added `event_type_filter` field with
  comment. §4.2 usage example extended with `event_type_filter` PolicyViolation example.
  Tauri IPC command table: `fossic_read_range` args updated to include `event_type_filter`.

- `docs/aseptic/TECH_DEBT.md` — TD-002 resolved: status `open` → `resolved`,
  `pass_resolved: v0.10.1`, resolution summary + `<details>` wrapper for history.
  `last_reviewed: v0.10.1`.

### Created

- `crates/fossic-tauri/tests/read_range.rs` — Two Tauri integration tests using
  `tauri::test::mock_builder()` + `plugin_with_test_helpers`: `event_type_filter_returns_matching`
  and `event_type_filter_none_returns_all`.

- `docs/aseptic/blast-radius/pass-10.1.md` — this file.
- `docs/aseptic/cross-pollination/pass-10.1.md` — adjacent-project impact notes.

---

## Public APIs

### Added

- `fossic::ReadQuery.event_type_filter: Option<String>` — filters `read_range` results to
  events whose `event_type` matches exactly. `None` returns all types (backward-compatible).

- `fossic-py` `ReadQuery(event_type_filter=None)` — keyword parameter added to Python
  `ReadQuery` constructor.

- `fossic-node` `ReadQuery.eventTypeFilter?: string | null` — optional field in the JS
  `ReadQuery` interface. napi-rs snake_case → camelCase.

- `fossic-tauri` `fossic_read_range(..., event_type_filter: Option<String>)` — optional
  IPC parameter. Tauri deserializes absent JSON keys as `None` automatically.

### Modified

None — all changes are purely additive. Existing callers work unchanged.

---

## Schema changes

None — this is a read-path filter only; no new tables, columns, or indexes.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

`read_range` now supports server-side event type filtering. When `event_type_filter` is set,
the SQL query includes `AND (?6 IS NULL OR event_type = ?6)` — filtering happens in SQLite,
not in application code. `None` (default) is fully backward-compatible.

---

## Living report updates

### TECH_DEBT.md

Resolved:
- **TD-002** — ReadQuery lacks event_type_filter field (RB-1)
  (was OPEN, status → RESOLVED, pass_resolved: v0.10.1)

---

## Pass notes

**xfail field name bug:** The existing `test_read_range_event_type_filter` xfail test was
using `ReadQuery(stream_id="test/s", event_type="Alpha")` — a wrong field name (`event_type`
instead of `event_type_filter`). The fix was to correct the field name, not to add a new
field named `event_type`.

**index.d.ts was empty:** fossic-node's `index.d.ts` was empty prior to this pass.
napi-rs CLI v3 does not auto-generate TypeScript definitions for napi-derive v2 code.
The file was written manually from the Rust source types. `FossicSubscription` includes
`next()` and `[Symbol.asyncDispose]()` which are added by the JS wrapper in `index.js`
(not by the Rust binding directly).

**SQL param order:** `?5` is LIMIT (positional sentinel), `?6` is event_type_filter (NULL-guard).
SQLite numbered params are order-independent; the `params![]` array maps positionally.
