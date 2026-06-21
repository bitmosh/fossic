# SR-07 — Cross-Stream Queries

**Series:** Fossic State Reports · Document 7 of 9
**Covers:** `src/cross_stream.rs`, `src/similarity.rs`, `fossic-py/src/store.rs` (aggregate/walk/correlation methods)
**Companion docs:** SR-03 (event lifecycle, read_range), SR-04 (subscriptions/glob), SR-06 (reducers)

---

## 1. Overview

All single-stream reads (`read_range`, `read_one`, `read_batch`, `read_by_external_id`) are scoped to one `(stream_id, branch)` pair. Cross-stream queries break that boundary and reason about relationships between events that may live in different streams, different branches, or be identified only by shared metadata.

Fossic v1 ships three implemented cross-stream mechanisms:

| API | Lookup key | SQL mechanism | Crosses branches? |
|-----|-----------|--------------|-------------------|
| `read_by_correlation` | `correlation_id` BLOB | indexed scan | yes |
| `walk_causation` | `causation_id` pointer chain | recursive CTE | yes |
| `aggregate` | stream_pattern + optional filters | GLOB + Rust post-filter | no (branch param) |

A fourth mechanism — `SimilaritySearchProvider` — is declared as a trait extension point but has no built-in implementation in v1.

All three implemented queries apply upcasters to returned events (the same `apply_upcaster` path used by `read_range`).

---

## 2. Correlation ID — read_by_correlation

### 2.1 API

```rust
// Rust
store.read_by_correlation(correlation_id: EventId) -> Result<Vec<StoredEvent>, Error>

// Python
events = store.read_by_correlation(correlation_id)  # correlation_id: EventId
```

Returns all events that carry the given `correlation_id`, ordered by `timestamp_us ASC`.

### 2.2 Schema and Index

```sql
-- Column on events table:
correlation_id  BLOB,   -- 32 bytes, optional grouping key

-- Partial index (only non-NULL rows indexed):
CREATE INDEX idx_events_correlation
    ON events(correlation_id) WHERE correlation_id IS NOT NULL;
```

The partial index keeps index size small — the majority of events typically have NULL `correlation_id`. Lookup cost is O(matching rows) after the index seek.

### 2.3 SQL

```sql
SELECT id, stream_id, branch, version, timestamp_us, causation_id, correlation_id,
       event_type, type_version, payload, external_id, indexed_tags
FROM events
WHERE correlation_id = ?1
ORDER BY timestamp_us ASC
```

No stream filter, no branch filter. This is a global bag query across the entire store.

### 2.4 Semantics and Invariants

**No branch scoping.** If `correlation_id = X` appears on both `main` and a speculative branch `repair-v1`, both events are returned. Callers that care about branch must post-filter:

```python
events = store.read_by_correlation(corr_id)
main_only = [e for e in events if e.branch == "main"]
```

**No stream scoping.** Events from `policy-scout/audit/sess-X`, `policy-scout/posture`, and `lattica/decisions` all return if they share a correlation_id.

**Ordering.** `timestamp_us ASC` means the initiating event (earliest timestamp) comes first, followed by downstream effects. For cross-process events written by relay agents, clock skew can affect this ordering; `causation_id` chains are more reliable than timestamp for causal ordering.

**`correlation_id` is set by the appender.** Fossic does not automatically propagate correlation_ids. Each `Append` must explicitly supply the correlation_id if it wants to be grouped. Convention: set `correlation_id` on all events that are part of the same workflow instance to a shared synthetic EventId (or to the id of the root event that started the workflow).

### 2.5 Use Cases

**Workflow fan-out tracking.** A policy decision event triggers writes on three streams. All three carry `correlation_id = policy_decision_event.id`. `read_by_correlation(policy_decision_event.id)` returns all three, letting you trace the full effect of one decision.

