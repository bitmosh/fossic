# SR-02 — Storage Schema and Concurrency Model

**Series:** Fossic State Reports · Document 2 of 9
**Covers:** `src/schema.rs`, `src/store.rs` (StoreInner, ReadGuard, open path), `src/types.rs` (OpenOptions)
**Companion docs:** SR-01 (identity/CCE), SR-03 (event lifecycle), SR-04 (subscriptions)

---

## Overview

Fossic stores every event in a single SQLite file using WAL mode. The entire concurrency model is deliberately simple: one write connection behind a `Mutex`, plus a bounded pool of read connections behind a crossbeam channel. There is no fossic daemon, no network layer, no distributed state — the SQLite file is the store. Understanding the schema and the connection model is prerequisite knowledge for everything else in this series.

---

## 1. The SQLite File

Fossic opens (or creates) a single `.db` file. The default location on Linux is:

```
${XDG_DATA_HOME}/fossic/store.db
```

If `XDG_DATA_HOME` is not set, the fallback is `~/.local/share/fossic/store.db`. On macOS the equivalent is `~/Library/Application Support/fossic/store.db`; on Windows, `%APPDATA%\fossic\store.db`. In practice, most Lattica consumers pass an explicit path to `Store::open` (or `Store.open(path)` in Python) — e.g., `~/.local/share/lattica/hub.db` — so the default path is rarely used in production.

The SQLite file is self-contained. There are no external dependencies, no sidecar files (beyond the WAL and SHM files that SQLite manages automatically while the database is open), and no registry entries. Backing up the store is a file copy (with WAL checkpointing first if you want a clean snapshot).

### Schema Version Tracking

The current schema version is **1**. It is stored in two places:

1. `PRAGMA user_version` — a 4-byte integer in the SQLite file header. The migration system reads this first. Value `0` means a fresh (uninitialized) database; value `1` means the v1 schema is present.
2. The `meta` table, key `fossic_schema_version` — a human-readable string copy ("1"). This is for inspection tools that don't want to parse the SQLite header.

The two are always in sync after `run_migrations` completes.

---

## 2. PRAGMA Configuration

