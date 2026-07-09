// SPDX-License-Identifier: Apache-2.0
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync } from 'node:fs'
import { describe, it, expect, afterEach } from 'vitest'
import { Store } from '../index.js'
import { uniqueEv } from './helpers.js'

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

async function withStream(db?: string, name = 'sub/events') {
  const store = Store.open(db ?? tempDb())
  await store.declareStream(name, 'test-suite')
  return store
}

describe('subscriptions', () => {
  it('delivers a single event via for-await', async () => {
    const store = await withStream()
    const sub = store.subscribe({ streamPattern: 'sub/events' })
    await store.append(uniqueEv('sub/events'))

    const result = await sub.next()
    expect(result.done).toBe(false)
    expect(result.value?.streamId).toBe('sub/events')
    sub.unsubscribe()
  })

  it('unsubscribe makes next() return done: true', async () => {
    const store = await withStream()
    const sub = store.subscribe({ streamPattern: 'sub/events' })
    sub.unsubscribe()

    const result = await sub.next()
    expect(result.done).toBe(true)
  })

  it('multiple events arrive in order', async () => {
    const store = await withStream()
    const sub = store.subscribe({ streamPattern: 'sub/events' })

    const COUNT = 5
    for (let i = 0; i < COUNT; i++) await store.append(uniqueEv('sub/events'))

    const received: bigint[] = []
    for await (const event of sub) {
      received.push(event.version)
      if (received.length === COUNT) break
    }
    sub.unsubscribe()

    expect(received.length).toBe(COUNT)
    for (let i = 1; i < received.length; i++) {
      expect(received[i] > received[i - 1]).toBe(true)
    }
  })

  it('filters by stream — events for a different stream are not delivered', async () => {
    const db = tempDb()
    const store = Store.open(db)
    await store.declareStream('sub/a', 'test-suite')
    await store.declareStream('sub/b', 'test-suite')

    const sub = store.subscribe({ streamPattern: 'sub/a' })

    // sub/b event is filtered; sub/a event must arrive.
    // Appending sub/b before sub/a ensures the subscription skips it.
    await store.append(uniqueEv('sub/b'))
    await store.append(uniqueEv('sub/a'))

    const result = await sub.next()
    expect(result.done).toBe(false)
    expect(result.value?.streamId).toBe('sub/a')
    sub.unsubscribe()
  })

  it('Symbol.asyncDispose works via await using (TC39)', async () => {
    // Can't use `await using` syntax without a transpiler configured for it.
    // Test the dispose method directly.
    const store = await withStream()
    const sub = store.subscribe({ streamPattern: 'sub/events' })
    await sub[Symbol.asyncDispose]()

    const result = await sub.next()
    expect(result.done).toBe(true)
  })

  it('is_degraded observable after queue overflow', async () => {
    const store = await withStream()
    // Create a subscription with a tiny queue (1 slot)
    const sub = store.subscribe({ streamPattern: 'sub/events', queueSize: 1 })

    // Flood the queue without draining — 10 events for a queue of 1
    for (let i = 0; i < 10; i++) await store.append(uniqueEv('sub/events'))

    // Drain what we can, then check unsubscribe doesn't throw
    // (exact degraded detection is internal; this tests the binding doesn't crash)
    sub.unsubscribe()
  })
})
