---
pass: 1
version: v0.1.0
sha: b92d00f
date: 2026-06-12
summary: Core Rust event store — append, read, CCE, typed errors, schema
---

# Blast Radius — Pass 1 (v0.1.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> All content updated from "(retroactive — Aseptic not yet active)" estimates
> to reflect the canonical git commit b92d00f.

## Files

### Created

13 files in commit b92d00f:

- `Cargo.toml` — workspace manifest + fossic crate definition; dependencies: rusqlite, serde, serde_json, rmp-serde, blake3, thiserror, uuid
- `Cargo.lock` — dependency lockfile
- `src/lib.rs` — public re-exports for the fossic crate
- `src/store.rs` — `Store` struct; open, declare_stream, append, read_range, read_one, read_since
- `src/types.rs` — `Append`, `ReadQuery`, `StoredEvent`, `EventId`, `StreamInfo`, `OpenOptions`
- `src/schema.rs` — SQL schema: `events`, `streams`, `meta` table creation and migrations
- `src/error.rs` — `FossicError` enum and all error variants
- `src/append.rs` — append-path internals: CCE computation, INSERT logic, dedup check
- `src/read.rs` — read-path internals: query construction, row mapping
- `src/stream.rs` — stream declaration and lookup
- `src/cce.rs` — content-addressed event identity: `canonical_content_encode`, `content_address_event`
- `tests/integration.rs` — end-to-end store tests (open, declare, append, read round-trips)
- `tests/cce_vectors.rs` — 12 deterministic BLAKE3 hash verification fixtures

---

## Public APIs

### Added

- `Store::open(path: impl AsRef<str>, opts: OpenOptions) -> Result<Store>` — open or create a fossic store at the given path
- `Store::declare_stream(stream_id, declared_by, description) -> Result<()>` — register a stream before appending
- `Store::append(a: Append) -> Result<EventId>` — append an event; computes CCE hash, deduplicates silently if already present
- `Store::read_range(query: ReadQuery) -> Result<Vec<StoredEvent>>` — paginated read with version range
- `Store::read_one(event_id: EventId) -> Result<Option<StoredEvent>>` — fetch a single event by content-addressed ID
- `Store::read_since(stream_id, branch, from_version, limit) -> Result<Vec<StoredEvent>>` — convenience read-forward
- `Append { stream_id, branch, event_type, type_version, payload, causation_id, correlation_id, external_id, indexed_tags }` — full append descriptor
- `ReadQuery { stream_id, branch, from_version, to_version, limit, event_type_filter, direction }` — full query descriptor
- `StoredEvent { id, stream_id, branch, version, timestamp_us, event_type, type_version, payload, causation_id, correlation_id, external_id, indexed_tags }` — event row
- `EventId` — 32-byte BLAKE3 content-addressed identifier; `as_bytes() -> &[u8; 32]`, `to_hex() -> String`
- `StreamInfo { stream_id, declared_by, description, created_at }` — stream metadata
- `OpenOptions { path, wal_mode, busy_timeout_ms }` — store configuration
- `cce::canonical_content_encode(payload: &Value) -> Vec<u8>` — deterministic JSON serialization
- `cce::content_address_event(event_type, type_version, causation_id, correlation_id, cce_payload) -> EventId` — hash computation
- `FossicError` variants: `StreamNotDeclared`, `HashMismatch`, `DuplicateStream`, `EventNotFound`, `SchemaVersion`, `StorageError`, `PurgeConfirmationError`, and others

---

## Schema changes

Initial schema created:

- `events` table — columns: `id BLOB PRIMARY KEY`, `stream_id TEXT`, `branch TEXT DEFAULT 'main'`, `version INTEGER`, `timestamp_us INTEGER`, `event_type TEXT`, `type_version INTEGER DEFAULT 1`, `payload BLOB`, `causation_id BLOB`, `correlation_id BLOB`, `external_id TEXT`, `indexed_tags TEXT`; indexes on (stream_id, branch, version)
- `streams` table — columns: `stream_id TEXT PRIMARY KEY`, `declared_by TEXT`, `description TEXT`, `created_at INTEGER`
- `meta` table — columns: `key TEXT PRIMARY KEY`, `value TEXT`; used for schema version tracking

---

## Configuration changes

- `OpenOptions` introduced — `wal_mode` (default true), `busy_timeout_ms` (default 5000)

---

## Dependency changes

In `Cargo.toml`:
- Added: `rusqlite` with bundled feature — SQLite storage
- Added: `serde`, `serde_json` — serialization
- Added: `rmp-serde` — msgpack encoding for event payloads
- Added: `blake3` — BLAKE3 hashing for CCE
- Added: `thiserror` — error derive macro
- Added: `uuid` — used internally for correlation IDs when not provided

---

## Behavior changes

- N/A — initial implementation.
- **CCE identity property:** `EventId` is deterministic. Two `Append` calls with identical `(event_type, type_version, causation_id, correlation_id, CCE(payload))` produce the same `EventId`. Duplicate appends are silently accepted and return the existing ID.
- **stream_id is NOT part of the CCE hash.** Identical event payloads across different streams share the same `EventId` — this is the intended cross-stream identity property.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