**Cross-agent session grouping.** Multiple agents that all participate in the same reasoning cycle write events to different streams, all tagged with the same session correlation_id. `read_by_correlation` gives a unified timeline.

**Debugging.** Given a specific EventId that you know caused downstream effects, use it as the correlation_id and retrieve the entire correlated event set.

### 2.6 Performance Characteristics

- Index seek cost: O(log n) where n is total events with non-NULL correlation_id.
- Result scan cost: O(k) where k is events matching the correlation_id.
- For correlation_ids that group many events (thousands), the result scan can be substantial. For sparse correlation_ids (grouping 3–20 events), this is very fast.
- **No streaming.** All results are buffered in memory before returning. For very large correlation groups, use `aggregate` with an `indexed_tags_filter` instead if you can tag events at write time.

---

## 3. Causation Chain Walk — walk_causation

### 3.1 API

```rust
// Rust
store.walk_causation(
    start: EventId,
    direction: WalkDirection,
    max_depth: usize,
) -> Result<Vec<StoredEvent>, Error>

pub enum WalkDirection {
    Forward,   // events caused BY start (start → children → grandchildren)
    Backward,  // events that CAUSED start (start → parents → grandparents)
    Both,      // union of Forward and Backward
}

// Python
events = store.walk_causation(start_id, direction="forward", max_depth=10)
events = store.walk_causation(start_id, direction="backward", max_depth=5)
events = store.walk_causation(start_id, direction="both", max_depth=10)
```

Direction strings accepted by Python: `"forward"`, `"backward"`, `"both"`. Any other string → `PyValueError`.

### 3.2 Causation Model

Every `Append` may carry a `causation_id: Option<EventId>`. This is a pointer to the event that directly caused this one to be written. The causation relationship is:

```
EventA (root, causation_id = None)
  └── EventB (causation_id = EventA.id)
        ├── EventC (causation_id = EventB.id)
        └── EventD (causation_id = EventB.id)
              └── EventE (causation_id = EventD.id)
```

**Forward walk** from EventA: returns A, B, C, D, E (the full subtree).
**Backward walk** from EventE: returns E, D, B, A (the ancestor chain).
**Both walk** from EventB: returns the union — all ancestors of B plus all descendants of B.

The graph is acyclic by construction (causation_id must reference a previously existing event — you cannot create a cycle with append-only writes).

### 3.3 Index

```sql
CREATE INDEX idx_events_causation
    ON events(causation_id) WHERE causation_id IS NOT NULL;
```

Partial index on `causation_id`. Each recursive step in the CTE does an indexed scan on `causation_id = <parent_id>`, finding all children efficiently.

### 3.4 Forward Walk SQL

```sql
WITH RECURSIVE causation_tree AS (
    -- Anchor: the start event itself
    SELECT id, causation_id, 0 AS depth
    FROM events
    WHERE id = ?1

    UNION ALL

    -- Recursive step: events caused by any node already in the tree
    SELECT e.id, e.causation_id, ct.depth + 1
    FROM events e
    JOIN causation_tree ct ON e.causation_id = ct.id
    WHERE ct.depth < ?2  -- max_depth guard
)
SELECT events.id, events.stream_id, events.branch, events.version,
       events.timestamp_us, events.causation_id, events.correlation_id,
       events.event_type, events.type_version, events.payload,
       events.external_id, events.indexed_tags
FROM events
JOIN causation_tree ON events.id = causation_tree.id
ORDER BY events.timestamp_us ASC
```

Note: `events.id` in the final SELECT uses the `events.` prefix to avoid SQLite's "ambiguous column name" error — the CTE also has a column named `id`. The `PREFIXED_SELECT_COLS` constant in `read.rs` provides this prefixed form for exactly this use case.

### 3.5 Backward Walk SQL

The backward walk inverts the join condition. Instead of "find events whose `causation_id` is in the tree", it follows `causation_id` pointers upward:

