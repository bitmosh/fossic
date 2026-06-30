---
pass: v1.1.5
version: v1.1.5
date: 2026-06-21
prior-commit: f44a6ba
summary: streaming iterators — RangeIter, CorrelationIter, CausationIter, pool-release invariant
---

# Blast Radius — Pass v1.1.5

## Files

### Created
- `tests/streaming_iters.rs` — 14 tests
- `docs/aseptic/blast-radius/pass-1.1.5.md` — this file

### Modified
- `src/store.rs` — `RangeIter`, `CorrelationIter`, `CausationIter` structs + `Iterator` impls + `FusedIterator` impls; three constructor methods on `Store`; `VecDeque` added to imports
- `src/cross_stream.rs` — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` added to `WalkDirection`
- `src/types.rs` — `#[derive(Clone)]` added to `ReadQuery`
- `src/lib.rs` — `CausationIter`, `CorrelationIter`, `RangeIter` re-exported
- `Cargo.toml` — `[[test]] streaming_iters` entry added
- `CHANGELOG.md` — v1.1.5 section added

---

## Changes

### Critical design invariant: pool connection never held across yield

Each iterator's `fetch_batch()` method calls the corresponding public bounded method
(`read_range_bounded`, `read_by_correlation_bounded`, `walk_causation_bounded`). Those methods
acquire a pool connection, run the query, apply upcasters, and return before `fetch_batch`
returns. By the time `Iterator::next` yields a value to the caller, the connection is back in
the pool.

The alternative — holding a cursor-style streaming query open — would require holding the
connection across yield points. With a pool of size N, this would starve the (N+1)th concurrent
reader until the iterator was dropped.

### Batch-based fetch pattern

```
next():
  if exhausted and buffer empty → None
  if buffer empty:
    fetch_batch()          ← acquires pool conn, fetches ≤100 events, releases conn
    if still empty → exhausted = true, return None
  pop_front from buffer → Some(Ok(event))
```

`ITER_BATCH_SIZE = 100` is an internal constant not observable to callers. It sets the
granularity of pool-connection churn vs. per-event overhead. 100 is the same default page size
used by the bounded read tests and is a reasonable production default.

### Fused contract

All three iterators implement `std::iter::FusedIterator`. The `exhausted: bool` flag is set:
- When a `ReadOutcome::Complete` batch is returned (no more data in the source)
- When a `ReadOutcome::Truncated` batch returns an empty buffer (shouldn't happen but handled)
- When `fetch_batch` returns an error (the iterator stops, the error is surfaced once)

After `exhausted = true` and `buffer.is_empty()`, `next()` returns `None` immediately without
any further pool interaction. The `FusedIterator` marker gives callers and adapters the
standard guarantee.

### `WalkDirection: Clone + Copy`

`WalkDirection` had no derives. `CausationIter` stores a `direction: WalkDirection` field and
passes it to `walk_causation_bounded` (which takes `direction` by value) on every batch fetch.
Added `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` — fieldless enum, zero cost.

### `ReadQuery: Clone`

`RangeIter` stores a `query: ReadQuery` and clones it on every `fetch_batch` call
(bounded method takes ownership). Added `#[derive(Clone)]` to `ReadQuery`. `ReadQuery` holds
`String` and `Option<String>` fields; cloning is cheap for typical stream IDs.

### Reuse of bounded methods

The iterators are thin orchestrators over the existing bounded impls. They add no new SQL
paths. Cursor decoding, upcaster application, direction validation, and budget resolution all
live in the bounded methods and are reused without duplication. The only new logic is the
batch/buffer/exhausted state machine in each iterator's `next()`.

---

## Public API additions

**New types (all re-exported from crate root):**
- `RangeIter` — `Iterator<Item = Result<StoredEvent, Error>>` + `FusedIterator`
- `CorrelationIter` — same
- `CausationIter` — same

**New methods on `Store`:**
```
Store::read_range_iter(query: ReadQuery) -> RangeIter
Store::read_by_correlation_iter(correlation_id: EventId) -> CorrelationIter
Store::walk_causation_iter(
    start: EventId,
    direction: WalkDirection,
    max_depth: usize,
    sampling: SamplingMode,
) -> CausationIter
```

**Breaking changes (small surface):**
- `WalkDirection` now derives `Debug, Clone, Copy, PartialEq, Eq`. Downstream code that
  matched on `WalkDirection` without `#[non_exhaustive]` is unaffected; the derives are
  additive.
- `ReadQuery` now derives `Clone`. Additive.

**No `aggregate_iter`:** aggregate is fold-shaped; iterator semantics don't fit. The `restore()`
gap from v1.1.4 also means resume isn't ready. Deferred to v1.2.x with the restore() design.

---

## Test coverage (`tests/streaming_iters.rs`)

| Test | Asserts |
|---|---|
| `range_iter_empty_stream_returns_no_items` | Empty stream yields nothing |
| `range_iter_collects_all_events_in_version_order` | 5 events, strict ascending versions |
| `range_iter_respects_from_version` | from_version=2 yields only events 2–4 |
| `range_iter_fused_after_exhaustion` | Two None calls after exhaustion |
| `range_iter_across_batch_boundary` | 105 events, no gaps or duplicates at batch=100 boundary |
| `correlation_iter_collects_all_correlated_events` | 6 correlated events all collected |
| `correlation_iter_empty_returns_no_items` | Event with no correlated siblings yields nothing |
| `correlation_iter_fused_after_exhaustion` | None after single event exhausted |
| `correlation_iter_across_batch_boundary` | 105 events across batch boundary |
| `causation_iter_forward_collects_descendants` | 4-level chain yields 4 descendants |
| `causation_iter_empty_returns_no_items` | Leaf node with no children yields nothing |
| `causation_iter_fused_after_exhaustion` | None after max_depth=1 exhausted |
| `causation_iter_respects_max_depth` | max_depth=2 yields exactly 2 events |
| `iterator_releases_pool_connection_between_yields` | pool_size=1 + concurrent reader succeeds — invariant confirmed |

---

## Tech debt / polish debt

**No new entries this pass.**

`ITER_BATCH_SIZE = 100` is hardcoded. A future `IterOptions { batch_size }` struct could make
it configurable per-iterator. Not introduced here (YAGNI).

---

## Adjacent project notifications

No FFI-visible changes. fossic-py, fossic-node, fossic-tauri unaffected.
Iterator types are not yet exposed through any binding layer.