Every connection (both the write connection and each read-pool connection) has the same PRAGMA set applied on open. From `src/schema.rs`:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 30000;
PRAGMA foreign_keys = ON;
```

### `journal_mode = WAL`

WAL (Write-Ahead Log) mode is **mandatory**. In WAL mode:

- Writers append to a separate WAL file rather than writing directly to the main database file.
- Readers read the main file directly and check the WAL for any newer pages. The database file itself is never modified during an active write.
- **Readers and writers do not block each other.** Multiple readers can run concurrently with a writer in progress. This is the property that makes the read pool worthwhile.

The WAL file is a sibling of the main db file (e.g., `store.db-wal`). The SHM file (`store.db-shm`) is a shared-memory index into the WAL used by SQLite for coordination between processes. Both are managed automatically and should never be manually deleted while the store is open.

Checkpointing (copying WAL contents back into the main file) happens automatically (Auto mode) as a background operation. The WAL file size is bounded by SQLite's checkpoint heuristic (default: checkpoint when the WAL reaches 1000 pages). In high-write workloads the WAL may grow larger between checkpoints.

### `synchronous = NORMAL`

In WAL mode, `synchronous=NORMAL` means SQLite fsyncs the WAL file only during a checkpoint, not after every commit. This is **safe in WAL mode**: a crash between commits can corrupt the WAL tail, but the main database file is always consistent. On next open, SQLite automatically rolls back any incomplete WAL entries during recovery.

`synchronous=FULL` would fsync after every commit, which is slower. `synchronous=OFF` skips all fsyncs, which is faster but risks corruption on a power failure. `NORMAL` is the right default for a local-first store where data loss on a clean shutdown is unacceptable but per-commit fsync overhead is unnecessary.

### `busy_timeout = 30000`

When a lock conflict occurs (e.g., two processes each trying to checkpoint), SQLite will retry internally for up to 30,000 milliseconds (30 seconds) before returning `SQLITE_BUSY`. Without this pragma, SQLite returns `SQLITE_BUSY` immediately on any lock conflict, which would cause unnecessary failures in normal operation.

Cross-process locking is uncommon in the Lattica deployment model (each process has its own store), but the WAL watcher (`src/wal_watch.rs`) opens its own read connection from outside the pool, so brief lock contention is possible.

### `foreign_keys = ON`

Referential integrity enforcement. Without this pragma, SQLite ignores `FOREIGN KEY` constraints even when they are declared. In the current v1 schema, there are no formal FK declarations (they were deemed unnecessary given fossic's internal invariants), but enabling this ensures any future FK additions are enforced correctly and prevents silent corruption if a schema migration adds FKs.

---

## 3. Full Schema V1

The entire schema is defined in `SCHEMA_V1: &str` in `src/schema.rs` and applied in a single `execute_batch` call. It creates 8 tables and 10 indices. Each is documented below.

### 3.1 `events` — the append log

```sql
CREATE TABLE events (
    id              BLOB    NOT NULL PRIMARY KEY,
    stream_id       TEXT    NOT NULL,
    branch          TEXT    NOT NULL DEFAULT 'main',
    version         INTEGER NOT NULL,
    timestamp_us    INTEGER NOT NULL,
    causation_id    BLOB,
    correlation_id  BLOB,
    event_type      TEXT    NOT NULL,
    type_version    INTEGER NOT NULL DEFAULT 1,
    payload         BLOB    NOT NULL,
    external_id     TEXT,
    indexed_tags    TEXT,
    UNIQUE (stream_id, branch, version)
);
```

**`id BLOB NOT NULL PRIMARY KEY`**

32 bytes of blake3 output. Stored as raw bytes, not hex. BLOB is more compact (32 bytes vs 64 bytes for the hex string) and avoids any case-sensitivity issues in comparisons. The `PRIMARY KEY` creates an implicit B-tree index on this column; point lookups by id (`read_one`) use this index directly.

Why blake3 / content-addressed? See SR-01. The short answer: two identical logical events produce the same id, so duplicate appends are naturally idempotent (the `INSERT OR IGNORE` on the duplicate id is a no-op).

**`stream_id TEXT NOT NULL`**

The logical stream identifier. Format: slash-separated path segments, e.g., `cerebra/agent-trace/sess-abc123`. No maximum length is enforced at the SQL level, but the glob system operates on path segments so extremely deep paths have a performance cost at query time. System streams use the `_fossic/` prefix (e.g., `_fossic/system`).

**`branch TEXT NOT NULL DEFAULT 'main'`**

Branch within the stream. Default is `'main'`. Custom branches are created via `Store::create_branch` / `Store.create_branch()`. Branch names must be registered in the `branches` table before events can be appended to them — the append path validates this.

**`version INTEGER NOT NULL`**

Monotonically increasing per `(stream_id, branch)`. Version 0 is the first event in a stream. Version assignment happens inside the IMMEDIATE transaction: `SELECT COALESCE(MAX(version), -1) + 1 FROM events WHERE stream_id = ? AND branch = ?`. The `UNIQUE (stream_id, branch, version)` constraint enforces uniqueness and provides an efficient lookup path. There is no global event counter — versions are stream-local.

**`timestamp_us INTEGER NOT NULL`**

Microseconds since Unix epoch, as `i64`. Populated by `now_us()` at append time (wall clock). Note: this is append time, not the time the event logically occurred. Two events appended in rapid succession can have the same `timestamp_us` value. The ordering guarantee for a given `(stream_id, branch)` is provided by `version`, not `timestamp_us`.

**`causation_id BLOB`**

Optional. 32-byte EventId of the event that directly caused this one. Used to build causation chains (`walk_causation`). Stored as raw bytes, same as `id`. May be NULL — most events are not in a causation relationship. The partial index on this column covers only non-NULL rows for efficiency.

**`correlation_id BLOB`**

Optional. 32-byte EventId identifying a logical group of related events. Used to look up all events in a transaction or request (`read_by_correlation`). Unlike `causation_id`, there is no inherent parent-child semantics — all events in a group share the same `correlation_id`. May be NULL. Partial index covers non-NULL rows.

**`event_type TEXT NOT NULL`**

PascalCase string identifying the event schema, e.g., `StepStarted`, `DecisionIssued`. Part of the CCE derivation input (see SR-01). Also used as the filter key in `ReadQuery::event_type_filter` and `AggregateQuery::event_type_filter`. The index on this column supports type-filtered aggregate queries.

**`type_version INTEGER NOT NULL DEFAULT 1`**

Schema version of the event type. Used to route upcasters at read time. A `StepStarted` event stored at `type_version=1` will be passed through the `StepStarted` upcaster chain starting from version 1 when an upcaster for `(StepStarted, from=1, to=2)` is registered. See SR-03 (upcasters) and SR-08 (schema evolution) for full detail.

**`payload BLOB NOT NULL`**

msgpack-encoded event payload. **Not JSON** — msgpack is more compact and faster to parse. The payload is msgpack-encoded at append time (after any payload transforms have run and after CCE encoding for the id derivation). At read time, upcasters receive and return msgpack bytes; the Rust API and Python bindings deserialize to the language's native dict/Value representation on request. The raw msgpack bytes are what's physically stored.

**`external_id TEXT`**

Consumer-supplied identifier. Typically a ULID, UUID, or any opaque string the upstream system uses. Used for idempotent relays: before appending a relayed event, call `read_by_external_id(stream_id, external_id)` — if it returns Some, skip the append. If it returns None, append with this external_id. The index on `(stream_id, external_id)` makes this lookup fast. A NULL external_id means the event has no consumer-supplied identity.

**`indexed_tags TEXT`**

A JSON object (not an array, not a scalar) stored as TEXT. Values must be JSON-serializable scalars or objects. Example: `{"session_id": "sess-abc", "model": "claude-3-5-sonnet"}`. Validated on insert: `indexed_tags` must parse as a JSON object if non-NULL. Used for `aggregate` query pushdown: `json_extract(indexed_tags, '$.key') = ?` is pushed to SQL before Rust post-filtering. Keys used in `AggregateQuery::indexed_tags_filter` must be alphanumeric plus underscore to avoid SQL injection in the dynamic query (the key is formatted into the SQL string, not bound as a parameter — see SR-07 for the security rationale).

**`UNIQUE (stream_id, branch, version)`**

Secondary unique constraint. The primary key on `id` prevents duplicate events by content; this constraint prevents duplicate version assignments within a stream+branch. In practice, the write lock and MAX+1 version assignment inside the IMMEDIATE transaction makes a collision essentially impossible — but the constraint provides defense-in-depth.

#### Indices on `events`

```sql
CREATE INDEX idx_events_stream_branch_version
    ON events(stream_id, branch, version);
