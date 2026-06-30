---
pass: 10
version: v0.10.0
date: "(retroactive estimate, not verified)"
impacts: [cerebra, bons.ai]
---

# Cross-Pollination — Pass 10 (v0.10.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## cerebra

**Severity:** NEEDS-AWARENESS

**What changed:** The Python `read_state` path now uses snapshot caching via the Rust
DynReducer bridge instead of full-event replay. Functionally identical output; substantially
different performance characteristic. The aggregate volume benchmark confirmed: p99 with
snapshot every 10 events is 0.054ms; with no snapshot, 46.6ms.

The benchmark also confirmed that the PyO3 bridge overhead is ~47μs/event. For Cerebra's
use case (thousands of lattice-node streams, continuous event appends), a snapshot cadence
of every 10 events keeps individual `read_state` calls well under 1ms. Without regular
snapshots, latency grows linearly with event count.

**Action required:**
1. The DynReducer protocol now requires class-level attributes: `name: str`, `version: int`,
   `state_schema_version: int` in addition to `initial_state()` and `apply()`. Verify
   Cerebra's reducer classes have these attributes.
2. Implement a snapshot cadence policy. `Store.take_snapshot(stream_id, branch)` should
   be called after every ~10 appends for each active lattice-node stream to keep `read_state`
   sub-millisecond.

**Advocate-agent message:**
> fossic v0.10.0 shipped. read_state now uses snapshot caching via the Rust DynReducer
> bridge (was: pure-Python full-event replay every call).
>
> Impact for cerebra:
>
> 1. Reducer protocol update required: your reducer classes must now have class-level
>    attributes `name: str`, `version: int`, `state_schema_version: int`. If missing,
>    `register_reducer` will raise at registration time (fail-loudly). Verify your
>    reducer classes.
>
> 2. Snapshot cadence matters now. Benchmark result: p99 with snap every 10 events =
>    0.054ms; p99 with no snap (1000-event replay) = 46.6ms. For lattice-node streams
>    with continuous appends, call `take_snapshot(stream_id, "main")` every ~10 appends
>    per stream. Without this, read_state latency will grow linearly with event count.
>
> The output of read_state is functionally identical — only the performance profile changed.
>
> Severity: NEEDS-AWARENESS. Action required: verify reducer class attributes; implement
> snapshot cadence.

---

## bons.ai

**Severity:** FYI

**What changed:** Same DynReducer protocol update as cerebra. bons.ai is currently benched
but should be aware that when it resumes, reducer classes need the three class-level
attributes.

**Advocate-agent message:**
> fossic v0.10.0: DynReducer protocol now requires `name`, `version`, `state_schema_version`
> class attributes on reducer objects. Impact for bons.ai: FYI while benched. When
> bons.ai resumes and uses fossic reducers, add these attributes to your reducer classes
> before calling register_reducer.

---

## policy-scout, bo, lumaweave, ai-stack, rhyzome

No impact. policy-scout and bo don't use reducers. lumaweave uses the Node binding
(no Python reducer path). ai-stack is indirect. rhyzome is benched.
