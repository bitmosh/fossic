---
pass: 8
version: v0.8.0
sha: 26001a1
date: 2026-06-12
summary: napi-rs Node.js binding with TypeScript types and full test suite
---

# Blast Radius — Pass 8 (v0.8.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content substantially rewritten — original bootstrap estimate included
> crates/fossic-tauri alongside the Node binding; fossic-tauri is v0.6.0.
> Actual commit 26001a1 is fossic-node only.

## Files

### Created

20 files in commit 26001a1:

**fossic-node crate:**
- `fossic-node/Cargo.toml` — napi, napi-derive, napi-build crates; fossic path dependency
- `fossic-node/Cargo.lock` — dependency lockfile
- `fossic-node/build.rs` — `napi_build::setup()` entry point
- `fossic-node/src/lib.rs` — napi module entry point and type registrations
- `fossic-node/src/store.rs` — `Store` napi class; async methods via `spawn_blocking`; cursor API
- `fossic-node/src/subscriptions.rs` — napi subscription handle class; base async machinery
- `fossic-node/src/types.rs` — JS-compatible type structs matching Rust core types
- `fossic-node/README.md` — Node.js binding installation and usage guide

**TypeScript / package:**
- `fossic-node/index.d.ts` — generated TypeScript type definitions for all Store methods and return types
- `fossic-node/package.json` — npm package manifest; binary platform resolution config
- `fossic-node/package-lock.json` — npm lockfile
- `fossic-node/tsconfig.json` — TypeScript configuration for type checking
- `fossic-node/tsconfig.test.json` — TypeScript configuration for test runner

**Tests:**
- `fossic-node/__test__/append-read.spec.ts` — append/read round-trip tests
- `fossic-node/__test__/branches.spec.ts` — branch lifecycle tests
- `fossic-node/__test__/cross-stream.spec.ts` — aggregate and correlation tests
- `fossic-node/__test__/helpers.ts` — shared test helpers and fixtures
- `fossic-node/__test__/lifecycle.spec.ts` — stream declare/open tests
- `fossic-node/__test__/reducers.spec.ts` — reducer registration and read_state tests
- `fossic-node/__test__/subscriptions.spec.ts` — subscription delivery tests (load-bearing issues surfaced here; fixed in v0.8.1)

Note: `fossic-node/index.js` (the napi-rs JS wrapper with Symbol.asyncIterator) is gitignored — it is auto-generated and was corrected in v0.8.1 (Pass 8.5).

---

## Public APIs

### Added

Node.js `Store` class (async throughout; all methods return `Promise`):
- `Store.open(path: string) -> Promise<Store>` — open or create store
- `Store.declareStream(streamId, declaredBy, description?) -> Promise<void>`
- `Store.append(a: AppendInput) -> Promise<string>` — returns hex EventId
- `Store.readRange(query: ReadQueryInput) -> Promise<StoredEvent[]>`
- `Store.readOne(eventId: string) -> Promise<StoredEvent | null>`
- `Store.readSince(streamId, branch, fromVersion, limit) -> Promise<StoredEvent[]>`
- `Store.createBranch(b: CreateBranchInput) -> Promise<void>`
- `Store.promoteBranch(streamId, branchId, reason?) -> Promise<void>`
- `Store.markBranchDeadEnd(streamId, branchId, reason?) -> Promise<void>`
- `Store.listBranches(streamId) -> Promise<BranchInfo[]>`
- `Store.resolveChain(streamId, branchId) -> Promise<BranchSegment[]>`
- `Store.registerReducer(pattern: string, reducer: ReducerProtocol) -> Promise<void>` — NOTE: `getCursor` / `setCursor` use simplified single-arg API (TIDYUP B2)
- `Store.readState(streamId, branch) -> Promise<unknown>`
- `Store.takeSnapshot(streamId, branch) -> Promise<SnapshotInfo>`
- `Store.getCursor(name: string) -> Promise<number | null>` — simplified 1-arg API vs 3-arg Rust (TIDYUP B2)
- `Store.setCursor(name: string, value: number) -> Promise<void>` — simplified API
- `Store.aggregate(query: AggregateQueryInput) -> Promise<StoredEvent[]>`
- `Store.readByExternalId(streamId, externalId) -> Promise<StoredEvent | null>`
- `Store.readByCorrelation(correlationId: string) -> Promise<StoredEvent[]>`
- `Store.walkCausation(start: string, direction: string, maxDepth: number) -> Promise<StoredEvent[]>`
- `Store.purgeEvent(eventId: string, confirm: string, reason: string, purgedBy: string) -> Promise<void>`
- `Store.subscribe(streamId: string, branch: string) -> Promise<SubscriptionHandle>`
- `SubscriptionHandle` — napi class with async iterator support (Symbol.asyncIterator bugs present at this version; fixed in v0.8.1)

All Node binding errors throw as `GenericFailure` string — no typed error hierarchy (TIDYUP A1). TypeScript callers cannot distinguish error types without string-parsing.

---

## Schema changes

None — Node binding shares the same SQLite database format as Rust core.

---

## Configuration changes

None at fossic core level. Node consumers install the pre-built `.node` binary via npm.

---

## Dependency changes

In `fossic-node/Cargo.toml`:
- Added: `napi = "2"` with `async` feature — napi-rs runtime
- Added: `napi-derive = "2"` — proc macros for napi class/function export
- Added: `napi-build = "1"` (build-dep) — build script support
- Added: `fossic` (path dependency)
- Added: `tokio` with `rt-multi-thread` and `macros` — napi-rs `spawn_blocking` requires a Tokio runtime in the Node binding (note: fossic CORE has no Tokio; the Node binding wraps fossic in its own Tokio runtime)

---

## Behavior changes

- **Node cursor API simplified:** `getCursor(name)` / `setCursor(name, value)` use a single-arg name that encodes consumer/stream/branch as a composed string. Differs from the 3-arg Rust API. (TIDYUP B2)
- **Error handling is string-only.** All fossic errors arrive as `GenericFailure` in the Node binding. (TIDYUP A1)
- **Subscription Symbol.asyncIterator broken at v0.8.0.** `for await...of` syntax does not work on the subscription handle returned by this version. Fixed in v0.8.1 (Pass 8.5).
- **walk_causation chain traversal bug** present at v0.8.0. Fixed in v0.8.1 (Pass 8.5).

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: Multiple TIDYUP findings (A1, B2) originate from this pass. DV-001 (Symbol.asyncIterator via JS wrapper) originates here and is resolved in v0.8.1.*
