# SR-03 — Event Lifecycle: Append to Read

**Series:** Fossic State Reports — research-grade implementation reference
**Scope:** Stream declaration, full append path, conditional/batch appends, external_id, indexed_tags, all read operations, row deserialization, cursors, upcaster application at read time.
**See also:** SR-01 (CCE/identity), SR-02 (storage/concurrency), SR-04 (subscriptions), SR-08 (schema evolution/upcasters in depth)

---

## 1. Stream Declaration

Every event in fossic belongs to a **stream**. Streams must be explicitly declared before they can receive events. This is a deliberate design constraint: it prevents typo-driven stream proliferation and makes the store's topology legible via `streams()`.

### declare_stream

```rust
store.declare_stream(stream_id, declared_by, description)?;
```

Internally:

```sql
INSERT OR IGNORE INTO streams (id, declared_by, declared_at, description)
VALUES (?1, ?2, ?3, ?4)
```

**`INSERT OR IGNORE`** makes this idempotent. Calling `declare_stream` on an already-declared stream does nothing and returns `Ok(())`. This is safe to call on every startup without checking existence first.

**`declared_at`** is set to `now_us()` — microseconds since Unix epoch as `i64` — at the time of the call. On subsequent calls (OR IGNORE path), the original `declared_at` is preserved in the row.

**`declared_by`** is a consumer-supplied string. It is never validated beyond being a non-empty Rust `&str`. Canonical convention across the codebase: the relay or service name and version, e.g. `"cerebra-relay-v1"`, `"policy-scout"`, `"fossic-internal"`.

**`description`** is `Option<&str>`. Pass `None` if no human-readable description is needed; `Some("...")` otherwise. Stored as TEXT or NULL.

### stream_id validation rules

Before the INSERT, `stream_id` is validated:

1. **Non-empty** — zero-length string returns `Error::InvalidStreamId`.
2. **No null bytes** — any `\0` byte returns `Error::InvalidStreamId`.
3. **Segment separator** — `/` is the canonical segment separator. Segments may contain any non-null, non-`/` characters.
4. **Reserved prefix** — `_fossic/` is reserved for internal system streams. Attempting to declare a stream with this prefix via the public API returns `Error::InvalidStreamId { reason: "prefix '_fossic/' is reserved" }`.

Valid stream IDs: `"cerebra/agent-trace/sess_abc"`, `"policy-scout/posture"`, `"lattica/hub/relay-status"`.

Invalid: `""`, `"has\0null"`, `"_fossic/anything"`.

### System streams

`_fossic/system` is created automatically during store bootstrap (in `bootstrap_system_streams`):

```sql
INSERT OR IGNORE INTO streams (id, declared_by, declared_at, description)
VALUES ('_fossic/system', 'fossic', ?1, 'Internal fossic system events')
```

This runs during `Store::open`, before any consumer code executes. `_fossic/system` never needs to be declared by consumers. It receives `SubscriptionDegraded` events, `Purged` audit events, and other internal operational events.

The dispatcher skips system streams when firing subscriber callbacks, preventing loops where a system event triggers a subscriber that writes another system event.

### stream_exists

```rust
let exists: bool = store.stream_exists(stream_id)?;
```

Non-mutating check. Uses a read connection from the pool. Returns `true` if the stream ID is in the `streams` table, `false` otherwise. Does not acquire the write lock.

### streams()

```rust
let all: Vec<StreamInfo> = store.streams()?;
```

Returns all declared streams, ordered by declaration time (or insertion order — SQLite heap order for a table scan without ORDER BY). The `StreamInfo` type:

```rust
pub struct StreamInfo {
    pub id: String,
    pub declared_by: String,
    pub declared_at: i64,   // microseconds since Unix epoch
    pub description: Option<String>,
}
```

This is the primary API for relay backfill loops: iterate `streams()`, filter by pattern, then `read_range` each matching stream.

---

## 2. The Append Type

Every write to fossic starts with an `Append` value:

```rust
pub struct Append {
    pub stream_id: String,
    pub branch: String,                          // "main" if not specified
    pub event_type: String,                      // e.g. "UserCreated", "StepStarted"
    pub type_version: u32,                       // schema version, default 1
    pub payload: serde_json::Value,              // the event payload as JSON
    pub causation_id: Option<EventId>,           // the event that caused this one
    pub correlation_id: Option<EventId>,         // grouping key for correlated events
    pub external_id: Option<String>,             // consumer-supplied dedup key
    pub indexed_tags: Option<serde_json::Value>, // must be JSON object if Some
}
```

### Field semantics

**`stream_id`** — must be declared before append. Returns `StreamNotDeclared` if not.

**`branch`** — defaults to `"main"`. Versions are per-`(stream_id, branch)` pair. Writing to a branch does not affect the `main` branch's version sequence. See SR-05 for full branch documentation.

**`event_type`** — the event type name. Used in CCE identity derivation and in subscriber/reducer pattern matching. Case-sensitive. No validation beyond being a non-empty string.

**`type_version`** — schema version for upcasting. Default is `1`. Increment when the payload schema changes in a backward-incompatible way and register a corresponding upcaster. See SR-08.

