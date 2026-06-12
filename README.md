# fossic

Local-first event sourcing library with content-addressed event identity.

Events are identified by a deterministic hash of `(event_type, type_version, causation_id, CCE(payload))`. Two identical events at the same causal position produce the same ID, giving idempotent append semantics without a distributed coordinator.

Storage is a single SQLite file with WAL mode. No daemon, no separate server.

## Crates in this workspace

| Crate | Path | Purpose |
|---|---|---|
| `fossic` | `.` | Rust core library |
| `fossic-py` | `fossic-py/` | PyO3 Python bindings |
| `fossic-node` | `fossic-node/` | napi-rs Node.js bindings |
| `fossic-tauri` | `crates/fossic-tauri/` | Tauri 2 IPC companion crate |

## Quick start (Rust)

```rust
use fossic::{Store, OpenOptions, Append};

let store = Store::open("store.db", OpenOptions::default())?;
store.declare_stream("my-app/events", "my-app", None)?;

let event_id = store.append(Append {
    stream_id: "my-app/events".into(),
    branch: "main".into(),
    event_type: "ThingHappened".into(),
    type_version: 1,
    payload: serde_json::json!({"key": "value"}),
    ..Default::default()
})?;
```

## Key concepts

- **Content-addressed IDs (CCE):** event identity is a deterministic function of content. Appending the same event twice returns the same ID and stores only one row.
- **Stream registry:** streams must be declared before append. Typos become errors at the point of mistake.
- **Subscription modes:** `Synchronous` (fires inside the write transaction) and `PostCommit` (fires on a dedicated thread after commit, with a bounded queue and degraded-state handling).
- **Branches:** lightweight pointer records — no event copying on branch creation.
- **Crypto-shredding:** per-stream DEKs allow GDPR-compliant deletion by destroying the key.

## Threading model

Fossic uses `std::thread` and `crossbeam-channel` — no async runtime required. See §14 of `docs/implement/FOSSIC_V1_SPEC.md` for the full threading model.

## Tests

```sh
cargo test --all-features
cargo test --test cce_vectors -- --nocapture
```

## License

MIT OR Apache-2.0