```sql
WITH RECURSIVE causation_tree AS (
    -- Anchor: the start event
    SELECT id, causation_id, 0 AS depth
    FROM events
    WHERE id = ?1

    UNION ALL

    -- Recursive step: the event that CAUSED each node in the tree
    SELECT e.id, e.causation_id, ct.depth + 1
    FROM events e
    JOIN causation_tree ct ON e.id = ct.causation_id  -- reversed: join on e.id = ct's causation_id
    WHERE ct.depth < ?2
)
SELECT events.id, events.stream_id, events.branch, events.version,
       events.timestamp_us, events.causation_id, events.correlation_id,
       events.event_type, events.type_version, events.payload,
       events.external_id, events.indexed_tags
FROM events
JOIN causation_tree ON events.id = causation_tree.id
ORDER BY events.timestamp_us ASC
```

For the backward walk, the index on `causation_id` is not used for the JOIN (the join is on `e.id = ct.causation_id`, which uses the primary key on `events.id` instead). This is efficient for backward walks because each ancestor lookup is a primary-key scan.

### 3.6 Both Direction Walk

Runs forward and backward CTEs independently, unions the result sets, deduplicates by id, and returns ordered by `timestamp_us ASC`. The start event itself appears once (included in both CTEs but deduplicated).

### 3.7 max_depth

SQLite recursive CTEs can loop indefinitely on cyclic graphs. The `WHERE ct.depth < ?2` guard is the only protection against runaway queries. In fossic's append-only model, true cycles are impossible (you can't set `causation_id` to an event that doesn't exist yet), but the depth guard also limits compute cost for deep trees.

Guidelines:
- `max_depth = 5–10`: typical agent step chains (StepStarted → SignalEvaluated → PredictionMade → OutcomeRecorded).
- `max_depth = 20`: multi-step agent cycles with nested reasoning.
- `max_depth = 50`: cross-process workflows where events causally chain across relay agents.
- `max_depth > 100`: be cautious; a branching-factor-2 tree at depth 100 has 2^100 nodes theoretically. In practice fossic trees are shallow and narrow.

### 3.8 Cross-Stream and Cross-Branch Behavior

**No stream or branch scoping.** The CTE joins on `events.id` and `events.causation_id` globally. A causation chain can:
- Span multiple streams: a `StepStarted` on `cerebra/agent-trace/sess-X` causes a `LockdownActivated` on `policy-scout/posture`.
- Span multiple branches: if a branch event has a causation_id pointing to a main-branch event, the walk crosses branches.

This is intentional. Causation is a semantic relationship that cuts across storage boundaries.

### 3.9 Performance

Cost: O(depth × branching_factor) recursive joins. Each level does an indexed scan on `causation_id` (forward) or a primary-key lookup (backward).

For typical agent traces (linear chains, depth ≤ 10): sub-millisecond.

For wide trees at depth 20 with branching factor 5: 5^20 ≈ 95 billion nodes theoretically (would be terminated by the depth guard), but in reality the actual tree is much smaller and the CTE terminates quickly.

**Do not use for very deep or very wide causation trees on large event sets.** For those cases, pre-aggregate with `indexed_tags` at write time instead.

### 3.10 Worked Example: Agent Trace Analysis

```python
# Find all events caused by a specific step start
step_started_id = EventId.from_hex("abcd...1234")

# Forward walk: everything this step caused
descendants = store.walk_causation(step_started_id, direction="forward", max_depth=10)
for ev in descendants:
    print(f"  {ev.event_type} on {ev.stream_id} (version {ev.version})")

# Backward walk: what caused this step to start
ancestors = store.walk_causation(step_started_id, direction="backward", max_depth=5)
# ancestors[0] is the oldest ancestor (the root of the chain)
# ancestors[-1] is the start event itself
root_event = next((e for e in ancestors if e.causation_id is None), None)
```