**`payload`** — the event payload as `serde_json::Value`. On the write path:
1. Serialized to msgpack bytes for storage (`rmp_serde::to_vec_named`).
2. (After transforms) deserialized back and CCE-encoded for identity derivation.

Consumers work with `serde_json::Value` but msgpack is the storage format — the round-trip is lossless for all JSON-representable values.

**`causation_id`** — optional `EventId` of the event that caused this append. Enables `walk_causation` graph traversal. If set, it is included in the CCE identity derivation — the same causation_id produces the same event id only if all other fields match too.

**`correlation_id`** — optional grouping key. Used by `read_by_correlation` to fetch all events sharing a correlation. Not part of the CCE identity formula (two events can share a correlation_id and still have different ids). Indexed via `idx_events_correlation`.

**`external_id`** — optional consumer-supplied identifier. Indexed per-stream for deduplication. See section 6.

**`indexed_tags`** — optional JSON object. If `Some`, must be a JSON object — not array, not string, not number. Returns `Error::InvalidIndexedTags { got: "array" }` (or whichever variant name applies) if the value is not an object. See section 7.

---

## 3. Full Append Path — Step by Step

`append_impl` in `src/append.rs` is the single codepath through which every event enters the store. Understanding it completely is essential for reasoning about consistency, performance, and subscriber timing.

### Step 1: Serialize payload to msgpack

Before acquiring the write lock, the `serde_json::Value` payload is serialized to msgpack bytes:

```rust
let payload_msgpack: Vec<u8> = rmp_serde::to_vec_named(&append.payload)?;
```

`rmp_serde::to_vec_named` uses named fields (the "name/value" msgpack variant) rather than positional. This is important for forward compatibility — named msgpack can evolve more gracefully.

This serialization happens before the write lock is acquired because it is purely computational and may allocate.

### Step 2: Acquire the write lock

```rust
let mut write_conn = self.inner.write_conn.lock()?;
```

A single `Mutex<Connection>` guards all writes. Only one writer can proceed at a time. The mutex is fair (FIFO on most platforms). Blocking here is expected under write contention; the busy_timeout PRAGMA (30,000ms) covers SQLite-level contention, but the Rust mutex has no timeout — a hung writer will block indefinitely.

### Step 3: Open IMMEDIATE transaction

```rust
let tx = write_conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
```

`IMMEDIATE` acquires the SQLite write lock at transaction start, not at first write. This prevents "upgrade from read to write" conflicts that can cause `SQLITE_BUSY` mid-transaction. Under WAL mode, IMMEDIATE is the correct choice for write transactions that will INSERT.

From this point, all reads within the transaction see a consistent snapshot of the database as it existed when the transaction opened.

### Step 4: Validate stream declared

```rust
let declared: bool = tx.query_row(
    "SELECT 1 FROM streams WHERE id = ?1",
    [&append.stream_id],
    |_| Ok(true),
).optional()?.is_some();

if !declared {
    return Err(Error::StreamNotDeclared { stream_id: append.stream_id.clone() });
}
```

This read happens INSIDE the write transaction. The stream table is stable (declare_stream is also a write operation), so this check is consistent.

### Step 5: Apply payload transforms

```rust
let payload_bytes = apply_transforms(
    &self.inner.transforms,
    &append.stream_id,
    &append.event_type,
    payload_msgpack,
)?;
```

Transforms receive and return msgpack bytes. Each registered `PayloadTransform` whose pattern matches `stream_id` is applied in registration order, chaining its output as the next transform's input. If no transforms match, the original msgpack bytes are returned unchanged without copying.

**The transform chain runs on msgpack bytes, not on `serde_json::Value`.** This is the canonical form: transforms work at the binary encoding level. A Python transform callable (`PyTransform`) decodes msgpack → Python dict → runs the Python function → re-encodes the result to msgpack. See SR-08 for the full transform documentation.

### Step 6: Derive event ID from the transformed payload

After transforms, the transformed msgpack bytes are decoded back to `serde_json::Value` for CCE encoding:

```rust
let payload_for_cce: serde_json::Value = rmp_serde::from_slice(&payload_bytes)?;

let id_bytes: [u8; 32] = fossic::cce::derive_event_id(
    &append.event_type,
    append.type_version,
    append.causation_id.as_ref().map(|id| id.as_bytes()),
    &payload_for_cce,
)?;

let event_id = EventId::from_bytes(id_bytes);
```

**Critical invariant:** the event ID is derived from the **post-transform** payload. If transforms modify the payload, the stored ID reflects the transformed form, not the original. This is intentional — it makes the ID a content address of what is actually stored, not what was submitted.

The full CCE formula (from SR-01):

```
blake3(
    b"fossic-cce-v1\0"
    || cce_encode_string(event_type)
    || cce_encode_uint_as_i64(type_version)
    || cce_encode_optional_bytes(causation_id)  // None → NULL tag; Some → BYTES tag
    || cce_encode(payload_for_cce)
)
```

`correlation_id`, `external_id`, `indexed_tags`, `branch`, and `stream_id` are **not** part of the identity formula. Two events with different correlation_ids but identical event_type, type_version, causation_id, and payload will produce the same EventId. This can cause a PRIMARY KEY conflict on INSERT — handle accordingly if using correlation_id to distinguish otherwise-identical events.

### Step 7: Assign version

