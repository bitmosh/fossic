---
pass: 10
version: v0.10.0
date: "(retroactive estimate, not verified)"
summary: DynReducer snapshot caching in Python binding; Rust BoxedReducer exposed for Python
---

# Blast Radius — Pass 10 (v0.10.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Modified
- `fossic/src/reducers.rs` — `BoxedReducer` made pub (or new `DynReducer` pub trait added)
  so PyO3 binding can call snapshot machinery with Python-provided reducer
- `fossic-py/src/store.rs` — `register_reducer`, `read_state`, `take_snapshot` wired to
  Rust snapshot path instead of pure-Python full replay
- `fossic-py/python/fossic/__init__.py` — `read_state` now uses Rust-backed snapshot
  caching; `DynReducer` protocol documented (name, version, state_schema_version, initial_state, apply)
- `fossic/tests/reducers.rs` — additional tests for DynReducer protocol (retroactive estimate)

### Created
- `benchmarks/aggregate_volume_bench.py` — Pass 10 prerequisite benchmark validating
  snapshot caching latency
- `benchmarks/results/aggregate_volume_raw.json` — raw benchmark results
- `benchmarks/results/aggregate_volume_sweep.md` — benchmark analysis and recommendations

---

## Public APIs

### Modified (non-breaking)
- Python `Store.register_reducer(pattern, reducer)` — reducer must now have class-level
  attributes `name: str`, `version: int`, `state_schema_version: int` plus `initial_state()`
  and `apply(state, payload)` methods. Previously pure-Python with fewer requirements.
- Python `Store.read_state(stream_id, branch)` — now uses snapshot caching via Rust path.
  Output identical; performance improved for streams with snapshots.

---

## Schema changes

None — snapshots table already existed; Python path now uses it.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

- Python `read_state` p99 latency:
  - No snapshot (1000-event replay): 46.6ms → unchanged (no snapshot = full replay)
  - Snapshot @v900 (100-event replay): was ~46.6ms (full replay), now 4.7ms (PyO3 bridge cost)
  - Snapshot every 10 events: p99 < 0.07ms
- **Benchmark finding:** PyO3 bridge overhead is ~47μs/event. Sub-millisecond p99 requires
  snapshot cadence ≤ ~20 events at the bridge cost. Cerebra's recommended cadence: every 10 events.
- Headline metric FAILS at snapshot @v900: p99 = 4.709ms vs 1ms target.
  Root cause: bridge overhead, not snapshot caching correctness.

---

## Living report updates

New entries:
- TECH_DEBT: TD-001 — Python DynReducer bridge cost dominates read_state latency

No entries resolved.
