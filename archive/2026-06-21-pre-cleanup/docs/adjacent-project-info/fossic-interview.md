# fossic — Library Profile and Pass 10 Completion Summary

fossic is the local-first event sourcing library at the centre of the Lattica platform.
This document records (a) the library's own technical identity and (b) the
deliverables implemented in Pass 10 (v1.0-rc.1 prep).

---

## Library identity

### Language and runtime

**Core:** Rust 2021 edition. Zero unsafe outside of `rusqlite` / `pyo3` / `napi-rs`
FFI boundaries. Async-free Rust API — callers choose threading model.

**Bindings:**
- `fossic-py` — PyO3 0.29; synchronous Python 3.11+ API (`Python::attach`).
- `fossic-node` — napi-rs 2; Promise-based Node.js API (`spawn_blocking` bridge).
- `fossic-tauri` — Tauri 2 command bridge; returns typed error structs.

**Storage:** SQLite WAL mode (single file, tilde-expandable path, `shellexpand`).
Snapshot rows in the same DB, content-addressed with BLAKE3-based CCE IDs.

---

### What fossic provides

1. **Append-only, content-addressed event log.**  
   Events are addressed by `blake3(CCE(event_type, type_version, causation,
   correlation, payload))`. Identical events under the same CCE hash are
   idempotent — re-appending returns the existing ID with `is_new: false`.

2. **Branchable history.**  
   Every stream supports named branches. Branches can be promoted (merged to main)
   or marked dead-end. The `resolve_chain` API walks the promotion graph.

3. **Typed reducer + snapshot engine.**  
   Static (`Reducer` trait) and dynamic (`DynReducer` trait) reducers.
   Snapshots are stored in SQLite and used as starting points for `read_state`,
   eliminating full-replay cost on warm paths.

4. **Glob-based subscriptions.**  
   `subscribe(stream_pattern, branch, mode)` with PostCommit and Synchronous
   modes. WAL-watcher delivers cross-process notifications.

5. **Upcasters and payload transforms.**  
   Chain of upcasters per `(event_type, from_version)` for schema migration.
   Payload transforms applied at append time (stream-pattern matched).

6. **DynReducer bridge (Pass 10, D1–D3).**  
   Foreign-language reducers (Python, JS) register via `DynReducer` trait and
   participate in snapshot caching identically to Rust reducers. No pure-Python
   replay loop in fossic-py.

7. **SimilaritySearchProvider stub (Pass 10, D6).**  
   `trait SimilaritySearchProvider` extension point for k-NN search over event
   embeddings. No v1 implementation ships; `Store::similarity_query` returns
   `Error::NotImplemented` when no provider is configured.

---

### Scale and shape

- Single-writer per process (SQLite WAL; cross-process via WAL-watcher).
- Target: < 10k events/sec sustained. Bursts fine — writes are synchronous.
- Typical payload: JSON object, msgpack-serialised, < 4KB.
- Streams: dozens to low thousands per store.
- Store file: single SQLite `.fossic` file on local disk.

---

### Persistence and lifecycle

- Events are **immutable and append-only**. Correction = new event.
- `purge_event` (GDPR/poisoned-payload path) tombstones a single event and
  writes an audit event to the system stream.
- `shred_stream` (per-stream DEK path) is reserved for encryption phase;
  returns `NotImplemented` in plaintext mode.
- No built-in retention policy in v1.
- Backup = copy the SQLite file. WAL-mode safe to `VACUUM`/`BACKUP` online.

---

## Pass 10 deliverables (v1.0-rc.1)

### D1 — DynReducer trait (Rust core)

- `pub trait DynReducer: Send + Sync + 'static` in `src/reducers.rs`.
- Methods: `name() -> &str`, `version() -> u32`, `state_schema_version() -> u32`,
  `initial_state_bytes() -> Result<Vec<u8>>`, `apply_bytes(state, event) -> Result<Vec<u8>>`.
