# fossic-similarity-hnsw

HNSW-backed implementation of [`fossic::SimilaritySearchProvider`](https://crates.io/crates/fossic) via [`hnsw_rs`](https://crates.io/crates/hnsw_rs).

Enables semantic search over fossic event payloads at 50k–500k vector scale. Wire it into a `Store` via `OpenOptions::similarity_provider`, or use it standalone from Python via `fossic.similarity.HnswProvider`.

## Installation

```toml
[dependencies]
fossic = "1.8.3"
fossic-similarity-hnsw = "1.8.3"
```

## Quick start (Rust)

```rust
use fossic::{OpenOptions, Store};
use fossic_similarity_hnsw::{HnswConfig, HnswProvider};
use std::sync::Arc;

// 1. Create the provider (opens or creates the hnsw/ directory beside the db).
let config = HnswConfig { dimensions: 1024, ..HnswConfig::default() };
let provider = Arc::new(HnswProvider::new("store.db", config)?);

// 2. Wire it into the Store so new appends can call provider.index().
let store = Store::open("store.db", OpenOptions {
    similarity_provider: Some(provider.clone()),
    ..Default::default()
})?;

// 3. Index an event embedding directly.
provider.index_with_stream_id(event_id, "docs/embeddings/abc123", &embedding)?;

// 4. Query k nearest neighbours.
use fossic::SimilarityQuery;
let hits = provider.query(SimilarityQuery {
    embedding: query_vec,
    k: 10,
    stream_pattern: Some("docs/embeddings/*".into()),
})?;

// 5. Persist.
provider.save_to_disk()?;
```

## Quick start (Python)

```python
from fossic.similarity import HnswProvider, SimilarityQuery

provider = HnswProvider("store.db", dimensions=1024)
provider.index_with_stream_id(event_id_bytes, "docs/embeddings/abc123", embedding)

sq = SimilarityQuery(embedding=query_vec, k=10, stream_pattern="docs/embeddings/*")
results = provider.query(sq.as_dict())
# results: [{"event_id": bytes, "score": float}, ...]

provider.save()
```

## HnswConfig fields and defaults

| Field | Default | Notes |
|---|---|---|
| `dimensions` | **required** | Must match your embedding model output exactly. |
| `max_elements` | 100 000 | Capacity hint; hnsw_rs resizes if exceeded. |
| `ef_construction` | 200 | Recall knob during index build. Higher = better recall, slower build. |
| `m` | 16 | HNSW graph degree per node. Values 8–64; 16 is a solid default. |
| `ef_search` | 50 | Recall knob at query time. Higher = better recall, slower queries. |
| `distance` | `Cosine` | `Cosine`, `Euclidean` (L2), or `InnerProduct` (dot product). |
| `stream_filter_fudge_factor` | 2 | Candidate-set multiplier for stream-filtered queries. See below. |

**Cosine vs. InnerProduct:** use `Cosine` for normalized embeddings (text, image). Use `InnerProduct` when your model produces un-normalized vectors where magnitude encodes relevance. Use `Euclidean` for coordinate-space embeddings.

**stream_filter_fudge_factor:** when `SimilarityQuery::stream_pattern` is set, the raw candidate set is expanded by this factor before filtering. A factor of 2 with `k=10` fetches 20 candidates from hnsw_rs before stream-filtering down to 10. Increase it when the matching stream is a small fraction of the index.

## Persistence model

Calling `save_to_disk()` writes three files to the `hnsw/` directory beside `store.db`:

| File | Contents |
|---|---|
| `index.hnsw.data` | hnsw_rs vector data file |
| `index.hnsw.graph` | hnsw_rs graph structure file |
| `mappings.bin` | msgpack-encoded ID map: `usize_id → EventId`, `EventId → stream_id` |

The save is **atomic across all three**: if any write fails, all three files are removed before the error is returned. There is never a partial save on disk — a recovered or restarted process either loads all three or starts with an empty index.

At construction, if all three files are present they are loaded automatically. If any are missing the index starts empty (triggering `HnswIndexBuilt`). If files exist but are corrupt, `HnswIndexCorrupted` is emitted and the index starts empty rather than panicking (see *Panic recovery* below).

The `mappings.bin` file includes a version byte (`0x01`) at offset 0 for forward compatibility.

## Background save pattern with `schedule_save`

For hot-write workloads, call `schedule_save` instead of `save_to_disk` directly:

```rust
// After indexing new events, schedule a deferred save.
// The closure captures a Weak<HnswProvider> — if the provider is dropped
// before the executor window opens, the save is a silent no-op.
HnswProvider::schedule_save(provider.clone(), &store, TaskPriority::Low);
```

`schedule_save` implements **storm prevention**: it stamps `save_pending = true` at schedule time (not at execution time — the "optimistic stamp" pattern). Calling it 1 000 times in a hot loop schedules exactly one task.

The task fires after the store has been idle for `executor_quiescence_window_ms` milliseconds (default 2 000 ms). If the provider is dropped before then, `Weak::upgrade()` returns `None` and the closure exits cleanly — no panic, no partial files, in-memory state is lost.

`TaskKind::Custom` tasks are not `persist_on_drop`, so no `DeferredTaskDropped` event is emitted on shutdown.

## Two-file storage format

`hnsw_rs` produces two files per `file_dump` call: `.hnsw.data` and `.hnsw.graph`. There is no single-file option in hnsw_rs 0.3.x. The library uses one file for point data and one for the graph adjacency structure. Both must be present to load an index; `save_to_disk` treats them as a unit for cleanup.

## Panic recovery from corrupted files

`hnsw_rs` uses `assert_eq!` internally for format validation. A corrupt `.hnsw.data` file triggers a panic rather than returning an error. `HnswProvider` wraps `load_hnsw` in `std::panic::catch_unwind(AssertUnwindSafe(...))` to catch these panics and convert them to `HnswError::IndexCorrupted`. The corrupt files are left in place (they may be useful for debugging); the provider starts with an empty index and emits `HnswIndexCorrupted` to the system stream.

## Performance characteristics

Informal numbers from a single developer machine (not a benchmark commitment):

- **Index throughput:** ~100k–500k inserts/second for 128-dim vectors at m=16 ef_construction=200.
- **Query latency:** sub-millisecond for k=10 at ef_search=50 with 100k vectors.
- **Memory footprint:** roughly 200 bytes per indexed vector at m=16, plus embedding storage.
- **Save latency:** ~50–200ms for a 100k-vector index (two hnsw_rs file dumps + msgpack mappings).

Exact numbers depend on embedding dimensionality, CPU cache behaviour, and `ef_construction`/`ef_search` settings. Dedicated benchmarks are deferred to a later pass.

## Stream-pattern filtering

Vectors indexed via `SimilaritySearchProvider::index` (the trait method) are **not** registered with a stream ID. `SimilarityQuery::stream_pattern` queries will exclude them. Use `HnswProvider::index_with_stream_id` to register stream provenance when you need filtered queries.

This is an intentional constraint: the trait signature has no `stream_id` parameter, preserving provider interchangeability. The `index_with_stream_id` inherent method is an opt-in extension for callers that need stream filtering.

## System events

The provider emits events to the `_fossic/system` stream with `indexed_tags = {"event_class": "hnsw"}`:

| Event type | When |
|---|---|
| `HnswIndexBuilt` | Index starts empty (first open or after corrupt recovery) |
| `HnswIndexLoaded` | Existing index loaded successfully from disk |
| `HnswIndexCorrupted` | Load failed; provider started empty |

## License

MIT
