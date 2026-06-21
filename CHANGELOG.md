# Changelog

All notable changes to fossic are documented here.
Format: semantic version sections, newest first. Each section links to the pass report.

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
