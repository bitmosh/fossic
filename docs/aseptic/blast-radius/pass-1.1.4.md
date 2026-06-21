---
pass: v1.1.4
version: v1.1.4
date: 2026-06-21
prior-commit: 25d08aa
summary: aggregate_bounded — A:Clone bound, ReadOutcome<A::Output>, no-cursor Truncated, two budget kinds
---

# Blast Radius — Pass v1.1.4

## Files

### Created
- `tests/aggregate_bounded.rs` — 11 tests
- `docs/aseptic/blast-radius/pass-1.1.4.md` — this file

### Modified
- `src/types.rs` — `ReadOutcome::Truncated.cursor` widened to `Option<TruncationCursor>`
- `src/cross_stream.rs` — `aggregate_bounded_impl` added; three `Truncated` construction sites updated
- `src/store.rs` — `Store::aggregate_bounded` added; import extended; one `Truncated` construction site updated
- `src/read.rs` — one `Truncated` construction site updated
- `Cargo.toml` — `[[test]] aggregate_bounded` entry added
- `CHANGELOG.md` — v1.1.4 section added
- `tests/bounded_foundation.rs` — updated for `Option<TruncationCursor>` field
- `tests/bounded_reads.rs` — updated cursor extraction and resume call sites
- `tests/causation_bounded.rs` — updated cursor extraction and resume call sites

---

## Changes

### `src/types.rs` — `ReadOutcome::Truncated.cursor` widened

```rust
// before
cursor: TruncationCursor,

// after
cursor: Option<TruncationCursor>,
```

Required to express the v1.1 aggregate semantic: fold-resume is not yet supported
(`Aggregate` has no partial-state injection), so `aggregate_bounded` returns `cursor: None`.
All pageable reads (range, correlation, causation walk) continue to return `Some(cursor)`.

Doc comment on `Truncated` explains both cases; the `None` path and the defer-to-v1.2.x
rationale are documented there.

**Blast radius on call sites:**

- Three construction sites in `src/` — wrapped existing local in `Some(...)`.
- Three store.rs pass-through sites — no change needed; they destructure `cursor` by name and
  re-assign it in the same field, so the type change flows through unchanged.
- `tests/bounded_foundation.rs` — construction wrapped in `Some`; `.as_bytes()` → `.unwrap().as_bytes()`.
- `tests/bounded_reads.rs` — two loop-resume sites changed `cursor_opt = Some(cursor)` to
  `cursor_opt = cursor`; one cursor-mismatch test changed `Some(range_cursor)` to `range_cursor`
  (already an Option).
- `tests/causation_bounded.rs` — same pattern: three resume/mismatch sites updated.

### `src/cross_stream.rs` — `aggregate_bounded_impl`

```rust
pub(crate) fn aggregate_bounded_impl<A: Aggregate + Clone>(
    conn: &Connection,
    query: AggregateQuery,
    mut agg: A,
    upcasters: &UpcasterRegistry,
    max_events: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ReadOutcome<A::Output>, Error>
```

**Budget loop** — checked before folding each glob-matched event:

- `exceed_count` fires when `events_scanned >= max_events` (we've already folded N events and
  there are more). For N=0 this fires immediately on the first event; for N≥1 we always fold
  at least one event before cutting.
- `exceed_bytes` fires when `byte_count + event_bytes > max_bytes AND events_scanned > 0`.
  The `> 0` guard is the at-least-one guarantee: the first event is always folded even if its
  payload alone exceeds the byte ceiling (consistent with all other bounded reads).
- `count` wins over `bytes` in the concurrent case (checked first).

**On truncation:**

```rust
let data = agg.clone().finalize();
return Ok(ReadOutcome::Truncated { data, cursor: None, reason });
```

`agg.clone()` snapshots the accumulated state at the cut point. `finalize()` is called on the
clone, consuming it. The original `agg` is dropped. No resume cursor is produced — see below.

**No resume cursor (`cursor: None`)**

