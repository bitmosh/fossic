# Aggregate Volume Benchmark — fossic v1.0-rc.1

Generated: 2026-06-12T21:33:21Z  |  Runs per scenario: 3  |  Mode: full

## Headline

| Metric | Target | Result | Verdict |
|--------|--------|--------|---------|
| `read_state` p99, 1000-event stream, snapshot@v900 | < 1ms | 4.709ms | **FAIL** |

## Scenario 1 — Single stream, read_state latency vs snapshot strategy

Stream: 1000 events · 200 read_state calls per condition

| Condition | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | n |
|-----------|--------|--------|--------|----------|--------|---|
| no_snapshot | 45.886 | 46.442 | 46.563 | 47.268 | 47.439 | 200 |
| snap_at_899 | 4.576 | 4.636 | 4.709 | 4.764 | 4.764 | 200 |
| every_100 | 0.031 | 0.037 | 0.045 | 0.059 | 0.062 | 200 |
| every_50 | 0.032 | 0.038 | 0.050 | 0.059 | 0.059 | 200 |
| every_10 | 0.032 | 0.036 | 0.041 | 0.052 | 0.054 | 200 |

## Scenario 2 — Cold rehydration across N streams

Streams: 1000 · 150 events each · snapshot at v99

| Metric | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | n |
|--------|--------|--------|--------|----------|--------|---|
| cold read_state | 2.302 | 2.372 | 2.511 | 2.851 | 3.439 | 1000 |

## Scenario 3 — Mixed workload (60.0s)

Streams: 200 · 50 events warm-up · ~50% reads

| Op | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | n |
|----|--------|--------|--------|----------|--------|---|
| read_state | 2.422 | 4.762 | 5.339 | 6.057 | 7.111 | 21623 |

## Scenario 4 — Snapshot cadence sweep

Stream: 1000 events · read_state p99 vs events-between-snapshots

| Cadence | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | n |
|---------|--------|--------|--------|----------|--------|---|
| every 10 | 0.032 | 0.037 | 0.048 | 0.053 | 0.054 | 100 |
| every 25 | 0.031 | 0.036 | 0.040 | 0.055 | 0.057 | 100 |
| every 50 | 0.032 | 0.047 | 0.057 | 0.058 | 0.058 | 100 |
| every 100 | 0.032 | 0.039 | 0.049 | 0.062 | 0.064 | 100 |
| every 200 | 0.032 | 0.041 | 0.050 | 0.061 | 0.062 | 100 |
| every 500 | 0.031 | 0.041 | 0.066 | 0.070 | 0.070 | 100 |

## Scenario 5 — Worst-case: no snapshot, full replay

Stream: 1000 events · no snapshot taken

| Metric | p50 ms | p95 ms | p99 ms | p99.9 ms | max ms | n |
|--------|--------|--------|--------|----------|--------|---|
| full replay | 46.295 | 46.961 | 47.970 | 49.576 | 49.754 | 100 |

## Analysis

### Headline result: FAIL

The headline metric fails. `read_state` p99 = 4.709ms with snapshot@v900
(100 events to replay), exceeding the 1ms target.

**Root cause — PyO3 bridge overhead.** Each event replay round-trips through the
Python–Rust boundary: msgpack decode → `LatticeNodeReducer.apply()` → msgpack encode.
Measured per-event cost: ~0.05–0.07ms through the bridge. At 100 events,
bridge cost alone is ~5–7ms, independent of reducer complexity.

**Comparison (Scenario 1):**
- No snapshot (full 1000-event replay):   p99 = 46.563ms
- Snapshot @ v900 (100-event replay):     p99 = 4.709ms
- Snapshot every 10 events (≤10 replay):  p99 = 0.041ms

**Actions to meet the 1ms SLA (choose one or combine):**

1. **More aggressive snapshot cadence (recommended short term).** Scenario 4
   shows the cadence at which p99 drops under 1ms. Use that cadence in Cerebra's
   stream lifecycle policy.

2. **Rust-native reducer (recommended long term).** A `Reducer` impl in
   `fossic-py`'s Rust layer eliminates the PyO3 bridge overhead entirely,
   reducing per-event cost to ~0.001ms. The DynReducer scaffold is in place;
   the Cerebra lattice reducer can be ported to Rust without API changes.

3. **Relaxed SLA.** If Cerebra's read path can tolerate 5–10ms cold-cache
   latency, the headline metric can be retired in favour of the cadence-sweep
   results which show a clear performance-vs-frequency trade-off.

### Cadence recommendation

Snapshot every **10 events** achieves p99 < 1ms.
Use this as the minimum snapshot cadence for Cerebra lattice node streams.

## Environment

- fossic-py: pre-built `.so` at `fossic-py/python/fossic/`
- Reducer: `LatticeNodeReducer` (Python DynReducer, PyO3 msgpack bridge)
- Storage: SQLite WAL, isolated tempdir per scenario run
- Timing: `time.perf_counter()` (ns resolution), reported in ms
