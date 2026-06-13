---
pass: 10
version: v0.10.0
sha: dfcb024
date: 2026-06-12
summary: DynReducer snapshot caching in Python binding; Rust BoxedReducer exposed for Python; similarity stub
---

# Blast Radius — Pass 10 (v0.10.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Header and Files section updated; content below reflects what the pass delivered.

## Files

### Created (actual commit dfcb024)
- `src/similarity.rs` — `SimilarityQuery` and `SimilaritySearchProvider` trait stub (TD-004)
- `tests/dyn_reducers.rs` — DynReducer protocol attribute enforcement tests
- `tests/similarity.rs` — similarity stub tests (verifies trait compiles; no impl yet)
- `benchmarks/aggregate_volume_bench.py` — aggregate volume benchmark validating snapshot caching latency
- `docs/aseptic/blast-radius/pass-10.md` — this file (retroactive blast-radius artifact)
- `docs/aseptic/cross-pollination/pass-10.md` — cerebra and bons.ai notifications

> **Note (retroactive commit walkthrough):** Source code modifications for this pass
> (`src/reducers.rs` DynReducer exposure, `fossic-py/src/store.rs` Rust snapshot wiring,
> `fossic-py/python/fossic/__init__.py` DynReducer protocol docs) are not represented in
> the git history. The retroactive commit walkthrough committed those files at their
> post-v0.10.x final state in the v0.2.0 and v0.5.0 commits respectively.
> `benchmarks/results/` is gitignored; raw results and sweep docs are not in the commit.

### Modified (historical — not in git diff, represented in earlier retroactive commits)
- `fossic/src/reducers.rs` — `DynReducer` trait made pub; enables PyO3 binding to call snapshot machinery
- `fossic-py/src/store.rs` — `register_reducer`, `read_state`, `take_snapshot` wired to Rust DynReducer snapshot path
- `fossic-py/python/fossic/__init__.py` — `read_state` now uses Rust-backed snapshot caching; DynReducer protocol documented

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