```rust
let version: i64 = tx.query_row(
    "SELECT COALESCE(MAX(version), -1) + 1 FROM events \
     WHERE stream_id = ?1 AND branch = ?2",
    [&append.stream_id, &append.branch],
    |r| r.get(0),
)?;
```

Version is a monotonically increasing `i64` starting at `0` for the first event in a `(stream_id, branch)` pair. `COALESCE(MAX(version), -1) + 1` elegantly handles both cases: no events → MAX returns NULL → COALESCE returns -1 → +1 = 0; existing events → MAX returns last version → +1 = next.

This MAX query runs inside the IMMEDIATE transaction, so no concurrent writer can sneak in a version between this read and the subsequent INSERT. The `UNIQUE(stream_id, branch, version)` constraint is a safety net, not the primary mechanism — the IMMEDIATE transaction prevents the race.

### Step 8: Get current timestamp

```rust
let timestamp_us: i64 = schema::now_us();
```

`now_us()` calls `SystemTime::now().duration_since(UNIX_EPOCH).as_micros() as i64`. Monotonicity is NOT guaranteed across calls. If the system clock is adjusted backward, two events in sequence could have the same or decreasing timestamps. Consumers that need strict ordering must use `version`, not `timestamp_us`. Timestamps are approximate wall-clock times, useful for human display and time-range queries but not for strict ordering within a stream.

### Step 9: Validate indexed_tags

```rust
if let Some(ref tags) = append.indexed_tags {
    match tags {
        serde_json::Value::Object(_) => {}
        other => return Err(Error::InvalidIndexedTags { got: other.type_name() }),
    }
}
```

Only objects are valid. Array, string, number, bool, and null all return `InvalidIndexedTags`. Key validation (alphanumeric + underscore) happens in the cross-stream `aggregate` codepath at query time, not at append time — keys are stored as-is in the JSON TEXT column. Callers that store non-alphanumeric keys will find them unfilterable via `indexed_tags_filter` in aggregate queries.

### Step 10: INSERT the event

```rust
tx.execute(
    "INSERT INTO events \
     (id, stream_id, branch, version, timestamp_us, \
      causation_id, correlation_id, event_type, type_version, \
      payload, external_id, indexed_tags) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    rusqlite::params![
        event_id,            // BLOB: 32 bytes via ToSql impl on EventId
        append.stream_id,
        append.branch,
        version,
        timestamp_us,
        append.causation_id,   // Option<EventId> → NULL or BLOB
        append.correlation_id, // Option<EventId> → NULL or BLOB
        append.event_type,
        append.type_version as i64,
        payload_bytes,       // BLOB: msgpack bytes
        append.external_id,  // Option<String> → NULL or TEXT
        indexed_tags_json,   // Option<String>: JSON.to_string() or NULL
    ],
)?;
```

**Stored types:**
- `id`: BLOB (32 bytes)
- `payload`: BLOB (msgpack)
- `indexed_tags`: TEXT (JSON string of the Value::Object), or NULL

**PRIMARY KEY collision:** if two events produce the same EventId (same type + version + causation + post-transform payload), the second INSERT fails with `SQLITE_CONSTRAINT_PRIMARYKEY`. This propagates as `Error::Sqlite(rusqlite::Error::SqliteFailure(...))`. In practice, identical inputs from different callers produce identical ids — this is a feature (the store is idempotent for identical events), but callers must decide whether to treat this as success or error.

### Step 11: Synchronous subscriber callbacks

If any subscribers are registered in `Synchronous` mode for the stream pattern, they fire **before** `tx.commit()`:

```rust
// Inside append_impl, before commit:
dispatch_sync(&registry, &event);
```

`dispatch_sync` calls each matching `Synchronous` subscriber's `on_event` callback while still inside the write transaction. The callback runs synchronously on the write thread. If the callback panics, the panic is caught, the subscription is marked degraded, and a `SubscriptionDegraded` event is written to `_fossic/system`. The transaction proceeds normally even if a synchronous subscriber panics.

The write lock is held during synchronous callbacks. Long-running synchronous callbacks block all writes.

### Step 12: Commit

```rust
tx.commit()?;
```

The IMMEDIATE transaction is committed. At this point the event is durably written (subject to `synchronous` PRAGMA — set to `NORMAL`, meaning fsyncs on WAL checkpoints but not every commit; this trades a small data-loss window for throughput).

After commit, the write lock (`Mutex<Connection>`) is released.

### Step 13: PostCommit subscriber dispatch

After the transaction commits and the write lock is released, the dispatcher notifies PostCommit subscribers:

```rust
dispatch_post_commit(&registry, event_id, &stream_id, &branch, version);
```

`dispatch_post_commit` increments the in-memory cursor for the stream and wakes up each matching subscriber's handler thread. The handler thread reads the new event from the store using a read pool connection and calls `on_event` on the subscriber's `SubscriptionHandler`.

PostCommit dispatch is **asynchronous with respect to the caller** — `append` returns to the caller before PostCommit subscribers have processed the event. See SR-04 for full subscription dispatch documentation.

### Step 14: Return EventId

```rust
Ok(event_id)
```

The 32-byte `EventId` is returned. This is the permanent identity of the stored event.

---

