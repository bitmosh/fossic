---
pass: 3
version: v0.3.0
date: "(retroactive estimate, not verified)"
summary: CCE — content-addressed event identity via BLAKE3, deduplication property
---

# Blast Radius — Pass 3 (v0.3.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic/src/cce.rs` — canonical content encoding: JSON canonicalization + BLAKE3

### Modified
- `fossic/src/store.rs` — append path now computes EventId via CCE before insert
- `fossic/src/types.rs` — Append gains `causation_id`, `correlation_id` fields;
  EventId gains `as_bytes()` method
- `fossic/tests/cce.rs` — CCE property tests (created)

---

## Public APIs

### Modified (non-breaking)
- `Append` — added `causation_id: Option<EventId>`, `correlation_id: Option<EventId>`
  (both default to None; existing call sites unaffected)
- `EventId` — added `as_bytes() -> &[u8; 32]` method

---

## Schema changes

None. `events.id` was already a 32-byte BLOB; the computation of its value changed
from an arbitrary random/sequential ID to the CCE-derived BLAKE3 hash.

---

## Configuration changes

None.

---

## Dependency changes

None if blake3 was already in Cargo.toml; otherwise:
- Added: `blake3` — BLAKE3 hashing for CCE

---

## Behavior changes

- **EventId is now deterministic.** Two `Append` calls with identical `(event_type,
  type_version, causation_id, CCE(payload))` produce the same `EventId` and are
  silently deduplicated. The second append returns the existing EventId without error.
- **stream_id is NOT part of the CCE hash.** Identical event payloads across different
  streams share the same EventId and deduplicate across streams. This is the intended
  cross-stream identity property.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
