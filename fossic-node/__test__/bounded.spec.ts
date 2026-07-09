// SPDX-License-Identifier: Apache-2.0
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync } from 'node:fs'
import { describe, it, expect, afterEach } from 'vitest'
import { Store, EventId, TruncationCursor, SamplingMode } from '../index.js'

let tmpDirs: string[] = []

function tempDb(): string {
  const dir = mkdtempSync(join(tmpdir(), 'fossic-bounded-'))
  tmpDirs.push(dir)
  return join(dir, 'test.db')
}

afterEach(() => {
  for (const d of tmpDirs) {
    try { rmSync(d, { recursive: true }) } catch { /* noop */ }
  }
  tmpDirs = []
})

async function appendN(store: any, n: number, stream = 's') {
  await store.declareStream(stream, 'test')
  for (let i = 0; i < n; i++) {
    await store.append({ streamId: stream, eventType: 'Evt', payload: { i } })
  }
}

async function appendCorrelated(store: any, n: number): Promise<EventId> {
  await store.declareStream('corr', 'test')
  const root = await store.append({ streamId: 'corr', eventType: 'Root', payload: {} })
  for (let i = 0; i < n; i++) {
    await store.append({ streamId: 'corr', eventType: 'Child', payload: { i }, correlationId: root.toHex() })
  }
  return root
}

// ── TruncationCursor round-trip ───────────────────────────────────────────────

describe('TruncationCursor', () => {
  it('round-trips bytes through toBytes / fromBytes', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 5)
    const outcome = await store.readRangeBounded({ streamId: 's' }, 2)
    expect(outcome.kind).toBe('truncated')
    const cursor = (outcome as any).nextCursor
    expect(cursor).not.toBeNull()
    const buf = cursor.toBytes()
    expect(buf).toBeInstanceOf(Buffer)
    const reconstructed = TruncationCursor.fromBytes(buf)
    expect(reconstructed.toBytes()).toEqual(buf)
  })

  it('empty bytes round-trip', () => {
    const c = TruncationCursor.fromBytes(Buffer.alloc(0))
    expect(c.toBytes().length).toBe(0)
  })
})

// ── SamplingMode constructors ─────────────────────────────────────────────────

describe('SamplingMode', () => {
  it('exhaustive has correct kind', () => {
    const m = SamplingMode.exhaustive()
    expect(m.kind).toBe('exhaustive')
  })

  it('breadthFirst carries maxPerLevel', () => {
    const m = SamplingMode.breadthFirst(10)
    expect(m.kind).toBe('breadthFirst')
    expect((m as any).maxPerLevel).toBe(10)
  })

  it('adaptive carries targetCount', () => {
    const m = SamplingMode.adaptive(250)
    expect(m.kind).toBe('adaptive')
    expect((m as any).targetCount).toBe(250)
  })
})

// ── ReadOutcome shape ─────────────────────────────────────────────────────────

describe('ReadOutcome shape', () => {
  it('complete outcome has correct properties', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 3)
    const outcome = await store.readRangeBounded({ streamId: 's' })
    expect(outcome.kind).toBe('complete')
    expect(outcome.results.length).toBe(3)
    // Option<T> fields on napi objects serialize to undefined (not null) when None
    expect((outcome as any).reason).toBeFalsy()
    expect((outcome as any).nextCursor).toBeFalsy()
  })

  it('truncated outcome has correct properties', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 5)
    const outcome = await store.readRangeBounded({ streamId: 's' }, 2)
    expect(outcome.kind).toBe('truncated')
    expect(outcome.results.length).toBe(2)
    expect((outcome as any).reason).toBe('result_count')
    expect((outcome as any).nextCursor).not.toBeNull()
  })
})

// ── readRangeBounded ──────────────────────────────────────────────────────────

describe('readRangeBounded', () => {
  it('no budget returns complete', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 5)
    const outcome = await store.readRangeBounded({ streamId: 's' })
    expect(outcome.kind).toBe('complete')
    expect(outcome.results.length).toBe(5)
  })

  it('truncates at result count', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 10)
    const outcome = await store.readRangeBounded({ streamId: 's' }, 3)
    expect(outcome.kind).toBe('truncated')
    expect(outcome.results.length).toBe(3)
    expect((outcome as any).reason).toBe('result_count')
  })

  it('complete when exactly at limit', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 5)
    const outcome = await store.readRangeBounded({ streamId: 's' }, 5)
    expect(outcome.kind).toBe('complete')
    expect(outcome.results.length).toBe(5)
  })

  it('truncates at byte budget (at-least-one guarantee)', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 10)
    const outcome = await store.readRangeBounded({ streamId: 's' }, undefined, 1)
    expect(outcome.kind).toBe('truncated')
    expect(outcome.results.length).toBe(1)
    expect((outcome as any).reason).toBe('byte_size')
  })

  it('resumes from cursor correctly', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 6)
    const page1 = await store.readRangeBounded({ streamId: 's' }, 3)
    expect(page1.kind).toBe('truncated')
    const versions1 = page1.results.map((e: any) => e.version)
    expect(versions1).toEqual([0n, 1n, 2n])

    const page2 = await store.readRangeBounded({ streamId: 's' }, 3, undefined, (page1 as any).nextCursor)
    const versions2 = page2.results.map((e: any) => e.version)
    expect(versions2).toEqual([3n, 4n, 5n])
  })

  it('full pagination collects all events', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 7)
    const allVersions: bigint[] = []
    let cursor: any = undefined
    while (true) {
      const outcome = await store.readRangeBounded({ streamId: 's' }, 3, undefined, cursor)
      allVersions.push(...outcome.results.map((e: any) => e.version))
      if (outcome.kind === 'complete') break
      cursor = (outcome as any).nextCursor
    }
    expect(allVersions).toEqual([0n, 1n, 2n, 3n, 4n, 5n, 6n])
  })

  it('uses defaultMaxResults from OpenOptions', async () => {
    const store = Store.open(tempDb(), { defaultMaxResults: 2 })
    await appendN(store, 5)
    const outcome = await store.readRangeBounded({ streamId: 's' })
    expect(outcome.kind).toBe('truncated')
    expect(outcome.results.length).toBe(2)
    expect((outcome as any).reason).toBe('result_count')
  })
})

