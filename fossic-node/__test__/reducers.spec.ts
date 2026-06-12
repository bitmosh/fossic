import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync } from 'node:fs'
import { describe, it, expect, afterEach } from 'vitest'
import { Store, FossicReducer, FossicError, FossicErrorCode } from '../index.js'

let tmpDirs: string[] = []

function tempDb(): string {
  const dir = mkdtempSync(join(tmpdir(), 'fossic-test-'))
  tmpDirs.push(dir)
  return join(dir, 'test.db')
}

afterEach(() => {
  for (const d of tmpDirs) {
    try { rmSync(d, { recursive: true }) } catch { /* noop */ }
  }
  tmpDirs = []
})

// Simple counter reducer using UTF-8 JSON for state serialization.
// The state is a JSON string: { "count": number }
const encoder = new TextEncoder()
const decoder = new TextDecoder()

const counterReducer: FossicReducer = {
  name: 'counter',
  version: 1,
  stateSchemaVersion: 1,
  initialState(): Uint8Array {
    return encoder.encode(JSON.stringify({ count: 0 }))
  },
  apply(state: Uint8Array, _event: unknown): Uint8Array {
    const s = JSON.parse(decoder.decode(state)) as { count: number }
    return encoder.encode(JSON.stringify({ count: s.count + 1 }))
  },
}

function decodeState(bytes: Uint8Array): { count: number } {
  return JSON.parse(decoder.decode(bytes)) as { count: number }
}

function withStore() {
  const store = Store.open(tempDb())
  return store
}

// seq must be unique per append to avoid CCE dedup (CCE hashes type+payload, not stream_id).
async function appendEvent(store: Store, streamId: string, seq: number = 0) {
  await store.append({ streamId, eventType: 'test.event', payload: { seq } })
}

describe('JS-side reducer API', () => {
  it('readState returns initial state when no events', async () => {
    const store = withStore()
    store.registerReducer('reducer/**', counterReducer)
    await store.declareStream('reducer/empty', 'test')
    const state = await store.readState('reducer/empty')
    expect(decodeState(state).count).toBe(0)
  })

  it('readState counts events correctly', async () => {
    const store = withStore()
    store.registerReducer('reducer/**', counterReducer)
    await store.declareStream('reducer/count', 'test')

    for (let i = 0; i < 5; i++) {
      await appendEvent(store, 'reducer/count', i)
    }

    const state = await store.readState('reducer/count')
    expect(decodeState(state).count).toBe(5)
  })

  it('readStateAtVersion folds only up to the given version', async () => {
    const store = withStore()
    store.registerReducer('reducer/**', counterReducer)
    await store.declareStream('reducer/versioned', 'test')

    for (let i = 0; i < 5; i++) {
      await appendEvent(store, 'reducer/versioned', i)
    }

    // Events are 0-indexed (v=0..4). version=2n includes v=0,1,2 = 3 events.
    const state = await store.readStateAtVersion('reducer/versioned', 'main', 2n)
    expect(decodeState(state).count).toBe(3)
  })

  it('readState throws FossicError.ReducerNotFound when no reducer registered', async () => {
    const store = withStore()
    try {
      await store.readState('unregistered/stream')
      expect.fail('should have thrown')
    } catch (e) {
      expect(e).toBeInstanceOf(FossicError)
      expect((e as FossicError).code).toBe(FossicErrorCode.ReducerNotFound)
    }
  })

  it('snapshot round-trip: writeSnapshotState + readState uses snapshot', async () => {
    const store = withStore()
    store.registerReducer('reducer/**', counterReducer)
    await store.declareStream('reducer/snap', 'test')

    for (let i = 0; i < 3; i++) {
      await appendEvent(store, 'reducer/snap', i)
    }

    // Events are 0-indexed: 3 appends → v=0,1,2. Snapshot at v=2 (last event).
    const stateAt2 = await store.readStateAtVersion('reducer/snap', 'main', 2n)
    await store.writeSnapshotState(
      'reducer/snap',
      'main',
      2n,
      counterReducer.name,
      counterReducer.version,
      counterReducer.stateSchemaVersion,
      Buffer.from(stateAt2),
    )

    // Append 2 more events → v=3, v=4
    for (let i = 3; i < 5; i++) {
      await appendEvent(store, 'reducer/snap', i)
    }

    // readState starts from snapshot (count=3, fromVersion=3n), applies v=3 and v=4 → count=5
    const finalState = await store.readState('reducer/snap')
    expect(decodeState(finalState).count).toBe(5)
  })

  it('getSnapshotState returns null when no snapshot', async () => {
    const store = withStore()
    await store.declareStream('reducer/nosnap', 'test')
    const snap = await store.getSnapshotState(
      'reducer/nosnap',
      'main',
      'counter',
      1,
    )
    expect(snap).toBeNull()
  })

  it('registerReducer supports multiple patterns', async () => {
    const store = withStore()
    store.registerReducer('aaa/**', counterReducer)
    store.registerReducer('bbb/**', counterReducer)

    await store.declareStream('aaa/events', 'test')
    await store.declareStream('bbb/events', 'test')

    // Use distinct seq values to avoid cross-stream CCE dedup (CCE hashes type+payload, not stream_id).
    await appendEvent(store, 'aaa/events', 0)
    await appendEvent(store, 'aaa/events', 1)
    await appendEvent(store, 'bbb/events', 2)

    const a = decodeState(await store.readState('aaa/events'))
    const b = decodeState(await store.readState('bbb/events'))
    expect(a.count).toBe(2)
    expect(b.count).toBe(1)
  })
})
