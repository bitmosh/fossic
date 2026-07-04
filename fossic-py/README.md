# fossic-py

PyO3 Python bindings for [fossic](../README.md), the local-first event sourcing library.

The Python API mirrors the Rust API with synchronous semantics. An async wrapper (`fossic_aio`) for asyncio consumers is published separately and wraps these bindings with `asyncio.to_thread`.

## Installation

Built with [maturin](https://github.com/PyO3/maturin):

```sh
pip install maturin
maturin develop          # editable install for development
maturin build --release  # build a wheel
```

## Quick start

```python
import os
from fossic import Store, Append, ReadQuery, OpenOptions, SubscriptionMode

# fossic does not expand tilde paths — expand before calling Store.open.
store = Store.open(
    path=os.path.expanduser("~/.fossic/store.db"),
    options=OpenOptions(
        encryption="plaintext",
        on_first_open="create_if_missing",
    ),
)

store.declare_stream("cerebra/lattice/abc123", declared_by="cerebra")

event_id = store.append(Append(
    stream_id="cerebra/lattice/abc123",
    event_type="MemoryRecordCommitted",
    type_version=1,
    payload={"content_hash": "...", "source": "..."},
))

events = store.read_range(ReadQuery(
    stream_id="cerebra/lattice/abc123",
    branch="main",
    from_version=0,
))
```

## Subscription delivery

Callbacks run on a Python-owned worker thread (not a Rust-spawned thread). This preserves `threading.local` state, asyncio contextvars, and logging context. See §4.2 of `docs/implement/FOSSIC_V1_SPEC.md` for the full explanation.

```python
with store.subscribe(
    stream_pattern="cerebra/lattice/*",
    mode=SubscriptionMode.post_commit(queue_size=1024),
) as sub:
    for event in sub:
        process(event)
```

## Bounded reads and streaming iterators

### Why bounded variants exist

`read_range` and `walk_causation` load all matching events into memory. On a stream that has grown to millions of events, this is an OOM risk. The bounded variants add a result count and/or byte budget; they return a `ReadOutcome` that tells you whether all results fit or the query was truncated.

### ReadOutcome

```python
from fossic import Store, ReadQuery, ReadOutcome, TruncationCursor

outcome = store.read_range_bounded(
    ReadQuery(stream_id="cerebra/lattice/session_42"),
    max_results=1000,
)

if outcome.complete:
    process_all(outcome.results)
elif outcome.is_truncated:
    process_page(outcome.results)
    # outcome.reason: "result_count" | "byte_size"
    # outcome.next_cursor: TruncationCursor | None
    if outcome.next_cursor:
        next_page = store.read_range_bounded(
            ReadQuery(stream_id="cerebra/lattice/session_42"),
            max_results=1000,
            cursor=outcome.next_cursor,
        )
```

Properties:
- `.results` — `list[StoredEvent]`, always present
- `.is_truncated` — `bool`
- `.complete` — `bool` (complement of `is_truncated`)
- `.reason` — `"result_count"` | `"byte_size"` | `None`
- `.next_cursor` — `TruncationCursor | None`

### TruncationCursor

Cursors are opaque. Pass them back to the next bounded call. Serialize with `.to_bytes()` and reconstruct with `TruncationCursor.from_bytes(b)`:

```python
# Persist a cursor for later resume:
cursor_bytes = outcome.next_cursor.to_bytes()

# Restore:
cursor = TruncationCursor.from_bytes(cursor_bytes)
next_page = store.read_range_bounded(query, cursor=cursor)
```

### SamplingMode

```python
from fossic import SamplingMode

# Full BFS — all reachable nodes up to max_depth (default):
SamplingMode.exhaustive()

# BFS capped per depth level:
SamplingMode.breadth_first(max_per_level=50)

# Adaptive — adjusts per-level cap to approach target_count total:
SamplingMode.adaptive(target_count=200)
```

### Streaming iterators

Each `__next__()` call fetches a batch of 100 events from the store and releases the read-pool connection before returning. Use standard `for` loops:

```python
for event in store.read_range_iter(ReadQuery(stream_id="cerebra/lattice/session_42")):
    process(event)

for event in store.walk_causation_iter(
    start=root_id,
    direction="forward",
    max_depth=100,
    sampling=SamplingMode.exhaustive(),
):
    process(event)
```

Iterators do not support cursor resume. For resumable streaming, use `read_range_bounded` in a loop.

### Bounded methods on Store

```python
store.read_range_bounded(query, max_results=None, max_bytes=None, cursor=None) -> ReadOutcome
store.read_by_correlation_bounded(correlation_id, max_results=None, max_bytes=None, cursor=None) -> ReadOutcome
store.walk_causation_bounded(start, direction="forward", max_depth=100,
    sampling=None, max_results=None, max_bytes=None, cursor=None) -> ReadOutcome

store.read_range_iter(query) -> RangeIter
store.read_by_correlation_iter(correlation_id) -> CorrelationIter
store.walk_causation_iter(start, direction="forward", max_depth=100,
    sampling=None) -> CausationIter
```

### OpenOptions note (CP-FOSSIC-3)

`default_max_results` and `default_max_bytes` store-level defaults are **not yet exposed** in the Python `OpenOptions`. Per-call limits work; if both are absent, the Rust layer applies no budget (unbounded behavior). This will be resolved in a follow-up pass.

## Similarity search (HNSW)

Wire in vector similarity via `fossic.similarity.HnswProvider` — backed by the `fossic-similarity-hnsw` Rust crate:

```python
from fossic.similarity import HnswProvider, SimilarityQuery

# Open (or create) the provider. Index files live in hnsw/ beside store.db.
provider = HnswProvider("store.db", dimensions=1024)

# Index an event with its stream ID for stream-pattern filtering.
provider.index_with_stream_id(event_id_bytes, "cerebra/lattice/abc", embedding)

# Query k nearest neighbours.
sq = SimilarityQuery(embedding=query_vec, k=10, stream_pattern="cerebra/lattice/*")
for hit in provider.query(sq.as_dict()):
    print(hit["event_id"].hex(), hit["score"])

# Persist synchronously.
provider.save()

# Or schedule a deferred background save (fires after quiescence_window_ms idle):
provider.schedule_save(priority="low")
```

The `HnswProvider` class is also importable directly from `fossic`:

```python
from fossic import HnswProvider, SimilarityQuery
```

**Config kwargs** (all optional except `dimensions`):

| Kwarg | Default | Notes |
|---|---|---|
| `dimensions` | — | **Required.** Must match your embedding model. |
| `distance` | `"cosine"` | `"cosine"`, `"euclidean"` / `"l2"`, `"inner_product"` / `"dot"` |
| `max_elements` | 100 000 | Capacity hint |
| `ef_construction` | 200 | Build-time recall knob |
| `m` | 16 | Graph degree per node |
| `ef_search` | 50 | Query-time recall knob |
| `stream_filter_fudge_factor` | 2 | Candidate expansion for stream-filtered queries |
| `quiescence_window_ms` | 2 000 | Idle window before `schedule_save` fires; lower in tests |

See [`crates/fossic-similarity-hnsw/README.md`](../crates/fossic-similarity-hnsw/README.md) for the full Rust API, persistence model, and performance notes.

## Requirements

- Python 3.12+
- PyO3 0.29+ (free-threaded Python 3.13+/3.14 supported)
- Rust stable toolchain

## License

MIT