```python
# Given an outcome event, trace the full causal ancestry
outcome_id = EventId.from_hex("ef01...5678")
full_chain = store.walk_causation(outcome_id, direction="backward", max_depth=20)

# Group by stream to see which systems were involved
from collections import defaultdict
by_stream = defaultdict(list)
for ev in full_chain:
    by_stream[ev.stream_id].append(ev.event_type)
```

---

## 4. Aggregate Query

### 4.1 API

```rust
// Rust
store.aggregate(query: AggregateQuery, collector: impl Aggregate) -> Result<Output, Error>

pub struct AggregateQuery {
    pub stream_pattern: String,              // fossic glob pattern
    pub branch: String,                      // default "main"
    pub event_type_filter: Option<String>,   // exact match on event_type (None = all types)
    pub from_timestamp_us: Option<i64>,      // inclusive lower bound on timestamp_us
    pub to_timestamp_us: Option<i64>,        // inclusive upper bound on timestamp_us
    pub indexed_tags_filter: Option<serde_json::Value>,  // must be JSON object if Some
}

// Python
from fossic import AggregateQuery
events = store.aggregate(AggregateQuery(
    stream_pattern="cerebra/**",
    branch="main",
    event_type_filter=None,
    from_timestamp_us=None,
    to_timestamp_us=None,
    indexed_tags_filter=None,
))  # returns list[StoredEvent]
```

### 4.2 The Two-Stage Filtering Architecture

Aggregate uses a two-stage filter to efficiently apply fossic's segment-aware glob semantics:

#### Stage 1: SQLite GLOB pre-filter (fast, approximate)

The `stream_pattern` is passed directly to SQLite's `GLOB` operator:
```sql
WHERE stream_id GLOB ?1
```

**SQLite `GLOB` semantics** (not the same as fossic glob):
- Case-sensitive.
- `*` matches ANY sequence of characters, including `/`.
- `?` matches any single character.
- No concept of path segments.

**The mismatch:** Fossic's `*` matches exactly one segment (no `/`). SQLite's `*` matches everything including `/`.

Result: SQLite GLOB is a **superset** of what fossic intends. It may return extra rows that fossic-glob would reject.

Examples of over-matching by SQLite GLOB:
- Fossic pattern `a/*/b`, stream `a/x/y/b`: fossic-glob rejects (two segments between `a` and `b`), SQLite GLOB accepts (`*` crosses `/`).
- Fossic pattern `cerebra/*`, stream `cerebra/agent-trace/sess-1`: fossic-glob rejects (two segments after `cerebra/`), SQLite GLOB accepts.

Examples that SQLite and fossic agree on:
- Fossic pattern `cerebra/**`, stream `cerebra/any/depth/here`: both accept (`**` and SQLite `*` both match any length).
- Fossic pattern `policy-scout/posture` (exact), stream `policy-scout/posture`: both accept.
- Fossic pattern `policy-scout/posture`, stream `cerebra/posture`: both reject.

#### Stage 2: Rust post-filter (exact, correct)

After SQL returns candidate rows, Rust applies the fossic glob algorithm:
```rust
crate::glob::matches(&query.stream_pattern, &event.stream_id)
```

Events that SQL returned but fossic-glob rejects are dropped here.

**Why maintain both stages?** The SQL GLOB pre-filter uses the `stream_id` TEXT column with the type index, dramatically reducing the Rust-side work. For patterns like `cerebra/**`, SQL quickly narrows from millions of events down to only cerebra-stream events. Rust then verifies the handful of edge cases. The cost of dropping a few extra SQL rows in Rust is negligible compared to scanning the full table in Rust.

**Pure SQL approach would require:** Either loading all events (expensive) or implementing fossic segment semantics in SQLite (not possible without user-defined functions).

### 4.3 Full SQL

The full SQL for aggregate (with optional filters):

