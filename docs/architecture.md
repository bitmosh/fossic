# fossic — Architecture Overview

**Version:** v1.8.2  
**Audience:** readers who want to understand how the system fits together before reading code or deep-dives.

---

## What fossic is

fossic is a local-first event sourcing library. Its storage backend is a single SQLite file in WAL mode. No daemon, no network port, no external server. The Rust core library (`fossic` crate) is the only required dependency; language bindings (Python, Node.js, Tauri) wrap it.

Events have content-addressed identities: the same logical event (same type, version, causation, payload) always produces the same 32-byte BLAKE3 ID. Appending the same event twice is a no-op — `INSERT OR IGNORE` returns the existing row. This gives idempotent relay and deduplication without a coordinator.

---

## Module map

```
src/
  lib.rs              Public re-export surface
  store.rs            Store + StoreInner (central type, all public API)
  types.rs            All public types (Append, StoredEvent, ReadQuery, OpenOptions, …)
  error.rs            Error enum (thiserror)
  cce.rs              CCE encoding + BLAKE3 event ID derivation
  schema.rs           SQLite schema (SCHEMA_V1), migrations, bootstrap
  append.rs           append_impl, append_batch_impl, append_if_impl
  read.rs             read_range_impl, read_batch_impl, read_one_impl, …
  cross_stream.rs     Causation walk, correlation read, aggregate (all cross-stream queries)
  subscriptions.rs    SubscriptionRegistry, SubscriptionHandler, SubscribeQuery
  executor.rs         BackgroundExecutor, BacklogTask, QuiescenceMonitor
  reducers.rs         Reducer trait, DynReducer, ReducerRegistry, snapshot policy
  snapshots.rs        find_latest_snapshot, write_snapshot, gc_orphaned_snapshots_impl
  branches.rs         create_branch_impl, promote_branch_impl, resolve_branch_chain
  cursors.rs          get_cursor_impl, set_cursor_impl
  deletion.rs         purge_event_impl, shred_stream_impl
  transforms.rs       PayloadTransform, apply_transforms
  upcasters.rs        Upcaster, UpcasterRegistry, apply_upcaster
  wal_watch.rs        WalWatcher (WAL frame notification via `notify` crate)
  system_stream.rs    SystemStreamWriter (internal event emission to _fossic/system)
  registry.rs         emit_project_registered, emit_relay_heartbeat
  similarity.rs       SimilaritySearchProvider trait, SimilarityQuery, SimilarityHit
  glob.rs             Glob pattern matching (*, **)
  stream.rs           declare_stream_impl, streams_impl, stream_exists_impl
```

---

## Workspace crates

| Crate | Path | Purpose |
|---|---|---|
| `fossic` | `.` | Rust core library |
| `fossic-py` | `fossic-py/` | PyO3 Python bindings (free-threaded Python 3.13+ target) |
| `fossic-node` | `fossic-node/` | napi-rs Node.js bindings with TypeScript types |
| `fossic-tauri` | `crates/fossic-tauri/` | Tauri 2 IPC companion crate |
| `fossic-similarity-hnsw` | `crates/fossic-similarity-hnsw/` | HNSW-backed `SimilaritySearchProvider` via hnsw_rs |

---

## The Store type

`Store` is the single entry point. It is `Clone` (Arc-backed) and safe to share across threads. All public API is on `Store`.

```rust
// src/store.rs:184-187
pub struct Store {
    inner: Arc<StoreInner>,
}
```

`StoreInner` holds everything:

| Field | Type | Role |
|---|---|---|
| `conn` | `Mutex<Connection>` | Single write connection |
| `read_pool_rx/tx` | `crossbeam_channel` bounded channel | Pool of read-only connections (default: 4) |
| `sub_registry` | `Arc<SubscriptionRegistry>` | In-memory subscriber table |
| `dispatch_tx` | `Sender<StoredEvent>` | Unbounded channel to dispatcher thread |
| `sync_degraded_tx` | `Sender<(sub_id, stream_id, branch, version)>` | Sync-subscriber panic signals |
| `reducers` | `RwLock<ReducerRegistry>` | Registered reducer table |
| `upcasters` | `RwLock<UpcasterRegistry>` | Registered upcaster chain |
| `transforms` | `RwLock<Vec<TransformEntry>>` | Payload transforms applied at append |
| `branch_cache` | `RwLock<BTreeMap<…, Vec<BranchSegment>>>` | In-memory ancestor chain cache |
| `background_executor` | `Mutex<Option<BackgroundExecutor>>` | Optional "fossic-bg" background thread |
| `quiescence` | `Arc<QuiescenceMonitor>` | Shared write/dispatch timestamp tracker |
| `snapshot_counters` | `parking_lot::RwLock<HashMap<…, u32>>` | Per-(stream, branch) event counter for EveryNEvents policy |
| `state_monitors` | `parking_lot::Mutex<HashMap<…, StateMonitor>>` | Rolling state-size and apply-cost buffers |
| `last_snapshot_us` | `parking_lot::RwLock<HashMap<…, i64>>` | Per-(stream, branch) last snapshot time for EveryNSeconds policy |
| `reducer_system_writer` | `parking_lot::Mutex<Option<SystemStreamWriter>>` | Lazy writer for ReducerStateLarge events |
| `project_registry_writer` | `parking_lot::Mutex<Option<SystemStreamWriter>>` | Lazy writer for ProjectRegistered/RelayHeartbeat events |

