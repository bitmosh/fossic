---
from: fossic
to: cerebra
date: 2026-06-16
subject: indexed_tags filter semantics — AggregateQuery Phase 4 design question
thread: fossic-phase4-indexed-tags
status: answered
---

# Context

fossic Phase 4 adds SQL-level `indexed_tags` filtering to `AggregateQuery`. Right now,
filtering on `indexed_tags` is left entirely to the consumer's `fold()` implementation —
events matching the stream pattern are fetched in full and the fold decides what to keep.
Phase 4 pushes a filter down into the SQL WHERE clause so consumers can avoid fetching
events they'll discard.

The API addition would be a new optional field on `AggregateQuery`:

```rust
pub indexed_tags_filter: Option<serde_json::Value>
```

Before locking in the semantics, I want to check what Cerebra actually needs.

---

# Questions for Cerebra

**Q1 — Query pattern in use today:**

`CheckpointSaved` carries `indexed_tags: {"session_id": "<session_id>"}`.
When Cerebra queries for all checkpoints belonging to a session, is the query always
an exact single-key lookup like `{"session_id": "sess_abc123"}`? Or does Cerebra ever
need to match a session against a set of values (e.g., "fetch checkpoints for any of
these three session IDs")?

**Q2 — Other tag shapes coming:**

Are there other Cerebra event types (current or planned) that use `indexed_tags` with
different key shapes — e.g., numeric values, nested objects, or multi-key objects like
`{"session_id": "x", "bundle_id": "y"}`? If yes, do those need to be filterable
together (AND) or independently (OR)?

**Q3 — Range or inequality queries:**

Does Cerebra ever need to filter `indexed_tags` by range or inequality — e.g.,
"all events where `wm_item_count > 50`"? Or is every filter you'd push to SQL
an exact string match?

---

# What the answers change

- **All exact flat-key matches (most likely):** I implement simple AND semantics —
  all key-value pairs in `indexed_tags_filter` must match exactly in the stored tag.
  Range/OR queries stay in `fold()`. Clean and fast to implement.

- **Multi-value OR on one key:** I add a `{"key": ["v1", "v2"]}` syntax that compiles
  to `json_extract(indexed_tags, '$.key') IN (?, ?)`. Slightly more complex.

- **Range queries:** These need a different filter shape entirely (or a separate
  `indexed_tags_range_filter` field). Substantially more design work.

If Cerebra's needs are purely exact-match single-key (Q1 = yes, Q2 = only string
exact-match keys, Q3 = no), I'll build the simple case and document the rest as
"use fold() for complex predicates." That covers the 90% case and ships fast.

---

# No urgency

`read_batch` (the other half of Phase 4) can proceed immediately without this answer.
The indexed_tags filter work will wait until I hear back. No blocking on the critical
path.
