---
pass: v1.1.7
version: v1.1.7
date: 2026-06-21
prior-commit: 64075a9
summary: Node binding surface — ReadOutcome discriminated union, TruncationCursor, SamplingMode, bounded reads, streaming async iterables, defaultMaxResults/defaultMaxBytes in OpenOptions
---

# Blast Radius — Pass v1.1.7

## Files

### Created
- `fossic-node/src/iters.rs` — `FossicRangeIter`, `FossicCorrelationIter`, `FossicCausationIter` async iterator wrappers
- `fossic-node/__test__/bounded.spec.ts` — 24-test vitest parity suite against Rust + Python suites

### Modified
- `fossic-node/src/types.rs` — `TruncationCursorJs` (napi class); `SamplingModeJs` (napi object) + `parse_sampling_mode`; `ReadOutcomeJs` (napi object) + `ReadOutcomeJs::from_outcome`; `OpenOptionsJs` extended with `default_max_results` / `default_max_bytes`; `parse_open_options` wires new fields
- `fossic-node/src/store.rs` — `parse_direction` helper extracted from inline walk_causation dispatch; bounded methods (`read_range_bounded`, `read_by_correlation_bounded`, `walk_causation_bounded`); iterator constructors (`read_range_iter`, `read_by_correlation_iter`, `walk_causation_iter`)
- `fossic-node/src/lib.rs` — `mod iters` added; six new types re-exported
- `fossic-node/index.d.ts` — full TypeScript declarations regenerated: `ReadOutcome` discriminated union, `TruncationCursor` class, `SamplingMode` namespace, `FossicRangeIter/CorrelationIter/CausationIter` async iterable classes, updated `Store` method signatures, `OpenOptions` additions
- `CHANGELOG.md` — v1.1.7 section added

### Gitignored (not committed)
- `fossic-node/index.js` — JS wrapper layer (gitignored by repo root `.gitignore`); patched with `TruncationCursor` alias, iterator JS wrappers, bounded-method cursor↔Buffer conversion helpers, `SamplingMode` namespace, updated `module.exports`

---

## Changes

### ReadOutcome — TypeScript discriminated union

Node callers branch on `kind` to determine completeness:

```typescript
const outcome = await store.readRangeBounded(query, 100)
if (outcome.kind === 'truncated') {
  const next = await store.readRangeBounded(query, 100, undefined, outcome.nextCursor)
} else {
  process(outcome.results)
}
```

Properties:
- `.kind` — `'complete'` | `'truncated'`
- `.results` — `StoredEvent[]`, always present
- `.reason` — `'result_count'` | `'byte_size'` | `null` (null when complete)
- `.nextCursor` — `TruncationCursor | null` (null when complete)

Data-shaped, not a class with methods — idiomatic TypeScript.

### TruncationCursor — opaque class

Node callers see only two operations: `.toBytes() → Buffer` and the static `TruncationCursor.fromBytes(buf: Buffer) → TruncationCursor`. The Range/Correlation/Causation variant discriminator stays in Rust bytes; callers pass cursors back opaquely. A wrong-type cursor raises `FossicError` at the Rust boundary.

**napi-rs constraint:** `Option<TruncationCursorJs>` cannot be embedded directly in a `#[napi(object)]` struct (trait bound mismatch). The `ReadOutcomeJs` Rust struct stores `next_cursor: Option<Buffer>` (raw bytes). The JS layer (`index.js`) wraps the returned `Buffer` in a `TruncationCursor` instance and unwraps `TruncationCursor` back to `Buffer` before bounded calls. The TypeScript declarations expose `TruncationCursor` throughout — the Buffer plumbing is transparent to callers.

### SamplingMode — namespace with constructor functions

Mirrors the Rust `SamplingMode` enum without exposing variant names:

```typescript
SamplingMode.exhaustive()
SamplingMode.breadthFirst(maxPerLevel)
SamplingMode.adaptive(targetCount)
```

Pure JS — no Rust binding needed for construction. Rust receives the tagged object and `parse_sampling_mode` dispatches.

### Bounded read methods on Store

Three new async methods mirror the Rust bounded read signatures, all parameters optional:

```typescript
store.readRangeBounded(query, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
store.readByCorrelationBounded(correlationId, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
store.walkCausationBounded(start, direction, maxDepth?, sampling?, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
```

All use `tokio::task::spawn_blocking` — same pattern as existing Store methods.

### Streaming async iterables

Three iterator constructors return `AsyncIterable<StoredEvent>`:

```typescript
store.readRangeIter(query)              // → FossicRangeIter
store.readByCorrelationIter(corrId)     // → FossicCorrelationIter
store.walkCausationIter(start, ...)     // → FossicCausationIter
```

`for await (const ev of store.readRangeIter(query))` works directly.

Each iterator wraps its Rust counterpart in `Arc<Mutex<Option<T>>>`. The async `rawNext()` method drives `spawn_blocking` one step at a time; the JS wrapper converts `null` to `{ done: true }`. Pool connections are released between yields — same invariant as v1.1.5.

### OpenOptions additions