## 4. append_if — Conditional Append

`append_if` extends the basic append with a user-supplied condition closure that is evaluated inside the IMMEDIATE transaction before the INSERT:

```rust
pub fn append_if<F>(&self, append: Append, condition: F) -> Result<Option<EventId>, Error>
where
    F: FnOnce(&Transaction) -> Result<bool, Error>,
```

Returns:
- `Ok(Some(event_id))` — condition returned `true`, event was appended.
- `Ok(None)` — condition returned `false`, transaction rolled back, no event stored.
- `Err(e)` — condition returned `Err`, or the append itself failed.

### Execution sequence

1. All steps through step 9 (validate, transforms, derive ID, assign version).
2. **Call condition closure** — passing the open `&Transaction`.
3. If `false` → `tx.rollback()` (or just drop) → return `Ok(None)`.
4. If `true` → proceed with INSERT, commit, dispatch.

### What the condition closure can do

The condition receives a `&Transaction` that is already in IMMEDIATE mode. It can:

- Run arbitrary `SELECT` queries against any table in the store.
- Use all rusqlite query APIs.
- Read the current version of any stream, check for the existence of specific events, inspect snapshots, etc.

The condition cannot write to the database — it receives a shared reference to the transaction, and rusqlite's API enforces this at the type level. Writes inside the condition would require a mutable reference.

The condition runs in the same IMMEDIATE transaction as the eventual INSERT. Any reads inside the condition are consistent: they see all commits that preceded this transaction's start, and nothing that has been committed since (WAL snapshot isolation).

### Use cases

**Optimistic concurrency / version guard:**
```rust
store.append_if(append, |tx| {
    let current_version: Option<i64> = tx.query_row(
        "SELECT MAX(version) FROM events WHERE stream_id = ?1 AND branch = 'main'",
        [&stream_id],
        |r| r.get(0),
    ).optional()?;
    Ok(current_version == Some(expected_version as i64))
})?;
```
Only appends if the current version matches `expected_version`. If a concurrent writer inserted a new event between when the caller read the version and when this transaction opens, the condition will see the new version and return `false`.

**Existence guard:**
```rust
store.append_if(append_completed, |tx| {
    let started_exists: bool = tx.query_row(
        "SELECT 1 FROM events WHERE stream_id = ?1 AND event_type = 'ProcessingStarted'",
        [&stream_id],
        |_| Ok(true),
    ).optional()?.is_some();
    Ok(started_exists)
})?;
```
Only appends `ProcessingCompleted` if `ProcessingStarted` has been recorded.

**Deduplication:**
```rust
store.append_if(append, |tx| {
    let already: bool = tx.query_row(
        "SELECT 1 FROM events WHERE stream_id = ?1 AND external_id = ?2",
        [&stream_id, &external_id_str],
        |_| Ok(true),
    ).optional()?.is_some();
    Ok(!already)
})?;
```
Manual deduplication (though `external_id` on `Append` handles this more idiomatically).

---

## 5. append_batch — Atomic Multi-Event Write

```rust
pub fn append_batch(&self, appends: &[Append]) -> Result<Vec<EventId>, Error>
```

Appends a slice of events in a single IMMEDIATE transaction. Returns a `Vec<EventId>` in the same order as the input slice. All events succeed or all fail atomically.

### Execution

