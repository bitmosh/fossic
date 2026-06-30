---
pass: 5
version: v0.5.0
sha: 6bf6a54
date: 2026-06-12
summary: PyO3 Python binding for full fossic API surface + consumer profile docs
---

# Blast Radius — Pass 5 (v0.5.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content completely rewritten — original bootstrap estimate described Rust-core
> snapshots/reducers (which are v0.2.0). Actual commit 6bf6a54 added the full
> PyO3 Python binding (fossic-py) and consumer profile documentation.

## Files

### Created

33 files in commit 6bf6a54:

**fossic-py crate:**
- `fossic-py/Cargo.toml` — pyo3 0.29, fossic path dependency
- `fossic-py/Cargo.lock` — dependency lockfile for the binding
- `fossic-py/pyproject.toml` — Python package manifest (maturin build backend)
- `fossic-py/README.md` — binding installation and usage guide
- `fossic-py/src/lib.rs` — PyO3 module entry point, type registrations
- `fossic-py/src/store.rs` — `PyStore` class: all Store method bindings via PyO3
- `fossic-py/src/types.rs` — Python-compatible type structs (Append, ReadQuery, StoredEvent, etc.)
- `fossic-py/src/errors.rs` — Python exception hierarchy mapping FossicError variants
- `fossic-py/src/subscriptions.rs` — subscription handle Python class
- `fossic-py/python/fossic/__init__.py` — Python wrapper `Store` class, `SubscriptionHandle`; `read_state` implemented as pure-Python full-event replay (no DynReducer caching yet — TD-001 origin)
- `fossic-py/python/fossic/_worker.py` — `SubscriptionWorker` background thread for async event delivery

**fossic-py tests:**
- `fossic-py/tests/conftest.py` — shared fixtures: `tmp_store`, `declared_store`
- `fossic-py/tests/test_append_read.py` — append/read round-trip tests
- `fossic-py/tests/test_branches.py` — branch lifecycle tests via Python binding
- `fossic-py/tests/test_cross_stream.py` — cross-stream query tests via Python binding
- `fossic-py/tests/test_deletion.py` — purge tests via Python binding
- `fossic-py/tests/test_lifecycle.py` — stream declare/open tests
- `fossic-py/tests/test_reducers.py` — reducer registration and read_state tests
- `fossic-py/tests/test_subscriptions.py` — subscription delivery and lifecycle tests
- `fossic-py/tests/test_transforms.py` — payload transform tests
- `fossic-py/tests/test_upcasters.py` — upcaster chain tests

**Benchmarks:**
- `benchmarks/baseline.json` — latency baseline measurements for regression tracking
- `benchmarks/sqlite_wal_contention.py` — multi-writer WAL contention benchmark

**Consumer profile docs:**
- `docs/FOSSIC_CONSUMER_PROFILES.md` — API surface mapping for each consumer project
- `docs/adjacent-project-info/PROJECT_EXTRACTION_TEMPLATE.md` — template for consumer profiles
- `docs/adjacent-project-info/aistack_extract.md` — ai-stack integration notes
- `docs/adjacent-project-info/cerebra_extract.md` — cerebra integration design
- `docs/adjacent-project-info/cerebra_read_adapter_addendum.md` — cerebra read adapter notes
- `docs/adjacent-project-info/cerebra_read_adapter_design.md` — cerebra adapter design doc
- `docs/adjacent-project-info/discord_bot_extract.md` — bo/discord-bot integration notes
- `docs/adjacent-project-info/fossic-interview.md` — developer interview capturing design intent
- `docs/adjacent-project-info/lumaweave_extract.md` — LumaWeave/Graph B integration notes
- `docs/adjacent-project-info/policy_scout_extract.md` — policy-scout integration notes

---

## Public APIs

### Added

Python `Store` class (mirrors full Rust API surface via PyO3):
- `Store.open(path: str) -> Store` — open or create; tilde expansion NOT yet handled at this version (added v0.9.0)
- `Store.declare_stream(stream_id, declared_by, description) -> None`
- `Store.append(a: dict | Append) -> str` — returns hex EventId
- `Store.read_range(query: dict | ReadQuery) -> list[StoredEvent]`
- `Store.read_one(event_id: str) -> StoredEvent | None`
- `Store.read_since(stream_id, branch, from_version, limit) -> list[StoredEvent]`
- `Store.create_branch(b: dict | CreateBranch) -> None`
- `Store.promote_branch(stream_id, branch_id, reason=None) -> None`
- `Store.mark_branch_dead_end(stream_id, branch_id, reason=None) -> None`
- `Store.list_branches(stream_id) -> list[BranchInfo]`
- `Store.resolve_chain(stream_id, branch_id) -> list[BranchSegment]`
- `Store.register_reducer(pattern, reducer) -> None` — pure Python reducer; DynReducer protocol class attrs (name, version, state_schema_version) validated at registration
- `Store.read_state(stream_id, branch) -> Any` — pure-Python full-event replay (no snapshot caching via Rust DynReducer path — TD-001)
- `Store.take_snapshot(stream_id, branch) -> SnapshotInfo`
- `Store.snapshot_info(stream_id, branch, reducer_name) -> SnapshotInfo | None`
- `Store.gc_orphaned_snapshots() -> int`
- `Store.register_upcaster(event_type, from_version, to_version, callable) -> None`
- `Store.register_payload_transform(stream_pattern, callable) -> None`
- `Store.aggregate(query: dict | AggregateQuery) -> list[StoredEvent]`
- `Store.read_by_external_id(stream_id, external_id) -> StoredEvent | None`
- `Store.read_by_correlation(correlation_id: str) -> list[StoredEvent]`
- `Store.walk_causation(start: str, direction: str, max_depth: int) -> list[StoredEvent]`
- `Store.get_cursor(consumer_id, stream_id, branch) -> int | None`
- `Store.set_cursor(consumer_id, stream_id, branch, version: int) -> None`
- `Store.purge_event(event_id: str, confirm: str, reason: str, purged_by: str) -> None`
- `Store.subscribe(stream_id, branch) -> SubscriptionHandle`
- `SubscriptionHandle.next(timeout_ms=None) -> StoredEvent | None`
- `SubscriptionHandle.cancel() -> None`
- Python exception hierarchy: `FossicError`, `StreamNotDeclaredError`, `DuplicateStreamError`, `BranchNotFoundError`, `BranchLifecycleError`, `PurgeConfirmationError`, `EventNotFoundError`, `StorageError`, `ReducerPatternAmbiguousError`

---

## Schema changes

None — Python binding uses the same SQLite database as Rust core.

---

## Configuration changes

None.

---

## Dependency changes

In `fossic-py/Cargo.toml`:
- Added: `pyo3 = "0.29"` with `extension-module` and `abi3-py312` features
- Added: `fossic` (path dependency — the core crate)
- Added: `serde_json` — JSON value passing across the PyO3 boundary

---

## Behavior changes

- Python `read_state` is implemented as a pure-Python full-event replay. Correct output; does not use snapshot caching via the Rust DynReducer path (that path requires a public DynReducer trait, not yet exposed). Performance is O(all events) regardless of snapshots. **TD-001 origin.**
- The `.so` binary (`_fossic.cpython-312-x86_64-linux-gnu.so`) is pre-built and gitignored. Python consumers install via `pip install fossic` (wheel) or use the pre-built `.so` directly.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: TD-001 (Python DynReducer bridge cost) originates structurally from this pass — the pure-Python replay was the initial implementation. TD-001 was formally logged when the benchmark confirmed the latency cliff in v0.10.0.*