CREATE INDEX idx_events_correlation
    ON events(correlation_id) WHERE correlation_id IS NOT NULL;
CREATE INDEX idx_events_causation
    ON events(causation_id) WHERE causation_id IS NOT NULL;
CREATE INDEX idx_events_external_id
    ON events(stream_id, external_id) WHERE external_id IS NOT NULL;
CREATE INDEX idx_events_timestamp
    ON events(timestamp_us);
CREATE INDEX idx_events_type
    ON events(event_type);
```

**`idx_events_stream_branch_version`** — The primary read index. Covers the `read_range` query exactly: `WHERE stream_id = ? AND branch = ? AND version >= ? AND version <= ? ORDER BY version ASC LIMIT ?`. All three columns are in the index, so this query is a pure index scan with no heap reads for the WHERE and ORDER BY clauses (though payload/indexed_tags are fetched from the heap for the SELECT columns).

**`idx_events_correlation`** — Partial index on `correlation_id WHERE correlation_id IS NOT NULL`. "Partial" means the index only contains rows where the condition is true. Since most events have no correlation_id, the partial index is much smaller than a full-table index, and lookups by correlation_id are fast.

**`idx_events_causation`** — Partial index on `causation_id WHERE causation_id IS NOT NULL`. Used by the `walk_causation` recursive CTE for the BFS anchor step (`WHERE causation_id = <start_id>`). Same rationale as the correlation index.

**`idx_events_external_id`** — Compound index on `(stream_id, external_id) WHERE external_id IS NOT NULL`. The compound is important: `read_by_external_id` filters on both `stream_id` and `external_id`, so a single-column index on `external_id` alone would scan more rows than necessary.

**`idx_events_timestamp`** — Index on `timestamp_us` for time-range filtering in aggregate queries (`AggregateQuery::from_timestamp_us`, `to_timestamp_us`). When a time-range filter is applied alongside a stream_pattern filter, SQLite's query planner picks the index that eliminates the most rows — which may be this one or the stream_branch_version index depending on the query.

**`idx_events_type`** — Index on `event_type` for type-filter pushdown in aggregate queries (`AggregateQuery::event_type_filter`). Without this, a type filter would require a full scan of the events table.

---

### 3.2 `branches` — branch lifecycle

```sql
CREATE TABLE branches (
    id              TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    parent_id       TEXT    NOT NULL,
    parent_version  INTEGER NOT NULL,
    description     TEXT,
    created_at      INTEGER NOT NULL,
    lifecycle       TEXT    NOT NULL DEFAULT 'ephemeral',
    closed_at       INTEGER,
    closed_reason   TEXT,
    alternatives    TEXT,
    PRIMARY KEY (stream_id, id)
);