1. A single IMMEDIATE transaction is opened for the entire batch.
2. For each `Append` in order:
   a. Validate stream declared (inside same transaction).
   b. Apply transforms.
   c. Derive event ID.
   d. Assign version (MAX query scoped to events already seen in this transaction — SQLite reads within a transaction see the transaction's own uncommitted writes).
   e. INSERT.
3. After all INSERTs: commit.
4. PostCommit dispatch for all events.

### Version contiguity

Because all INSERTs happen inside one IMMEDIATE transaction, the version numbers assigned are contiguous and sequential. If `main` is currently at version 4 and you batch three events, the batch assigns versions 5, 6, 7 — no gaps, no interleaving with concurrent writers (IMMEDIATE prevents that).

This property is load-bearing for consumers that scan `read_range` from a cursor: a batch of events will always be fetched together if the cursor starts before the batch's first event.

### Multi-stream batches

`append_batch` accepts appends to **multiple different streams**. Each append's `stream_id` is validated independently. Versions are assigned per-stream within the transaction. A three-event batch touching streams A, B, and A (in order) assigns A:v1, B:v1, A:v2.

### Error handling

If any event in the batch fails (stream not declared, transform error, PRIMARY KEY conflict), the entire transaction is rolled back. No partial batch is ever committed. The returned `Err` indicates which step failed but not which event in the batch caused it — callers that need this resolution should inspect the error type and correlate with the input slice.

---

## 6. external_id — Consumer-Supplied Deduplication Key

`external_id` is an optional `String` on each `Append`. When set, it becomes part of a unique index:

```sql
CREATE INDEX idx_events_external_id
    ON events(stream_id, external_id) WHERE external_id IS NOT NULL;
```

This is a partial unique index (only non-NULL rows). Two events in the same stream with the same non-NULL `external_id` will conflict on INSERT with `SQLITE_CONSTRAINT_UNIQUE`.

### Primary use case: relay deduplication

A relay agent moves events from a source fossic store to a hub store. Each relayed event is stored in the hub with `external_id` set to the source event's hex ID:

```python
hub_store.append(Append(
    stream_id="cerebra/agent-trace/sess_abc",
    event_type=source_event.event_type,
    payload=source_event.payload_decoded,
    external_id=source_event.id.hex(),  # <-- dedup key
))
```

If the relay restarts and re-processes events it already relayed:
1. `append` is called with the same `external_id`.
2. SQLite raises `SQLITE_CONSTRAINT_UNIQUE`.
3. This propagates as `Error::Sqlite(...)`.
4. The relay catches this error and treats it as "already relayed".

Alternatively, the relay can pre-check with `read_by_external_id`:

```python
existing = hub_store.read_by_external_id(stream_id, source_event.id.hex())
if existing is None:
    hub_store.append(...)
```

The pre-check has a TOCTOU race: between the `read_by_external_id` and the `append`, another relay instance could insert the same event. The `UNIQUE` constraint on INSERT is the reliable enforcement; `read_by_external_id` is for pre-checking and avoiding noisy exceptions on the happy path.

### Scope: per-stream uniqueness

`external_id` is unique within a `(stream_id, external_id)` pair, not globally. The same `external_id` string can appear in different streams without conflict. This means a relay can use the same source event hex ID when routing to multiple destination streams.

### read_by_external_id

```rust
store.read_by_external_id(stream_id, external_id)?; // -> Option<StoredEvent>
```

SQL:
```sql
SELECT {SELECT_COLS} FROM events
WHERE stream_id = ?1 AND external_id = ?2
LIMIT 1
```

Uses `idx_events_external_id`. Returns the matching event (if any), with upcasters applied. Returns `None` if no event with that `external_id` exists in the stream.

---

## 7. indexed_tags — Projection Column for Cross-Stream Queries

`indexed_tags` solves a specific problem: the event payload is stored as msgpack (binary), which SQLite cannot query directly. `indexed_tags` is the subset of payload fields that are worth querying, materialized as a JSON TEXT column that SQLite's `json_extract` function can reach.

### What it stores

A JSON object. Example:

```json
{"session_id": "sess_abc123", "agent_id": "cerebra-v2", "score": "0.87"}
```

All values are stored as JSON (strings, numbers, booleans, null). Key names must be alphanumeric + underscore for `aggregate` filter pushdown — this is validated at query time in `aggregate_impl` but not at append time.

### Storage

```sql
-- indexed_tags is stored as a JSON TEXT string
INSERT INTO events (..., indexed_tags) VALUES (..., '{"session_id": "sess_abc"}')
```

SQLite does not have a native JSON column type. `indexed_tags` is a `TEXT` column containing a JSON string. SQLite's `json_extract` reads it at query time.

### Query-time pushdown

In `aggregate_impl` (cross_stream.rs), filter keys from `AggregateQuery::indexed_tags_filter` are pushed down as SQL conditions:

```sql
AND json_extract(indexed_tags, '$.session_id') = 'sess_abc123'
AND json_extract(indexed_tags, '$.agent_id') = 'cerebra-v2'
```

This is evaluated by SQLite's JSON1 extension on each row. Without a generated column + index, this is a full scan of the matched rows. For most fossic use cases (per-device store, moderate event counts), this is acceptable. At very high event counts, consumers can materialize computed columns externally.

### What to put in indexed_tags

Only fields you need to filter across multiple streams. Don't copy the entire payload here — that defeats the purpose of msgpack storage and bloats the TEXT column. Typical contents:
- Session, trace, or correlation identifiers
- Categorical labels (agent name, model id, event class)
- Numeric values that need range filtering

Fields that are only read (never filtered) belong exclusively in `payload`, not in `indexed_tags`.

### Validation at append time

```rust
match append.indexed_tags {
    Some(Value::Object(_)) => {} // valid
    Some(other) => return Err(Error::InvalidIndexedTags { got: other.type_name() }),
    None => {}
}
```

Arrays, strings, numbers, booleans, and null are all rejected. Only `Value::Object` passes. An empty object `{}` is valid.

---

## 8. Read Operations

All read operations acquire a connection from the **read pool** (a bounded `crossbeam_channel` of `Connection` objects). The pool has a configurable size (default 4) and a configurable timeout (default 30s). If all pool connections are checked out and none is returned within the timeout, `Error::PoolExhausted { pool_size, timeout_ms }` is returned. See SR-02 for full pool documentation.

All read operations apply upcasters to each returned event before returning. See section 12 and SR-08.

### 8.1 read_range

The primary API for sequential event consumption:

```rust
pub fn read_range(&self, query: ReadQuery) -> Result<Vec<StoredEvent>, Error>

pub struct ReadQuery {
    pub stream_id: String,
    pub branch: String,                   // e.g. "main"
    pub from_version: Option<u64>,        // inclusive lower bound; None = 0
    pub to_version: Option<u64>,          // inclusive upper bound; None = i64::MAX
    pub limit: Option<usize>,             // max events to return; None = no limit
    pub event_type_filter: Option<String>, // exact match on event_type
}
```

SQL (from `read_range_impl` in `src/read.rs`):

```sql
SELECT id, stream_id, branch, version, timestamp_us, causation_id, correlation_id,
       event_type, type_version, payload, external_id, indexed_tags
FROM events
WHERE stream_id = ?1
  AND branch = ?2
  AND version >= ?3
  AND version <= ?4
  AND (?6 IS NULL OR event_type = ?6)
ORDER BY version ASC
LIMIT ?5
```

Parameter mapping:
- `?1` = `query.stream_id`
- `?2` = `query.branch`
- `?3` = `query.from_version.unwrap_or(0) as i64`
- `?4` = `query.to_version.map(|v| v as i64).unwrap_or(i64::MAX)`
- `?5` = `query.limit.map(|l| l as i64).unwrap_or(i64::MAX)`
- `?6` = `query.event_type_filter` (NULL when None, enabling the `?6 IS NULL` short-circuit)

Results are ordered by `version ASC` — the natural stream ordering. This is the only guaranteed ordering in `read_range`; `timestamp_us` is not sorted.

**Pagination pattern:**
```rust
let mut cursor = 0u64;
loop {
    let page = store.read_range(ReadQuery {
        stream_id: "cerebra/agent-trace/sess_abc".into(),
        branch: "main".into(),
        from_version: Some(cursor),
        to_version: None,
        limit: Some(100),
        event_type_filter: None,
    })?;
    if page.is_empty() { break; }
    // process page...
    cursor = page.last().unwrap().version + 1;
}
```

### 8.2 read_one

Point lookup by EventId:

```rust
pub fn read_one(&self, id: EventId) -> Result<Option<StoredEvent>, Error>
```

SQL:
```sql
SELECT {SELECT_COLS} FROM events WHERE id = ?1
```

O(1) via the `id BLOB NOT NULL PRIMARY KEY` B-tree index. Returns `None` if the EventId is not found (e.g. if the event was purged). Upcasters applied.

### 8.3 read_batch

Multi-ID lookup in a single query:

```rust
pub fn read_batch(&self, ids: &[EventId]) -> Result<Vec<StoredEvent>, Error>
```

SQL (built dynamically):
```sql
SELECT {SELECT_COLS} FROM events
WHERE id IN (?1, ?2, ?3, ...)
ORDER BY timestamp_us ASC
```

The `IN (...)` placeholders are `?1`, `?2`, ..., `?N` — generated from `(1..=ids.len()).map(|i| format!("?{i}"))`. Results are ordered by `timestamp_us ASC`, not by input order. If you need input-order results, sort the output by matching against the input slice.

**SQLite parameter limit:** SQLite allows at most 32,766 bound parameters per statement. `read_batch` does not enforce this internally — passing more than ~32,766 IDs will cause a SQLite error. The documented operational limit is ≤ 4,096 IDs per call.

**Missing IDs are silently omitted.** If you pass 10 IDs and 2 have been purged, you get 8 events back. Detect missing IDs by comparing `ids.len()` against the returned `Vec` length.

`PREFIXED_SELECT_COLS` (`events.id, events.stream_id, ...`) is used in JOIN queries within other modules (like `cross_stream.rs`) to avoid "ambiguous column name" errors. `read_batch` itself uses the plain `SELECT_COLS` constant since there is no join.

### 8.4 read_by_external_id

Consumer-ID lookup:

```rust
pub fn read_by_external_id(
    &self,
    stream_id: &str,
    external_id: &str,
) -> Result<Option<StoredEvent>, Error>
```

SQL:
```sql
SELECT {SELECT_COLS} FROM events
WHERE stream_id = ?1 AND external_id = ?2
LIMIT 1
```

Uses `idx_events_external_id` (partial index on non-NULL external_id values, keyed on `(stream_id, external_id)`). Returns `None` if not found. Upcasters applied.

---

## 9. row_to_event — Row Deserialization

All read operations share `row_to_event`, which maps a SQLite `Row` to a `StoredEvent`. The column order matches `SELECT_COLS`:

```
Position 0:  id              → EventId       (BLOB, 32 bytes)
Position 1:  stream_id       → String        (TEXT)
Position 2:  branch          → String        (TEXT)
Position 3:  version         → u64           (i64 cast to u64)
Position 4:  timestamp_us    → i64           (INTEGER)
Position 5:  causation_id    → Option<EventId> (BLOB or NULL)
Position 6:  correlation_id  → Option<EventId> (BLOB or NULL)
Position 7:  event_type      → String        (TEXT)
Position 8:  type_version    → u32           (i64 cast to u32)
Position 9:  payload         → Vec<u8>       (BLOB, msgpack bytes)
Position 10: external_id     → Option<String> (TEXT or NULL)
Position 11: indexed_tags    → Option<serde_json::Value> (TEXT→JSON or NULL)
```

### indexed_tags deserialization

```rust
let indexed_tags_json: Option<String> = row.get(11)?;
let indexed_tags = indexed_tags_json
    .as_deref()
    .map(serde_json::from_str)
    .transpose()
    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
        11,
        rusqlite::types::Type::Text,
        Box::new(e),
    ))?;
```

The JSON TEXT is parsed at read time. A malformed `indexed_tags` JSON string (e.g. from a bug in an earlier version of the code) would cause `serde_json::from_str` to fail, which is then wrapped as `rusqlite::Error::FromSqlConversionFailure`. This propagates as `Error::Sqlite(...)`. In practice, `indexed_tags` JSON is always written by fossic's own validated code path, so this error is not expected in normal operation.

### Type casts

`version` is stored as `INTEGER` (SQLite's 64-bit signed integer) and is retrieved as `i64`, then cast to `u64`. Since fossic versions start at 0 and increment by 1, they will never reach `i64::MAX` in practice, making the cast safe.

`type_version` similarly: stored as `i64`, retrieved as `i64`, cast to `u32`. Values up to `u32::MAX` (~4 billion) are safe.

### EventId SQLite binding

`EventId` implements `rusqlite::types::ToSql` and `rusqlite::types::FromSql`. It stores as a 32-byte BLOB and reads back as exactly 32 bytes. If the BLOB in the database is not exactly 32 bytes, `FromSql` returns an error — this would indicate database corruption.

---

## 10. StoredEvent — The Read-Side Event Type

```rust
pub struct StoredEvent {
    pub id: EventId,
    pub stream_id: String,
    pub branch: String,
    pub version: u64,
    pub timestamp_us: i64,
    pub causation_id: Option<EventId>,
    pub correlation_id: Option<EventId>,
    pub event_type: String,
    pub type_version: u32,      // stored version — NOT updated by upcasters
    pub payload: Vec<u8>,       // msgpack bytes — may be transformed by upcasters
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,
}
```

### Decoding the payload

The `payload` field is always msgpack bytes. To decode to a `serde_json::Value`:

```rust
let value: serde_json::Value = rmp_serde::from_slice(&event.payload)?;
```

To decode to a specific type:
```rust
#[derive(serde::Deserialize)]
struct MyPayload { field: String }

let decoded: MyPayload = rmp_serde::from_slice(&event.payload)?;
```

In Python via fossic-py, `event.payload` returns the decoded Python dict directly (the binding performs the msgpack → JSON → Python conversion internally).

### type_version is the stored version

`type_version` on `StoredEvent` reflects the version at which the event was **written**, not the version that would result after upcasting. If you wrote an event with `type_version: 1` and later registered an upcaster from 1 → 2, reading that event back gives you `StoredEvent { type_version: 1, payload: <upcast bytes> }`. The `payload` is the upcasted bytes, but `type_version` still says `1`. This means `type_version` alone cannot tell you which payload schema to use after upcasting — the upcaster chain determines the effective schema version.

### id is permanent and immutable

The `EventId` in `StoredEvent.id` is computed from the original (post-transform) payload at append time and never changes. Purging an event removes the row from the database; the id does not change on any surviving event. Upcasting changes payload bytes in memory at read time but does not modify the stored row. The id is the permanent identity for the lifetime of the store.

---

## 11. Cursor API

Cursors are consumer-managed progress pointers. They record "I have successfully processed events up to version N in stream S, branch B" and are stored in the `cursors` table.

### Schema

```sql
CREATE TABLE cursors (
    consumer_id     TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    branch          TEXT    NOT NULL DEFAULT 'main',
    version         INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (consumer_id, stream_id, branch)
);
```

### get_cursor

```rust
pub fn get_cursor(
    &self,
    consumer_id: &str,
    stream_id: &str,
    branch: &str,
) -> Result<Option<u64>, Error>
```

SQL:
```sql
SELECT version FROM cursors
WHERE consumer_id = ?1 AND stream_id = ?2 AND branch = ?3
```

Returns `None` if no cursor has been set for this `(consumer_id, stream_id, branch)` triple.

### set_cursor

```rust
pub fn set_cursor(
    &self,
    consumer_id: &str,
    stream_id: &str,
    branch: &str,
    version: u64,
) -> Result<(), Error>
```

SQL (UPSERT):
```sql
INSERT INTO cursors (consumer_id, stream_id, branch, version, updated_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(consumer_id, stream_id, branch)
DO UPDATE SET version = excluded.version, updated_at = excluded.updated_at
```

The `updated_at` field is set to `now_us()` on every write.

### When to use cursors

**Use cursors when:** the consumer has no transactional storage of its own. A script that processes events and sends them to stdout, or writes to a log file, has no other place to record progress. The fossic cursor table is the right choice.

**Do not use cursors when:** the consumer writes to its own transactional database (Postgres, MySQL, another SQLite file, etc.). In that case, the cursor must be stored atomically with the consumer's own side effects to avoid the dual-write problem:

```
# WRONG: two separate writes — if either fails, state is inconsistent
hub_store.append(relayed_event)      # write 1
src_store.set_cursor(...)            # write 2
```

The correct pattern for a transactional consumer is to store the cursor in its own database in the same transaction as its side effects, then use `read_range(from_version=cursor+1)` on startup to resume.

### Cursors vs subscription cursors

The `cursors` table is **only for external consumer use**. The subscription system maintains its own in-memory cursor positions in `SubscriptionRegistry`, advanced by `dispatch_post_commit`. These internal cursors are never stored in the `cursors` table and are reset on process restart (the WAL watcher handles cross-process re-sync on restart). See SR-04.

### Consumer ID conventions

`consumer_id` is any string. Convention: use a descriptive, versioned identifier:
- `"cerebra-relay-v1"` — the cerebra relay at version 1
- `"policy-scout-processor"` — PS event processor
- `"fossic-aggregate-worker"` — aggregate processing worker

If the consumer changes its processing logic in an incompatible way, bump the consumer ID so the new version starts from the beginning (or from a checkpoint the operator sets manually).

---

## 12. Upcaster Application at Read Time

Every read operation that returns `StoredEvent` values applies the registered upcaster chain before returning. This happens in `apply_upcaster(registry, event)` in `src/upcasters.rs`.

### What happens

```rust
pub(crate) fn apply_upcaster(
    registry: &UpcasterRegistry,
    mut event: StoredEvent,
) -> Result<StoredEvent, Error> {
    event.payload = registry.apply(&event.event_type, event.type_version, event.payload)?;
    Ok(event)
}
```

Only `payload` is mutated. `id`, `type_version`, and all other fields are unchanged.

### The chain walk

`UpcasterRegistry::apply` looks up all registered upcasters for `event.event_type`, sorted by `from_version`. Starting at `event.type_version`:

1. Find an entry where `entry.from == current_version`.
2. Call `entry.upcaster.upcast(&payload)` → new payload bytes.
3. Advance `current_version = entry.to`.
4. Repeat until no entry for `current_version`.

If no upcasters are registered for the `event_type`, or the stored version is already at or beyond the end of the chain, returns the original payload unchanged.

### UpcasterChainGap

If there are entries with `from > current_version` but no entry for `current_version` (i.e. a gap in the chain), returns:

```rust
Err(Error::UpcasterChainGap { event_type: "...", from: <current_version> })
```

This is a consumer bug — gaps in the upcaster chain indicate a missed migration. The consumer registered `1→2` and `3→4` but forgot `2→3`. The event stored at `type_version: 1` would be upcasted to version 2, then hit a gap going to 3.

### When no upcasters exist

If `UpcasterRegistry` has no entries for `event.event_type`, `apply` returns `Ok(payload)` immediately without any allocation. Reading old events without any upcasters registered is zero-cost.

### Stored vs effective payload schema

After upcasting, the in-memory `StoredEvent.payload` bytes reflect the "current" schema, but `StoredEvent.type_version` still reflects the stored (original) version. Consumers that need to know the effective schema version after upcasting must walk the upcaster chain themselves (or simply deserialize the payload and trust that the upcast brought it to the current shape).

For full upcaster registration, chain requirements, and the Python callable bridge, see SR-08.

---

## Appendix A: Append Decision Tree

```
Append submitted
    │
    ├─ stream declared? ──No──► Error::StreamNotDeclared
    │
    ├─ apply transforms (chain in registration order)
    │   └─ transform error? ──Yes──► Error propagated, tx rolled back
    │
    ├─ derive EventId (blake3 over CCE of post-transform payload)
    │
    ├─ assign version (MAX query inside IMMEDIATE tx)
    │
    ├─ validate indexed_tags (object or None)
    │   └─ not object? ──Yes──► Error::InvalidIndexedTags
    │
    ├─ INSERT INTO events
    │   └─ PRIMARY KEY conflict? ──Yes──► Error::Sqlite (duplicate EventId)
    │   └─ UNIQUE(stream_id, external_id) conflict? ──Yes──► Error::Sqlite
    │
    ├─ fire Synchronous subscribers (inside tx, before commit)
    │   └─ panic? ──Yes──► caught, subscription degraded, tx continues
    │
    ├─ COMMIT
    │
    ├─ fire PostCommit subscribers (after commit, async)
    │
    └─ return EventId
```

## Appendix B: Read Connection Lifecycle

```
read_range / read_one / read_batch / read_by_external_id
    │
    ├─ acquire ReadGuard from pool (blocks up to read_pool_timeout_ms)
    │   └─ all connections busy? ──Yes──► Error::PoolExhausted after timeout
    │
    ├─ execute query on pooled Connection
    │
    ├─ apply upcasters to each event
    │
    ├─ ReadGuard drops → Connection returned to pool channel
    │
    └─ return Vec<StoredEvent>
```

---

## Appendix C: Field Index Summary

| Field | Type | Index | Notes |
|---|---|---|---|
| `id` | BLOB 32B | PRIMARY KEY | EventId, blake3 over CCE |
| `stream_id` | TEXT | Covering (with branch, version) | Must be declared |
| `branch` | TEXT | Covering (with stream_id, version) | "main" default |
| `version` | INTEGER | Covering (with stream_id, branch) | Monotonic per stream+branch |
| `timestamp_us` | INTEGER | `idx_events_timestamp` | Microseconds, not monotonic |
| `causation_id` | BLOB 32B | `idx_events_causation` (partial, NOT NULL) | Links to parent event |
| `correlation_id` | BLOB 32B | `idx_events_correlation` (partial, NOT NULL) | Grouping key |
| `event_type` | TEXT | `idx_events_type` | Pattern-matched by subscriptions |
| `type_version` | INTEGER | None | Schema version for upcasters |
| `payload` | BLOB | None | msgpack encoded |
| `external_id` | TEXT | `idx_events_external_id` (partial, NOT NULL) | Per-stream unique |
| `indexed_tags` | TEXT | None | JSON object, json_extract at query time |