- `DynReducerAdapter` bridges `Box<dyn DynReducer>` into internal `BoxedReducer`.
- `ReducerRegistry::register_dyn(pattern, reducer)` and `find_by_name(name)`.
- Store methods: `register_dyn_reducer`, `read_state_bytes`, `read_state_bytes_at_version`,
  `read_state_at_version_with_reducer`, `get_snapshot_state`, `write_snapshot_state`.
- Tests: `tests/dyn_reducers.rs` — 6 tests, all passing.

### D2 — Python DynReducer wiring (fossic-py)

- `PyDynReducer` struct in `fossic-py/src/store.rs` bridges Python reducer objects
  (duck-typed protocol: `name`, `version`, `state_schema_version`, `initial_state()`,
  `apply(state, event)`) to Rust `DynReducer` via `Python::attach` + msgpack.
- `PyStore::register_reducer(pattern, reducer)` → wraps as `PyDynReducer`, calls
  `register_dyn_reducer`.
- `PyStore::read_state` / `read_state_at_version` → delegate to Rust
  `read_state_bytes` / `read_state_bytes_at_version`, decode msgpack → Python dict.
- Pure-Python reducer loop removed from `fossic/__init__.py`.

### D3 — Node.js DynReducer wiring (fossic-node)

- Rust napi layer exposes snapshot primitives: `getSnapshotState` / `writeSnapshotState`.
- JS layer (`index.js`) adds:
  - `Store.prototype.registerReducer(pattern, reducer)` — WeakMap storage, no Rust call.
  - `Store.prototype.readState(streamId, branch)` — fetches snapshot, replays via `readRange`.
  - `Store.prototype.readStateAtVersion(streamId, branch, version, reducerName?)`.
  - Glob matching helpers `_globMatches` / `_specificity` / `_findReducer`.
- Tests: `fossic-node/__test__/reducers.spec.ts` — pending Node build.

### D4 — Tauri reducer_name plumbing

- `fossic_read_state_at_version` in `crates/fossic-tauri/src/commands.rs` passes
  `reducer_name` to `store.read_state_at_version_with_reducer` when provided.

### D5 — Typed error hierarchies

- `FossicTauriError { code: String, message: String }` replaces bare `String` in
  Tauri command `Result<T, _>` returns. `impl From<fossic::Error>` maps all variants.
- `FossicErrorCode` enum and `FossicError extends Error` class in `fossic-node/index.js`
  and `index.d.ts`.

### D6 — SimilaritySearchProvider stub

- `pub trait SimilaritySearchProvider` in `src/similarity.rs`.
- `SimilarityQuery { embedding, k, stream_pattern }` and `SimilarityHit { event_id, score }`.
- `OpenOptions::similarity_provider: Option<Arc<dyn SimilaritySearchProvider>>`.
- `Store::similarity_query` delegates to provider or returns `Error::NotImplemented`.
- Tests: `tests/similarity.rs` — 3 tests, all passing.

### D7 — Spec tilde expansion fix

- `docs/implement/FOSSIC_V1_SPEC.md` §4.2 and §4.3 updated: examples now show
  `Store.open("~/.fossic/store.db")` with comment `# tilde expanded by binding`.
  `os.path.expanduser` / `path.join(os.homedir(), ...)` calls removed.

---

## Test results (post-Pass-10)

```
fossic (core):       158/158 passing  (branches, cce_vectors, cross_stream,
                                       cursors, deletion, dyn_reducers, glob,
                                       integration, reducers, similarity,
                                       snapshots, subscriptions, transforms,
                                       upcasters, wal_watch + unit tests)
fossic-py:           compiles clean, maturin tests pending maturin build
fossic-node:         compiles clean, napi tests pending build step
fossic-tauri:        compiles clean (cargo check)
```

---

## Known gaps for v1.0 final

- fossic-node reducer tests (`reducers.spec.ts`) require a compiled `.node` binary —
  not yet run in this pass (napi build step needed).
- fossic-py Python-layer snapshot caching test (apply_count measurement) not yet
  written — the Rust layer is wired; test scaffolding is in the Python test suite.
- `shred_stream` (per-stream DEK) returns `NotImplemented` in plaintext mode by design.
- No `SimilaritySearchProvider` implementation ships in v1.