CREATE INDEX idx_branches_stream   ON branches(stream_id);
CREATE INDEX idx_branches_lifecycle ON branches(stream_id, lifecycle);
```

**`id TEXT NOT NULL`** — Branch identifier. Consumer-supplied string. Typically a short slug: `exp-20260614`, `hotfix-auth`. No format is enforced beyond being a valid TEXT value. Validated for non-empty and no-slash rules by the application layer.

**`stream_id TEXT NOT NULL`** — The stream this branch belongs to. Branches are per-stream — two different streams can each have a branch named `experiment-1` without conflict. The composite PK `(stream_id, id)` enforces this.

**`parent_id TEXT NOT NULL`** — The branch this branch was cut from. For branches cut directly from main, this is the literal string `'main'`. For branches cut from another branch, this is that branch's `id`.

**`parent_version INTEGER NOT NULL`** — The version of the parent branch at branch-cut time. Events on the new branch start from version `parent_version + 1`. For root branches (cut from `'main'` at the beginning), `parent_version` is 0. The branch chain resolves by walking `parent_id` pointers until reaching `'main'`.

**`lifecycle TEXT NOT NULL DEFAULT 'ephemeral'`** — One of three string values: `'ephemeral'`, `'promoted'`, `'dead_end'`. All branches start as `'ephemeral'`. A branch is `'promoted'` when its events are accepted into the parent stream; `'dead_end'` when the experimental line is abandoned. Transitions are idempotent. There is no further state after `promoted` or `dead_end` — these are terminal. See SR-05 for the full branch lifecycle.

**`alternatives TEXT`** — JSON array of branch IDs representing other branches that were tried at the same decision point. Example: `["exp-a", "exp-b"]`. Used to link competing experiments for review. NULL if there are no alternatives.

**`closed_at INTEGER, closed_reason TEXT`** — Populated when lifecycle transitions to `'promoted'` or `'dead_end'`. `closed_at` is microseconds since epoch; `closed_reason` is a consumer-supplied string explaining why the branch was closed.

**Indices:** `idx_branches_stream` supports listing all branches for a stream (`list_branches`). `idx_branches_lifecycle` supports filtering by lifecycle status (e.g., listing all live `'ephemeral'` branches).

---

### 3.3 `snapshots` — reducer state cache

```sql
CREATE TABLE snapshots (
    stream_id            TEXT    NOT NULL,
    branch               TEXT    NOT NULL DEFAULT 'main',
    version              INTEGER NOT NULL,
    reducer_name         TEXT    NOT NULL,
    reducer_version      INTEGER NOT NULL DEFAULT 1,
    state_schema_version INTEGER NOT NULL DEFAULT 1,
    state_blob           BLOB    NOT NULL,
    created_at           INTEGER NOT NULL,
    PRIMARY KEY (stream_id, branch, reducer_name, state_schema_version, version)
);

CREATE INDEX idx_snapshots_lookup
    ON snapshots(stream_id, branch, reducer_name, state_schema_version, version DESC);
```

**`reducer_name TEXT NOT NULL`** — The `Reducer::NAME` constant. Identifies which reducer this snapshot belongs to. Distinct reducers can each have snapshots for the same stream.

**`reducer_version INTEGER NOT NULL DEFAULT 1`** — The `Reducer::VERSION` constant at snapshot-write time. Stored but not used as a lookup key — it is informational, recording which code version produced this snapshot.

**`state_schema_version INTEGER NOT NULL DEFAULT 1`** — The `Reducer::STATE_SCHEMA_VERSION` constant. **This is part of the primary key.** When a reducer changes its state schema (incompatible state shape) it bumps `STATE_SCHEMA_VERSION`. Old snapshots are under the old schema version and are invisible to the new code — the new code starts folding events from scratch until it produces a new snapshot under the new schema version. This is the migration path for reducer state.

**`state_blob BLOB NOT NULL`** — msgpack-encoded reducer state. The Rust `ReducerState` serialization goes through `rmp_serde`. In Python, state is serialized via JSON-as-intermediary (Python dict → JSON → msgpack). The raw bytes stored here are opaque to SQLite.

**Primary key:** `(stream_id, branch, reducer_name, state_schema_version, version)`. This composite key allows multiple snapshots per stream (at different versions) and ensures that the `INSERT OR REPLACE` used in `write_snapshot` overwrites only the exact same snapshot slot.

**Index:** `idx_snapshots_lookup` has `version DESC` as the last column. `find_latest_snapshot` runs `ORDER BY version DESC LIMIT 1`, and this index makes that a single-row lookup rather than a scan.

---

### 3.4 `streams` — stream registry

```sql
CREATE TABLE streams (
    id              TEXT    NOT NULL PRIMARY KEY,
    declared_by     TEXT    NOT NULL,
    declared_at     INTEGER NOT NULL,
    description     TEXT
);
```

**Purpose:** A stream must be declared before any event can be appended to it. The append path validates that the `stream_id` exists in this table inside the IMMEDIATE transaction. If it doesn't, `Error::StreamNotDeclared` is returned.

**`declared_by TEXT NOT NULL`** — Consumer-supplied identifier for the declaring agent. E.g., `"cerebra-relay-v1"`, `"lattica-hub"`. Used for auditing/debugging — which component introduced this stream?

**`declared_at INTEGER NOT NULL`** — Microseconds since epoch, populated by `now_us()`.

**Bootstrap:** The `_fossic/system` stream is always present. It is inserted via `bootstrap_system_streams` during `Store::open` using `INSERT OR IGNORE`, so re-opening an existing store does not fail. User-facing streams are registered by calling `Store::declare_stream(stream_id, declared_by, description)`.

**`declare_stream` is idempotent:** if the stream already exists, the operation is a no-op (the implementation uses `INSERT OR IGNORE`).

---

### 3.5 `stream_deks` — Data Encryption Keys

```sql
CREATE TABLE stream_deks (
    stream_id       TEXT    NOT NULL PRIMARY KEY,
    key_id          TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    shredded_at     INTEGER,
    shredded_reason TEXT
);
```

**Status in v1: schema present, implementation deferred.** The table is created as part of the v1 schema, but `shred_stream` always returns `Error::NotImplemented` in the current build.

**Design intent:** Each stream that opts into encryption has a Data Encryption Key (DEK) stored in this table. The DEK is identified by `key_id` (a reference to a key in the OS keyring or environment variable store — the actual key material is never in SQLite). When `shred_stream` is called:

1. The DEK is destroyed from the external key store.
2. `stream_deks.shredded_at` and `shredded_reason` are set.
3. All subsequent reads of the stream's payload blobs fail to decrypt — the data is cryptographically inaccessible.

The payload bytes remain in the `events` table (they are not deleted), but without the DEK, they are unrecoverable. This provides GDPR "right to erasure" compliance without requiring a DELETE that would break append-only invariants.

**Why not just DELETE?** Deletion breaks audit trails. Crypto-shredding preserves the event metadata (stream_id, version, event_type, timestamp, causation/correlation) while making the payload unrecoverable. This is sufficient for most compliance requirements that target PII in payloads rather than event metadata.

---

### 3.6 `cursors` — consumer progress pointers

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

**Purpose:** Lightweight durable progress pointers for consumers that process events from a stream and need to remember where they left off across restarts.

**Usage pattern:**

```python
# On startup: find the last processed version
cursor = store.get_cursor("my-consumer", "cerebra/agent-trace/sess-abc", "main")
start_from = (cursor + 1) if cursor is not None else 0

