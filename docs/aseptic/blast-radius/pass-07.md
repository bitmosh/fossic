---
pass: 7
version: v0.7.0
date: "(retroactive estimate, not verified)"
summary: Upcasters, payload transforms, indexed_tags, external_id, aggregate queries, cursors
---

# Blast Radius — Pass 7 (v0.7.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic/src/upcasters.rs` — upcaster chain registration and application on read
- `fossic/src/transforms.rs` — payload transform hook registry (or integrated in store.rs)
- `fossic/tests/upcasters.rs` — chain test, gap test
- `fossic/tests/transforms.rs` — payload transform tests

### Modified
- `fossic/src/store.rs` — register_upcaster, register_payload_transform, aggregate,
  get_cursor, set_cursor, read_by_external_id, read_by_correlation, walk_causation
- `fossic/src/types.rs` — AggregateQuery, Append gains `type_version`, `external_id`,
  `indexed_tags` fields; correlation_id/causation_id fully wired
- `fossic/src/schema.rs` — external_id and indexed_tags columns on events table;
  cursors table; new indexes
- `fossic-py/src/store.rs` — Python bindings for all new methods
- `fossic-py/python/fossic/__init__.py` — Python wrappers for new methods
- `fossic-py/tests/test_cross_stream.py` — aggregate and cross-stream tests (created)
- `fossic-py/tests/test_upcasters.py` — upcaster tests (created)
- `fossic-py/tests/test_transforms.py` — transform tests (created)
- `fossic-py/tests/test_deletion.py` — purge and cursor tests (created)

---

## Public APIs

### Added
- `Store::register_upcaster(event_type, from_version, to_version, callable)`
- `Store::register_payload_transform(stream_pattern, callable)` — fires at append time;
  callable signature: `(event_type: str, payload: dict) -> dict`
- `Store::aggregate(query: AggregateQuery) -> Result<Vec<StoredEvent>>`
- `Store::get_cursor(consumer_id, stream_id, branch) -> Result<Option<u64>>`
- `Store::set_cursor(consumer_id, stream_id, branch, version)`
- `Store::read_by_external_id(stream_id, external_id) -> Result<Option<StoredEvent>>`
- `Store::read_by_correlation(correlation_id) -> Result<Vec<StoredEvent>>`
- `Store::walk_causation(start, direction, max_depth) -> Result<Vec<StoredEvent>>`
- `Store::purge_event(event_id, confirm, reason, purged_by)`
- `Store::shred_stream(stream_id, reason)` — requires encryption mode; raises NotImplemented
- `AggregateQuery { stream_pattern, event_type_filter, ... }`
- `Append` extended: `type_version`, `external_id`, `indexed_tags`

---

## Schema changes

- `events` table: added columns `external_id TEXT`, `indexed_tags TEXT`
- `cursors` table created: consumer_id, stream_id, branch, version
- Added indexes: `idx_events_external_id`, `idx_events_timestamp`, `idx_events_type`

---

## Configuration changes

None.

---

## Dependency changes

None (retroactive estimate — upcaster/transform logic uses only existing dependencies).

---

## Behavior changes

- Payload transforms fire at APPEND TIME, not read time. A transform registered after
  an event is stored has no effect on that event's payload.
- Upcasters fire at READ TIME. They are applied in chain order when reading events
  with a `type_version` below the current registered version.
- `purge_event` removes events from the read path entirely (read_one returns None).
  The purge is recorded as a `Purged` audit event in `_fossic/system`.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: TD-002 (ReadQuery missing event_type_filter) likely originates from this pass —
AggregateQuery got event_type_filter but ReadQuery did not. DV-002 (purge semantics)
also originates here.*