---

## SQLite schema

All data lives in one file. Schema version is tracked with `PRAGMA user_version`. There is one schema version in v1 (CURRENT_SCHEMA_VERSION = 1).

Eight tables:

| Table | Purpose |
|---|---|
| `events` | Core event log. PK: `id` (32-byte BLOB). Unique constraint on `(stream_id, branch, version)`. |
| `branches` | Branch metadata records. PK: `(stream_id, id)`. No event copying — branches share event rows. |
| `snapshots` | Reducer state blobs. PK: `(stream_id, branch, reducer_name, state_schema_version, version)`. |
| `streams` | Stream registry. PK: `id`. Streams must be declared before first append. |
| `stream_deks` | Per-stream data encryption keys (for crypto-shredding). |
| `cursors` | Consumer position tracking. PK: `(consumer_id, stream_id, branch)`. |
| `upcasters_registered` | Audit log of registered upcasters. |
| `meta` | Store metadata: fossic_schema_version, cce_version, created_at_us, encryption_mode. |

SQLite PRAGMAs set at open: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=30000`, `foreign_keys=ON`. Read connections additionally have `query_only=ON`.

---

## Event identity: CCE + BLAKE3

Every event ID is derived deterministically from four inputs (source: `src/cce.rs`):

```
event_id = BLAKE3(
    "fossic-cce-v1\0"         // fixed prefix, NUL separator
    || CCE(event_type)         // TAG_STRING || u32_LE(len) || NFC(utf8)
    || CCE(type_version)       // TAG_INT || i64_LE(value)
    || CCE(causation_id?)      // TAG_NULL or TAG_BYTES || u32_LE(32) || bytes
    || CCE(payload)            // recursive CCE encoding of serde_json::Value
)
```

**What is excluded:** `stream_id`, `branch`, `version`, `timestamp_us`, `correlation_id`, `external_id`. Two events on different streams with the same `(event_type, type_version, causation_id, payload)` produce the same ID.

The underlying INSERT is `INSERT OR IGNORE`. A collision is a silent no-op; the return value indicates whether this was a new row (`is_new: bool`).

See `docs/deep-dives/identity-and-cce.md` for the full CCE specification.

---

## Append pipeline

```
store.append(a: Append)
  │
  ├─ prepare_payload(stream_id, event_type, payload)
  │    rmp_serde::to_vec(payload)
  │    → apply registered PayloadTransforms (glob-matched, in registration order)
  │    → rmp_serde::from_slice (decode back for CCE ID derivation)
  │
  ├─ ACQUIRE write lock (Mutex<Connection>)
  │
  ├─ append_impl(conn, a, payload_val, payload_bytes)
  │    CCE::derive_event_id(event_type, type_version, causation_id, &payload_val)
  │    INSERT OR IGNORE INTO events …
  │
  ├─ if is_new && has_subscribers && !is_system:
  │    build_stored_event(outcome, a)          // no DB round-trip
  │    sub_registry.dispatch_sync(&event)      // fire Synchronous subscribers
  │    → panics caught, sub marked degraded, ID appended to sync_degraded vec
  │
  ├─ RELEASE write lock
  │
  ├─ quiescence.note_write()
  ├─ send sync_degraded IDs → sync_degraded_tx
  └─ send StoredEvent → dispatch_tx (unbounded)