# Read from where we left off
events = store.read_range(ReadQuery(
    stream_id="cerebra/agent-trace/sess-abc",
    branch="main",
    from_version=start_from,
))

for event in events:
    process(event)
    store.set_cursor("my-consumer", event.stream_id, event.branch, event.version)
```

**Implementation:** `set_cursor` uses `INSERT INTO cursors ... ON CONFLICT(...) DO UPDATE SET version = excluded.version, updated_at = excluded.updated_at`. This is a standard UPSERT.

**Important caveat:** The cursor table is a **convenience facility**, not a transactional guarantee. If your consumer writes processed state to another database (Postgres, another SQLite file), you should store the cursor there atomically alongside the side effect — not here. Using `set_cursor` after the side effect leaves a window where a crash between the side effect and `set_cursor` causes the event to be reprocessed. The only safe pattern is: side effect and cursor update in the same transaction.

**Not used internally:** The subscription dispatcher (`src/subscriptions.rs`) tracks its own in-memory cursors. The `cursors` table is not written by the subscription system — only by code that explicitly calls `get_cursor`/`set_cursor`.

---

### 3.7 `upcasters_registered` — upcaster audit log

```sql
CREATE TABLE upcasters_registered (
    event_type      TEXT    NOT NULL,
    from_version    INTEGER NOT NULL,
    to_version      INTEGER NOT NULL,
    registered_at   INTEGER NOT NULL,
    PRIMARY KEY (event_type, from_version, to_version)
);
```

**Purpose:** Audit log of upcaster registrations. Not read at query time — the in-memory `UpcasterRegistry` (populated by `Store::register_upcaster`) drives actual upcasting.

**Design intent:** When you open an existing store and register upcasters, this table records which upcasters were ever registered, providing a history of schema evolution for debugging and compliance purposes. If you need to know "has `StepStarted` ever been upcasted from v1 to v2 on this store?", query this table.

---

### 3.8 `meta` — store metadata

```sql
CREATE TABLE meta (
    key     TEXT    NOT NULL PRIMARY KEY,
    value   TEXT    NOT NULL
);
```

Bootstrap entries (set via `INSERT OR IGNORE` so re-opening an existing store is safe):

| Key | Value | Notes |
|-----|-------|-------|
| `fossic_schema_version` | `"1"` | Human-readable copy of `PRAGMA user_version` |
| `cce_version` | `"fossic-cce-v1"` | CCE protocol version used by this store |
| `created_at_us` | `"<microseconds>"` | When the store was first created |
| `created_by_version` | `"0.10.0r"` | Fossic crate version at creation time |
| `encryption_mode` | `"plaintext"` / `"os_keyring"` / `"env_var"` | Encryption mode at creation time |

The `meta` table is append-only in normal operation (after bootstrap). No runtime code updates these entries. They are purely informational.

---

## 4. Migration System

`run_migrations(conn: &Connection)` is called during `Store::open` on the write connection, before any read connections are created. The algorithm from `src/schema.rs`:

```rust
let stored: u32 = conn
    .query_row("PRAGMA user_version", [], |r| r.get(0))
    .unwrap_or(0);

if stored > CURRENT_SCHEMA_VERSION {
    return Err(Error::SchemaMismatch { stored, required: CURRENT_SCHEMA_VERSION });
}

if stored == CURRENT_SCHEMA_VERSION {
    return Ok(());
}

