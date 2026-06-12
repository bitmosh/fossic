# fossic

**fossic** is a local-first event sourcing library backed by a single SQLite file in WAL mode. It provides content-addressed event identity, an immutable append log, and a thin query/projection layer — with zero network dependencies.

## Core properties

| Property | Guarantee |
|----------|-----------|
| **Content-addressed IDs** | `id = blake3(CCE(event_type ‖ type_version ‖ causation_id ‖ payload))` — identical events produce the same ID regardless of when or where they are appended. |
| **Idempotent append** | `INSERT OR IGNORE` on the content-addressed primary key; duplicate events are silently deduplicated. |
| **Append-only log** | Rows are never updated; `purge_event` is the explicit escape hatch with a friction gate. |
| **Upcasters at read time** | Stored events keep their original bytes; transformation chains run when a consumer reads, never at write time. |
| **Cross-stream queries** | `read_by_correlation`, `walk_causation` (recursive CTE, configurable depth and direction), and `aggregate` (fold/finalize pattern). |

## Quick start

```rust
use fossic::{Append, OpenOptions, ReadQuery, Store};

let store = Store::open("data.db", OpenOptions::default())?;

store.declare_stream("orders/placed", "my-service", None)?;

let event_id = store.append(Append {
    stream_id:    "orders/placed".into(),
    event_type:   "OrderPlaced".into(),
    type_version: 1,
    payload:      serde_json::json!({ "order_id": "ord_42", "total_usd": 99 }),
    ..Default::default()
})?;

let events = store.read_range(ReadQuery::stream("orders/placed"))?;
assert_eq!(events[0].id, event_id);
```

## Crates

| Crate | Language | Published |
|-------|----------|-----------|
| `fossic` | Rust | [crates.io/crates/fossic](https://crates.io/crates/fossic) |
| `fossic-py` | Python (PyO3) | [pypi.org/project/fossic](https://pypi.org/project/fossic) |
| `fossic-node` | Node.js (napi-rs) | [npmjs.com/package/fossic](https://www.npmjs.com/package/fossic) |
