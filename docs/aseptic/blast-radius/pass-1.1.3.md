---
pass: v1.1.3
version: v1.1.3
date: 2026-06-21
prior-commit: 7187499
summary: walk_causation_bounded — Rust-side BFS loop, three sampling modes, level-boundary budget cuts
---

# Blast Radius — Pass v1.1.3

## Files

### Created
- `tests/causation_bounded.rs` — 14 tests covering all sampling modes and pagination
- `docs/aseptic/blast-radius/pass-1.1.3.md` — this file

### Modified
- `src/types.rs` — `CursorInner::Causation` shape corrected
- `src/cross_stream.rs` — BFS helpers + `walk_causation_bounded_impl`
- `src/store.rs` — `walk_causation_bounded` public method; imports extended
- `Cargo.toml` — `[[test]] causation_bounded` entry added
- `CHANGELOG.md` — v1.1.3 section added

---

## Changes

### `src/types.rs` — `CursorInner::Causation` shape fix

v1.1.0 design had `{ start_id, depth, last_seen_id }` — wrong for frontier-based BFS resume.
Replaced with `{ frontier: Vec<[u8;32]>, direction: u8, depth_consumed: u32 }`:

- `frontier`: the last-yielded BFS level's event IDs. On resume, expand this set to obtain the
  next level. Initialising `seen` from frontier prevents re-yielding via convergent paths.
- `direction`: 0=Forward, 1=Backward, 2=Both. Validated at cursor decode to prevent mismatched
  direction resume (tested: cursor direction mismatch returns `Error::Internal`).
- `depth_consumed`: BFS levels fully consumed before the cut point. On resume,
  `start_depth = depth_consumed + 1`.

`CursorInner` is `pub(crate)` with no external consumers; safe to correct before any call site exists.

### `src/cross_stream.rs` — Rust-side BFS loop

The existing `walk_causation_impl` uses a recursive CTE. Recursive CTEs can't be interrupted
mid-traversal, making budget enforcement impossible without materialising the whole result.

`walk_causation_bounded_impl` replaces the CTE with a Rust loop: one SQL query per BFS level,
stopping between levels when a budget is hit.

**`bfs_expand_forward(conn, frontier)`**
```sql
SELECT ... FROM events WHERE causation_id IN (?1, ?2, ...) ORDER BY id ASC
```
Forward expansion: events whose `causation_id` is in the current frontier.

**`bfs_expand_backward(conn, frontier)`**
```sql
SELECT ... FROM events
WHERE id IN (
    SELECT causation_id FROM events WHERE id IN (?1, ...) AND causation_id IS NOT NULL
) ORDER BY id ASC
```
Backward expansion: the events that are the causal parents of the current frontier.

**`expand_frontier(conn, frontier, direction)`**
Dispatches to forward/backward; for `Both`, unions the results and deduplicates via a
`HashSet`, then re-sorts by `id ASC`.

**`apply_bfs_sampling(events, sampling, max_depth)`**
Truncation applied after expansion and dedup, before budget check:
- `Exhaustive` → no truncation.
- `BreadthFirst { max_per_level }` → `events.truncate(max_per_level)`.
- `Adaptive { target_count }` → `max_per_level = max(1, target_count / max_depth)`.
  Distributes the target count evenly across all depth levels.

All truncation is by `id ASC` first-N — deterministic, not random.

**At-least-one guarantee**
Budget cuts fire only when `!results.is_empty()`. The first BFS level is always yielded in full
(even if it alone exceeds the budget), preventing an infinite loop when a single level exceeds
the ceiling.

**`seen: HashSet<[u8;32]>`**
Tracks every event ID yielded within a single call. On fresh start, seeded with `start` itself
(the root event). On resume, seeded with `cursor.frontier` (the last-yielded level) to prevent
re-yielding via convergent paths on resume.

**Cursor encoding**
On truncation: frontier = `current_frontier` (the unexpanded level that would have been next),
`depth_consumed = depth - 1`. On resume, the caller checks `cursor_dir == expected_dir` before
passing the state to the impl.

### `src/store.rs` — `walk_causation_bounded` public method

Pattern matches the other bounded methods:
1. **Budget resolution**: `effective = per_call.or(self.inner.options.default_X)`.
2. **Cursor decode**: type mismatch → `Error::Internal`; direction mismatch → `Error::Internal`.
3. **Impl call** under a read-pool connection.
4. **Upcaster application** to every event in both `Complete` and `Truncated` arms.

---

## Public API additions

**New method:**
```
Store::walk_causation_bounded(
    start: EventId,
    direction: WalkDirection,
    max_depth: usize,
    sampling: SamplingMode,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
    resume: Option<TruncationCursor>,
) -> Result<ReadOutcome<Vec<StoredEvent>>, Error>
```

**Breaking changes (internal only):**
`CursorInner::Causation` field shape — `pub(crate)`, no external callers, safe at this stage.

---

## Test coverage (`tests/causation_bounded.rs`)

| Test | Asserts |
|---|---|
| `forward_no_budget_returns_complete` | Complete with 3 descendants, no budget set |
| `backward_walk_finds_ancestors` | Complete(3), all ancestor IDs present |
| `both_direction_walk_finds_neighbors` | Complete(3): root, d2, d3 from middle node |
| `truncates_at_result_count` | Truncated(1), reason=ResultCount (budget=1) |
| `truncates_at_byte_budget_at_least_one` | Truncated(1), reason=ByteSize (1-byte budget); first level yielded despite exceeding budget |
| `complete_when_exactly_at_limit` | Complete(3) when budget == event count |
| `resume_full_pagination` | All 3 descendants collected across 3 pages; no duplicates |
| `max_depth_respected` | Complete(2) — d1 and d2 only, d3 excluded by max_depth=2 |
| `no_children_returns_complete_empty` | Complete([]) for event with no causal children |
| `breadth_first_sampling_caps_per_level` | Complete(1) — BreadthFirst{1} on 3-child tree |
| `adaptive_sampling_distributes_count` | Complete(2) — Adaptive{target=2, depth=1} → max_per_level=2 |
| `uses_store_default_max_results` | Truncated(1), reason=ResultCount via store default |
| `wrong_cursor_type_returns_error` | Err on Range cursor passed to causation walk |
| `cursor_direction_mismatch_returns_error` | Err on Forward cursor passed to Backward walk |

---

## CCE identity note (discovered during test debugging)

Events are content-addressed by `(event_type, type_version, causation_id, payload)` only.
`stream_id`, `branch`, `version`, and `timestamp_us` are NOT part of the CCE hash. Two
appends with identical type/version/causation/payload produce the same `EventId`, and the
second insert silently collides with the first (UNIQUE constraint on `id`). Test helpers that
create "wide" trees must give each sibling a distinct payload.

---

## Tech debt / polish debt

**No new entries this pass.**

Dead-code warnings for `Error::ReadBudgetExceeded` and `BudgetKind` persist — these were
introduced in v1.1.0 and are not yet raised by any call site. Will resolve when call sites
are wired in a future pass.

---

## Adjacent project notifications

No FFI-visible changes. fossic-py, fossic-node, fossic-tauri unaffected.
`walk_causation_bounded` is not yet exposed through any binding layer.