// ── readByCorrelationBounded ──────────────────────────────────────────────────

describe('readByCorrelationBounded', () => {
  it('no budget returns complete', async () => {
    const store = Store.open(tempDb())
    const root = await appendCorrelated(store, 4)
    const outcome = await store.readByCorrelationBounded(root)
    expect(outcome.kind).toBe('complete')
    expect(outcome.results.length).toBe(4)
  })

  it('truncates at result count', async () => {
    const store = Store.open(tempDb())
    const root = await appendCorrelated(store, 6)
    const outcome = await store.readByCorrelationBounded(root, 3)
    expect(outcome.kind).toBe('truncated')
    expect(outcome.results.length).toBe(3)
  })

  it('paginates to collect all correlated events', async () => {
    const store = Store.open(tempDb())
    const root = await appendCorrelated(store, 6)
    const allIds: string[] = []
    let cursor: any = undefined
    while (true) {
      const outcome = await store.readByCorrelationBounded(root, 3, undefined, cursor)
      allIds.push(...outcome.results.map((e: any) => e.id))
      if (outcome.kind === 'complete') break
      cursor = (outcome as any).nextCursor
    }
    expect(allIds.length).toBe(6)
    const sorted = [...allIds].sort()
    expect(allIds).toEqual(sorted)
  })

  it('lone event returns complete empty', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('lone', 'test')
    const lone = await store.append({ streamId: 'lone', eventType: 'Lone', payload: {} })
    const outcome = await store.readByCorrelationBounded(lone, 10)
    expect(outcome.kind).toBe('complete')
    expect(outcome.results.length).toBe(0)
  })

  it('wrong cursor type returns error', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 3)
    const root = await appendCorrelated(store, 3)
    const rangeOutcome = await store.readRangeBounded({ streamId: 's' }, 1)
    expect(rangeOutcome.kind).toBe('truncated')
    const rangeCursor = (rangeOutcome as any).nextCursor
    await expect(
      store.readByCorrelationBounded(root, 10, undefined, rangeCursor)
    ).rejects.toThrow()
  })
})

// ── Streaming iterators ───────────────────────────────────────────────────────

describe('readRangeIter', () => {
  it('collects all events via for-await', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 5)
    const versions: bigint[] = []
    for await (const ev of store.readRangeIter({ streamId: 's' })) {
      versions.push((ev as any).version)
    }
    expect(versions).toEqual([0n, 1n, 2n, 3n, 4n])
  })

  it('empty stream yields nothing', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('empty', 'test')
    const items: any[] = []
    for await (const ev of store.readRangeIter({ streamId: 'empty' })) {
      items.push(ev)
    }
    expect(items.length).toBe(0)
  })

  it('crosses batch boundary without gaps', async () => {
    const store = Store.open(tempDb())
    await appendN(store, 105)
    const versions: bigint[] = []
    for await (const ev of store.readRangeIter({ streamId: 's' })) {
      versions.push((ev as any).version)
    }
    expect(versions.length).toBe(105)
    for (let i = 0; i < 105; i++) {
      expect(versions[i]).toBe(BigInt(i))
    }
  })
})

describe('readByCorrelationIter', () => {
  it('collects all correlated events', async () => {
    const store = Store.open(tempDb())
    const root = await appendCorrelated(store, 6)
    let count = 0
    for await (const _ of store.readByCorrelationIter(root)) {
      count++
    }
    expect(count).toBe(6)
  })
})

describe('walkCausationIter', () => {
  it('forward collects descendants', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('chain', 'test')
    const e0 = await store.append({ streamId: 'chain', eventType: 'N', payload: {} })
    let prev = e0
    for (let i = 1; i <= 4; i++) {
      prev = await store.append({ streamId: 'chain', eventType: 'N', payload: { i }, causationId: prev.toHex() })
    }
    let count = 0
    for await (const _ of store.walkCausationIter(e0, 'forward', 10, SamplingMode.exhaustive())) {
      count++
    }
    expect(count).toBe(4)
  })
})
