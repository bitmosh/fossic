# fossic

Local-first event sourcing library with content-addressed event identity.

Events are identified by a deterministic BLAKE3 hash of `(event_type, type_version, causation_id, CCE(payload))`. Two identical events at the same causal position produce the same ID, giving idempotent append semantics without a distributed coordinator. Storage is a single SQLite file in WAL mode — no daemon, no network port, no separate server.

**Current version:** v1.8.1 (panic isolation hardening — SR-10 A-5, A-6, A-11)

---

## Crates in this workspace

| Crate | Path | Purpose |
|---|---|---|
| `fossic` | `.` | Rust core library |
| `fossic-py` | `fossic-py/` | PyO3 Python bindings |
| `fossic-node` | `fossic-node/` | napi-rs Node.js bindings with TypeScript types |
| `fossic-tauri` | `crates/fossic-tauri/` | Tauri 2 IPC companion crate |
| `fossic-similarity-hnsw` | `crates/fossic-similarity-hnsw/` | HNSW-backed semantic search provider |

---

## Quick start (Rust)

```rust
use fossic::{Store, OpenOptions, Append};

let store = Store::open("store.db", OpenOptions::default())?;
store.declare_stream("my-app/events", "my-app", None)?;

let event_id = store.append(Append {
    stream_id: "my-app/events".into(),
    event_type: "ThingHappened".into(),
    type_version: 1,
    payload: serde_json::json!({ "key": "value" }),
    ..Default::default()
})?;
```

For Python, Node.js, and Tauri quick starts, see the binding READMEs:
- [`fossic-py/README.md`](fossic-py/README.md)
- [`fossic-node/README.md`](fossic-node/README.md)
- [`crates/fossic-tauri/README.md`](crates/fossic-tauri/README.md)

---

## Documentation

| Doc | What it covers |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | System overview: Store internals, SQLite schema, threading model, append pipeline, subsystem map |
| [`docs/operating.md`](docs/operating.md) | How to use fossic: all major API patterns with working code, test commands, OpenOptions reference |
| [`docs/gotchas.md`](docs/gotchas.md) | Sharp edges: CCE identity collisions, dispatch channel backlog, subscription degradation, event ID ordering |
| [`docs/history.md`](docs/history.md) | Development narrative: 8 inflection points from Lattica origin through v1.8.1 |
| [`CHANGELOG.md`](CHANGELOG.md) | Versioned release history |

**Deep dives** (one per non-obvious subsystem):

| Doc | Subsystem |
|---|---|
| [`docs/deep-dives/identity-and-cce.md`](docs/deep-dives/identity-and-cce.md) | CCE encoding and BLAKE3 event ID derivation |
| [`docs/deep-dives/storage-schema-concurrency.md`](docs/deep-dives/storage-schema-concurrency.md) | SQLite schema, WAL mode, concurrency model |
| [`docs/deep-dives/event-lifecycle.md`](docs/deep-dives/event-lifecycle.md) | Event lifecycle end-to-end |
| [`docs/deep-dives/subscriptions-wal-watch.md`](docs/deep-dives/subscriptions-wal-watch.md) | Subscriptions, degradation mechanics, WAL watcher |
| [`docs/deep-dives/branches.md`](docs/deep-dives/branches.md) | Branch model and lifecycle |
| [`docs/deep-dives/reducers-snapshots.md`](docs/deep-dives/reducers-snapshots.md) | Reducers, snapshot lifecycle, auto-snapshot policies |
| [`docs/deep-dives/cross-stream-queries.md`](docs/deep-dives/cross-stream-queries.md) | Causation walk, correlation read, aggregate |
| [`docs/deep-dives/schema-evolution-deletion-errors.md`](docs/deep-dives/schema-evolution-deletion-errors.md) | Upcasters, deletion, error catalogue |
| [`docs/deep-dives/python-bindings.md`](docs/deep-dives/python-bindings.md) | PyO3 binding internals and patterns |
| [`docs/deep-dives/failure-modes.md`](docs/deep-dives/failure-modes.md) | SR-10 failure mode analysis (17 findings, open triage) |
| [`docs/deep-dives/extension-patterns.md`](docs/deep-dives/extension-patterns.md) | Sibling crate author guide: SystemStreamWriter, BackgroundExecutor, TaskKind::Custom |

**Implementation specs** (for implementors of bindings or the CCE protocol):

- [`docs/implement/CCE_SPEC.md`](docs/implement/CCE_SPEC.md) — Canonical Content Encoding specification with test vectors
- [`docs/implement/FOSSIC_V1_SPEC.md`](docs/implement/FOSSIC_V1_SPEC.md) — Full implementation spec (schema, threading, error catalogue, stream registry contract)
- [`docs/implement/AGENT_TRACE_VOCABULARY.md`](docs/implement/AGENT_TRACE_VOCABULARY.md) — Standard event vocabulary for agent trace recording
- [`docs/implement/POLICY_SCOUT_EVENT_VOCABULARY.md`](docs/implement/POLICY_SCOUT_EVENT_VOCABULARY.md) — Policy Scout event vocabulary

**Decision records:**

- [`docs/adr/`](docs/adr/) — ADRs: NATS rejection, Lattica platform origin, SQLite choice, workspace structure

---

## Key concepts

**Content-addressed IDs.** `event_id = BLAKE3("fossic-cce-v1\0" || CCE(event_type, type_version, causation_id, payload))`. Stream, branch, and timestamp are excluded — appending the same event twice returns the same ID and stores only one row.

**Stream registry.** Streams must be declared before appending. `declare_stream(id, declared_by, description)` is idempotent; typos become errors at the point of mistake.

**Subscription modes.** `Synchronous` fires inside the write transaction before `append()` returns. `PostCommit` fires on a dedicated thread after commit, through a bounded channel. Queue overflow causes permanent degradation — subscribe to `_fossic/system` with `include_system: true` to detect it.

**Reducers.** Pure `(State, Event) → State` functions registered against glob stream patterns. `read_state` folds events through the reducer using the most recent snapshot as a starting point. Panics in `apply` return `Error::ReducerPanicked` rather than unwinding.

**Branches.** Lightweight pointer records — no event copying on branch creation. A branch diverges from a parent at a specific version; all events since that version on the parent are accessible to the branch via the ancestor chain.

**Bounded reads.** All major read operations have `_bounded` variants returning `ReadOutcome<T>` (`Complete` or `Truncated { data, cursor, reason }`). Pass the cursor back to resume. Streaming iterators release the read-pool connection between yields.

**No async required.** `std::thread` + `crossbeam-channel` throughout the core. No Tokio handle needed.

---

## Running tests

```sh
just test        # Rust + Python + Node (first run ~2 min, subsequent ~30 s)
just test-rust   # Rust workspace only
just test-py     # Python binding only
just test-node   # Node binding only
```

---

## License

MIT OR Apache-2.0