```sql
SELECT id, stream_id, branch, version, timestamp_us, causation_id, correlation_id,
       event_type, type_version, payload, external_id, indexed_tags
FROM events
WHERE stream_id GLOB ?1
  AND branch = ?2
  AND (?3 IS NULL OR event_type = ?3)
  AND (?4 IS NULL OR timestamp_us >= ?4)
  AND (?5 IS NULL OR timestamp_us <= ?5)
  AND (?6 IS NULL OR (
      indexed_tags IS NOT NULL
      AND json_extract(indexed_tags, '$.key1') = ?7
      AND json_extract(indexed_tags, '$.key2') = ?8
      -- ... one condition per indexed_tags_filter key
  ))
ORDER BY timestamp_us ASC
```

The `indexed_tags_filter` portion is dynamically generated — one `json_extract` condition per key in the filter object.

### 4.4 indexed_tags_filter — Pushdown Details

`indexed_tags_filter` must be a JSON object (not array, not scalar). Each key-value pair becomes an `AND json_extract(indexed_tags, '$.key') = value` condition.

**Key constraint:** Filter keys must match `[a-zA-Z0-9_]+` (alphanumeric and underscore). This is validated at query time (same constraint enforced at append time for stored keys). The reason: safe direct interpolation into `'$.key'` format without risk of SQL injection through the key string itself. Note: values are bound as SQL parameters (safe regardless of content).

**`indexed_tags IS NOT NULL` guard:** When any `indexed_tags_filter` is present, the SQL adds `AND indexed_tags IS NOT NULL`. Events with NULL `indexed_tags` never match a non-empty filter.

**No separate index for indexed_tags:** SQLite evaluates `json_extract` inline during the WHERE scan — there is no additional index on individual tag values. For high-frequency aggregate queries with tight tag filters, consider:
1. Narrow the `stream_pattern` to reduce the pre-filtered candidate set.
2. Add an `event_type_filter` to further reduce candidates.
3. For extremely performance-critical queries on known key-value pairs, create a generated column and index: `CREATE INDEX idx_events_project ON events(json_extract(indexed_tags, '$.project'))` (SQLite 3.25+).

**Matching semantics:** The comparison is exact string/value equality via `json_extract`. There is no LIKE, range query, or partial match available through `indexed_tags_filter`. For range queries by time, use `from_timestamp_us`/`to_timestamp_us`. For more complex filters, post-process the returned event list in application code.

### 4.5 Aggregate Trait (Rust)

```rust
pub trait Aggregate: 'static {
    type Output;
    fn fold(&mut self, event: &StoredEvent);
    fn finalize(self) -> Self::Output;
}
```

`fold` is called once per matching event in `timestamp_us ASC` order. `finalize` is called once when all events have been processed and returns the accumulated result.

The `Aggregate` trait allows zero-copy server-side aggregation without materializing all events into a `Vec`. A counting aggregator never allocates a vector of events:

```rust
struct CountByStream {
    counts: std::collections::HashMap<String, usize>,
}

impl Aggregate for CountByStream {
    type Output = std::collections::HashMap<String, usize>;

    fn fold(&mut self, event: &StoredEvent) {
        *self.counts.entry(event.stream_id.clone()).or_insert(0) += 1;
    }

    fn finalize(self) -> Self::Output {
        self.counts
    }
}

let counts = store.aggregate(
    AggregateQuery {
        stream_pattern: "cerebra/**".into(),
        branch: "main".into(),
        event_type_filter: Some("SignalEvaluated".into()),
        from_timestamp_us: Some(one_hour_ago_us),
        to_timestamp_us: None,
        indexed_tags_filter: None,
    },
    CountByStream { counts: std::collections::HashMap::new() },
)?;
// counts: {"cerebra/agent-trace/sess-abc": 42, "cerebra/agent-trace/sess-def": 17, ...}
```

### 4.6 CollectAll — Python's Aggregate