```

The dispatcher thread receives from `dispatch_tx` and fans events to PostCommit subscribers.

---

## Threading model

No async runtime. All threads are OS threads (`std::thread`), all channels are `crossbeam-channel`.

| Thread | Started by | Purpose |
|---|---|---|
| Application thread(s) | Caller | Write path, read path |
| Dispatcher thread | `Store::open` | Receives from `dispatch_tx`, fans to PostCommit subscriber queues, emits `SubscriptionDegraded` system events |
| Per-subscriber thread | `Store::subscribe` (PostCommit mode) | Drains per-subscription bounded channel, calls `handler.on_event` |
| WAL watcher thread | `Store::open` (optional) | Uses `notify` crate to detect WAL frame writes; delivers events to cross-process subscribers |
| `fossic-bg` thread | `Store::open` | Runs `BackgroundExecutor`; executes GC and snapshot tasks during quiescent windows |

The dispatcher thread exits when `dispatch_tx` is dropped (i.e., when `StoreInner` drops and the last `Store` clone is released).

---

## Subscriptions

Two delivery modes exist for every `Store::subscribe` call:

**Synchronous** — handler fires while the write connection lock is held, before `append()` returns. Panics are caught; subscription marked degraded. `SubscriptionDegraded` is emitted to `_fossic/system` after the lock is released (via `sync_degraded_tx`).

**PostCommit** — handler fires from a per-subscription thread fed by a bounded `crossbeam_channel`. The store-level dispatcher fans events from `dispatch_tx` into per-sub channels. If the per-sub channel is full, the subscription is permanently and immediately degraded; a `SubscriptionDegraded` system event is emitted.

Subscriptions match events via a glob pattern on `stream_id` (`*` = one segment, `**` = any). System streams (`_fossic/*`) are excluded by default; set `include_system: true` to receive them.

See `docs/deep-dives/subscriptions-wal-watch.md` for full degradation semantics and WAL watcher behavior.

---

## Reducers and snapshots

A `Reducer` is a pure function `(State, Event) → State`. Reducers are registered against a glob pattern; the most-specific pattern wins for a given stream. State is stored as msgpack bytes.

`Store::read_state<S>(stream_id, branch)` folds all events since the latest snapshot through the reducer and returns the decoded state. Each `apply` call is wrapped in `catch_unwind` (via `apply_reducer_guarded`); a panic returns `Error::ReducerPanicked` instead of unwinding the application.

Snapshot policies (set at registration):
- `Manual` — caller calls `take_snapshot` explicitly
- `EveryNEvents(n)` — auto-snapshot after n cumulative events applied per `read_state`
- `EveryNSeconds(n)` — schedules a background snapshot via `BackgroundExecutor` after n seconds of quiet time
- `StateAdaptive { target_replay_cost_us, min_events_between }` — fires when estimated replay cost exceeds threshold

See `docs/deep-dives/reducers-snapshots.md` for the full lifecycle.

---

## BackgroundExecutor

A single "fossic-bg" OS thread runs background maintenance tasks. It holds a `Weak<dyn StoreOps>` reference — if the store is dropped, the executor skips remaining tasks rather than keeping the store alive.

Tasks are held in a max-heap (`BinaryHeap<BacklogTask>`) ordered by priority then deadline. Before running any task, the executor checks the **quiescence gate**: both `last_write_us` and `last_subscription_dispatch_us` must be at least `quiescence_window_ms` (default: 2 s) in the past. This prevents background work from racing concurrent writes.

Task kinds: `GcOrphanSnapshots`, `TakeSnapshot { stream_id, branch }`, and `Custom(Arc<dyn Fn()>)`. Custom tasks are wrapped in `catch_unwind` — a panic is logged and the executor continues.

See `docs/deep-dives/failure-modes.md` for the SR-10 analysis of executor failure modes.

---

## Cross-stream queries

Three cross-stream operations exist (all in `src/cross_stream.rs`):

- **`read_by_correlation(correlation_id)`** — all events sharing a correlation ID, across all streams. Ordered by `timestamp_us ASC`.
- **`walk_causation(start, direction, max_depth)`** — BFS through the causation graph from a root event. Direction: Forward (children), Backward (parents), or Both. Optional sampling: Exhaustive, BreadthFirst { max_per_level }, Adaptive { target_count }.
- **`aggregate(query, agg)`** — fold events matching a `ReadQuery` through a caller-supplied `Aggregate` impl. Streaming, no materialization of all events.

All three have bounded variants returning `ReadOutcome<T>` with a `TruncationCursor` for pagination.

---

## System stream (`_fossic/system`)

fossic emits diagnostic events to `_fossic/system`. These are written via dedicated `SystemStreamWriter` instances (one per owning thread) to avoid deadlocking against the main write path.

System event types:
- `SubscriptionDegraded` — a subscriber's queue overflowed (PostCommit) or its handler panicked (Synchronous)
- `ReducerStateLarge` — mean reducer state exceeds configured threshold
- `DeferredTaskDropped` — a `persist_on_drop` task was not executed before store shutdown
- `ProjectRegistered` — emitted by relay agents on startup
- `RelayHeartbeat` — periodic relay health signal

Subscribers must set `include_system: true` to receive these events.

---

## Crypto-shredding

Each stream can have a per-stream data encryption key (DEK) in the `stream_deks` table. `store.shred_stream(stream_id, reason)` destroys the key, rendering the stream's encrypted payload permanently unreadable. This provides GDPR-compliant deletion without deleting event rows.

Encryption is not implemented in v1 — `EncryptionMode::Plaintext` is the only supported option. The `stream_deks` table and `shred_stream` method are present for forward compatibility.

---

## What is not in v1

- `EncryptionMode::OsKeyring` and `EncryptionMode::EnvVar` return `Error::NotImplemented`
- `CheckpointMode::Manual` returns `Error::NotImplemented`
- `aggregate_bounded` does not produce a resume cursor (fold-resume deferred to v1.2.x)
- Pre-built binary wheels for fossic-py — callers must have Rust installed

---

*Source references: all module paths above verified against `src/` tree. All type and field names verified against `src/store.rs`, `src/types.rs`, `src/schema.rs` at v1.8.1.*