`defaultMaxResults?: number` and `defaultMaxBytes?: number` exposed in `OpenOptionsJs`, wired to the Rust `OpenOptions` budget fields. Fixes the CP-FOSSIC-3 gap carried from v1.1.6 (Python) — not repeated here.

### parse_direction helper

The direction-string → `WalkDirection` dispatch was previously inline in `walk_causation`. Extracted to a private `parse_direction(s: &str) -> Result<WalkDirection>` helper shared by `walk_causation`, `walk_causation_bounded`, and `walk_causation_iter`.

---

## Public API additions (fossic-node)

**New types (all exported from `fossic-node`):**
- `ReadOutcome` — TypeScript discriminated union type
- `TruncationCursor` — opaque class; `.toBytes()` / static `.fromBytes(buf)`
- `SamplingMode` — namespace with constructor functions
- `FossicRangeIter` — `AsyncIterable<StoredEvent>`
- `FossicCorrelationIter` — same
- `FossicCausationIter` — same

**New methods on `Store`:**
```
Store.readRangeBounded(query, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
Store.readByCorrelationBounded(correlationId, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
Store.walkCausationBounded(start, direction, maxDepth?, sampling?, maxResults?, maxBytes?, cursor?) → Promise<ReadOutcome>
Store.readRangeIter(query) → FossicRangeIter
Store.readByCorrelationIter(correlationId) → FossicCorrelationIter
Store.walkCausationIter(start, direction, maxDepth?, sampling?) → FossicCausationIter
```

**OpenOptions additions:**
```
OpenOptions.defaultMaxResults?: number
OpenOptions.defaultMaxBytes?: number
```

**No breaking changes.** All new methods are additive. Existing method signatures unchanged.

---

## Test coverage (`fossic-node/__test__/bounded.spec.ts`)

| Test | Asserts |
|---|---|
| `TruncationCursor > round-trips bytes through toBytes / fromBytes` | cursor → `.toBytes()` → `fromBytes()` → `.toBytes()` is identity |
| `TruncationCursor > empty bytes round-trip` | empty bytes round-trips |
| `SamplingMode > exhaustive has correct kind` | `kind === 'exhaustive'` |
| `SamplingMode > breadthFirst carries maxPerLevel` | `kind === 'breadthFirst'`, `maxPerLevel` preserved |
| `SamplingMode > adaptive carries targetCount` | `kind === 'adaptive'`, `targetCount` preserved |
| `ReadOutcome shape > complete outcome has correct properties` | `.kind`, `.results.length`, `.reason`, `.nextCursor` all correct for Complete |
| `ReadOutcome shape > truncated outcome has correct properties` | `.kind`, `.results.length`, `.reason === 'result_count'`, `.nextCursor` present |
| `readRangeBounded > no budget returns complete` | no-budget call returns Complete with all 5 events |
| `readRangeBounded > truncates at result count` | 10 events, limit 3 → Truncated with 3 |
| `readRangeBounded > complete when exactly at limit` | 5 events, limit 5 → Complete |
| `readRangeBounded > truncates at byte budget` | 1-byte budget → Truncated with 1 event |
| `readRangeBounded > resumes from cursor correctly` | page 1 versions [0n,1n,2n], page 2 versions [3n,4n,5n] |
| `readRangeBounded > full pagination collects all events` | 7 events paginated at 3 → all 7 versions in order |
| `readRangeBounded > uses defaultMaxResults from OpenOptions` | store-level default respected |
| `readByCorrelationBounded > no budget returns complete` | 4 correlated events, no budget → Complete with 4 |
| `readByCorrelationBounded > truncates at result count` | 6 correlated, limit 3 → Truncated with 3 |
| `readByCorrelationBounded > paginates to collect all correlated events` | 6 events paginated at 3 → 6 unique ids in ascending order |
| `readByCorrelationBounded > lone event returns complete empty` | lone event with no siblings → Complete(0) |
| `readByCorrelationBounded > wrong cursor type returns error` | range cursor passed to correlation query → rejects |
| `readRangeIter > collects all events via for-await` | 5 events, versions [0n..4n] |
| `readRangeIter > empty stream yields nothing` | 0 items |
| `readRangeIter > crosses batch boundary without gaps` | 105 events, no gaps or duplicates |
| `readByCorrelationIter > collects all correlated events` | 6 events all collected |
| `walkCausationIter > forward collects descendants` | 4-level chain yields 4 descendants |

---

## Tech debt / polish debt

**`aggregate_bounded` not exposed.**
Same as Python — aggregate requires a JS callable that can cross the Rust boundary. Deferred.

**`FusedIterator` semantics not formally exposed.**
The JS wrappers set `_done = true` after the first `null` from `rawNext()` and return `{ done: true }` immediately thereafter — functionally fused. No formal `Symbol.iterator` contract needed in JS for this guarantee.

**`index.js` is gitignored.**
The JS wrapper layer lives on disk but is not committed. Fresh clones need a build step to regenerate it. This is a pre-existing repo configuration; not changed here.

---

## Adjacent project notifications

fossic-tauri unaffected (no Rust API surface changed — v1.1.7 only adds to the Node binding layer).