Python does not expose the `Aggregate` trait — it would require wrapping Python callables in a way that's complex across the GIL. Instead, Python uses a built-in `CollectAll` collector that gathers all matching events into a `Vec<StoredEvent>`:

```rust
struct CollectAll(Vec<fossic::StoredEvent>);

impl Aggregate for CollectAll {
    type Output = Vec<fossic::StoredEvent>;
    fn fold(&mut self, event: &fossic::StoredEvent) { self.0.push(event.clone()); }
    fn finalize(self) -> Self::Output { self.0 }
}
```

Python callers receive a `list[StoredEvent]` and fold it themselves:

```python
events = store.aggregate(AggregateQuery(
    stream_pattern="cerebra/**",
    branch="main",
    event_type_filter="SignalEvaluated",
))

# Fold in Python:
from collections import defaultdict, Counter
counts_by_stream = Counter(e.stream_id for e in events)
total_composite = sum(e.payload.get("composite_score", 0) for e in events)
```

The trade-off: `CollectAll` materializes all matching events into memory on the Python side. For queries returning millions of events, prefer Rust-side Aggregate implementations or narrow the query further.

### 4.7 Python AggregateQuery

```python
from fossic import AggregateQuery

query = AggregateQuery(
    stream_pattern="policy-scout/**",   # required
    branch="main",                       # default "main"
    event_type_filter="DecisionIssued", # optional, exact match
    from_timestamp_us=1_700_000_000_000_000,  # optional
    to_timestamp_us=None,                # optional
    indexed_tags_filter={                # optional, must be dict
        "decision_type": "approval",
        "session_id": "sess-xyz",
    },
)
events = store.aggregate(query)
```

The `indexed_tags_filter` dict is converted by the bridge via `py_to_json` (Python dict → JSON string → `serde_json::Value` via `json.dumps` + `serde_json::from_str`). Keys and values from Python dict must be JSON-serializable.

### 4.8 Aggregate Performance Characteristics

**Stream pattern selectivity matters most.** A wide pattern like `**` causes SQL to scan the entire events table (GLOB `*` matches everything). A narrow pattern like `policy-scout/posture` restricts the SQL scan to a single stream.

**Index usage by filter type:**
- `stream_pattern`: SQL uses `idx_events_type` only if the pattern starts with a non-wildcard prefix. For `**`, no useful index exists; full table scan.
- `event_type_filter`: `idx_events_type` on `event_type` can be used when combined with a narrow stream pattern.
- `from_timestamp_us`/`to_timestamp_us`: `idx_events_timestamp` — useful when the time range significantly narrows the result set.
- `indexed_tags_filter`: no index; inline `json_extract`. Always applied after SQL pre-filters.

**Rust post-filter cost:** Minimal for prefix patterns (`cerebra/**` → SQLite and fossic-glob agree). Non-negligible for single-wildcard patterns on multi-level streams (`cerebra/*` rejects many SQLite-returned rows).

---

## 5. SimilaritySearchProvider — Extension Point

### 5.1 Trait Declaration

```rust
pub trait SimilaritySearchProvider: Send + Sync + 'static {
    /// Called when an event with an embedding is appended.
    fn index(&self, event_id: EventId, embedding: &[f32]) -> Result<(), Error>;
    /// Run a k-nearest-neighbor query.
    fn query(&self, q: SimilarityQuery) -> Result<Vec<SimilarityHit>, Error>;
}

pub struct SimilarityQuery {
    pub embedding: Vec<f32>,
    pub k: usize,
    pub stream_pattern: Option<String>,  // fossic glob, None = search all streams
}

pub struct SimilarityHit {
    pub event_id: EventId,
    pub score: f32,
}
```

### 5.2 v1 Status

No implementation ships with fossic v1. The trait is the extension point. Consumers that need vector search register their own provider:

