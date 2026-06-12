import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync } from 'node:fs'
import { describe, it, expect, afterEach } from 'vitest'
import { Store, EventId } from '../index.js'
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

describe('cross-stream queries', () => {
  it('read_by_correlation returns all events sharing a correlation ID', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('cross/a', 'test-suite')
    await store.declareStream('cross/b', 'test-suite')

    // Use a fixed known correlation ID
    const corrHex = 'cc'.repeat(32)
    const corrId = EventId.fromHex(corrHex)

    await store.append({
      ...uniqueEv('cross/a'),
      correlationId: corrHex,
    })
    await store.append({
      ...uniqueEv('cross/b'),
      correlationId: corrHex,
    })

    const correlated = await store.readByCorrelation(corrId)
    expect(correlated.length).toBe(2)
    const streams = correlated.map(e => e.streamId)
    expect(streams).toContain('cross/a')
    expect(streams).toContain('cross/b')
  })

  it('walk_causation forward traverses a causal chain', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('causal/chain', 'test-suite')

    // Build a 3-event causal chain: e1 → e2 → e3
    const e1 = await store.append(uniqueEv('causal/chain'))
    const e2 = await store.append({ ...uniqueEv('causal/chain'), causationId: e1.toHex() })
    const e3 = await store.append({ ...uniqueEv('causal/chain'), causationId: e2.toHex() })

    const chain = await store.walkCausation(e1, 'forward')
    const hexIds = chain.map(e => e.id)
    expect(hexIds).toContain(e2.toHex())
    expect(hexIds).toContain(e3.toHex())
  })

  it('walk_causation backward traverses to root', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('causal/back', 'test-suite')

    const e1 = await store.append(uniqueEv('causal/back'))
    const e2 = await store.append({ ...uniqueEv('causal/back'), causationId: e1.toHex() })
    const e3 = await store.append({ ...uniqueEv('causal/back'), causationId: e2.toHex() })

    const chain = await store.walkCausation(e3, 'backward')
    const hexIds = chain.map(e => e.id)
    expect(hexIds).toContain(e1.toHex())
    expect(hexIds).toContain(e2.toHex())
  })

  it('walk_causation max_depth limits traversal', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('causal/depth', 'test-suite')

    // Chain of 5 events
    let prev: EventId | undefined
    for (let i = 0; i < 5; i++) {
      const ev = { ...uniqueEv('causal/depth'), causationId: prev?.toHex() }
      prev = await store.append(ev)
    }
    const root = (await store.readRange({ streamId: 'causal/depth' }))[0]
    const rootId = EventId.fromHex(root.id)

    // With max_depth=2, should only get at most 2 descendants
    const limited = await store.walkCausation(rootId, 'forward', 2)
    expect(limited.length).toBeLessThanOrEqual(2)
  })

  it('cursor get/set roundtrip', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('cur/stream', 'test')
    expect(await store.getCursor('my-consumer', 'cur/stream', 'main')).toBeNull()
    await store.setCursor('my-consumer', 'cur/stream', 'main', 42n)
    expect(await store.getCursor('my-consumer', 'cur/stream', 'main')).toBe(42n)
  })
})
