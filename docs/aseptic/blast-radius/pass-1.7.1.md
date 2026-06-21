---
pass: v1.7.1
version: v1.7.1
date: 2026-06-21
prior-commit: 1735cb3
summary: D2 core — HnswProvider full HNSW implementation with stream-pattern filtering and integration tests
---

# Blast Radius — Pass v1.7.1

## Files

### Modified
- `crates/fossic-similarity-hnsw/src/provider.rs` — full rewrite: `HnswIndex` enum, `HnswInner` struct, `SimilaritySearchProvider::index` + `query` live, inherent `index_with_stream_id` / `len` / `is_empty` / `remove`
- `crates/fossic-similarity-hnsw/tests/integration.rs` — 11 integration tests (placeholder replaced)
- `CHANGELOG.md` — v1.7.1 section added
- `Cargo.toml` (root) — version bumped to `1.7.1`
- `fossic-py/Cargo.toml` — version `1.7.0` → `1.7.1`
- `fossic-node/Cargo.toml` — version `1.7.0` → `1.7.1`
- `crates/fossic-tauri/Cargo.toml` — version `1.7.0` → `1.7.1`
- `crates/fossic-similarity-hnsw/Cargo.toml` — version `1.7.0` → `1.7.1`
- `docs/aseptic/blast-radius/pass-1.7.1.md` — this file

---

## Commits (this pass)

| SHA | Message |
|---|---|
| (substantive) | feat(v1.7.1): HnswProvider core HNSW implementation + integration tests |
| (bump) | chore: bump to v1.7.1 |
| (blast-radius) | chore(v1.7.1): blast-radius doc |

---

## Changes

### HnswIndex enum

Three variants — `Cosine(Hnsw<'static, f32, DistCosine>)`, `Euclidean(Hnsw<'static, f32, DistL2>)`, `InnerProduct(Hnsw<'static, f32, DistDot>)` — dispatch `insert`, `search`, and `nb_points` through match arms. Distance type imports via `hnsw_rs::anndists::dist::distances::{DistCosine, DistL2, DistDot}` (hnsw_rs re-exports anndists as `pub use anndists`).

`HnswIndex` is `pub(crate)` so `HnswInner.index` field can be `pub(crate)` without a private-interfaces warning.

### HnswInner struct

- `index: HnswIndex` — the hnsw_rs index
- `usize_to_event_id: Vec<EventId>` — position `n` holds the EventId for the vector inserted with hnsw_rs DataId `n`. Grows in lock-step; never shuffled.
- `event_id_to_stream_id: HashMap<EventId, String>` — only populated via `index_with_stream_id`. CP-D2-2 documents why this map is sparse.
- `next_id: usize` — monotonically incrementing insert counter. Stays == `usize_to_event_id.len()`.

`HnswInner` is lazily initialized — `Mutex<Option<HnswInner>>` stays `None` until the first `index()` call.

### SimilaritySearchProvider::index

1. Validates `embedding.len() == config.dimensions` → `HnswError::InvalidDimensions` (wrapped as `fossic::Error::Internal`).
2. Lock `inner`, `get_or_insert_with(|| HnswInner::new(...))`.
3. `inner.index.insert(embedding, next_id)`.
4. Push `event_id` to `usize_to_event_id`; increment `next_id`.
5. Stream_id not populated (CP-D2-2).

### SimilaritySearchProvider::query

1. Validates `q.embedding.len() == config.dimensions`.
2. Early return empty if `q.k == 0` or index is empty.
3. `internal_k = k × fudge_factor` when `stream_pattern` is Some, else `k`.
4. `inner.index.search(&q.embedding, internal_k, config.ef_search)`.
5. For each `Neighbour { d_id, distance }`: look up `usize_to_event_id[d_id]`; if `stream_pattern` set, check `event_id_to_stream_id.get(event_id)` against `fossic::glob::matches`; events without a registered stream_id fail the check (excluded from filtered results).
6. Collect into `Vec<SimilarityHit>`, break when `hits.len() >= k`.

### Inherent methods

- `index_with_stream_id(event_id, stream_id, embedding)` — same insert path as `index` but also sets `event_id_to_stream_id[event_id] = stream_id`. This is the correct path for stream-pattern-filterable indexing.
- `len() -> usize` — `hnsw.get_nb_point()` or 0 if not yet initialized.
- `is_empty() -> bool` — complement of `len()`.
- `remove(event_id) -> Result<(), HnswError>` — always returns `Err(HnswError::Hnsw("not supported"))`. hnsw_rs 0.3.4 has no delete API. Full deletion support deferred; documented.

### Persistence helpers (scaffolded for v1.7.2)

- `index_basename() -> String` — returns `"index"` (hnsw_rs `file_dump` prefix, produces `index.hnsw.data` + `index.hnsw.graph`)
- `mappings_bin_path() -> PathBuf` — `<index_dir>/mappings.bin` for msgpack-serialized mapping tables

Both have `#[allow(dead_code)]` until v1.7.2 activates them.

---

## Test Results

**All 11 integration tests pass · 0 failed · 0 skipped**

Full workspace: all suites green (0 failures).

The `post_commit_overflow_writes_system_event` timing-sensitive test that had intermittent failures in v1.7.0 passed in this run.

---

## CP Notes

**CP-D2-2 (stream_id gap in trait):** Documented in provider.rs and CHANGELOG. `SimilaritySearchProvider::index` does not receive `stream_id`. Events indexed via the trait path are excluded from stream-pattern filtered queries (not silently included with wrong matches — a deliberate safe default). The inherent `index_with_stream_id` is the correct path. Trait signature fix deferred to v2 of the trait.

---

## Sharp Edges

**`usize_to_event_id` is never compacted.** If v1.7.2+ adds tombstoning for soft deletes, the Vec must be padded (not truncated) to maintain DataId invariant.

**fudge_factor behavior when index is small.** If the index has fewer points than `internal_k`, hnsw_rs returns fewer results than requested — the stream filter then sees fewer candidates and may return fewer than `k` results even when `k` matching events exist. This is documented behavior, not a bug.

**hnsw_rs max_layer is hardcoded to 16.** NB_LAYER_MAX in hnsw_rs defaults to 16; no config knob exists. This is fine for the expected `max_elements` range (up to ~10M points).
