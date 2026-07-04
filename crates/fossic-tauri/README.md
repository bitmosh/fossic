# fossic-tauri

Tauri 2 IPC companion crate for [fossic](../../README.md). Registers a set of `invoke`-callable commands that expose the fossic store to Tauri webview frontends.

Tauri webviews are Chromium browser contexts — napi-rs bindings cannot load there. This crate is the correct integration path for Tauri applications. For Node.js consumers, use [fossic-node](../../fossic-node/README.md) instead.

## Setup

```rust
// src-tauri/src/lib.rs
use fossic::{Store, OpenOptions};
use fossic_tauri;

fn main() {
    let store = Store::open("store.db", OpenOptions::default())
        .expect("failed to open fossic store");

    tauri::Builder::default()
        .plugin(fossic_tauri::plugin(store))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## IPC commands

| Command | Args | Returns |
|---|---|---|
| `fossic_list_streams` | — | `StreamInfo[]` |
| `fossic_list_branches` | `stream_id` | `BranchInfo[]` |
| `fossic_read_range` | `stream_id, branch, from_version, to_version, limit` | `SerializedEvent[]` |
| `fossic_read_one` | `event_id` (hex) | `SerializedEvent \| null` |
| `fossic_read_by_external_id` | `stream_id, external_id` | `SerializedEvent \| null` |
| `fossic_read_state_at_version` | `stream_id, branch, version, reducer_name` | `SerializedState` |
| `fossic_subscribe` | `stream_id, branch` | `subscription_id: string` |
| `fossic_unsubscribe` | `subscription_id` | — |
| `fossic_read_by_correlation` | `correlation_id` (hex) | `SerializedEvent[]` |
| `fossic_walk_causation` | `start, direction, max_depth` | `SerializedEvent[]` |
| `fossic_read_range_bounded` | `stream_id, branch?, from_version?, to_version?, event_type_filter?, max_results?, max_bytes?` | `ReadOutcome` |
| `fossic_read_range_from_cursor` | same + `cursor` (base64) | `ReadOutcome` |
| `fossic_read_by_correlation_bounded` | `correlation_id, max_results?, max_bytes?` | `ReadOutcome` |
| `fossic_read_by_correlation_from_cursor` | same + `cursor` (base64) | `ReadOutcome` |
| `fossic_walk_causation_bounded` | `start, direction, max_depth?, sampling?, max_results?, max_bytes?` | `ReadOutcome` |
| `fossic_walk_causation_from_cursor` | same + `cursor` (base64) | `ReadOutcome` |
| `fossic_aggregate_bounded` | `stream_pattern, branch?, event_type_filter?, from_timestamp_us?, to_timestamp_us?, indexed_tags_filter?, max_events_scanned?, max_bytes?` | `AggregateOutcome` |

## Bounded read commands

### ReadOutcome shape

All bounded commands return a `ReadOutcome` JSON object:

```typescript
// Complete — all results fit within the budget
{ kind: "complete", results: SerializedEvent[] }

// Truncated — budget hit; more results may remain
{
  kind: "truncated",
  results: SerializedEvent[],
  reason: "result_count" | "byte_size",
  next_cursor: string   // base64-encoded opaque bytes
}
```

`reason` and `next_cursor` are omitted (not `null`) on complete outcomes.

### Cursor resumption

Pass the cursor string back to the corresponding `_from_cursor` command:

```typescript
import { invoke } from '@tauri-apps/api/core'

const page1 = await invoke('fossic_read_range_bounded', {
    streamId: 'cerebra/lattice/session_42',
    maxResults: 500,
})

if (page1.kind === 'truncated' && page1.next_cursor) {
    const page2 = await invoke('fossic_read_range_from_cursor', {
        streamId: 'cerebra/lattice/session_42',
        maxResults: 500,
        cursor: page1.next_cursor,
    })
}
```

The `_bounded` command always starts from the beginning. The `_from_cursor` command requires a valid cursor from a prior call to the same query mode.

### SamplingMode for walk_causation

Pass a JSON object as the `sampling` argument:

```typescript
{ kind: "exhaustive" }                         // full BFS (default if absent)
{ kind: "breadthFirst", maxPerLevel: 50 }      // BFS capped per level
{ kind: "adaptive", targetCount: 200 }         // adaptive cap
```

### fossic_aggregate_bounded

Collects events across streams matching a glob pattern and accumulates them into a flat result array. Unlike the other bounded commands, `next_cursor` is always absent on truncation — fold-resume requires aggregator-state injection, deferred to v1.2.x.

```typescript
const result = await invoke('fossic_aggregate_bounded', {
    streamPattern: 'cerebra/lattice/*',
    maxEventsScan: 5000,
})
// result.kind: "complete" | "truncated"
// result.results: SerializedEvent[]
```

### Streaming limitation

Tauri IPC is request-response only — there is no native push-event stream for large result sets. Use bounded commands with cursor resumption as the equivalent of streaming. For real-time event delivery, `fossic_subscribe` + the `fossic:event` Tauri event is the correct approach.

## Push events

`fossic_subscribe` registers a fossic subscription and, on each event, calls `app_handle.emit("fossic:event", payload)`. The payload shape:

```typescript
{
  subscription_id: string;
  event: SerializedEvent;
}
```

Frontend listeners use `listen("fossic:event")` from `@tauri-apps/api/event`.

**Note:** `fossic_subscribe` accepts an exact `stream_id` (not a glob pattern) in v1.

## Payload format

The IPC boundary is JSON. `SerializedEvent.payload` is a JSON value decoded from the stored msgpack on the Rust side before crossing the IPC boundary.

## Test helpers

Enable the `test-helpers` feature flag to expose `fossic_dispatch_test_event`, a command that injects a synthetic event for testing push-notification flows without a real write.

## Cargo.toml

```toml
[dependencies]
fossic-tauri = { path = "path/to/crates/fossic-tauri" }
```

## License

MIT
