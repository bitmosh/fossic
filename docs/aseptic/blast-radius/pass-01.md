---
pass: 1
version: v0.1.0
date: "(retroactive estimate, not verified)"
summary: Initial fossic Rust crate — SQLite store, stream declaration, append, read
---

# Blast Radius — Pass 1 (v0.1.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic/src/lib.rs` — public API exports
- `fossic/src/store.rs` — Store struct, open/declare/append/read_range/read_one
- `fossic/src/types.rs` — Append, ReadQuery, StoredEvent, EventId, StreamInfo
- `fossic/src/schema.rs` — SQL schema creation (or inline in store.rs)
- `fossic/src/error.rs` — FossicError hierarchy
- `fossic/Cargo.toml` — initial dependencies (rusqlite, serde, rmp-serde, blake3 stub)
- `fossic/tests/append_read.rs` — basic store tests

---

## Public APIs

### Added
- `Store::open(path: &str) -> Result<Store>` — open or create store
- `Store::declare_stream(stream_id, declared_by, description) -> Result<()>`
- `Store::append(a: Append) -> Result<EventId>`
- `Store::read_range(query: ReadQuery) -> Result<Vec<StoredEvent>>`
- `Store::read_one(event_id: EventId) -> Result<Option<StoredEvent>>`
- `Append { stream_id, event_type, payload }` — minimal initial shape
- `ReadQuery { stream_id }` — minimal initial shape
- `StoredEvent { id, stream_id, version, event_type, payload() }` — minimal shape
- `EventId` — opaque 32-byte identifier

---

## Schema changes

- `events` table created — initial columns: id, stream_id, branch, version, timestamp_us,
  event_type, type_version, payload (retroactive estimate: causation/correlation may have
  been added in a later pass)
- `meta` table created — schema versioning
- `streams` table created (or stream declaration tracked via meta)

---

## Configuration changes

- `OpenOptions` not yet present (retroactive estimate — added later)

---

## Dependency changes

- Added: `rusqlite` — SQLite storage
- Added: `serde`, `serde_json` — serialization
- Added: `rmp-serde` — msgpack encoding for payloads
- Added: `blake3` — content-addressed event identity (retroactive estimate — may have
  landed in Pass 3 with CCE)

---

## Behavior changes

- N/A — initial implementation

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