```rust
// Hypothetical consumer code (not fossic built-in):
struct HnswProvider { index: hnsw::HnswIndex }
impl SimilaritySearchProvider for HnswProvider { ... }

let store = Store::open(path, OpenOptions {
    similarity_provider: Some(Box::new(HnswProvider::new())),
    ..Default::default()
})?;
```

### 5.3 Design Rationale

Fossic does not bundle a vector database because:
1. Vector search library choice is highly deployment-specific (in-process vs. external, CPU vs. GPU, approximate vs. exact).
2. Embedding dimensionality, distance metrics, and index parameters vary by use case.
3. Adding a mandatory vector dependency would significantly increase binary size and compilation complexity for consumers that don't need similarity search.

The `index` method fires at append time for events that carry embeddings. This keeps the vector index in sync with the event store without requiring a separate indexing pipeline.

The `stream_pattern` in `SimilarityQuery` allows filtering which streams to search — useful when embeddings from different streams (e.g., `cerebra/agent-trace/*` vs. `cerebra/memory/*`) should be searched separately.

### 5.4 What v1 Does Not Include

- Persistent embedding storage in SQLite (embeddings are indexed in the external provider, not stored in the events table).
- Automatic re-indexing from a snapshot.
- Multi-modal embedding support (text, image, structured).
- Python binding for `SimilaritySearchProvider`.

---

## 6. Comparing the Three Mechanisms

| Dimension | read_by_correlation | walk_causation | aggregate |
|-----------|--------------------|--------------:|-----------|
| Lookup key | `correlation_id` value | `causation_id` pointer chain | stream_pattern + optional filters |
| Set by appender | Explicitly on each Append | Explicitly on each Append | N/A (stream_id is always set) |
| Branch scope | None (crosses all branches) | None (crosses all branches) | Yes (single `branch` param) |
| Stream scope | None (crosses all streams) | None (crosses all streams) | Yes (stream_pattern) |
| Result ordering | timestamp_us ASC | timestamp_us ASC | timestamp_us ASC |
| Performance ceiling | O(matching events) | O(depth × branching_factor) | O(GLOB-matching events) |
| Best for | Workflow grouping | Causal ancestry/descendants | Cross-stream analytics |
| Upcasters applied | Yes | Yes | Yes |

---

## 7. Practical Patterns

### Pattern: Unified workflow timeline

```python
# Reconstruct the complete timeline of a policy workflow
# (all events that share the same correlation_id)
def workflow_timeline(store, root_event_id):
    # Use the root event's id as correlation_id (if that convention was followed)
    events = store.read_by_correlation(root_event_id)
    return sorted(events, key=lambda e: e.timestamp_us)

timeline = workflow_timeline(store, policy_decision.id)
for ev in timeline:
    print(f"{ev.timestamp_us:>20}  {ev.stream_id:<40}  {ev.event_type}")
```

### Pattern: Agent step trace

```python
def trace_step(store, step_started_id, max_depth=10):
    """Return all events caused by a step, grouped by type."""
    events = store.walk_causation(step_started_id, direction="forward", max_depth=max_depth)
    from collections import defaultdict
    by_type = defaultdict(list)
    for ev in events:
        by_type[ev.event_type].append(ev)
    return dict(by_type)

trace = trace_step(store, step_id)
print(f"Predictions made: {len(trace.get('PredictionMade', []))}")
print(f"Signals evaluated: {len(trace.get('SignalEvaluated', []))}")
```

### Pattern: Root cause finder

```python
def find_root(store, event_id, max_depth=20):
    """Walk backward to find the root event in a causation chain."""
    ancestors = store.walk_causation(event_id, direction="backward", max_depth=max_depth)
    # Root has no causation_id
    for ev in ancestors:
        if ev.causation_id is None:
            return ev
    return ancestors[0] if ancestors else None
```

### Pattern: Activity report across streams

