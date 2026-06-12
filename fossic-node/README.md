# fossic-node

napi-rs Node.js bindings for [fossic](../README.md), the local-first event sourcing library.

This package is for **Node.js consumers** (tests, standalone services, time-travel demos). It is **not** for use inside a Tauri webview — see [fossic-tauri](../crates/fossic-tauri/README.md) for Tauri IPC commands.

## Installation

Built with [@napi-rs/cli](https://napi.rs):

```sh
npm install
npm run build
```

Pre-built binaries are published to npm for common targets via the `npm/` directory.

## Quick start

```typescript
import * as os from 'os';
import * as path from 'path';
import { Store, SubscriptionMode } from 'fossic';

// fossic does not expand tilde paths — resolve to an absolute path first.
const store = await Store.open({
  path: path.join(os.homedir(), '.fossic/store.db'),
  options: { encryption: 'plaintext', onFirstOpen: 'create_if_missing' },
});

await store.declareStream('lumaweave/graph', { declaredBy: 'lumaweave' });

const eventId = await store.append({
  streamId: 'lumaweave/graph',
  eventType: 'PhysicsDialectChanged',
  typeVersion: 1,
  payload: { previous: 'radial-backbone', next: 'parallel-spines' },
});

// Subscription is AsyncIterable and supports TC39 explicit resource management.
const sub = store.subscribe({
  streamPattern: 'lumaweave/graph',
  mode: SubscriptionMode.postCommit(1024),
});

try {
  for await (const event of sub) {
    console.log(event.eventType, event.version);
  }
} finally {
  sub.unsubscribe();
}
```

## Type notes

- `version` and `EventId` cross the napi boundary as `bigint` and `Uint8Array` respectively.
- `store.subscribe()` returns a `FossicSubscription` directly (synchronous) — subscription registration is in-memory, no async I/O needed.

## Tests

```sh
npm test
```

Tests are written with [vitest](https://vitest.dev) and live in `__test__/`.

## Requirements

- Node.js 18+
- Rust stable toolchain (only needed to build from source)

## License

MIT OR Apache-2.0
