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

## Bounded reads and streaming iterators

### Why bounded variants exist

`readRange` loads all matching events into memory. The bounded variants accept a `maxResults` and/or `maxBytes` budget and return a `ReadOutcome` discriminated union — callers know immediately whether the result set is complete or was cut short, and can resume from the returned cursor.

### ReadOutcome

```typescript
import { Store, SamplingMode, TruncationCursor } from 'fossic'

const outcome = await store.readRangeBounded(
    { streamId: 'cerebra/lattice/session_42' },  // ReadQuery
    1000,     // maxResults
)

if (outcome.kind === 'complete') {
    processAll(outcome.results)
} else {
    // outcome.kind === 'truncated'
    processPage(outcome.results)
    // outcome.reason: 'result_count' | 'byte_size'
    // outcome.nextCursor: TruncationCursor | null

    if (outcome.nextCursor) {
        const nextPage = await store.readRangeBounded(
            { streamId: 'cerebra/lattice/session_42' },
            1000,
            undefined,           // maxBytes
            outcome.nextCursor,  // cursor
        )
    }
}
```

### TruncationCursor

Cursors are opaque. Pass them back to the next bounded call. Serialize to `Buffer` and reconstruct:

```typescript
// Serialize for persistence:
const buf = cursor.toBytes()  // Buffer

// Restore:
const cursor = TruncationCursor.fromBytes(buf)
const nextPage = await store.readRangeBounded(query, 1000, undefined, cursor)
```

### SamplingMode

```typescript
SamplingMode.exhaustive()                    // Full BFS (default)
SamplingMode.breadthFirst(50)                // BFS capped at 50 nodes/level
SamplingMode.adaptive(200)                   // Adaptive: approaches 200 total
```

### Streaming iterators

Each `rawNext()` call advances the Rust iterator one step and releases the pool connection before resolving. Use `for await`:

```typescript
for await (const event of store.readRangeIter({ streamId: 'cerebra/lattice/session_42' })) {
    process(event)
}

for await (const event of store.walkCausationIter(
    rootId,
    'forward',
    100,                               // maxDepth
    SamplingMode.exhaustive(),
)) {
    process(event)
}
```

Iterator types implement `AsyncIterable<StoredEvent>` and `Symbol.asyncDispose` (TC39 explicit resource management).

### Bounded methods on Store

```typescript
store.readRangeBounded(query, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
store.readByCorrelationBounded(correlationId, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
store.walkCausationBounded(start, direction, maxDepth?, sampling?, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>

store.readRangeIter(query) → FossicRangeIter           // AsyncIterable<StoredEvent>
store.readByCorrelationIter(correlationId) → FossicCorrelationIter
store.walkCausationIter(start, direction, maxDepth?, sampling?) → FossicCausationIter
```

### OpenOptions defaults

`defaultMaxResults` and `defaultMaxBytes` are exposed in the Node binding from v1.1.7:

```typescript
const store = Store.open('store.db', {
    defaultMaxResults: 10_000,
    defaultMaxBytes: 50 * 1024 * 1024,
})
```

Per-call limits take precedence when provided. These are the only binding where store-level defaults are currently exposed; Python and Tauri will follow.

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
