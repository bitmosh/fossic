# Fossic v1 Specification

**Status:** Draft for review · 2026-06-12
**Companion documents:** `CCE_SPEC.md` (canonical content encoding) · `AGENT_TRACE_VOCABULARY.md` (standard event types and extensions) · `BUILD_AND_DISTRIBUTION.md` (wheels, prebuilts, CI)

---

## 1. Scope and Non-Goals

Fossic is a local-first event sourcing library: a single Rust crate (`fossic`) with bindings for Python (`fossic`, via PyO3) and Node.js (`fossic`, via napi-rs), backed by a single SQLite database file with WAL mode. It is designed to be embedded in-process by every consumer — there is no fossic daemon.

### What v1 ships

Storage and identity: single-file SQLite store with content-addressed events derived from a custom canonical encoding (CCE), append-only writes, branchable history with lifecycle states, optional snapshots, per-stream payload-transform hooks, an `indexed_tags` JSON projection column for efficient cross-stream aggregation, an `external_id` column for consumer-supplied IDs, and an upcaster protocol for schema evolution.

API surface: Rust crate (canonical), PyO3 Python bindings (sync-first), napi-rs Node bindings (Promise-based), and a recommended Tauri IPC command set for consumers that embed fossic in a Tauri backend. Pattern-based reducer registration (one reducer per stream pattern, not per stream).

Subscriptions: two modes — `Synchronous` (callback fires in the write transaction; for correctness-critical reducers) and `PostCommit` (callback fires from a bounded dispatch queue after commit; default and recommended).

Deletion: three modes — append-only (default), crypto-shredding with per-stream DEKs (opt-in via `OpenOptions::encryption`), and a `purge_event` escape hatch with strong operational friction.

Cross-stream queries: correlation ID lookup, causation chain walk (recursive CTE), and aggregation over `indexed_tags`. A `SimilaritySearchProvider` trait is declared but not implemented in v1 — consumers can register their own vector indices.

Standard agent trace event vocabulary, an OpenTelemetry GenAI span exporter, and per-tool determinism registry — see `AGENT_TRACE_VOCABULARY.md`.

Pre-built distribution: Python wheels for `linux x86_64`, `macOS arm64`, `macOS x86_64`, `windows x86_64`. napi-rs prebuilt binaries for the same four targets. See `BUILD_AND_DISTRIBUTION.md`.

### What v1 does not ship

