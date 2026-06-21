---
pass: v1.1.8
version: v1.1.8
date: 2026-06-21
prior-commit: dee169a
summary: Tauri IPC bounded read commands — 7 new commands, SerializedReadOutcome, cursor base64 helpers, serialization helpers
---

# Blast Radius — Pass v1.1.8

## Files

### Created
- `docs/aseptic/blast-radius/pass-1.1.8.md` — this file

### Modified
- `crates/fossic-tauri/Cargo.toml` — promoted `base64 = "0.22"` from transitive dep to explicit dependency
- `crates/fossic-tauri/src/serialization.rs` — added `SerializedReadOutcome` + `from_outcome`, `parse_cursor`, `parse_direction`, `parse_sampling_mode`
- `crates/fossic-tauri/src/commands.rs` — 7 new bounded IPC commands, `CollectAggregate`, `SerializedAggregateOutcome`
- `crates/fossic-tauri/src/lib.rs` — registered 7 new commands in `plugin()`, `plugin_with_test_helpers()`, and inline `register_commands` doc example
- `crates/fossic-tauri/tests/read_range.rs` — 11 new tests for bounded read commands
- `CHANGELOG.md` — v1.1.8 section added

---

## Changes

### 7 New Tauri IPC Commands

Tauri IPC is request-response only (no native iterator protocol). Bounded reads with cursor resumption provide the equivalent of streaming. Each query mode is split into two commands — `_bounded` (first page, no cursor) and `_from_cursor` (subsequent pages, cursor required) — for ergonomic IPC.

| Command | Description |
|---|---|
| `fossic_read_range_bounded` | First page of stream range read |
| `fossic_read_range_from_cursor` | Subsequent page using a prior cursor |
| `fossic_read_by_correlation_bounded` | First page of correlation group |
| `fossic_read_by_correlation_from_cursor` | Subsequent page using a prior cursor |
| `fossic_walk_causation_bounded` | First page of causation walk |
| `fossic_walk_causation_from_cursor` | Subsequent page using a prior cursor |
| `fossic_aggregate_bounded` | Cross-stream aggregate with event/byte budget |

All existing commands are unchanged.

**Streaming limitation:** Native streaming (push events over Tauri IPC) is deferred to v1.2.x. Callers requiring streaming should use cursor-based pagination with these bounded commands.

### SerializedReadOutcome

JSON-serializable bounded read response:

```json
{ "kind": "complete", "results": [...] }
{ "kind": "truncated", "results": [...], "reason": "result_count", "next_cursor": "<base64>" }
```

`reason` and `next_cursor` are omitted (not null) on complete outcomes — `#[serde(skip_serializing_if = "Option::is_none")]`.

### TruncationCursor over IPC

Serialized as base64 (not `Buffer` like the Node.js binding). `parse_cursor(s: &str)` decodes and reconstructs the cursor; the encoded string is produced by `B64.encode(cursor.as_bytes())` inside `from_outcome`.

### SamplingMode over IPC

Accepted as a JSON tagged object: `{"kind":"exhaustive"}` | `{"kind":"breadthFirst","maxPerLevel":N}` | `{"kind":"adaptive","targetCount":N}`. `parse_sampling_mode(v: Option<Value>)` dispatches. Absent/null → `Exhaustive`.

### fossic_aggregate_bounded

Uses an internal `CollectAggregate` struct implementing `fossic::Aggregate` to collect events across streams. No cursor is produced on truncation — fold-resume requires partial aggregator state that the `Aggregate` trait does not yet expose (deferred to v1.2.x).

```rust
struct CollectAggregate { events: Vec<SerializedEvent> }
impl fossic::Aggregate for CollectAggregate {
    type Output = Vec<SerializedEvent>;
    fn fold(&mut self, e: &StoredEvent) { self.events.push(SerializedEvent::from_stored(e)) }
    fn finalize(self) -> Vec<SerializedEvent> { self.events }
}
```

---

## Test Results

**13/13 passed** (all fossic-tauri tests with `--features test-helpers`).

New tests:
- `read_range_bounded_no_cursor_returns_complete`
- `read_range_bounded_truncates_at_max_results`
- `read_range_from_cursor_resumes_correctly`
- `read_range_bounded_full_pagination`
- `read_by_correlation_bounded_paginates`
- `read_by_correlation_from_cursor_resumes`
- `cursor_base64_round_trip`
- `walk_causation_bounded_forward`
- `walk_causation_bounded_truncates_at_max_results`
- `aggregate_bounded_cross_stream`
- `aggregate_bounded_truncates_at_max_events`

---

## Sharp Edges

**CCE collision in multi-stream tests:** The CCE identity hash excludes `stream_id` — two streams receiving identical `(event_type, payload, causation_id)` produce the same `event_id`. `INSERT OR IGNORE` silently drops the second event; `append()` returns `Ok(id)` without panicking. Tests that assert on cross-stream event counts must include a stream-distinguishing field in each payload. Fixed in the aggregate tests by adding `"s": "a"` / `"s": "b"` to payloads.

**Aggregate cursor deferred:** `fossic_aggregate_bounded` returns `kind: "truncated"` with `reason` set but `next_cursor` always `null`. Resume requires re-feeding partial aggregator state — not yet supported. Documented in the command's response shape.
