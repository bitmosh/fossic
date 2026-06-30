---
pass: v1.1.2
version: v1.1.2
date: 2026-06-20
prior-commit: ca27c72
summary: read_range_bounded + read_by_correlation_bounded — paged resume, budget fallback to OpenOptions defaults
---

# Blast Radius — Pass v1.1.2

## Files

### Created
- `tests/bounded_reads.rs` — 14 tests covering both bounded read methods
- `docs/aseptic/blast-radius/pass-1.1.2.md` — this file

### Modified
- `src/types.rs` — `CursorInner::Correlation::after_timestamp_us: i64` → `last_seen_id: [u8; 32]`
- `src/read.rs` — `read_range_bounded_impl` added; imports extended with bounded-read types
- `src/cross_stream.rs` — `read_by_correlation_bounded_impl` added; imports extended
- `src/store.rs` — `read_range_bounded` + `read_by_correlation_bounded` public methods; imports extended
- `Cargo.toml` — `[[test]] bounded_reads` entry added
- `CHANGELOG.md` — v1.1.2 section added

---

## Changes

### `src/types.rs` — `CursorInner::Correlation` field fix

`after_timestamp_us: i64` renamed/retyped to `last_seen_id: [u8; 32]`. The v1.1.0 design
was wrong for the actual resume predicate (`id > last_seen_id ORDER BY id ASC` requires a
32-byte BLOB, not a timestamp). `CursorInner` is `pub(crate)` with no external call sites;
corrected now before any consumer exists.

### `src/read.rs` — `read_range_bounded_impl`

Row-by-row budget tracking with two stop conditions:

- **Result count:** `events.len() >= max_results` — checked before adding the current row.
  Cursor carries `next_version: event.version` (the first excluded event's version).
- **Byte size:** `byte_count + event.payload.len() > max_bytes` AND `!events.is_empty()`.
  The `!events.is_empty()` guard ensures at least one event is always returned, preventing
  an infinite loop when one event exceeds the byte budget alone. Cursor carries
  `next_version: event.version`.

`resume_version` overrides `q.from_version` when set, enabling exact paged resume.

No SQL `LIMIT` is passed — the budget loop terminates iteration early.

### `src/cross_stream.rs` — `read_by_correlation_bounded_impl`

Same budget model as the range variant. Two key differences from the unbounded
`read_by_correlation_impl`:

1. **`ORDER BY id ASC`** (32-byte BLOB lexicographic) instead of `ORDER BY timestamp_us ASC`.
   Necessary for deterministic resume: timestamp ties would make cursor position ambiguous.

2. **`(?2 IS NULL OR id > ?2)` resume clause.** When `resume_after_id` is `None`, rusqlite
   binds SQL `NULL` for `?2` and the IS NULL branch passes — giving no lower bound. When
   `resume_after_id` is `Some([u8; 32])`, `?2` is a BLOB and `id > ?2` filters correctly.
   One SQL path handles both first-page and resume cases.

Cursor carries `last_seen_id = events.last().id` (last included event's id). Resume
restarts from the next event via `id > last_seen_id`.

### `src/store.rs` — `read_range_bounded` + `read_by_correlation_bounded`

Both public methods follow the same pattern:
1. **Budget resolution:** `effective = per_call.or(self.inner.options.default_X)`.
   Resolution is here, not in the impl — keeps the impl pure.
2. **Cursor decode:** `TruncationCursor::decode()` → match the correct `CursorInner`
   variant; `Internal` error on type mismatch (tested).
3. **Impl call** under a read-pool connection.
4. **Upcaster application** to every event in the `ReadOutcome` (both `Complete` and
   `Truncated` branches).

---

## Public API additions

**New methods:**
- `Store::read_range_bounded(q: ReadQuery, max_results: Option<usize>, max_bytes: Option<usize>, resume: Option<TruncationCursor>) -> Result<ReadOutcome<Vec<StoredEvent>>, Error>`
- `Store::read_by_correlation_bounded(correlation_id: EventId, max_results: Option<usize>, max_bytes: Option<usize>, resume: Option<TruncationCursor>) -> Result<ReadOutcome<Vec<StoredEvent>>, Error>`

**Breaking changes (internal only):**
`CursorInner::Correlation` field rename — `pub(crate)`, no external callers, safe at this stage.

---

## Test coverage (`tests/bounded_reads.rs`)

| Test | Asserts |
|---|---|
| `range_bounded_no_budget_returns_complete` | Complete with all 5 events when no budget |
| `range_bounded_truncates_at_result_count` | Truncated(3), reason=ResultCount |
| `range_bounded_complete_when_exactly_at_limit` | Complete when limit == event count |
| `range_bounded_truncates_at_byte_budget` | Truncated(1), reason=ByteSize (1-byte budget) |
| `range_bounded_resume_continues_from_cursor` | Page 1 = [0,1,2], page 2 = [3,4,5] |
| `range_bounded_resume_full_pagination` | All 7 events collected across pages |
| `range_bounded_uses_store_default_max_results` | Store default of 2 triggers truncation |
| `range_bounded_per_call_overrides_store_default` | Per-call 4 overrides store default 2 |
| `correlation_bounded_no_budget_returns_complete` | Complete with all 4 children |
| `correlation_bounded_truncates_at_result_count` | Truncated(3), reason=ResultCount |
| `correlation_bounded_resume_continues_from_cursor` | All 6 ids collected, ascending BLOB order |
| `correlation_bounded_no_events_returns_complete_empty` | Complete([]) for unmatched correlation |
| `correlation_bounded_uses_store_default_max_results` | Store default of 2 triggers truncation |
| `correlation_bounded_wrong_cursor_type_returns_error` | Err on Range cursor passed to correlation |

---

## Tech debt / polish debt

**No new entries this pass.**

Dead-code warnings for `CursorInner::Causation` and `CursorInner::encode/decode` persist —
expected until `walk_causation_bounded` ships (v1.1.3+).

---

## Adjacent project notifications

No FFI-visible changes. fossic-py, fossic-node, fossic-tauri unaffected.
The two new `Store` methods are not yet exposed through any binding layer.
