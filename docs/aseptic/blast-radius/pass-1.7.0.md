---
pass: v1.7.0
version: v1.7.0
date: 2026-06-21
prior-commit: 29a29c9
summary: D2 foundation — fossic-similarity-hnsw crate scaffold + substrate visibility opens (SystemStreamWriter, BackgroundExecutor, TaskKind::Custom)
---

# Blast Radius — Pass v1.7.0

## Files

### Created
- `crates/fossic-similarity-hnsw/Cargo.toml`
- `crates/fossic-similarity-hnsw/README.md`
- `crates/fossic-similarity-hnsw/src/lib.rs`
- `crates/fossic-similarity-hnsw/src/config.rs`
- `crates/fossic-similarity-hnsw/src/error.rs`
- `crates/fossic-similarity-hnsw/src/provider.rs`
- `crates/fossic-similarity-hnsw/tests/integration.rs` (placeholder)
- `docs/aseptic/blast-radius/pass-1.7.0.md` — this file

### Modified
- `Cargo.toml` (root) — added `crates/fossic-similarity-hnsw` to `workspace.members`; bumped version 1.5.0 → 1.6.0 → 1.7.0
- `fossic-py/Cargo.toml` — version 0.1.0 → 1.7.0
- `fossic-node/Cargo.toml` — version 0.1.0 → 1.7.0
- `crates/fossic-tauri/Cargo.toml` — version 0.1.0 → 1.7.0
- `src/system_stream.rs` — `SystemStreamWriter` struct, `new`, `emit` changed from `pub(crate)` to `pub`
- `src/executor.rs` — `TaskPriority`, `TaskKind`, `BacklogTask`, `BackgroundExecutor` changed from `pub(crate)` to `pub`; `TaskKind::Custom` variant added; `execute_task` updated to dispatch Custom; inline test added
- `src/lib.rs` — added `pub use system_stream::SystemStreamWriter` and `pub use executor::{BackgroundExecutor, BacklogTask, TaskKind, TaskPriority}`
- `CHANGELOG.md` — v1.7.0 section added

---

## Commits (this pass)

| SHA | Message |
|---|---|
| `8964b8d` | chore: bump to v1.6.0 — formalize Phase 1 close in Cargo.toml |
| `5b330be` | feat: expose SystemStreamWriter for substrate extensions |
| `6944251` | feat: expose BackgroundExecutor for substrate extensions with TaskKind::Custom variant |
| `c5b7806` | feat(v1.7.0): fossic-similarity-hnsw crate scaffold |
| `9fb9a84` | chore: bump to v1.7.0 |
| `29a29c9` | chore(v1.7.0): add placeholder integration test file |

---

## Changes

### Version alignment

All four binding crates (fossic-py, fossic-node, fossic-tauri) were at `0.1.0`, fossic core at `1.5.0`. Phase 1 close was documented in CHANGELOG as v1.6.0 but never bumped in Cargo.toml. This pass formalizes the alignment: all five crates are now at `1.7.0`.

### SystemStreamWriter visibility

`SystemStreamWriter`, `new`, and `emit` are now `pub`. `emit_subscription_degraded` remains `pub(crate)` — it's an internal fossic protocol detail, not a substrate extension API.

Export added to `src/lib.rs`: `pub use system_stream::SystemStreamWriter`.

### BackgroundExecutor visibility + TaskKind::Custom

`BackgroundExecutor`, `BacklogTask`, `TaskKind`, `TaskPriority` are now `pub`. `BackgroundExecutor::spawn` remains `pub(crate)` — sibling crates schedule tasks, they don't create executors.

`TaskKind::Custom(Arc<dyn Fn() + Send + Sync + 'static>)` added. Deviates from brief's `FnOnce(&Store)` spec: using `Arc<dyn Fn()>` avoids a circular import between executor.rs and store.rs (`Store` lives in store.rs, `BackgroundExecutor` in executor.rs — importing `Store` into executor.rs creates a mutual dependency). Closures capture `Arc` context at scheduling time, which covers the HnswProvider save use case without the circular import.

`execute_task` match updated to call the closure for `Custom` arms. Name method updated to return `"Custom"`. Inline unit test `custom_task_closure_executes` verifies the closure runs.

Export added to `src/lib.rs`: `pub use executor::{BackgroundExecutor, BacklogTask, TaskKind, TaskPriority}`.

### fossic-similarity-hnsw crate scaffold

**`HnswConfig`**: All fields have defaults except `dimensions` (required). `with_dimensions(n)` builder method. `stream_filter_fudge_factor` (default 2) added per D2 spec — when `SimilarityQuery::stream_pattern` is set, the index is queried for `k × fudge_factor` candidates, filtered by stream pattern, then truncated to `k`.

**`DistanceMetric`**: `Cosine | Euclidean | InnerProduct`.

**`HnswError`**: Crate-local error type using `thiserror`. Variants: `InvalidDimensions`, `IndexCorrupted`, `MappingsVersionMismatch`, `Io`, `MsgpackEncode`, `MsgpackDecode`, `Hnsw`. Implements `From<HnswError> for fossic::Error` (wraps as `Error::Internal`).

**`HnswProvider`**: Struct skeleton implementing `SimilaritySearchProvider` via the actual trait (`index` + `query`, not `add/search`). Validates dimensions in both methods; returns empty results. Full HNSW logic lands in v1.7.1. Holds `Mutex<Option<HnswInner>>` (inner HNSW state) and `Mutex<Option<SystemStreamWriter>>` (lazy-init system event writer).

`index_dir` is `<parent_of_store_db>/hnsw/` — created at `HnswProvider::new` time. `index_bin_path()` and `mappings_bin_path()` accessors for v1.7.2 persistence.

**Dependencies**: `hnsw_rs = "0.3"`, `thiserror = "2"`, `serde_json` (via workspace), `rmp-serde` (via workspace), `parking_lot` (via workspace), `fossic` (via workspace).

---

## Test Results

**172 passed · 1 failed · 0 skipped**

Failing test: `post_commit_overflow_writes_system_event` (subscriptions suite) — pre-existing timing-sensitive test (200ms sleep awaiting dispatcher write). Passes when run in isolation. Unrelated to D2 changes — no subscription or dispatch code was modified.

---

## CP Notes

**CP-D2-1 (TaskKind::Custom):** Brief specified `FnOnce(&Store) + Send + 'static`. Implemented as `Arc<dyn Fn() + Send + Sync + 'static>` to avoid circular module import between executor.rs and store.rs. Arc<Fn> is also Clone (required by TaskKind::Clone derive) and supports recurring tasks. For the HnswProvider save use case, the closure captures `Arc<parking_lot::Mutex<...>>` at schedule time — no store reference needed.

---

## Sharp Edges

**`BackgroundExecutor::spawn` remains `pub(crate)`**: Sibling crates get the executor via `Store::background_executor()` (to be added in v1.7.3) — they do not call spawn themselves.

**`SystemStreamWriter::emit_subscription_degraded` remains `pub(crate)`**: Internal fossic protocol, not part of the extension API surface.

**`HnswProvider` dimensions validation in stub**: Even in the skeleton, `index` and `query` validate that embedding dimensions match `config.dimensions`. Callers get `InvalidDimensions` immediately rather than silently indexing wrong-dimension vectors.
