import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync } from 'node:fs'
import { describe, it, expect, afterEach } from 'vitest'
import { Store } from '../index.js'

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

describe('lifecycle', () => {
  it('opens a new store', () => {
    const store = Store.open(tempDb())
    expect(store).toBeDefined()
  })

  it('opens with explicit create_if_missing policy', () => {
    const store = Store.open(tempDb(), { onFirstOpen: 'create_if_missing' })
    expect(store).toBeDefined()
  })

  it('fails when fail_if_not_found and file missing', () => {
    expect(() => {
      Store.open(tempDb() + '-nonexistent', { onFirstOpen: 'fail_if_not_found' })
    }).toThrow()
  })

  it('declares and reads back a stream', async () => {
    const store = Store.open(tempDb())
    await store.declareStream('lifecycle/events', 'test-suite')
    const all = await store.streams()
    const ids = all.map(s => s.id)
    expect(ids).toContain('lifecycle/events')
  })

  it('stream_exists returns false for undeclared streams', async () => {
    const store = Store.open(tempDb())
    expect(await store.streamExists('no/such/stream')).toBe(false)
  })

  it('close() is safe to call explicitly', () => {
    const store = Store.open(tempDb())
    expect(() => store.close()).not.toThrow()
  })

  it('fossicVersion() returns a semver string', async () => {
    const { fossicVersion } = await import('../index.js')
    expect(fossicVersion()).toMatch(/^\d+\.\d+\.\d+/)
  })
})
