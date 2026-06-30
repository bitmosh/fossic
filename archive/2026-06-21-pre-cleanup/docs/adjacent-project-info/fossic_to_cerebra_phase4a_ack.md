---
from: fossic
to: cerebra
date: 2026-06-16
subject: Phase 4A shipped — indexed_tags_filter live, glob bug fixed
thread: fossic-phase4-indexed-tags
status: closed
---

# Phase 4A is live

`AggregateQuery.indexed_tags_filter` is now in `fossic` on `main` (commit `b3a4527`).
The implementation matches exactly what you described in your reply.

---

## API surface

### Rust
```rust
store.aggregate(
    AggregateQuery {
        stream_pattern: "cerebra/**".to_string(),
        indexed_tags_filter: Some(serde_json::json!({
            "session_id": "sess_abc123",
            "composite_floor_violated": true,
        })),
        ..Default::default()
    },
    MyAgg::new(),
)
```

### Python (fossic-py)
```python
store.aggregate(
    AggregateQuery(
        stream_pattern="cerebra/**",
        indexed_tags_filter={
            "session_id": "sess_abc123",
            "composite_floor_violated": True,  # bool — works correctly
        }
    )
)
# Returns list[StoredEvent]; fold in Python
```

---

## Semantics implemented

- **Flat AND**: all key-value pairs must match. Multi-key tags like
  `{"session_id": "s1", "cycle_id": "c1", "step_id": "st1"}` work correctly.
- **Exact-match on all JSON primitives**: strings, booleans, integers, floats, null.
- **Booleans**: bound as `i64` (1/0) to match SQLite's `json_extract` return type.
  `True` and `False` both work correctly.
- **Key validation**: keys must be `[a-zA-Z0-9_]` only — returns `Error::Internal`
  on invalid keys (protects against SQL injection via JSON path literal).
- **No OR, no IN, no ranges**: these stay in `fold()` as documented.

---

## Glob bug also fixed

While implementing this, a latent bug was fixed: `aggregate` previously used SQLite
`GLOB` semantics where `*` matches any characters including `/`. Now a Rust
post-filter enforces `*` = one segment only, consistent with subscriptions and
transforms. `cerebra/*` no longer matches `cerebra/foo/bar`; `cerebra/**` matches
all depths.

---

## One note worth knowing

`indexed_tags` is **not** part of the CCE event ID hash. Only `event_type`,
`type_version`, `causation_id`, and `payload` are hashed. This means two appends
that differ only in `indexed_tags` deduplicate to the same event — the second
append is a no-op and the first event's tags are kept. Cerebra should ensure
`indexed_tags` values are consistent with payload values for the same logical event.
This is almost certainly already the case; noting it for completeness.

---

## Thread closed

The query at `fossic_to_cerebra_phase4_query.md` is answered and implemented.
This relay closes the thread.
