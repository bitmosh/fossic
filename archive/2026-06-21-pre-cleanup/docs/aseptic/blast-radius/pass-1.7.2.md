# Blast Radius — pass-1.7.2 — HnswProvider persistence

**Version:** v1.7.2
**Date:** 2026-06-21
**Scope:** `crates/fossic-similarity-hnsw` only — no changes to core fossic, fossic-node, fossic-py, fossic-tauri

---

## Files changed

| File | Change |
|---|---|
| `crates/fossic-similarity-hnsw/src/provider.rs` | Full persistence implementation — save_to_disk, load-on-construction, atomic cleanup, system events, load_hnsw_catching_panics |
| `crates/fossic-similarity-hnsw/tests/integration.rs` | 3 new persistence tests (5 total new, 11 carried from v1.7.1 → 14 total) |
| `Cargo.toml` (root) | version 1.7.1 → 1.7.2 |
| `fossic-node/Cargo.toml` | version 1.7.1 → 1.7.2 |
| `fossic-py/Cargo.toml` | version 1.7.1 → 1.7.2 |
| `fossic-py/pyproject.toml` | version 1.7.1 → 1.7.2 |
| `crates/fossic-tauri/Cargo.toml` | version 1.7.1 → 1.7.2 |
| `crates/fossic-similarity-hnsw/Cargo.toml` | version 1.7.1 → 1.7.2 |
| `CHANGELOG.md` | v1.7.2 section added |

---

## What changed inside provider.rs

### New public API
- `HnswProvider::save_to_disk() -> Result<(), HnswError>` — persists live index atomically

### New private / pub(crate) code
- `save_mappings(&HnswInner)` — serializes MappingsFile with version byte prefix
- `save_empty_mappings()` — valid empty mappings.bin for zero-vector index
- `load_mappings()` — reads version byte, deserializes MappingsFile
- `try_load_or_init()` — called from `new()`; auto-loads if all three files exist, falls back to empty on corruption
- `load_inner(now_us)` — loads hnsw_rs graph via HnswIo + transmute + mappings; emits HnswIndexLoaded
- `cleanup_index_files()` — best-effort remove of all three files
- `load_hnsw_catching_panics::<D>(io)` — panic-catching wrapper; hnsw_rs uses assert_eq! for format validation

### New types
- `MappingsFile { usize_to_event_id, event_id_to_stream_id }` — serde derive, msgpack via rmp_serde
- `MAPPINGS_VERSION: u8 = 0x01`

### New system events
- `HnswIndexLoaded` — emitted after successful load; carries dimensions, vector_count, file sizes, timestamp_us
- `HnswIndexCorrupted` — emitted when load fails; carries error_message, attempted_path, timestamp_us

---

## Invariants / constraints preserved

- **Atomic save**: either all three files are present and consistent after save_to_disk, or none are (cleanup_index_files on any failure).
- **CP-D2-2**: unchanged — `SimilaritySearchProvider::index` still lacks stream_id; `index_with_stream_id` remains the workaround.
- **No background saves**: v1.7.2 does not schedule periodic saves. Callers must call `save_to_disk()` explicitly.
- **No mmap**: `HnswIo` uses `ReloadOptions::default()` which has `datamap: false`. The `'b → 'static` transmute is safe only under this constraint. Documented in load_hnsw_catching_panics.

---

## What was NOT changed

- Core fossic crate (`src/`): no changes
- `fossic-node`, `fossic-tauri`, `fossic-py`: version bumps only
- `HnswConfig`: no new fields
- `HnswError`: `MappingsVersionMismatch(u8)` and `IndexCorrupted(String)` were added in v1.7.1; this pass uses them but does not add new variants

---

## Test coverage

14 tests passing. Workspace: 283 tests passing, 0 failing.

New tests specifically for this pass:
- `persistence_round_trip_with_stream_filter`
- `save_and_load_empty_index`
- `corrupt_index_data_file_recovers_to_empty`
- `corrupt_mappings_version_byte_recovers_to_empty`
- `partial_save_cleans_up_all_files`

---

## Deviation from brief

Brief specified `index.bin` as the saved filename. hnsw_rs `file_dump` produces two files (`index.hnsw.data` + `index.hnsw.graph`) with no single-file format. Both treated as a unit for save/load/cleanup. Documented in provider.rs.
