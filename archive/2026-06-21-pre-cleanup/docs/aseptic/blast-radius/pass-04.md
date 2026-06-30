---
pass: 4
version: v0.4.0
sha: 65eefe5
date: 2026-06-12
summary: Cross-stream queries, upcasters, transforms, deletion, cursors
---

# Blast Radius — Pass 4 (v0.4.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content completely rewritten — original bootstrap estimate described branches
> (which are v0.2.0). Actual commit 65eefe5 added cross-stream, upcasters,
> transforms, deletion, and cursors.

## Files

### Created

10 files in commit 65eefe5:

- `src/cross_stream.rs` — `aggregate`, `read_by_correlation`, `walk_causation`, `read_by_external_id`
- `src/cursors.rs` — `get_cursor`, `set_cursor` for durable consumer position tracking
- `src/deletion.rs` — `purge_event`, `shred_stream` (shred requires encryption mode; raises NotImplemented)
- `src/transforms.rs` — `register_payload_transform` — append-time payload mutation hook
- `src/upcasters.rs` — `register_upcaster` — read-time schema migration chain
- `tests/cross_stream.rs` — aggregate query tests, causation chain traversal tests
- `tests/cursors.rs` — cursor get/set round-trip tests, consumer position tests
- `tests/deletion.rs` — purge tests, audit trail verification, shred NotImplemented tests
- `tests/transforms.rs` — payload transform tests; verifies transform fires at append time
- `tests/upcasters.rs` — upcaster chain tests, version gap detection tests

---

## Public APIs

### Added

**Cross-stream queries:**
- `Store::aggregate(query: AggregateQuery) -> Result<Vec<StoredEvent>>` — event query across streams matching a pattern
- `Store::read_by_external_id(stream_id, external_id: &str) -> Result<Option<StoredEvent>>` — lookup by `external_id` field
- `Store::read_by_correlation(correlation_id: EventId) -> Result<Vec<StoredEvent>>` — all events sharing a correlation ID
- `Store::walk_causation(start: EventId, direction: CausationDirection, max_depth: usize) -> Result<Vec<StoredEvent>>` — traverse causation chain up or down
- `AggregateQuery { stream_pattern, event_type_filter, from_version, to_version, limit }` — cross-stream query descriptor
- `CausationDirection::Upstream` / `CausationDirection::Downstream`

**Cursors:**
- `Store::get_cursor(consumer_id: &str, stream_id: &str, branch: &str) -> Result<Option<u64>>` — retrieve durable consumer position
- `Store::set_cursor(consumer_id: &str, stream_id: &str, branch: &str, version: u64) -> Result<()>` — persist consumer position

**Deletion:**
- `Store::purge_event(event_id: EventId, confirm: &str, reason: &str, purged_by: &str) -> Result<()>` — remove event from read path; `confirm` must equal `"CONFIRM"` or `PurgeConfirmationError` is raised; audit event written to `_fossic/system` stream
- `Store::shred_stream(stream_id, reason: &str) -> Result<()>` — raises `NotImplementedError`; requires encryption mode not yet present

**Transforms + Upcasters:**
- `Store::register_payload_transform(stream_pattern: &str, transform: Box<dyn PayloadTransform>) -> Result<()>` — fires at append time; transform signature: `(event_type: &str, payload: Value) -> Value`
- `Store::register_upcaster(event_type: &str, from_version: u32, to_version: u32, upcaster: Box<dyn Upcaster>) -> Result<()>` — fires at read time; migrates payloads below current version
- `PayloadTransform` trait — `apply(event_type: &str, payload: Value) -> Value`
- `Upcaster` trait — `from_version() -> u32`, `to_version() -> u32`, `upcast(payload: Value) -> Value`

---

## Schema changes

- `cursors` table created — columns: `consumer_id TEXT`, `stream_id TEXT`, `branch TEXT`, `version INTEGER`, composite primary key (consumer_id, stream_id, branch)
- `events` table: `external_id TEXT` column and `indexed_tags TEXT` column (retroactive — may have been in v0.1.0 initial schema; `Append` struct already had these fields)
- Added indexes: `idx_events_external_id` on events.external_id, `idx_events_correlation` on events.correlation_id

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

- **Payload transforms fire at APPEND TIME**, not read time. A transform registered after an event is stored has no effect on that event's payload.
- **Upcasters fire at READ TIME.** Applied in chain order when reading events with `type_version` below the current registered version.
- **`purge_event` removes events from the read path entirely.** `read_one` returns `None` after purge; `read_range` omits the event. The purge is recorded as an audit event in `_fossic/system`. (DV-002 origin — spec used "tombstone" language implying a marked-but-readable event; implementation chose "removes from read path" model. See DEVIATION.md DV-002.)
- **`shred_stream` raises NotImplementedError** — spec §9 defines shred but encryption mode is not implemented.
- TD-002 origin: `ReadQuery` does not have `event_type_filter`; `AggregateQuery` does. The asymmetry is a gap.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: DV-002 (purge_event removes from read path vs. tombstone spec language) and TD-002 (ReadQuery missing event_type_filter) originate from this pass.*
