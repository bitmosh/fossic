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

async function withStream(name = 'test/events') {
  const store = Store.open(tempDb())
  await store.declareStream(name, 'test-suite')
  return store
}

describe('append + read', () => {
  it('append returns an EventId', async () => {
    const store = await withStream()
    const id = await store.append(uniqueEv('test/events'))
    expect(id).toBeInstanceOf(EventId)
    expect(id.toHex()).toMatch(/^[0-9a-f]{64}$/)
  })

  it('identical events return same ID (CCE dedup)', async () => {
    const store = await withStream()
    const ev = { streamId: 'test/events', eventType: 'Stable', payload: { x: 1 } }
    const id1 = await store.append(ev)
    const id2 = await store.append(ev)
    expect(id1.toHex()).toBe(id2.toHex())
  })

  it('unique_ev() payloads produce distinct IDs', async () => {
    const store = await withStream()
    const id1 = await store.append(uniqueEv('test/events'))
    const id2 = await store.append(uniqueEv('test/events'))
    expect(id1.toHex()).not.toBe(id2.toHex())
  })

  it('read_range returns appended events in order', async () => {
    const store = await withStream()
    await store.append(uniqueEv('test/events'))
    await store.append(uniqueEv('test/events'))
    await store.append(uniqueEv('test/events'))

    const events = await store.readRange({ streamId: 'test/events' })
    expect(events.length).toBe(3)
    // versions are monotonically increasing BigInts
    for (let i = 1; i < events.length; i++) {
      expect(events[i].version > events[i - 1].version).toBe(true)
    }
  })

  it('read_range from_version filters correctly', async () => {
    const store = await withStream()
    for (let i = 0; i < 5; i++) await store.append(uniqueEv('test/events'))
    const events = await store.readRange({ streamId: 'test/events' })
    const midVersion = events[2].version

    const tail = await store.readRange({
      streamId: 'test/events',
      fromVersion: midVersion,
    })
    expect(tail.length).toBe(3) // versions 2, 3, 4 (0-indexed)
    expect(tail[0].version).toBe(midVersion)
  })

  it('read_range limit caps result count', async () => {
    const store = await withStream()
    for (let i = 0; i < 10; i++) await store.append(uniqueEv('test/events'))
    const events = await store.readRange({ streamId: 'test/events', limit: 3 })
    expect(events.length).toBe(3)
  })

  it('read_one retrieves a known event', async () => {
    const store = await withStream()
    const id = await store.append(uniqueEv('test/events'))
    const event = await store.readOne(id)
    expect(event).not.toBeNull()
    expect(event!.id).toBe(id.toHex())
  })

  it('read_one returns null for unknown ID', async () => {
    const store = await withStream()
    const unknownId = EventId.fromHex('a'.repeat(64))
    const event = await store.readOne(unknownId)
    expect(event).toBeNull()
  })

  it('external_id round-trips', async () => {
    const store = await withStream()
    await store.append({
      ...uniqueEv('test/events'),
      externalId: 'ext-abc-123',
    })
    const event = await store.readByExternalId('test/events', 'ext-abc-123')
    expect(event).not.toBeNull()
    expect(event!.externalId).toBe('ext-abc-123')
  })

  it('append_batch inserts multiple events atomically', async () => {
    const store = await withStream()
    const ids = await store.appendBatch([
      uniqueEv('test/events'),
      uniqueEv('test/events'),
      uniqueEv('test/events'),
    ])
    expect(ids.length).toBe(3)
    const unique = new Set(ids.map(id => id.toHex()))
    expect(unique.size).toBe(3)
  })

  it('EventId.fromHex roundtrips via toBytes', () => {
    const hex = 'ab'.repeat(32)
    const id = EventId.fromHex(hex)
    expect(id.toHex()).toBe(hex)
    const bytes = id.toBytes()
    expect(bytes.length).toBe(32)
    const id2 = new EventId(bytes)
    expect(id2.toHex()).toBe(hex)
  })

  it('append to undeclared stream throws', async () => {
    const store = Store.open(tempDb())
    await expect(store.append(uniqueEv('no/such/stream'))).rejects.toThrow()
  })
})