```python
import time
from fossic import AggregateQuery
from collections import Counter

def activity_report(store, hours=1):
    """Count events by type and stream for the last N hours."""
    cutoff = int((time.time() - hours * 3600) * 1_000_000)
    events = store.aggregate(AggregateQuery(
        stream_pattern="**",
        branch="main",
        from_timestamp_us=cutoff,
    ))
    return {
        "total": len(events),
        "by_type": Counter(e.event_type for e in events),
        "by_stream": Counter(e.stream_id for e in events),
    }
```

### Pattern: Narrowing aggregate with indexed_tags

```python
# Find all approval decisions for a specific policy band
events = store.aggregate(AggregateQuery(
    stream_pattern="policy-scout/audit/**",
    branch="main",
    event_type_filter="DecisionIssued",
    indexed_tags_filter={
        "band": "SENSITIVE",
        "outcome": "ALLOW",
    },
))
```

**Append side (required):** For the filter to work, appends must have included these keys:
```python
store.append(Append(
    stream_id="policy-scout/audit/sess-abc",
    event_type="DecisionIssued",
    payload={"cmd": "rm -rf /tmp/x", "outcome": "ALLOW"},
    indexed_tags={"band": "SENSITIVE", "outcome": "ALLOW"},  # must match filter keys
))
```

### Pattern: Time-bounded aggregate (sliding window)

```python
def events_in_window(store, pattern, window_us):
    """Events from the last `window_us` microseconds."""
    import time
    now_us = int(time.time() * 1_000_000)
    return store.aggregate(AggregateQuery(
        stream_pattern=pattern,
        branch="main",
        from_timestamp_us=now_us - window_us,
        to_timestamp_us=now_us,
    ))

# Events in the last 90 seconds on all cerebra streams
recent = events_in_window(store, "cerebra/**", 90 * 1_000_000)
```

---

## 8. Choosing the Right Mechanism

**Use `read_by_correlation` when:**
- Events are written by multiple systems/streams to document participation in a shared workflow.
- You set `correlation_id` at write time to group a logical transaction.
- You need a flat timeline of all effects from one initiating event.

**Use `walk_causation` when:**
- You need to traverse the causal graph (parent-child relationships via `causation_id`).
- You want to find all effects of one event (forward) or trace the chain of events that led to a specific outcome (backward).
- The causation relationship is explicit in your data model (`causation_id` set on each Append).

**Use `aggregate` when:**
- You want to query across streams by pattern (e.g., "all cerebra streams", "all policy-scout audit streams").
- You need time-window filtering, event-type filtering, or indexed_tags filtering.
- You're computing analytics (counts, sums, distributions) over a large set of events across many streams.
- Neither correlation_id nor causation_id is available or appropriate.

**The mechanisms compose.** `walk_causation` forward from a root event gives you all caused events; you can then group those by `correlation_id` or feed them into an aggregate. `read_by_correlation` gives you a workflow's events; you can then `walk_causation` from any of those to find their subtrees.

---

## 9. Error Cases

### read_by_correlation
- `Error::Sqlite(...)`: database-level errors (disk full, corrupted WAL, etc.).
- No `StreamNotDeclared` or `BranchNotFound` — the query is global.
- Empty result (`Vec::new()`) if no events carry that correlation_id.

### walk_causation
- `Error::Sqlite(...)`: database-level errors.
- Python: `PyValueError` if direction string is not `"forward"`, `"backward"`, or `"both"`.
- If `start` EventId does not exist in the store, the CTE anchor matches no rows — returns `Vec::new()` (no error).
- No `ReducerNotFound` or other domain errors — purely a read operation.

### aggregate
- `Error::InvalidIndexedTags { got }`: if `indexed_tags_filter` is not a JSON object.
- `Error::Sqlite(...)`: database errors.
- Python: `PyValueError` if `indexed_tags_filter` is not a Python dict.
- Empty result if no events match all filters.
- Does NOT return `StreamNotDeclared` — aggregate scans events regardless of whether streams are currently declared (a stream can have events even if its declaration row was somehow missing).
