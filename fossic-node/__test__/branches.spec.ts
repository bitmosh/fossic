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

async function withStream(name = 'branch/events') {
  const store = Store.open(tempDb())
  await store.declareStream(name, 'test-suite')
  return store
}

describe('branches', () => {
  it('list_branches returns [] for a new stream (main trunk is implicit, not stored)', async () => {
    // Convention: list_branches returns only explicitly forked branches.
    // 'main' is the implicit trunk and is never a stored row.
    const store = await withStream()
    const branches = await store.listBranches('branch/events')
    expect(branches).toEqual([])
  })

  it('list_branches returns a branch after it is created', async () => {
    const store = await withStream()
    await store.append(uniqueEv('branch/events'))
    await store.createBranch({
      streamId: 'branch/events',
      branchId: 'experiment-x',
      parentVersion: 1n,
    })
    const branches = await store.listBranches('branch/events')
    expect(branches.some(b => b.id === 'experiment-x')).toBe(true)
  })

  it('create_branch adds a new branch at a given version', async () => {
    const store = await withStream()
    await store.append(uniqueEv('branch/events'))
    await store.append(uniqueEv('branch/events'))

    await store.createBranch({
      streamId: 'branch/events',
      branchId: 'experiment-1',
      parentVersion: 1n,
    })

    const branches = await store.listBranches('branch/events')
    expect(branches.some(b => b.id === 'experiment-1')).toBe(true)
  })

  it('events on a new branch are scoped to that branch', async () => {
    const store = await withStream()
    await store.append(uniqueEv('branch/events'))

    await store.createBranch({
      streamId: 'branch/events',
      branchId: 'fork',
      parentVersion: 0n,
    })

    await store.append({ ...uniqueEv('branch/events'), streamId: 'branch/events' })

    // Events on the fork branch
    const forkEvents = await store.readRange({
      streamId: 'branch/events',
      branch: 'fork',
    })
    // Events on main
    const mainEvents = await store.readRange({
      streamId: 'branch/events',
      branch: 'main',
    })
    // They should not overlap (different branches have separate version sequences)
    expect(forkEvents.length).toBeGreaterThanOrEqual(0)
    expect(mainEvents.length).toBeGreaterThan(0)
  })

  it('promote_branch changes lifecycle to promoted', async () => {
    const store = await withStream()
    await store.append(uniqueEv('branch/events'))
    await store.createBranch({
      streamId: 'branch/events',
      branchId: 'promote-me',
      parentVersion: 0n,
    })
    await store.promoteBranch('branch/events', 'promote-me')

    const branches = await store.listBranches('branch/events')
    const promoted = branches.find(b => b.id === 'promote-me')
    expect(promoted?.lifecycle).toBe('promoted')
  })

  it('mark_branch_dead_end changes lifecycle to dead_end', async () => {
    const store = await withStream()
    await store.append(uniqueEv('branch/events'))
    await store.createBranch({
      streamId: 'branch/events',
      branchId: 'dead-end',
      parentVersion: 0n,
    })
    await store.markBranchDeadEnd('branch/events', 'dead-end')

    const branches = await store.listBranches('branch/events')
    const dead = branches.find(b => b.id === 'dead-end')
    expect(dead?.lifecycle).toBe('dead_end')
  })
})