// stored == 0: fresh database
conn.execute_batch(SCHEMA_V1)?;
conn.execute_batch(&format!("PRAGMA user_version = {};", CURRENT_SCHEMA_VERSION))?;
```

**Three cases:**

1. **`stored > CURRENT_SCHEMA_VERSION`** — The database was written by a newer version of fossic than the binary reading it. Return `Error::SchemaMismatch`. This prevents a newer binary's schema changes from being silently ignored by an older binary, which could cause data corruption.

2. **`stored == CURRENT_SCHEMA_VERSION`** — The database is at the expected version. No-op; continue with open.

3. **`stored == 0`** — Fresh (uninitialized) database. Run the complete `SCHEMA_V1` DDL via `execute_batch`, then stamp `PRAGMA user_version = 1`.

**Future migrations:** There is currently no `stored == 1 && CURRENT == 2` branch because v1 is the only schema version. When v2 is introduced, the pattern will be:

```rust
if stored == 0 {
    conn.execute_batch(SCHEMA_V1)?;
    // fall through to v1→v2
}
if stored <= 1 {
    conn.execute_batch(SCHEMA_V1_TO_V2)?;
}
conn.execute_batch(&format!("PRAGMA user_version = 2;"))?;
```

Migrations are always run in sequence (v0→v1→v2), never by applying a single "v0→v2" migration that skips intermediate states. This guarantees that old databases go through every intermediate state and that the migration code is always exercised.

**No downgrade path:** There is no mechanism to migrate down from v2 to v1. Once a store is migrated to a newer schema, it requires the newer binary.

---

## 5. Read Pool

The read pool is defined in `StoreInner` as a `crossbeam_channel::Sender<Connection>` / `Receiver<Connection>` pair, created with `crossbeam_channel::bounded(read_pool_size)`.

### Initialization

During `Store::open`, after migrations complete:

```rust
for _ in 0..options.read_pool_size {  // default: 4
    let conn = Connection::open(&path)?;
    // Apply the same PRAGMAs as the write connection
    apply_pragmas(&conn)?;
    pool_sender.send(conn).unwrap();
}
```

All pool connections are pre-created at open time, not lazily. This means the first read is not penalized by connection setup overhead.

### Acquiring a Read Connection

```rust
fn acquire_read(&self) -> Result<ReadGuard, Error> {
    self.inner.pool_receiver.recv_timeout(
        Duration::from_millis(self.inner.options.read_pool_timeout_ms)
    )
    .map(|conn| ReadGuard { conn, sender: self.inner.pool_sender.clone() })
    .map_err(|_| Error::PoolExhausted {
        pool_size: self.inner.options.read_pool_size,
        timeout_ms: self.inner.options.read_pool_timeout_ms,
    })
}
```

`recv_timeout` blocks until either:
- A connection becomes available (Ok(conn)) → wrap in ReadGuard and return.
- The timeout expires (Err(RecvTimeoutError::Timeout)) → return `Error::PoolExhausted`.

The error message is: `"read pool exhausted: all {pool_size} connections busy after {timeout_ms}ms; increase OpenOptions::read_pool_size"`.

### `ReadGuard` — RAII return

```rust
pub struct ReadGuard {
    conn: Connection,
    sender: crossbeam_channel::Sender<Connection>,
}

