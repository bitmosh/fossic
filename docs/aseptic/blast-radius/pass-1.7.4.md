# Blast Radius — pass-1.7.4 — Python binding for HnswProvider

**Version:** v1.7.4
**Date:** 2026-06-21
**Scope:** `fossic-py/` (Rust binding + Python layer + tests), `crates/fossic-similarity-hnsw/README.md`, `README.md`, `fossic-py/README.md`, workspace Cargo.toml

---

## Files changed

| File | Change |
|---|---|
| `Cargo.toml` (root) | `fossic-similarity-hnsw` added to `[workspace.dependencies]` |
| `fossic-py/Cargo.toml` | `fossic-similarity-hnsw = { workspace = true }` added |
| `fossic-py/src/similarity.rs` | New — `PyHnswProvider` PyO3 binding |
| `fossic-py/src/lib.rs` | `mod similarity;` + `m.add_class::<PyHnswProvider>()` |
| `fossic-py/python/fossic/similarity.py` | New — `SimilarityQuery` dataclass + `HnswProvider` re-export |
| `fossic-py/python/fossic/__init__.py` | `from fossic.similarity import HnswProvider, SimilarityQuery` added |
| `fossic-py/tests/test_similarity.py` | New — 17 Python integration tests |
| `crates/fossic-similarity-hnsw/README.md` | Skeleton replaced with full README |
| `README.md` | Crate table updated; similarity quick-start section added |
| `fossic-py/README.md` | HNSW section added |
| `Cargo.toml` (root + 4 crates) | 1.7.3 → 1.7.4 |
| `fossic-py/pyproject.toml` | 1.7.3 → 1.7.4 |
| `CHANGELOG.md` | v1.7.4 section added |

---

## What changed in fossic-py/src/similarity.rs

`PyHnswProvider` is a `#[pyclass(name = "HnswProvider")]` struct holding:
- `provider: Arc<HnswProvider>` — the HNSW index
- `store: Store` — internal executor host for `schedule_save`

The Store is opened on the same `db_path` as the index. Its only role is providing the background executor; no events are written to it. Custom tasks write only to `hnsw/`.

### Constructor signature

```rust
#[pyo3(signature = (db_path, dimensions, *, distance="cosine", max_elements=100_000, 
    ef_construction=200, m=16, ef_search=50, stream_filter_fudge_factor=2, 
    quiescence_window_ms=2_000))]
```

`quiescence_window_ms` maps to `OpenOptions::executor_quiescence_window_ms` on the internal Store. Exposed so test code can use short windows (e.g. 100ms) without sleeping 2 seconds per test.

### Methods exposed

| Python method | Rust target |
|---|---|
| `.index(event_id, embedding)` | `SimilaritySearchProvider::index` (trait, no stream_id) |
| `.index_with_stream_id(event_id, stream_id, embedding)` | `HnswProvider::index_with_stream_id` |
| `.query(dict) -> list[dict]` | `SimilaritySearchProvider::query` |
| `.save()` | `HnswProvider::save_to_disk` |
| `.schedule_save(priority="low")` | `HnswProvider::schedule_save(provider.clone(), &self.store, prio)` |
| `.len()` | `HnswProvider::len` |
| `.is_empty()` | `HnswProvider::is_empty` |
| `.remove(event_id)` | `HnswProvider::remove` (always errors in v1) |
| `.is_dirty()` | `HnswProvider::is_dirty` |
| `.is_save_pending()` | `HnswProvider::is_save_pending` |

### schedule_save semantics

`schedule_save` calls `HnswProvider::schedule_save(self.provider.clone(), &self.store, prio)`. The Weak<HnswProvider> in the closure captures the PyHnswProvider's provider — if the Python object is garbage-collected before the executor window opens, the Weak upgrade fails and no save occurs.

### Concurrent Store note

If the user also holds a `fossic.Store` on the same `db_path`, two SQLite connections exist. Custom tasks write only to `hnsw/`, not SQLite. Schema-setup writes during open may serialize briefly under WAL lock but both opens succeed. Documented in the class docstring and fossic-py README.

---

## What changed in fossic/similarity.py

`SimilarityQuery` dataclass:
```python
@dataclasses.dataclass
class SimilarityQuery:
    embedding: list
    k: int
    stream_pattern: Optional[str] = None
    
    def as_dict(self) -> dict: ...
```

`HnswProvider` is a direct re-export of `_fossic.HnswProvider` (the PyO3 class). No Python wrapper needed — all methods are on the Rust class.

`fossic.__init__` now re-exports both:
```python
from fossic.similarity import HnswProvider, SimilarityQuery
```

---

## Test coverage (17 Python tests)

**Basic ops:** empty query, index+query roundtrip, wrong dims raises, zero-k, len/is_empty, 32-byte event_id constraint, top-k bounded, remove raises

**Persistence:** round-trip (save + reload from new instance), save empty index, dirty flag lifecycle

**SimilarityQuery helper:** as_dict, stream_pattern field

**Stream filtering:** index_with_stream_id correctly excludes non-matching stream

**Background scheduling:** fires when dirty (quiescence_window_ms=100, sleep 800ms), noop when not dirty, storm prevention (100 schedule_save calls → one pending)

All 17 tests pass. Workspace Rust tests: 288+ passing, 0 failures (post_commit_overflow is a pre-existing timing flake, passes in isolation).

---

## Invariants preserved

- **v1.7.2–1.7.3 API contract**: unchanged — no modifications to existing Rust methods
- **CP-D2-2**: Python `.index()` still goes through trait path (no stream_id); stream-filtered queries require `.index_with_stream_id()`
- **CP-D2-3**: `schedule_save` still takes `&Store` internally; Python users don't need to supply a Store (it's created internally)
- **Two-file atomic save**: unchanged
- **Panic recovery**: unchanged

---

## What was NOT changed

- Any existing Rust methods in `fossic` or `fossic-similarity-hnsw`
- The `fossic-node` binding (HNSW exposure deferred)
- `SUBSTRATE_EXTENSION_PATTERNS.md` (no new CPs this pass)

---

## Upcoming: v1.8.0

D2 close commit: version bump across all five crates to v1.8.0 + CHANGELOG narrative covering the full v1.7.0 → v1.8.0 arc.
