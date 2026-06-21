---
pass: v1.1.6
version: v1.1.6
date: 2026-06-21
prior-commit: daccd85
summary: Python binding surface — PyReadOutcome, PyTruncationCursor, PySamplingMode, bounded reads, streaming iterators
---

# Blast Radius — Pass v1.1.6

## Files

### Created
- `fossic-py/tests/test_bounded.py` — pytest parity suite against Rust bounded_foundation + bounded_reads

### Modified
- `fossic-py/src/types.rs` — `PyTruncationCursor`, `PySamplingMode`, `PyReadOutcome` added
- `fossic-py/src/store.rs` — `parse_direction` helper; bounded methods (`read_range_bounded`, `read_by_correlation_bounded`, `walk_causation_bounded`); iterator constructors (`read_range_iter`, `read_by_correlation_iter`, `walk_causation_iter`); `PyRangeIter`, `PyCorrelationIter`, `PyCausationIter` wrapper types
- `fossic-py/src/lib.rs` — six new types registered in `_fossic` module
- `fossic-py/python/fossic/__init__.py` — six new names re-exported; six `Store` pass-through methods added; `__all__` updated
- `CHANGELOG.md` — v1.1.6 section added

---

## Changes

### PyReadOutcome — tagged-union shape

Python callers branch on `.is_truncated` rather than matching a tuple or dict:

```python
outcome = store.read_range_bounded(query, max_results=100)
if outcome.is_truncated:
    next_page = store.read_range_bounded(query, max_results=100, cursor=outcome.next_cursor)
else:
    process(outcome.results)
```

Properties:
- `.results` — `list[StoredEvent]`, always present
- `.is_truncated` — bool
- `.complete` — bool (complement of `is_truncated`)
- `.reason` — `str | None`: `"result_count"` | `"byte_size"` | `None`
- `.next_cursor` — `TruncationCursor | None`

### PyTruncationCursor — opaque pass-through

Python callers see only two operations: `.to_bytes() -> bytes` and the classmethod `TruncationCursor.from_bytes(b: bytes) -> TruncationCursor`. The Range/Correlation/Causation variant discriminator stays in Rust bytes; Python callers pass cursors back through opaquely. A wrong-type cursor (e.g. Range cursor passed to a correlation query) raises `FossicError` at the Rust boundary.

### PySamplingMode — static constructors

Mirrors the Rust `SamplingMode` enum without exposing variant names:

```python
SamplingMode.exhaustive()
SamplingMode.breadth_first(max_per_level=N)
SamplingMode.adaptive(target_count=N)
```

### Bounded read methods on Store

Three new methods mirror the Rust `read_range_bounded`, `read_by_correlation_bounded`, `walk_causation_bounded` signatures, with all parameters optional:

```python
store.read_range_bounded(query, max_results=None, max_bytes=None, cursor=None) -> ReadOutcome
store.read_by_correlation_bounded(correlation_id, max_results=None, max_bytes=None, cursor=None) -> ReadOutcome
store.walk_causation_bounded(start, direction="forward", max_depth=100,
    sampling=None, max_results=None, max_bytes=None, cursor=None) -> ReadOutcome
```

Store-level defaults (`default_max_results`, `default_max_bytes` in `OpenOptions`) are respected by the Rust layer but not yet configurable from Python — `PyOpenOptions` only exposes `encryption` and `on_first_open`. The two related test cases are explicitly skipped.

### Streaming iterators

Three iterator constructors return Python iterators (implement `__iter__` and `__next__`):

```python
store.read_range_iter(query)              -> RangeIter
store.read_by_correlation_iter(corr_id)  -> CorrelationIter
store.walk_causation_iter(start, ...)    -> CausationIter
```

Each `__next__` drives exactly one Rust `Iterator::next()` call, which batch-fetches internally (ITER_BATCH_SIZE=100) and releases the pool connection before returning. The same pool-release invariant from v1.1.5 applies.

### parse_direction helper

The existing `walk_causation` direction-string → `WalkDirection` dispatch was duplicated into `walk_causation_bounded` and `walk_causation_iter`. Extracted to a private `parse_direction(s: &str) -> PyResult<WalkDirection>` helper; `walk_causation` now calls it too.