Cross-stream semantic similarity search — extension point only. DEK rotation — single static DEK per stream. Background checkpoint thread — `CheckpointMode::Manual` API shape is reserved but only `Auto` is implemented. HTTP gateway for containerized consumers (LiteLLM's case) — designed-for in the binding architecture, ships as separate v1.1 package. WASM binding for browser consumers — out of scope. Multi-writer replication or multi-machine deployment — fossic is single-machine. Cross-language snapshot portability — snapshots are language-bound (Model 1).

### Non-goals beyond v1.x

Distributed consensus, multi-region replication, hosted service mode, schema-registry-as-a-service, RBAC. Fossic is a local-first library; consumers that need any of these compose fossic with other tools rather than asking fossic to grow into them.

---

## 2. Storage Schema

Single SQLite file (default location: `${XDG_DATA_HOME}/fossic/store.db` on Linux, equivalent platform paths elsewhere). WAL mode mandatory. Schema version recorded in the `meta` table.

```sql
-- v1 schema. Migrations land via the meta.fossic_schema_version key.

CREATE TABLE events (
    id              BLOB    NOT NULL PRIMARY KEY,  -- 32 bytes, blake3 over CCE prefix
    stream_id       TEXT    NOT NULL,
    branch          TEXT    NOT NULL DEFAULT 'main',
    version         INTEGER NOT NULL,              -- monotonic per (stream_id, branch)
    timestamp_us    INTEGER NOT NULL,              -- microseconds since Unix epoch
    causation_id    BLOB,                          -- 32 bytes, references events.id
    correlation_id  BLOB,                          -- 32 bytes, optional grouping
    event_type      TEXT    NOT NULL,
    type_version    INTEGER NOT NULL DEFAULT 1,
    payload         BLOB    NOT NULL,              -- msgpack-encoded
    external_id     TEXT,                          -- consumer-supplied (evt_uuid, ulid, etc.)
    indexed_tags    TEXT,                          -- JSON object, validated on insert
    UNIQUE (stream_id, branch, version)
);

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

CREATE TABLE branches (
    id              TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    parent_id       TEXT    NOT NULL,              -- 'main' for root branches
    parent_version  INTEGER NOT NULL,              -- 0 for root branches
    description     TEXT,
    created_at      INTEGER NOT NULL,
    lifecycle       TEXT    NOT NULL DEFAULT 'ephemeral',
                       -- 'ephemeral' | 'promoted' | 'dead_end'
    closed_at       INTEGER,
    closed_reason   TEXT,
    alternatives    TEXT,                          -- JSON array, may be NULL
    PRIMARY KEY (stream_id, id)
);

CREATE INDEX idx_branches_stream ON branches(stream_id);
CREATE INDEX idx_branches_lifecycle ON branches(stream_id, lifecycle);

CREATE TABLE snapshots (
    stream_id            TEXT    NOT NULL,
    branch               TEXT    NOT NULL DEFAULT 'main',
    version              INTEGER NOT NULL,         -- snapshot reflects state at this version
    reducer_name         TEXT    NOT NULL,
    reducer_version      INTEGER NOT NULL DEFAULT 1,
    state_schema_version INTEGER NOT NULL DEFAULT 1,
    state_blob           BLOB    NOT NULL,         -- msgpack-encoded reducer state
    created_at           INTEGER NOT NULL,
    PRIMARY KEY (stream_id, branch, reducer_name, state_schema_version, version)
);

CREATE INDEX idx_snapshots_lookup
    ON snapshots(stream_id, branch, reducer_name, state_schema_version, version DESC);

CREATE TABLE streams (
    id              TEXT    NOT NULL PRIMARY KEY,
    declared_by     TEXT    NOT NULL,
    declared_at     INTEGER NOT NULL,
    description     TEXT
);

CREATE TABLE stream_deks (
    -- Present only when store is opened with EncryptionMode != Plaintext.
    -- Row exists for any stream that has been written under encryption mode.
    -- shredded_at IS NOT NULL means the DEK has been destroyed.
    stream_id       TEXT    NOT NULL PRIMARY KEY,
    key_id          TEXT    NOT NULL,              -- keyring entry identifier
    created_at      INTEGER NOT NULL,
    shredded_at     INTEGER,
    shredded_reason TEXT
);

CREATE TABLE cursors (
    consumer_id     TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    branch          TEXT    NOT NULL DEFAULT 'main',
    version         INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (consumer_id, stream_id, branch)
);

CREATE TABLE upcasters_registered (
    -- Bookkeeping only — actual upcaster code is consumer-side.
    -- This table records which upcasters have been registered against the
    -- current process for audit/diagnostic purposes.
    event_type      TEXT    NOT NULL,
    from_version    INTEGER NOT NULL,
    to_version      INTEGER NOT NULL,
    registered_at   INTEGER NOT NULL,
    PRIMARY KEY (event_type, from_version, to_version)
);

CREATE TABLE meta (
    key             TEXT    NOT NULL PRIMARY KEY,
    value           TEXT    NOT NULL
);
-- Required entries written at store creation:
--   fossic_schema_version    = '1'
--   cce_version              = 'fossic-cce-v1'
--   created_at_us            = <microseconds>
--   created_by_version       = '<crate semver>'
--   encryption_mode          = 'plaintext' | 'sqlcipher' | ...
```

### Schema notes

The `id` field is the full event identity, derived per the formula in Section 3 below and `CCE_SPEC.md`. It is the primary key and is sufficient for deduplication: two identical events at the same causal position produce the same id, and `INSERT OR IGNORE` semantics make idempotent retry safe.

`indexed_tags` is JSON. Validated as a JSON object (not array, not scalar) on insert. Consumers populate the few fields they need for cross-stream projection queries; payload remains msgpack and is never parsed for queries. Use SQLite's JSON1 extension at query time:

```sql
SELECT * FROM events
WHERE indexed_tags IS NOT NULL
  AND json_extract(indexed_tags, '$.arm_id') = ?
```

`external_id` is unique only per `(stream_id, external_id)`, not globally. The index reflects this. Consumers can reuse external IDs across streams.

The `streams` table is a registry. Append to an undeclared stream raises `StreamNotDeclaredError` — see Section 6. The `streams` table also enables stream listing for the time-travel viewer and similar consumers.

The `stream_deks` table is present in plaintext mode too, but always empty. This keeps the schema shape stable across encryption modes.

---

## 3. CCE — Canonical Content Encoding

Event identity is computed via blake3 over a CCE encoding of the event's identifying fields. CCE is a small purpose-built canonical encoding that is intentionally decoupled from the msgpack storage format, so that ID stability never depends on msgpack library version, struct field order, or encoder configuration.

The full spec lives in `CCE_SPEC.md`. Summary:

```
event_id = blake3(
    "fossic-cce-v1\0" ||
    cce_encode_string(event_type) ||
    cce_encode_uint(type_version) ||
    cce_encode_optional_bytes(causation_id) ||
    cce_encode(payload)
)
```

CCE has nine tag types: null, bool, i64, f64, string (NFC-normalized UTF-8), bytes, array, map (sorted by byte-lex of CCE-encoded key), and tagged-variant (for enums/sum types). The encoding is purpose-built for hashing — never used for storage. Integers normalize to i64, floats to f64 (with NaN and -0.0 canonicalization), and maps sort their keys.

Implementations exist in Rust (canonical), Python, and TypeScript. All three share a test vector file (`cce-test-vectors.json`) that every implementation must pass. Third-party implementations against the spec are welcome and supported.

---

## 4. API Surface

### 4.1 Rust core

```rust
pub struct Store { /* opaque */ }

pub struct OpenOptions {
    pub encryption: EncryptionMode,         // Plaintext | OsKeyring | EnvVar(String)
    pub checkpoint_mode: CheckpointMode,    // Auto (only mode in v1)
    pub similarity_search: Option<Box<dyn SimilaritySearchProvider>>,
    pub on_first_open: FirstOpenPolicy,     // CreateIfMissing | RequireExisting
}

impl Store {
    // --- Lifecycle ---
    pub fn open(path: impl AsRef<Path>, options: OpenOptions) -> Result<Store>;
    pub fn close(self) -> Result<()>;

    // --- Stream registry ---
    pub fn declare_stream(&self, stream_id: &str, declared_by: &str,
                          description: Option<&str>) -> Result<()>;
    pub fn streams(&self) -> Result<Vec<StreamInfo>>;
    pub fn stream_exists(&self, stream_id: &str) -> Result<bool>;

    // --- Append ---
    pub fn append(&self, append: Append) -> Result<EventId>;
    pub fn append_batch(&self, appends: &[Append]) -> Result<Vec<EventId>>;

    // --- Read ---
    pub fn read_range(&self, q: ReadQuery) -> Result<Vec<StoredEvent>>;
    pub fn read_one(&self, id: EventId) -> Result<Option<StoredEvent>>;
    pub fn read_by_external_id(&self, stream_id: &str,
                                external_id: &str) -> Result<Option<StoredEvent>>;

    // --- Reducer / aggregate ---
    pub fn register_reducer<R: Reducer>(&self, pattern: &str, reducer: R) -> Result<()>;
    pub fn read_state<S>(&self, stream_id: &str, branch: &str) -> Result<S>
        where S: ReducerState;
    pub fn read_state_at_version<S>(&self, stream_id: &str, branch: &str,
                                     version: u64) -> Result<S>
        where S: ReducerState;

    // --- Subscriptions ---
    pub fn subscribe<H: SubscriptionHandler>(&self, q: SubscribeQuery,
                                              mode: SubscriptionMode,
                                              handler: H) -> Result<SubscriptionHandle>;

    // --- Branches ---
    pub fn create_branch(&self, b: CreateBranch) -> Result<()>;
    pub fn promote_branch(&self, branch_id: &str, reason: Option<&str>) -> Result<()>;
    pub fn mark_branch_dead_end(&self, branch_id: &str,
                                 reason: Option<&str>) -> Result<()>;
    pub fn list_branches(&self, stream_id: &str) -> Result<Vec<BranchInfo>>;

    // --- Snapshots ---
    pub fn take_snapshot(&self, stream_id: &str, branch: &str) -> Result<SnapshotInfo>;
    pub fn snapshot_info(&self, stream_id: &str, branch: &str,
                          reducer_name: &str) -> Result<Option<SnapshotInfo>>;

    // --- Cross-stream queries ---
    pub fn read_by_correlation(&self, correlation_id: EventId) -> Result<Vec<StoredEvent>>;
    pub fn walk_causation(&self, start: EventId, direction: WalkDirection,
                           max_depth: usize) -> Result<Vec<StoredEvent>>;
    pub fn aggregate<A: Aggregate>(&self, query: AggregateQuery) -> Result<A::Output>;

    // --- Upcasters ---
    pub fn register_upcaster<U: Upcaster>(&self, event_type: &str,
                                           from: u32, to: u32, upcaster: U) -> Result<()>;

    // --- Payload transforms ---
    pub fn register_payload_transform<T: PayloadTransform>(&self, stream_pattern: &str,
                                                            transform: T) -> Result<()>;

    // --- Deletion ---
    pub fn shred_stream(&self, stream_id: &str, reason: &str) -> Result<()>;
    pub fn purge_event(&self, id: EventId, confirm: &str, reason: &str,
                        purged_by: &str) -> Result<()>;

    // --- Cursors ---
    pub fn get_cursor(&self, consumer_id: &str, stream_id: &str,
                       branch: &str) -> Result<Option<u64>>;
    pub fn set_cursor(&self, consumer_id: &str, stream_id: &str, branch: &str,
                       version: u64) -> Result<()>;
}
```

Trait surfaces:

```rust
pub trait Reducer: Send + Sync + 'static {
    type State: ReducerState;
    type Event: DeserializeOwned;

    const NAME: &'static str;
    const VERSION: u32;
    const STATE_SCHEMA_VERSION: u32;

    fn initial_state(&self) -> Self::State;
    fn apply(&self, state: Self::State, event: &Self::Event) -> Self::State;
}

pub trait ReducerState: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {}

pub trait Upcaster: Send + Sync + 'static {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>>;
}

pub trait PayloadTransform: Send + Sync + 'static {
    fn transform(&self, event_type: &str, payload: &[u8]) -> Result<Vec<u8>>;
}

pub trait SubscriptionHandler: Send + Sync + 'static {
    fn on_event(&self, event: &StoredEvent);
}

pub trait SimilaritySearchProvider: Send + Sync + 'static {
    fn index(&self, event: &StoredEvent) -> Result<()>;
    fn query(&self, q: SimilarityQuery) -> Result<Vec<SimilarityHit>>;
}

pub trait Aggregate: Send + Sync + 'static {
    type Output;
    fn fold(&mut self, event: &StoredEvent);
    fn finalize(self) -> Self::Output;
}
```

Value types:

```rust
pub struct Append {
    pub stream_id: String,
    pub branch: String,                    // default "main"
    pub event_type: String,
    pub type_version: u32,
    pub payload: serde_json::Value,        // or any Serialize type via append_typed
    pub causation_id: Option<EventId>,
    pub correlation_id: Option<EventId>,
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,  // must be JSON object
    pub timestamp_us: Option<i64>,         // defaults to now if None
}

pub struct StoredEvent {
    pub id: EventId,
    pub stream_id: String,
    pub branch: String,
    pub version: u64,
    pub timestamp_us: i64,
    pub causation_id: Option<EventId>,
    pub correlation_id: Option<EventId>,
    pub event_type: String,
    pub type_version: u32,
    pub payload: Vec<u8>,                  // msgpack; use deserialize_payload<T>
    pub external_id: Option<String>,
    pub indexed_tags: Option<serde_json::Value>,
}

pub struct ReadQuery {
    pub stream_id: String,
    pub branch: String,
    pub from_version: Option<u64>,
    pub to_version: Option<u64>,
    pub limit: Option<usize>,
}

pub enum SubscriptionMode {
    /// Callback fires in the write transaction's commit hook. Holds the
    /// writer until the callback returns. Use only for correctness-critical
    /// in-process reducers. See Section 7.
    Synchronous,
    /// Callback fires from a dispatch thread after commit. Bounded queue
    /// per subscription; on overflow, the subscription is marked degraded
    /// and the consumer must replay from cursor to recover. Default.
    PostCommit { queue_size: usize },
}

pub enum WalkDirection { Forward, Backward, Both }

pub struct CreateBranch {
    pub stream_id: String,
    pub branch_id: String,
    pub parent_id: String,
    pub parent_version: u64,
    pub description: Option<String>,
    pub alternatives: Option<serde_json::Value>,
}

pub enum EncryptionMode {
    Plaintext,
    OsKeyring,                             // macOS Keychain / etc.
    EnvVar(String),                        // env var name holding the passphrase
}

pub enum CheckpointMode {
    Auto,                                  // SQLite's default wal_autocheckpoint=1000
    Manual { interval_ms: u64 },           // v1: returns Err(NotImplemented)
}
```

### 4.2 PyO3 Python binding

The Python API mirrors the Rust API with synchronous semantics. Python is the most-represented consumer language in the profiles and they are all synchronous. An async wrapper layer (`fossic_aio`) is published as a separate package for asyncio consumers (Bo, LiteLLM) — it wraps the sync API in `asyncio.to_thread`.

```python
from fossic import Store, Append, ReadQuery, OpenOptions, SubscriptionMode

# Open
# tilde expanded by binding
store = Store.open(
    path="~/.fossic/store.db",
    options=OpenOptions(
        encryption="plaintext",
        on_first_open="create_if_missing",
    ),
)

# Declare streams up front (constraint-design: typos become errors)
store.declare_stream("cerebra/lattice/abc123", declared_by="cerebra")

# Append
event_id = store.append(Append(
    stream_id="cerebra/lattice/abc123",
    event_type="MemoryRecordCommitted",
    type_version=1,
    payload={"content_hash": "...", "source": "..."},
    external_id="evt_01J5...",        # Cerebra's existing UUID
    indexed_tags={"record_type": "memory"},
))

# Read
events = store.read_range(ReadQuery(
    stream_id="cerebra/lattice/abc123",
    branch="main",
    from_version=0,
))

# Lookup by external ID (Cerebra's evt_<uuid>)
event = store.read_by_external_id(
    stream_id="cerebra/lattice/abc123",
    external_id="evt_01J5...",
)

# Reducer registration (pattern-based)
from cerebra.fossic_reducers import LatticeNodeReducer

store.register_reducer(
    pattern="cerebra/lattice/*",
    reducer=LatticeNodeReducer(),
)

# Read aggregate state
state = store.read_state(
    stream_id="cerebra/lattice/abc123",
    branch="main",
)

# Subscription (context manager idiomatic)
with store.subscribe(
    stream_pattern="cerebra/lattice/*",
    mode=SubscriptionMode.post_commit(queue_size=1024),
) as sub:
    for event in sub:
        process(event)
```

**PyO3 binding internals and the Python-owned worker pattern.** The binding targets PyO3 ≥ 0.26, which renamed the core APIs (`with_gil → attach`, `allow_threads → detach`) and adds support for free-threaded Python 3.13+/3.14. Earlier PyO3 versions are not supported; their GIL semantics differ enough to affect the threading model below.

Subscription delivery does **not** invoke Python callbacks directly from a Rust-spawned thread. The reason is PyO3 issue #5467 (open as of this writing): Python `threading.local` state is reset on every `Python::attach` call from a non-Python-owned thread. This breaks `asyncio` contextvars, logging context, Django thread-locals, and any other Python machinery that relies on stable thread-local storage — all of which consumers reasonably expect to work inside a callback.

The correct pattern, used by the binding, is a Python-owned worker thread:

```
+-------------+  enqueue   +---------------+  invoke     +------------+
| Rust        | ---------> | sync queue    | <---------- | Python     |
| dispatcher  |            | (bounded,     |             | worker     |
| (PostCommit |            |  back-        |             | thread     |
|  mode)      |            |  pressured)   |             | (owned by  |
+-------------+            +---------------+             |  Python's  |
                                                          |  threading |
                                                          |  module)   |
                                                          +------+-----+
                                                                 |
                                                                 v
                                                          consumer callback
                                                          (full thread-local
                                                           context intact)
```

The Rust side produces events into a bounded sync queue. A Python worker thread, spawned by the binding from Python (via `threading.Thread`, not by Rust), reads the queue and invokes the consumer's callback on a thread Python owns. Thread-locals, asyncio contextvars, and logging context all behave correctly because the callback runs on a Python-managed thread.

Other PyO3 specifics: subscription handles support both context manager (`with`) and explicit `unsubscribe()`. Subscription iteration via `for event in sub:` uses the same queue under the hood, with the worker thread bypassed. Consumers must not call back into the store from inside a Synchronous-mode callback (deadlock); PostCommit-mode callbacks may call back into the store freely. The `register_reducer` and `register_payload_transform` callbacks are invoked synchronously on the calling thread (not the worker), so the same restriction applies in Synchronous-equivalent contexts.

### 4.3 napi-rs Node.js binding

The Node binding is Promise-based at the binding boundary so it doesn't block the V8 event loop. It is for Node.js consumers (tests, the optional standalone time-travel demo, future non-Tauri Node services) — **not** for Tauri webview JS. See Section 4.4.

```typescript
import { Store, OpenOptions, SubscriptionMode } from 'fossic';

// tilde expanded by binding
const store = await Store.open({
  path: '~/.fossic/store.db',
  options: {
    encryption: 'plaintext',
    onFirstOpen: 'create_if_missing',
  },
});

await store.declareStream('lumaweave/graph', { declaredBy: 'lumaweave' });

const eventId = await store.append({
  streamId: 'lumaweave/graph',
  eventType: 'PhysicsDialectChanged',
  typeVersion: 1,
  payload: { previous: 'radial-backbone', next: 'parallel-spines' },
});

const events = await store.readRange({
  streamId: 'lumaweave/graph',
  branch: 'main',
  fromVersion: 0n,
});

// Subscription returns an AsyncIterable; the handle is disposable.
const sub = await store.subscribe({
  streamPattern: 'lumaweave/graph',
  mode: SubscriptionMode.postCommit(1024),
});

try {
  for await (const event of sub) {
    process(event);
  }
} finally {
  sub.unsubscribe();
}
```

`version` and `EventId` are bigint and `Uint8Array` respectively across the napi boundary. The binding uses TC39 explicit resource management (`using`) where the runtime supports it; explicit `unsubscribe()` is the documented fallback.

### 4.4 Tauri IPC command surface

Tauri webviews are Chromium browser contexts, not Node.js runtimes — napi-rs bindings cannot load in the webview. Consumers that need fossic from a Tauri frontend must register IPC commands in their Rust backend that delegate to the fossic Rust crate. Fossic ships a `fossic-tauri` companion crate that registers the recommended command set against a `tauri::App`:

```rust
// In src-tauri/src/lib.rs
use fossic::Store;
use fossic_tauri::register_commands;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let store = Store::open(
                app.path().app_data_dir()?.join("store.db"),
                OpenOptions::default(),
            )?;
            app.manage(store);
            register_commands(app)?;  // registers es_* commands and event listener
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Commands registered by `fossic-tauri`:

| Command | Args | Returns |
|---|---|---|
| `fossic_list_streams` | — | `StreamInfo[]` |
| `fossic_list_branches` | `stream_id` | `BranchInfo[]` |
| `fossic_read_range` | `stream_id, branch, from_version, to_version, limit` | `SerializedEvent[]` |
| `fossic_read_one` | `event_id` (hex) | `SerializedEvent \| null` |
| `fossic_read_by_external_id` | `stream_id, external_id` | `SerializedEvent \| null` |
| `fossic_read_state_at_version` | `stream_id, branch, version, reducer_name` | `SerializedState` |
| `fossic_subscribe` | `stream_pattern, branch` | `subscription_id: string` |
| `fossic_unsubscribe` | `subscription_id` | — |
| `fossic_read_by_correlation` | `correlation_id` (hex) | `SerializedEvent[]` |
| `fossic_walk_causation` | `start, direction, max_depth` | `SerializedEvent[]` |

Push notifications use Tauri's native `emit` mechanism. The `fossic_subscribe` command registers a fossic subscription and, on each event, calls `app_handle.emit("fossic:event", &serialized_event)`. Frontend subscribers use `listen("fossic:event")` from `@tauri-apps/api/event`.

**Payload format across the IPC boundary is JSON, not msgpack.** Tauri serializes commands via `serde_json`. The `payload` field of `SerializedEvent` is JSON, deserialized from the stored msgpack on the Rust side before crossing the boundary. msgpack is the persistence format; the IPC layer sees structured JSON.

**Playwright test note for Tauri consumers:** the `listen()` push path is separate from the `invoke()` request path. Existing test mocks that wrap `invoke` do not cover `listen`. Consumers will need a separate mock mechanism for tests that need to inject synthetic fossic events. The `fossic-tauri` crate provides a test feature flag that exposes a `dispatch_test_event` helper for this purpose.

---

## 5. Reducer Model and Pattern-Based Registration

Fossic uses Model 1 for snapshot ownership: reducers are consumer code, and snapshots are language-bound. A snapshot row carries `reducer_name`, `reducer_version`, and `state_schema_version` so that a snapshot can only be read by a reducer registration that matches all three.

**Pattern-based registration** is the v1 API. Consumers register one reducer against a glob-style stream pattern; the reducer applies to every stream matching the pattern. This is the right model when consumers have many similar streams (Cerebra's per-lattice-node streams, rhyzome's per-repair-session streams, bons.ai's per-idea streams).

```python
store.register_reducer(
    pattern="cerebra/lattice/*",
    reducer=LatticeNodeReducer(),
)

# Resolves to LatticeNodeReducer for any stream matching the pattern.
state = store.read_state("cerebra/lattice/lineage_abc")
```

Pattern syntax: `*` matches a single path segment (no `/`), `**` matches any number of segments. `cerebra/lattice/*` matches `cerebra/lattice/abc` but not `cerebra/lattice/sub/abc`. `cerebra/**` matches both.

Pattern conflict resolution: when multiple registered patterns match a stream, the most specific (longest non-wildcard prefix) wins. Ambiguous matches raise `ReducerPatternAmbiguousError` at registration time, not at read time. The check fires when the second conflicting pattern is registered.

Reducers are pure synchronous functions. The trait surface enforces this — there is no `&mut self`, no async, no I/O methods exposed. A reducer that needs I/O is misshapen; the I/O happens before the events that the reducer folds.

**Snapshot lifecycle.** Snapshots are optimization-only — correctness must never depend on a snapshot existing. The store can take a snapshot when explicitly asked (`store.take_snapshot(...)`) or on an auto-cadence configured per reducer (`SnapshotPolicy::EveryNEvents(100)`). When reading aggregate state, fossic looks for the most recent snapshot whose `(reducer_name, state_schema_version)` matches the registered reducer; if found, it loads the snapshot and replays events from `snapshot.version + 1` forward. If no snapshot is found, fossic replays from version 0.

A consumer changing their state shape bumps `STATE_SCHEMA_VERSION` on the reducer. Old snapshots become invisible (different state_schema_version), and the next read does a full replay. Old snapshots are eligible for garbage collection by a separate `store.gc_orphaned_snapshots()` call. They were never needed for correctness.

---

## 6. Stream Registry

Stream IDs are TEXT and must be declared before append. An append to an undeclared stream raises `StreamNotDeclaredError`. This is constraint-design — typos in stream IDs become errors at the point of mistake, not silent new streams that pollute the timeline.

```python
store.declare_stream("cerebra/lattice/abc123", declared_by="cerebra",
                     description="lineage_abc memory record aggregate")

# This raises StreamNotDeclaredError — "cerbra" is a typo
store.append(Append(stream_id="cerbra/lattice/abc123", ...))
```

Stream ID conventions are documented but not enforced by the type:

- **Format:** `{module}/{resource-type}/{resource-id}[/{sub-type}]`. Examples: `cerebra/lattice/lineage_abc`, `rhyzome/repair/session_42`, `bonsai/idea/idea_xyz`, `lumaweave/graph`, `policy-scout/audit`, `bo/conversation/channel_42`.
- **Max length:** 256 characters.
- **Allowed characters:** alphanumeric, `-`, `_`, `/`. No whitespace, no quote characters, no path separators other than `/`.
- **Max path segments:** 4.

These rules are validated at `declare_stream` time and produce typed errors with descriptive messages.

`store.streams()` returns the registry. The Tauri IPC command `fossic_list_streams` delegates to this.

---

## 7. Subscription Model

Two modes, made explicit at subscription registration. The choice is consequential and should not be defaulted accidentally.

### Synchronous mode

The callback fires inside the write transaction's commit hook, before the writer's `commit()` returns. The writer waits for the callback to complete. If the callback panics, the append still commits (the subscriber is notified, not the gatekeeper) and the panic is logged; the subscription is marked degraded.

Use Synchronous when:
- The subscriber maintains an in-process projection that must be consistent with the event store for correctness reasons (e.g., a unique-name index that must reject a duplicate before the next append).
- The subscriber's work is small (< 1 ms) and bounded.
- You understand that a slow subscriber blocks every write.

Do not use Synchronous when:
- The subscriber does I/O.
- The subscriber's work is variable in cost.
- More than one Synchronous subscription is attached to the same stream (their work serializes onto every write).

### PostCommit mode (default)

The callback fires from a dedicated dispatch thread after commit returns. Each subscription has a bounded queue (default 1024 events). On queue overflow, the subscription is marked degraded; the next event is dropped, a `SubscriptionDegraded` event is appended to a `_fossic/system` stream, and the consumer must replay from a cursor to recover.

Use PostCommit (which is almost always) when:
- The subscriber does anything I/O-like (network, file, slow CPU work).
- The subscriber is in a different process from the writer (e.g., a separate Python consumer for a writer in Rust).
- You want backpressure to surface as a degraded subscription, not as a blocked writer.

### Cross-language subscription delivery

Inside a single process, subscriptions are direct callback invocations (no IPC). Across processes (e.g., Tauri Rust writer + Python reader on the same store file), cross-process notification uses a WAL-file-watch combined with a `PRAGMA data_version` cross-check. This is the established pattern from production projects doing similar work (Honker, the iMessage MCP server).

The mechanism:

1. The fossic dispatcher registers a `notify`-crate file watcher on `<store>.db-wal`.
2. On any modification event, the dispatcher calls `PRAGMA data_version` against its read connection. `data_version` is a monotonically-incrementing integer that bumps on every write by any process; SQLite guarantees it visible to all open connections.
3. If `data_version` changed since the last check, the dispatcher fans out wakeups to subscribers — readers query for new events with a normal `SELECT ... WHERE version > cursor` against their tracked cursor.
4. If `data_version` is unchanged, the file event was likely a checkpoint or WAL truncation. The dispatcher re-establishes the watcher if the file descriptor was invalidated (PASSIVE checkpoints don't truncate; FULL/RESTART/TRUNCATE checkpoints do) and continues.

The cross-check matters because PASSIVE checkpoint events can modify the WAL file without representing new committed transactions visible to readers, and WAL truncations can create a window where naive file-watching would miss events. `data_version` is the canonical "did the data change" signal; the file-watch is just the cheap wakeup.

Latency: sub-millisecond cross-process wakeup, no daemon, no socket. Bench data (Section 11 results) shows zero contention up to 5 concurrent processes; the WAL-watch is not at risk of pathological wake storms because `data_version` checks are O(1) and only fan out when something actually changed.

**Sharp edge documented for implementers:** the `notify` crate's behavior on macOS (FSEvents) coalesces rapid file events. This is fine for our use case — coalesced events still trigger a `data_version` check, and missing an intermediate state doesn't matter because subscribers query for `version > cursor` and pick up whatever has happened. The Linux (inotify) and Windows (ReadDirectoryChangesW) backends behave similarly enough. No platform-specific code paths needed.

### Cursor ownership invariant

The PostCommit dispatcher is the **sole writer to subscription cursors**. Exactly one code path calls `set_cursor` for a subscription: `dispatch_post_commit`, after a successful `try_send` to the subscriber's bounded queue. No other code path — including the WAL watcher, the read path, or consumer code — may advance a subscription cursor.

Violating this invariant causes one of two failure modes:
- **Skip-delivery:** a cursor advanced before the event is dispatched causes the dispatcher's `version <= cursor` guard to fire, silently dropping events the consumer never received.
- **Double-delivery:** a cursor advanced after dispatch but outside the dispatcher can race with normal dispatch, delivering the same event twice.

The WAL watcher's role is to detect that new events may exist and wake the dispatcher. It does not know which subscribers are interested or whether queue delivery succeeded. Cursor advancement is conditional on successful `try_send`, which only the dispatcher can observe.

### Subscription handle lifecycle

| Language | Idiomatic cleanup | Fallback |
|---|---|---|
| Rust | `Drop` impl on `SubscriptionHandle` | — |
| Python | `with store.subscribe(...) as sub:` | explicit `handle.unsubscribe()`, with `__del__` warning if neither used |
| TypeScript | `using sub = await store.subscribe(...)` (TC39) | explicit `await sub.unsubscribe()`, with FinalizationRegistry warning |

Subscription handles must be released. A leaked handle holds memory for the bounded queue and prevents the subscriber's closures from being collected. The Python `__del__` and JS `FinalizationRegistry` paths emit a one-line warning on unreleased handles to surface the leak in development.

---

## 8. Branch Model

A branch is a pointer record: `(stream_id, branch_id, parent_id, parent_version)`. No events are copied on branch creation. Reading a branch resolves the ancestor chain and reads each segment in order.

### Branch creation

```python
store.create_branch(CreateBranch(
    stream_id="rhyzome/repair/session_42",
    branch_id="repair-class_bug/rethink_state_design-1",
    parent_id="main",
    parent_version=23,
    description="Strategy B counterfactual after A failed",
    alternatives=[
        {"strategy": "ast_guided_patch", "score": 0.82, "rank": 1},
        {"strategy": "rethink_state_design", "score": 0.71, "rank": 2},
        {"strategy": "fix_imports", "score": 0.45, "rank": 3},
    ],
))
```

`alternatives` is JSON, may be NULL, and is **strongly recommended** when the branch is created from a decision point (strategy selection, bandit arm pick). Recording alternatives at branch creation time means replay can reconstruct what choices were available without re-running upstream logic. This is documented as convention; not enforced at create-branch time, because not all branches arise from a decision.

Branch IDs are TEXT, validated as `{format: same as stream-id segments, max 128 chars, no whitespace, no quotes, no path separators other than /}`. Branch IDs are unique per `(stream_id, branch_id)`, so the same branch_id can exist on different streams.

### Branch lifecycle

`lifecycle` field on the `branches` row:

| Value | Meaning |
|---|---|
| `ephemeral` (default) | Branch is a candidate. Eligible for cold archival / discard per retention policy. Most rhyzome strategy branches are ephemeral. |
| `promoted` | Branch succeeded or is otherwise canonical. Kept forever. |
| `dead_end` | Branch failed in a way worth remembering. Kept forever, but visualization and default queries can de-emphasize. bons.ai culled-idea streams use this. |

Transitions:

```python
store.promote_branch("repair-42/strategy-b-1", reason="tests passed; smallest diff")
store.mark_branch_dead_end("idea-xyz/explore-mutation", reason="similarity to ancestor > 0.95")
```

Lifecycle is one-way: ephemeral → (promoted | dead_end). No transitions back. Retention sweeps query the `lifecycle` column and apply policy per consumer's configuration.

### Branch read semantics

Reading a branch resolves the ancestor chain:

```
read_range(stream="rhyzome/repair/session_42", branch="repair-42/strategy-b-1",
           from=0, to=latest)
  → events 0..=23 from main + events 24.. from repair-42/strategy-b-1
```

Chain resolution is cached in an in-process LRU keyed on `(stream_id, branch_id)`. Cache invalidates when a new branch is created on any ancestor.

### Branch and storage

Two branches of the same stream can be appended to concurrently (different `branch` values, different version sequences). They contend only at the SQLite WAL writer lock, which the bench showed is not an issue at realistic throughput. bons.ai's parallel-cycles-on-same-parent concern is addressed by this: parallel cycles map to parallel branches.

---

## 9. Deletion Model

Three modes, picked at store-open time and at call site.

### 9.1 Append-only (default)

`OpenOptions::encryption = Plaintext`. No deletion API except `purge_event` (Section 9.3). Every write is permanent. This is the right default for time-travel and replay invariants.

### 9.2 Crypto-shredding (opt-in)

`OpenOptions::encryption = OsKeyring` (or `EnvVar`). Each stream gets a per-stream Data Encryption Key (DEK) at first write. The DEK lives in the OS keyring (macOS Keychain / Windows Credential Manager / Secret Service on Linux), or in an environment variable for headless contexts. Event payloads in the SQLite database are stored encrypted with the stream's DEK.

```python
store = Store.open(
    path="~/.fossic/store.db",
    options=OpenOptions(encryption="os_keyring"),
)

# Per-stream DEK auto-generated on first write to a new stream
store.append(Append(stream_id="cerebra/session/abc123", ...))

# Later: user requests deletion
store.shred_stream(
    stream_id="cerebra/session/abc123",
    reason="user requested deletion per GDPR Article 17",
)

# After shred:
# - The DEK is removed from the keyring
# - The encrypted payloads remain in SQLite but are mathematically unrecoverable
# - The stream remains visible as "shredded" (metadata preserved)
# - Reads return ShredddedStreamError or, opt-in, stub events with shredded=true
```

**Granularity is per-stream.** Per-event DEKs would be a key-management nightmare with no real benefit — users request deletion of coherent units ("this session", "this source"), not individual events.

**Shredded streams remain visible.** The stream's existence, timestamps, type, and stream registration metadata are preserved. The payload is gone. This matches Cerebra's witness-substrate semantics: "this happened and was forgotten" is epistemically different from "this never happened."

A `ShreddedStreamMarker` event is appended to a `_fossic/system` stream on every successful shred:

```json
{
  "stream_id": "cerebra/session/abc123",
  "shredded_at": 1718200000000000,
  "reason": "user requested deletion per GDPR Article 17",
  "event_count_at_shred": 47,
  "version_at_shred": 47
}
```

DEK rotation is not in v1. A 90-day rotation policy is a v1.1 candidate; until then, a stream's DEK is generated on first write and destroyed only on shred.

**Keyring availability and the encrypted-filesystem fallback.** OS keyring access is reliable on desktop but has known failure modes on headless Linux (Secret Service requires an unlocked Gnome keyring; common in CI and minimal-container environments) and on locked macOS Keychain sessions. Fossic's `OsKeyring` mode automatically falls back to an encrypted-filesystem keystore at `<store_dir>/keys.age` when the OS keyring is unavailable. The fallback uses age X25519 encryption with the key derived from either a process-level passphrase (set via `FOSSIC_KEYSTORE_PASSPHRASE` env var) or, if that's unset at fallback time, fossic emits a one-time prompt on stdin (with `FOSSIC_KEYSTORE_NONINTERACTIVE=1` to suppress the prompt and require the env var). Consumers in headless contexts (CI, containers) should set the env var; consumers in interactive contexts can rely on the prompt. The fallback file is portable across machines if the passphrase is portable; the OS keyring path is not.

**Linux Secret Service deadlock warning.** The `keyring` crate's Secret Service backend uses async DBus internally. Calling keyring operations on a Tokio runtime thread can deadlock against the DBus event loop. Fossic's keyring access always happens on a dedicated thread, never on a Tokio runtime thread — this is enforced internally and is invisible to consumers. Custom code that calls into fossic's keyring layer from external Tokio contexts should not exist; if a consumer thinks it needs to, they should file an issue rather than work around the constraint.

### 9.3 `purge_event` (escape hatch with friction)

For genuine emergencies: a committed secret, a malformed PII payload, a poisoned event that breaks reducers. Not for routine deletion.

```python
store.purge_event(
    id=event_id,
    confirm="I understand this breaks replay-from-zero",
    reason="API key leaked in payload; reviewed by security team",
    purged_by="security-team@example.com",
)
```

`confirm` must be the literal string `"I understand this breaks replay-from-zero"`. Any other value raises `PurgeConfirmationError`. The function logs at WARN level with a full stack trace on every call. A `Purged` event is appended **before** the original row is deleted, so the purge is itself permanently recorded:

```json
{
  "event_id_purged": "...",
  "original_event_type": "...",
  "original_stream_id": "...",
  "original_timestamp_us": 1718200000000000,
  "reason": "API key leaked in payload; reviewed by security team",
  "purged_at_us": 1718999999999999,
  "purged_by": "security-team@example.com"
}
```

The original payload is **not** included in the `Purged` event — that's the point.

Consumers of the unified timeline see the `Purged` event in the stream and can handle the gap gracefully. Replay-from-zero is structurally broken for that stream (one event is missing); downstream reducers should treat the `Purged` event as a "skip and continue" signal in their pattern matching.

### 9.4 `_fossic/system` stream event conventions

Fossic reserves the `_fossic/system` stream for internal bookkeeping events. Consumer subscriptions should not subscribe to this stream unless they are explicitly consuming system metadata (e.g., a monitoring layer watching for `SubscriptionDegraded` events).

**Naming convention:** all event types on `_fossic/system` use bare PascalCase with no namespace prefix or dot separator.

| Event type | Trigger | Key payload fields |
|---|---|---|
| `SubscriptionDegraded` | PostCommit subscription queue overflow | `subscription_id`, `stream_id`, `cursor_version` |
| `ShreddedStreamMarker` | Successful `shred_stream` call (§9.2) | `stream_id`, `shredded_at`, `reason`, `event_count_at_shred`, `version_at_shred` |
| `Purged` | Successful `purge_event` call (§9.3) | `event_id_purged`, `original_event_type`, `original_stream_id`, `original_timestamp_us`, `reason`, `purged_at_us`, `purged_by` |

**Dispatcher skip rule:** the PostCommit dispatcher skips events on streams matching `_fossic/` when deciding which subscriptions to fan out to, preventing a `SubscriptionDegraded` event from triggering a degraded subscription and causing an infinite loop.

---

## 10. Cross-Stream Queries

Three first-class APIs and one extension point.

### 10.1 Correlation lookup

`store.read_by_correlation(correlation_id)` returns all events across all streams with the matching `correlation_id`. Uses the `idx_events_correlation` index. O(matching events).

Use cases: LumaWeave subgraph highlighting (one user action triggers events across Cerebra, Policy Scout, LumaWeave; correlation_id ties them), debugging a multi-module operation.

### 10.2 Causation chain walk

`store.walk_causation(start_event_id, direction, max_depth)` walks the `causation_id` graph forward (children: events whose causation_id is the start), backward (ancestors), or both. Implemented as a SQLite recursive CTE.

Use cases: bons.ai lineage walk ("show me the full ancestor chain of idea X"), incident forensics ("what did this initial event lead to").

### 10.3 Aggregation over indexed_tags

`store.aggregate(query)` with the `Aggregate` trait runs efficient cross-stream group-by/sum/count using the `indexed_tags` JSON column:

```python
result = store.aggregate(AggregateQuery(
    stream_pattern="bonsai/idea/*",
    where={"indexed_tags.generation": 5},
    group_by="indexed_tags.arm_id",
    aggregations={
        "count": "count(*)",
        "mean_reward": "avg(indexed_tags.reward)",
    },
))
# → {"arm_X": {"count": 12, "mean_reward": 0.73}, "arm_Y": {...}}
```

Consumers populate `indexed_tags` at append time with the few fields they need for aggregation. Cost: one indexed JSON column. Benefit: efficient projection queries without msgpack decoding.

### 10.4 Similarity search (extension point only)

The `SimilaritySearchProvider` trait is declared in v1 but has no default implementation:

```rust
pub trait SimilaritySearchProvider: Send + Sync + 'static {
    fn index(&self, event: &StoredEvent) -> Result<()>;
    fn query(&self, q: SimilarityQuery) -> Result<Vec<SimilarityHit>>;
}
```

Consumers (bons.ai's cross-stream semantic-similarity-on-idea-text use case) can register their own provider against a vector index (ChromaDB, sqlite-vec, etc.). The trait shape is the v1 contract; implementations are out of v1 scope.

When a `SimilaritySearchProvider` is registered, fossic calls `index(event)` for every appended event (in PostCommit mode; never blocks the writer). Consumers call `store.similarity_query(...)` to retrieve hits.

---

## 11. Agent Trace Adapter

See `AGENT_TRACE_VOCABULARY.md` for the full event type list and per-tool determinism registry. Summary:

Standard event types: `llm_call`, `llm_response`, `tool_call`, `tool_result`, `reasoning_step`.

Rhyzome extension: `strategy_selected`, `ast_gate_evaluated`, `strategy_exhausted`.

Bons.ai extension: `bandit_arm_selected`, `bandit_arm_updated`, `bandit_decision`, `stagnation_detected`, `adaptation_applied`, `memory_retrieved`, `embedding_stored`.

Per-tool determinism: `deterministic` defaults to `false` (safer default — re-execute on replay is visible latency; serve-stale is invisible corruption). Tools opt in to `true` via the `register_tool` API on the trace adapter.

OpenTelemetry GenAI span export: optional, runs as a background dispatcher subscribed to streams matching `*/agent-trace/*` and any pattern the consumer configures. Default OTLP endpoint `localhost:4317`. No-op if no collector is configured.

---

## 12. Payload Transform Hook

Consumers register transforms per stream pattern. The transform fires before content-addressing is computed, so the resulting `id` reflects the transformed payload. Policy Scout's `redact_dict()` pass is the canonical case.

```python
from fossic import PayloadTransform

class RedactPii(PayloadTransform):
    def transform(self, event_type: str, payload: bytes) -> bytes:
        decoded = msgpack.unpackb(payload)
        redacted = redact_dict(decoded)
        return msgpack.packb(redacted, default=str)

store.register_payload_transform(
    stream_pattern="policy-scout/audit",
    transform=RedactPii(),
)
```

Transforms run in the append path before CCE encoding. They are pure functions (no I/O, no state). A transform that needs to fail must raise; the append fails atomically.

Multiple transforms can be registered against overlapping patterns; they chain in registration order. Order is deterministic.

---

## 13. Upcaster Protocol

When a consumer changes the shape of an event payload, they bump `type_version` and register an upcaster. Reducers always see the latest version; old events go through chained upcasters at read time.

```python
from fossic import Upcaster

class ScoreFieldUpcaster(Upcaster):
    def upcast(self, payload: bytes) -> bytes:
        old = msgpack.unpackb(payload)
        # v1 had "score: int"; v2 splits to "raw_score: int, normalized_score: float"
        new = {
            **old,
            "raw_score": old.pop("score"),
            "normalized_score": old["score"] / 100.0,
        }
        return msgpack.packb(new, default=str)

store.register_upcaster(
    event_type="MemoryRecordScored",
    from_version=1,
    to_version=2,
    upcaster=ScoreFieldUpcaster(),
)
```

Chained upcasters compose: an event at `type_version=1` with registered upcasters `1→2` and `2→3` is upcast through both before reaching the reducer.

Upcasters are pure synchronous functions. They are consumer-side code, registered at startup. The `upcasters_registered` table records the registration for audit but does not store upcaster code.

Replay determinism note: the `id` of an event is computed at original append time over the original payload. Upcasting changes the payload the reducer sees but does not change the `id`. This is correct — the event's identity is what it was, not what it is now.

---

## 14. Threading Model

Fossic's background infrastructure runs on `std::thread`, not Tokio. No async runtime is required, embedded, or injected.

**Subscription dispatcher:** one `std::thread` per PostCommit subscription, spawned by `Store::subscribe`. The thread blocks on `crossbeam-channel::Receiver::recv`, waiting for events from the store-level dispatcher. The store-level dispatcher is a single thread that receives from the write path's `dispatch_tx` sender and fans out to each subscriber's bounded channel via `try_send`. The dispatcher thread exits when the `SubscriptionHandle` is dropped (channel close wakes the thread).

**WAL watcher:** one `std::thread`, spawned at `Store::open`. Registered with the `notify` crate for file-system events on `<store>.db-wal`. On any modification, it checks `PRAGMA data_version` against the last-known value to confirm new committed data exists, then pings the store-level dispatcher. Does not advance subscription cursors — see §7 "Cursor ownership invariant".

**Keyring access:** `OsKeyring` operations run on a dedicated `std::thread::spawn` call. The `keyring` crate's Secret Service backend on Linux uses blocking DBus I/O internally; isolating it on a dedicated thread avoids blocking the calling thread.

**No Tokio in the Rust core.** The `fossic` crate has no `tokio` dependency. The `fossic-node` binding uses napi-rs's bundled Tokio runtime for its own async surface, but this is entirely within the binding layer and invisible to the core. The `fossic-tauri` companion crate has no Tokio runtime injection; Tauri's own runtime handles Tauri's async work independently.

**Consumer rule:** fossic's internal threads are opaque and unaddressable. `OpenOptions` has no thread-handle or runtime-handle injection field. If you need custom scheduling behavior for your subscription callback, implement it inside your `SubscriptionHandler` — the handler receives events on a fossic-managed thread and may forward to whatever executor you own.

---

## 15. Open Extension Points (Designed-For, Not Implemented in v1)

The following are API surfaces shaped now to avoid breaking changes later. v1 ships the trait or enum variant; the implementation lands in v1.x or v2.

**`CheckpointMode::Manual { interval_ms }`** — v1 accepts the enum variant but `Store::open` returns `Err(NotImplemented)` if Manual is requested. The implementation (background checkpoint thread with `wal_autocheckpoint=0` and explicit timer) lands in v1.1 if any consumer hits the burst-checkpoint stall in production.

**`SimilaritySearchProvider`** — declared trait, no default impl. v1 calls `index()` for every event if a provider is registered. `store.similarity_query(...)` returns `Err(NoProviderRegistered)` otherwise. bons.ai's anticipated v2 work registers an implementation.

**`fossic-http-gateway`** — a companion crate (not part of v1) that exposes the fossic API over a localhost HTTP server. Designed for containerized consumers (LiteLLM) that can't easily embed PyO3. v1.1 candidate; the v1 Rust API is shaped to be wrappable without changes.

**DEK rotation** — `stream_deks` schema accepts multiple DEKs per stream (currently exactly one row per stream), but v1 only generates a single DEK per stream. v1.x adds `store.rotate_dek(stream_id)` and the schema accommodates the multi-DEK shape.

**Cross-language snapshot portability** — Model 1 keeps snapshots language-bound in v1. If a real cross-language read-snapshot use case appears (which would be the LumaWeave time-travel viewer needing to deserialize Cerebra's reducer state), a v2 WASM reducer model becomes a candidate. No commitment.

---

## 16. Invariants

These invariants are structural — enforced by the API surface, not by discipline. Each is a constraint that, if violated, breaks fossic's guarantees.

1. **Events are immutable.** The Rust core exposes no `update_event` or `delete_event` (only `purge_event` with its friction). Content-addressed IDs make tampering detectable.
2. **`id` is a deterministic function of `(event_type, type_version, causation_id, CCE(payload))`.** Two identical events at the same causal position produce the same id. The deduplication property follows.
3. **`version` is monotonic per `(stream_id, branch)`.** Enforced by the unique constraint.
4. **Snapshots never affect correctness.** Reading aggregate state without any snapshot for a stream always produces the same result as reading with snapshots. Snapshots can be deleted at any time without consequence beyond replay cost.
5. **Reducers are pure synchronous functions.** Trait shape forbids `&mut self`, async, and I/O. A reducer that calls into the store is misshapen.
6. **`indexed_tags` is a JSON object.** Arrays and scalars are rejected at append time.
7. **Stream IDs must be declared.** Append to undeclared stream raises.
8. **`Purge` events always precede the row deletion.** The act of purging is itself permanently recorded.
9. **Branches are pointers, not copies.** Branch creation does not duplicate parent events.
10. **Fossic does not write to source data.** When fossic surfaces events from a Cerebra `inspector_events` row via a read adapter, the adapter never writes back.

---

## 17. Reference: Consumer Profile Mapping

A reverse-index of which v1 features each consumer profile drove:

| Feature | Driving consumer(s) |
|---|---|
| Single-file SQLite | All; bench confirms |
| `external_id` column | Cerebra (`evt_<uuid>`), Policy Scout (`request_id`) |
| `indexed_tags` column | bons.ai (population aggregation queries) |
| Payload transform hook | Policy Scout (`redact_dict`) |
| Pre-built wheels | All Python consumers |
| Sync-first Python | Cerebra (hard requirement), Policy Scout |
| Branch lifecycle states | Rhyzome (ephemeral/promoted), bons.ai (dead_end) |
| `alternatives` on branch creation | Rhyzome (StrategyRouter ranked list), bons.ai (BanditDecision candidates) |
| Pattern-based reducer registration | Cerebra (thousands of lattice nodes), rhyzome (per-repair-session) |
| Crypto-shredding (per-stream DEK) | Cerebra (session/source deletion) |
| Causation walk + correlation lookup | bons.ai (lineage), LumaWeave (subgraph highlight) |
| `SimilaritySearchProvider` extension | bons.ai (semantic NN on idea text) |
| Upcaster protocol | Cerebra (`schema_version` first-class field) |
| Tauri IPC command set | LumaWeave (webview consumer) |
| `deterministic: false` default | Rhyzome (push: visible cost is correct failure mode) |
| Per-tool determinism registry | bons.ai (per-role table), rhyzome (per-tool table) |
| Subscription mode split | LumaWeave (real-time Graph B), Cerebra (backpressure-tolerant) |

---

*End of v1 spec. Companion documents: `CCE_SPEC.md`, `AGENT_TRACE_VOCABULARY.md`, `BUILD_AND_DISTRIBUTION.md`.*
