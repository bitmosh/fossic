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

MIT OR Apache-2.0