---

## Public API additions (fossic-py)

**New types (all re-exported from `fossic`):**
- `ReadOutcome` — tagged union with `.is_truncated`, `.complete`, `.results`, `.reason`, `.next_cursor`
- `TruncationCursor` — opaque; `.to_bytes()` / `.from_bytes(b)`
- `SamplingMode` — static constructors `.exhaustive()`, `.breadth_first(n)`, `.adaptive(n)`
- `RangeIter` — Python iterator over `StoredEvent`
- `CorrelationIter` — same
- `CausationIter` — same

**New methods on `Store`:**
```
Store.read_range_bounded(query, max_results, max_bytes, cursor) -> ReadOutcome
Store.read_by_correlation_bounded(correlation_id, max_results, max_bytes, cursor) -> ReadOutcome
Store.walk_causation_bounded(start, direction, max_depth, sampling, max_results, max_bytes, cursor) -> ReadOutcome
Store.read_range_iter(query) -> RangeIter
Store.read_by_correlation_iter(correlation_id) -> CorrelationIter
Store.walk_causation_iter(start, direction, max_depth, sampling) -> CausationIter
```

**No breaking changes.** All new methods are additive. Existing `walk_causation` signature unchanged.

---

## Test coverage (`fossic-py/tests/test_bounded.py`)

| Test | Asserts |
|---|---|
| `test_truncation_cursor_bytes_round_trip` | cursor → `.to_bytes()` → `from_bytes()` → `.to_bytes()` is identity |
| `test_truncation_cursor_empty_bytes` | empty bytes round-trips |
| `test_sampling_mode_exhaustive` | repr contains "exhaustive" |
| `test_sampling_mode_breadth_first_carries_limit` | repr contains limit value |
| `test_sampling_mode_adaptive_carries_target` | repr contains target value |
| `test_read_outcome_complete_properties` | `.complete`, `.is_truncated`, `.reason`, `.next_cursor` all correct for Complete |
| `test_read_outcome_truncated_properties` | `.is_truncated`, `.complete`, `.reason == "result_count"`, `.results` count correct |
| `test_range_bounded_no_budget_returns_complete` | no-budget call returns Complete with all 5 events |
| `test_range_bounded_truncates_at_result_count` | 10 events, limit 3 → Truncated with 3 |
| `test_range_bounded_complete_when_exactly_at_limit` | 5 events, limit 5 → Complete |
| `test_range_bounded_truncates_at_byte_budget` | 1-byte budget → Truncated with 1 event |
| `test_range_bounded_resume_continues_from_cursor` | page 1 versions [0,1,2], page 2 versions [3,4,5] |
| `test_range_bounded_resume_full_pagination` | 7 events paginated at 3 → [0..6] exactly |
| `test_range_bounded_uses_store_default_max_results` | SKIPPED — OpenOptions.default_max_results not exposed |
| `test_range_bounded_per_call_overrides_store_default` | SKIPPED — same reason |
| `test_correlation_bounded_no_budget_returns_complete` | 4 correlated events, no budget → Complete with 4 |
| `test_correlation_bounded_truncates_at_result_count` | 6 correlated, limit 3 → Truncated with 3 |
| `test_correlation_bounded_resume_continues_from_cursor` | 6 events paginated at 3 → 6 unique ids in ascending order |
| `test_correlation_bounded_no_events_returns_complete_empty` | lone event with no siblings → Complete(0) |
| `test_correlation_bounded_wrong_cursor_type_returns_error` | Range cursor passed to correlation → raises FossicError |

---

## Tech debt / polish debt

**`PyOpenOptions` does not expose `default_max_results` / `default_max_bytes`.**
Two test cases are skipped. A follow-up pass can expose these fields and un-skip.

**`aggregate_bounded` not exposed.**
Python aggregate is currently a collect-all fold implemented in Python. Exposing `aggregate_bounded` requires a `Clone`able Python callable, which is a separate design problem. Deferred.

---

## Adjacent project notifications

fossic-node, fossic-tauri unaffected (no Rust API surface changed — v1.1.6 only adds to the Python binding layer).
