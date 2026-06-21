# Changelog

All notable changes to fossic are documented here.
Format: semantic version sections, newest first. Each section links to the pass report.

---

## v1.2.0 — 2026-06-20 — SnapshotPolicy: EveryNEvents registration and wiring

**Pass report:** `docs/aseptic/blast-radius/pass-1.2.0.md`

### Added

- `SnapshotPolicy` — public enum: `Manual` (default), `EveryNEvents(u32)`, `EveryNSeconds(u32)`,
  `StateAdaptive { target_replay_cost_us, min_events_between }`. Re-exported from crate root.
- `Error::SnapshotPolicyInvalid(String)` — returned when a policy is structurally invalid
  (e.g. `EveryNEvents(0)`).
- `ReducerRegistry::register_with_policy` / `register_dyn_with_policy` — register a reducer
  with an explicit `SnapshotPolicy`; existing `register` / `register_dyn` delegate to these
  with `Manual`.
- `ReducerRegistry::find_arc_with_policy` — returns `Option<(Arc<dyn BoxedReducer>, SnapshotPolicy)>`;
  existing `find_arc` delegates to this.
- `reducers::validate_snapshot_policy(policy) -> Result<(), Error>` — standalone validation
  function; called at registration time.
- `Store::register_reducer_with_policy<R: Reducer>` — public surface for typed reducers.
- `Store::register_dyn_reducer_with_policy` — public surface for `DynReducer` bridges.
- `StoreInner::snapshot_counters: parking_lot::RwLock<HashMap<(String, String), u32>>` — per-
  `(stream_id, branch)` accumulating event counter; resets to 0 when a snapshot fires.
- `Store::read_state` and `Store::read_state_bytes` wired to call `maybe_auto_snapshot` after
  folding. Historical variants (`read_state_at_version`, `read_state_bytes_at_version`) are
  intentionally not wired (historical reads must not advance the snapshot cadence counter).
- `tests/snapshot_policy.rs` — 7 tests covering policy validation and `EveryNEvents` behavior.

### Not yet implemented

- `EveryNSeconds` and `StateAdaptive` return `Error::NotImplemented` at registration time.
  `EveryNSeconds` requires the Phase 7 background executor (v1.3.x);
  `StateAdaptive` requires v1.2.1 state-size monitoring.

---

## v1.1.2 — 2026-06-20 — Bounded Resource API: read_range_bounded + read_by_correlation_bounded

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.2.md`

### Added

- `Store::read_range_bounded(q, max_results, max_bytes, resume)` — bounded `read_range`
  variant. Stops at `max_results` events or `max_bytes` payload, whichever is first.
  Always returns at least one event (byte-budget at-least-one guarantee). Returns
  `ReadOutcome::Complete` when the budget is not hit.
- `Store::read_by_correlation_bounded(correlation_id, max_results, max_bytes, resume)` —
  bounded `read_by_correlation` variant. Uses `ORDER BY id ASC` (32-byte BLOB
  lexicographic) for deterministic resume; resume predicate is `id > last_seen_id`.
- `read_range_bounded_impl` in `src/read.rs` — row-by-row budget tracking; cursor
  encodes `next_version` for exact resume.
- `read_by_correlation_bounded_impl` in `src/cross_stream.rs` — same budget model;
  `(?2 IS NULL OR id > ?2)` resume clause so no-resume and resume share one SQL path.
- `tests/bounded_reads.rs` — 14 tests: truncation at count/bytes, full pagination,
  resume correctness, store-level default fallback, cursor type mismatch error.

### Fixed

- `CursorInner::Correlation::after_timestamp_us: i64` → `last_seen_id: [u8; 32]`.
  The v1.1.0 field name/type was wrong for the `id > last_seen_id` predicate.
  `CursorInner` is `pub(crate)` with no external call sites; safe to correct now.

### Budget resolution (both methods)

Per-call arg → `OpenOptions::default_max_results` / `default_max_bytes` → unbounded.
Resolution happens in the public `Store` method, not in the `*_bounded_impl` helper.

---

## v1.1.0 — 2026-06-20 — Bounded Resource API: Foundation Types

**Pass report:** `docs/aseptic/blast-radius/pass-1.1.0.md`

### Added

- `ReadOutcome<T>` — discriminated enum for bounded read results: `Complete(T)` and
  `Truncated { data, cursor, reason }`. Structurally distinct from existing unbounded reads.
- `TruncationCursor` — opaque resume token. Internal msgpack encoding; three inner variants
  (`Range`, `Correlation`, `Causation`) corresponding to the three cross-stream read shapes.
  Public API: `from_bytes`, `into_bytes`, `as_bytes`.
- `TruncationReason` — `ResultCount` | `ByteSize`; indicates which budget was hit.
- `BudgetKind` — `ResultCount` | `ByteSize`; used in `Error::ReadBudgetExceeded`.
- `SamplingMode` — `Exhaustive` | `BreadthFirst { max_per_level }` | `Adaptive { target_count }`;
  controls graph-walk truncation strategy for the upcoming `walk_causation` bounded variant.
- `Error::ReadBudgetExceeded { budget: BudgetKind, limit: usize }` — returned when a bounded
  read exceeds its configured ceiling. Not yet raised by any production code path (v1.1.2+).
- `OpenOptions::default_max_results: Option<usize>` — store-level default result-count ceiling
  for bounded reads. `None` = no default (callers supply per-call budget).
- `OpenOptions::default_max_bytes: Option<usize>` — store-level default byte-size ceiling.
- `Store::dispatch_channel_pressure() -> usize` — current pending-event count in the
  post-commit dispatch channel. Live observable; useful for back-pressure detection.
- `Store::dispatch_channel_high_water_mark() -> usize` — historical peak channel depth since
  store open. Updated atomically at each `append` / `append_batch` send site.
- `StoreInner::dispatch_channel_high_water_mark: Arc<AtomicUsize>` — backing field for HWM.
- `tests/bounded_foundation.rs` — 15 tests covering all new types and observability methods.

### Changed

- `Cargo.toml` version bumped: `0.1.0` → `1.1.0`.

### Not yet raised

- `Error::ReadBudgetExceeded` — present in the error enum but no call site yet. Ships in v1.1.2
  when `read_range_bounded` and `read_by_correlation_bounded` are implemented.
- `TruncationCursor::encode` / `decode` — present but unused until bounded read methods ship.

---

## v1.0.0aa — 2026-06-17

Relay infrastructure shipped: `RelayConfig`, `RelayAgent`, `relay_append`, `run_relay` in
`fossic-py/fossic/relay.py`. See commit 42ca201.