impl Drop for ReadGuard {
    fn drop(&mut self) {
        // Return the connection to the pool.
        // Safety: the channel is bounded but we know there is always a slot
        // because we took one connection out at construction.
        let conn = std::mem::replace(&mut self.conn, /* placeholder */ ...);
        let _ = self.sender.try_send(conn);
    }
}
```

When `ReadGuard` is dropped (at end of scope, or on panic), the connection is returned to the pool via `try_send`. This is always safe because the channel capacity equals the pool size and at most `pool_size` connections can be outstanding at once.

Using `ReadGuard` in code:

```rust
let guard = self.acquire_read()?;
let events = read_range_impl(&guard.conn, query)?;
// guard drops here, connection returns to pool
```

### Why a Bounded Pool?

Unbounded read concurrency would mean N simultaneous readers each holding a connection, issuing queries in parallel. In WAL mode, readers don't block each other at the SQLite lock level, but they do compete for:
- OS file descriptor limits
- Page cache entries
- CPU time for query execution

A pool of 4 is enough for the Lattica deployment model (single developer, multiple tile subscription callbacks and read operations). For high-throughput server deployments, `read_pool_size` can be increased via `OpenOptions`.

### Pool Exhaustion in Practice

Pool exhaustion happens when 4 or more concurrent read operations are all outstanding at once. Common causes:
- Subscription callbacks that are slow to process events (holding the connection longer than expected).
- A blocking aggregate query running while other reads are needed.
- A WAL watcher scan running while the pool is saturated.

The 30-second timeout is generous. In practice, if pool exhaustion is observed, either increase `read_pool_size` or investigate why reads are taking so long.

---

## 6. Write Path

All mutations go through a single `Mutex<Connection>`:

```rust
fn acquire_write(&self) -> MutexGuard<Connection> {
    self.inner.write_conn.lock().expect("write mutex poisoned")
}
```

**One writer at a time.** There is no write queue, no batching, no lock escalation. If a write is in progress, all subsequent writes wait at the Mutex. This is safe and correct for the expected write rate (events/second to low thousands/second) on a local-first store.

### IMMEDIATE Transactions

All writes use `TransactionBehavior::Immediate`:

```rust
let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
```

In WAL mode, an IMMEDIATE transaction acquires the write lock (a "reserved lock") at transaction start, before any write statements execute. This has two properties:

1. **No lock-upgrade deadlocks.** A DEFERRED transaction starts with a read lock and tries to upgrade to write lock when the first write statement runs. In WAL mode with multiple writers, this upgrade can fail with `SQLITE_BUSY` even with a generous `busy_timeout`. IMMEDIATE avoids this by grabbing the write lock upfront.

2. **Consistent snapshot for the entire transaction.** Since the write lock is held from the start, no other writer can change the database between the reads and writes within the transaction. This is critical for `append_if` (which reads a condition and conditionally writes inside one atomic transaction) and for version assignment (which reads `MAX(version)` then INSERTs).

### Write Lock and Read Pool Coexistence

In WAL mode, writers holding the write lock do **not** block readers. Readers access the main database file (or the WAL for newer pages) independently. There is a brief moment during WAL checkpointing where the database is exclusively locked, but `busy_timeout=30000` handles this.

The one time a write blocks reads is when the WAL has grown large enough that SQLite initiates a "passive checkpoint" that cannot keep up — in that case, SQLite may try an "exclusive checkpoint" that temporarily locks out readers. This is a WAL housekeeping operation, not a normal write. It is handled by the `busy_timeout` setting.

---

## 7. OpenOptions

`OpenOptions` is the configuration struct passed to `Store::open`. All fields have defaults; the only required input is the file path.

```rust
pub struct OpenOptions {
    pub encryption:            EncryptionMode,
    pub checkpoint_mode:       CheckpointMode,
    pub first_open_policy:     FirstOpenPolicy,
    pub read_pool_size:        usize,
    pub read_pool_timeout_ms:  u64,
}
```

### `encryption: EncryptionMode`

```rust
pub enum EncryptionMode {
    Plaintext,
    OsKeyring,
    EnvVar(String),
}
```

**Default:** `Plaintext`.

- **`Plaintext`** — No encryption. Payload bytes in the `events` table are plaintext msgpack. `shred_stream` returns `NotImplemented`. This is the only fully implemented mode in v1.

- **`OsKeyring`** — DEK retrieved from the OS keyring (e.g., libsecret on Linux, Keychain on macOS). Planned for v1.1. `shred_stream` would destroy the keyring entry and mark `stream_deks.shredded_at`.

- **`EnvVar(String)`** — DEK retrieved from the named environment variable. Useful for containerized deployments. Planned for v1.1. The `String` is the env var name (e.g., `"FOSSIC_STORE_KEY"`).

The encryption mode is recorded in the `meta` table at store creation time. Opening an existing plaintext store with `OsKeyring` options (or vice versa) is not currently detected or enforced — a future migration path will add validation.

### `checkpoint_mode: CheckpointMode`

```rust
pub enum CheckpointMode {
    Auto,
    Manual,
}
```

**Default:** `Auto`.

- **`Auto`** — SQLite's built-in WAL auto-checkpoint triggers after every 1000 WAL frames (default). Checkpointing runs synchronously in the writer thread when the threshold is reached.

- **`Manual`** — API shape is reserved; not implemented in v1. The intent is to give the consumer control over when WAL checkpoints occur (e.g., during a maintenance window or after a batch write).

### `first_open_policy: FirstOpenPolicy`

```rust
pub enum FirstOpenPolicy {
    CreateIfMissing,
    MustExist,
}
```

**Default:** `CreateIfMissing`.

- **`CreateIfMissing`** — If the file doesn't exist, create it and run the v1 schema migration. This is the normal mode for all consumers.

- **`MustExist`** — If the file doesn't exist, return `Error::StoreNotFound { path }`. Useful for tools that should only read an existing store, never create one accidentally (e.g., a backup tool, an audit tool).

### `read_pool_size: usize`

**Default:** `4`. Number of read connections in the pool. Increasing this allows more concurrent reads at the cost of more open file descriptors and memory.

### `read_pool_timeout_ms: u64`

**Default:** `30_000` (30 seconds). How long `acquire_read()` blocks waiting for a pool connection before returning `Error::PoolExhausted`.

---

## 8. Bootstrap Sequence

The full sequence executed by `Store::open(path, options)`:

**Step 1 — File handling.** Check `first_open_policy`. If `MustExist` and file not found, return `Error::StoreNotFound`. Otherwise, call `rusqlite::Connection::open(path)` which creates the file if needed.

**Step 2 — Write connection PRAGMAs.** Apply `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=30000`, `foreign_keys=ON` to the write connection.

**Step 3 — `run_migrations(write_conn)`**. Read `PRAGMA user_version`. Branch on the three cases (see §4 above). On a fresh database, the full `SCHEMA_V1` DDL is executed in a single `execute_batch` call.

**Step 4 — `bootstrap_meta(write_conn, encryption_mode)`**. Insert the 5 `meta` entries with `INSERT OR IGNORE`. On an existing store, all 5 INSERTs are no-ops.

**Step 5 — `bootstrap_system_streams(write_conn)`**. Insert `_fossic/system` stream with `INSERT OR IGNORE`:

```sql
INSERT OR IGNORE INTO streams(id, declared_by, declared_at, description)
VALUES ('_fossic/system', 'fossic', <now_us()>, 'Internal fossic system events')
```

**Step 6 — Read pool creation.** For each of `read_pool_size` iterations: open a new connection, apply PRAGMAs, `pool_sender.send(conn)`.

**Step 7 — WAL watcher.** Spawn the background thread that watches for cross-process writes via `notify::RecommendedWatcher` on the parent directory. See SR-04 for detail.

**Step 8 — Return.** Construct `StoreInner`, wrap in `Arc`, return `Store { inner: Arc<StoreInner> }`.

This sequence is synchronous and completes before `Store::open` returns. There are no lazy-initialization races.

---

## 9. The `now_us()` Utility

Used in every timestamp column throughout the schema:

```rust
pub(crate) fn now_us() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}
```

Returns microseconds since the Unix epoch as `i64`. Notes:

- The `as i64` cast is safe: `i64::MAX` microseconds is approximately 292,471 years from 1970, i.e., year ~292,471 AD. The cast will not overflow in the store's operational lifetime.
- `unwrap_or_default()` handles the case where the system clock is set before the Unix epoch (1970-01-01). In that case, `duration_since` returns an error and `unwrap_or_default()` returns `Duration::ZERO`, which as_micros gives 0. This is an extreme edge case (incorrect system clock) and is acceptable behavior.
- The timestamp reflects wall clock time, not event logical time. It is the "when did fossic write this" timestamp, not "when did this event logically occur in the domain." If your domain events have their own logical timestamps, store them in the event payload.
- There is no monotonicity guarantee between two events in different streams. Within a single `(stream_id, branch)`, the version ordering is authoritative; `timestamp_us` ordering within a stream is advisory.

---

## 10. `Store` Clone Semantics

`Store` is a newtype wrapper over `Arc<StoreInner>`:

```rust
pub struct Store {
    inner: Arc<StoreInner>,
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Store { inner: Arc::clone(&self.inner) }
    }
}
```

Cloning a `Store` is a cheap reference count increment — no connection is duplicated, no new files are opened, no pool slots are consumed. All clones share:
- The single write `Mutex<Connection>`
- The read pool channel (`Sender<Connection>` / `Receiver<Connection>`)
- The subscription registry
- The reducer registry
- The upcaster registry
- The transform entries
- The WAL watcher thread
- The branch chain cache (`RwLock<BTreeMap>`)

**Thread safety:** `StoreInner` is `Send + Sync`. The write Mutex provides exclusive write access. The read pool channel provides multiple-consumer multiple-producer access to read connections. The subscription registry uses its own internal locking. All in-memory state is protected by the appropriate synchronization primitive.

This design makes it idiomatic to pass a `Store` clone into background threads, async tasks, and callback closures without any external synchronization.

---

## 11. Cross-Process Access

Multiple processes can safely open the same fossic store file because WAL mode handles concurrent access at the SQLite level (using the SHM file for coordination). However, fossic imposes an additional layer:

- The **WAL watcher** (`src/wal_watch.rs`) detects when another process has written to the store by polling `PRAGMA data_version` after receiving a filesystem notification. When a change is detected, the watcher triggers the subscription dispatcher, which reads new events and dispatches them to subscribers. This is how subscriptions receive events from other processes. See SR-04 for the full WAL watcher protocol.

- Cross-process access is a **single-writer** model at the fossic level: only one process should be the designated writer at a time. Multiple readers (via WAL) plus one writer is the expected topology. Two simultaneous writers from different processes will contend on the SQLite write lock; `busy_timeout=30000` handles transient contention, but sustained multi-writer access is not a design goal.

---

## Summary

| Topic | Key Detail |
|-------|-----------|
| Storage | Single SQLite file, WAL mode mandatory |
| Schema version | `PRAGMA user_version`, current = 1 |
| PRAGMAs | WAL, synchronous=NORMAL, busy_timeout=30s, foreign_keys=ON |
| Tables | 8: events, branches, snapshots, streams, stream_deks, cursors, upcasters_registered, meta |
| Indices on events | 6: (stream,branch,version), correlation, causation, external_id, timestamp, event_type |
| Migration | run_migrations: 0=fresh, n=version check, n>current=error |
| Read pool | bounded crossbeam channel, default 4 connections, RAII ReadGuard |
| Write path | single Mutex<Connection>, IMMEDIATE transactions |
| OpenOptions | encryption (Plaintext/OsKeyring/EnvVar), checkpoint (Auto/Manual), first_open_policy, pool_size=4, pool_timeout=30s |
| Bootstrap | open file → PRAGMAs → migrations → meta → system streams → read pool → WAL watcher |
| Clone | Arc<StoreInner>, cheap, thread-safe |
| Cross-process | WAL handles concurrent reads; WAL watcher detects cross-process writes |