Fold-resume would require injecting the partial aggregator state into a new instance. `Aggregate`
has no such method: it only supports `fold(&mut self, event)` + `finalize(self)`. Adding a
`restore(partial_output) -> Self` or similar is a v1.2.x design question; not introduced here.
Callers that need resume semantics can re-run the full query with a `from_timestamp_us` offset,
or switch to the unbounded `aggregate` if result-size bounding is not needed.

### `src/store.rs` — `Store::aggregate_bounded`

```rust
pub fn aggregate_bounded<A: Aggregate + Clone>(
    &self,
    query: AggregateQuery,
    agg: A,
    max_events_scanned: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ReadOutcome<A::Output>, Error>
```

Budget resolution:
- effective events = `max_events_scanned` ?? `OpenOptions::default_max_results` ?? unbounded
- effective bytes  = `max_bytes` ?? `OpenOptions::default_max_bytes` ?? unbounded

`default_max_results` is reused as the events-scanned default because aggregates are bounded
on input size (events read), not output size — the naming is slightly imprecise at the
`OpenOptions` level but avoids adding a new config field for a v1.1 feature.

---

## Public API additions

**New method:**
```
Store::aggregate_bounded<A: Aggregate + Clone>(
    query: AggregateQuery,
    agg: A,
    max_events_scanned: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ReadOutcome<A::Output>, Error>
```

**`Aggregate` trait bound — not extended.** The `Clone` bound is on the bounded variant only;
the existing `Store::aggregate` signature is unchanged.

**`ReadOutcome::Truncated.cursor` — type change.**
Changed from `TruncationCursor` to `Option<TruncationCursor>`. This is an API-level breaking
change for downstream code that pattern-matches `ReadOutcome::Truncated` and accesses `cursor`
directly (no longer a bare value). All in-tree call sites updated. No external consumers exist
at this stage (no bindings layer exposes it).

---

## Test coverage (`tests/aggregate_bounded.rs`)

| Test | Asserts |
|---|---|
| `aggregate_bounded_no_budget_returns_complete` | Complete(4) with no budget set |
| `aggregate_bounded_empty_stream_returns_complete_empty` | Complete([]) for empty stream |
| `aggregate_bounded_event_count_truncation` | Truncated(3), reason=ResultCount, cursor=None |
| `aggregate_bounded_complete_when_exactly_at_limit` | Complete(3) when limit == event count |
| `aggregate_bounded_byte_truncation_at_least_one` | Truncated(1), reason=ByteSize, 1-byte budget, at-least-one guarantee |
| `aggregate_bounded_count_wins_over_bytes` | ResultCount reason when both budgets set; count fires first |
| `aggregate_bounded_finalize_accumulates_correct_state` | Complete — sum aggregator totals correctly |
| `aggregate_bounded_truncated_finalizes_partial_state` | Truncated — partial finalize produces correct partial sum |
| `aggregate_bounded_uses_store_default_max_results` | Truncated via store default (no per-call budget) |
| `aggregate_bounded_per_call_overrides_store_default` | Per-call budget overrides store default |
| `aggregate_bounded_event_type_filter_respected` | Complete(2) — only "Even" events folded |

---

## Tech debt / polish debt

**`default_max_results` reused as events-scanned default.** `OpenOptions::default_max_results`
was designed for result-count bounding on vec-returning reads. `aggregate_bounded` reuses it
as the events-scanned default because aggregates are bounded on input, not output. If a future
caller wants different defaults for aggregate vs. vec reads on the same store, a dedicated
`default_max_events_scanned` field would be needed. Flagged for v1.2.x review.

Dead-code warnings for `Error::ReadBudgetExceeded` and `BudgetKind` persist — introduced in
v1.1.0, not yet raised by any call site. Will resolve when call sites are wired.

---

## Adjacent project notifications

No FFI-visible changes. fossic-py, fossic-node, fossic-tauri unaffected.
`aggregate_bounded` is not yet exposed through any binding layer.
