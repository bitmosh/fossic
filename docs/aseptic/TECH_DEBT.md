---
title: Tech Debt — Living Report
last_reviewed: v0.10.0.u
---

# Tech Debt — Living Report

Functional but known-bad implementation choices. Every entry has a trigger condition.
See `LIVING_REPORTS.md` for entry format and resolution conventions.

---

---
id: TD-001
type: tech_debt
status: open
pass_opened: v0.10.0
severity: MEDIUM
---

### TD-001 — Python DynReducer bridge cost dominates read_state latency

**What it is:** The Python `read_state` path works correctly and uses snapshot caching
(shipped in Pass 10), but the PyO3 bridge overhead is ~47μs per event replayed. With a
snapshot at version 900 and 100 events to replay, p99 ≈ 4.7ms — 4.7× over the
sub-millisecond headline metric.

**Why it was necessary:** The snapshot caching implementation in Pass 10 is correct: it
starts from the most recent snapshot and replays only events since that snapshot. The
overhead is structural — every event crosses the Rust↔Python boundary (msgpack decode →
Python call → msgpack encode) at ~47μs each. Moving this to a Rust-native reducer
eliminates the boundary crossing but requires writing the LatticeNodeReducer in Rust,
which is a separate architectural decision.

**Known cost:** For high-event-count streams (>100 events since last snapshot),
read_state p99 exceeds 1ms. At Cerebra's scale (thousands of lattice-node streams,
continuous appends), this creates a compounding latency problem that snapshot cadence
alone can only partially address.

**Trigger:** When BOTH:
1. The Cerebra witness layer is implemented (gives us a Rust-native computation path)
2. AND lattice read latency is measurable in user-facing response time

Mitigation in the meantime: aggressive snapshot cadence (every 10 events achieves
p99 < 0.07ms for the replay portion; p99 < 0.5ms total with bridge overhead amortized
across the snapshot-write cost).

**Evidence:** `benchmarks/results/aggregate_volume_sweep.md` — scenario "snap_at_899"
shows p99 = 4.709ms; scenario "every_10" shows p99 = 0.054ms. The PyO3 bridge cost
is ~0.047ms/event confirmed in scenario analysis.

---

---
id: TD-002
type: tech_debt
status: resolved
pass_opened: v0.8.6
pass_resolved: v0.10.1
severity: MEDIUM
---

### ~~TD-002 — ReadQuery lacks event_type_filter field (RB-1)~~

> **Resolved in v0.10.1** — `event_type_filter: Option<String>` added to `ReadQuery`
> across all four layers: Rust core, fossic-py, fossic-node, fossic-tauri.
> SQL NULL-guard pattern applied in `read_range_impl`. The `xfail` test in
> `fossic-py/tests/test_cross_stream.py` has been resolved and now passes.
> 5 Rust integration tests, 1 Python test, 4 Node tests, 2 Tauri tests added.
> See blast-radius/pass-10.1.md.

<details>
<summary>Original entry</summary>

**What it is:** `ReadQuery` (used by `read_range`) has no `event_type` filter field.
`AggregateQuery` (used by `aggregate`) has `event_type_filter`. Consumers who want to
read events of a specific type from a stream must fetch all events and filter client-side.

**Why it was necessary:** Deferred in the initial `ReadQuery` implementation. The filtering
logic exists in `AggregateQuery`'s path but was not applied to `ReadQuery` — likely because
the initial use cases (Cerebra's lattice node replay) needed all events on a stream.

**Known cost:** Client-side filtering is correct but wastes the read bandwidth for streams
with mixed event types. For Policy Scout's use case (reading only `PolicyViolation` events
from a mixed audit stream), this is non-trivial overhead.

**Trigger:** Before v1.0-rc.1 tag. This is a spec-promised API surface that's absent.
Decision needed: land as v0.10.1 (additive, non-breaking) or defer to v1.0.0.

**Evidence:** `fossic-py/tests/test_cross_stream.py::test_read_range_event_type_filter`
is marked `xfail(strict=True)` — it raises `TypeError` because `ReadQuery` accepts no
`event_type` keyword argument.

</details>

---

---
id: TD-003
type: tech_debt
status: open
pass_opened: v0.8.0
severity: LOW
---

### TD-003 — `time = "=0.3.37"` exact-pin in fossic-tauri

**What it is:** `crates/fossic-tauri/Cargo.toml` pins `time = "=0.3.37"` (exact version,
not caret range) to avoid a `cookie 0.18.1` coherence conflict.

**Why it was necessary:** Tauri 2 at the time of implementation pulled in `cookie 0.18.1`
which was designed against `time 0.3.36–0.3.37`. The exact pin prevents the resolver from
choosing a newer `time` version that would conflict.

**Known cost:** Any consumer workspace that has a different `time` requirement will have
a resolution conflict with `fossic-tauri`. This is isolated to `fossic-tauri`'s own
`Cargo.lock` (it's not in the workspace) so the blast radius is limited, but it's a
surprise for anyone trying to integrate `fossic-tauri` into a larger workspace.

**Trigger:** When Tauri 2 bumps `cookie` to a version that no longer requires `time 0.3.37`
(check the Tauri changelog). At that point: remove the pin, run `cargo update`, verify build.

**Evidence:** `crates/fossic-tauri/Cargo.toml:21` — comment on the line explains the pin.
TIDYUP survey Issue 7 confirmed; necessity not fully verified (would require Tauri changelog check).

---

---
id: TD-004
type: tech_debt
status: open
pass_opened: v0.5.0
severity: MEDIUM
---

### TD-004 — SimilaritySearchProvider declared in spec, absent from code

**What it is:** `FOSSIC_V1_SPEC.md` §10.4 and §15 describe `SimilaritySearchProvider`
as a declared extension point — a `pub trait` that consumers implement to plug in their
own vector index. The `similarity` feature flag exists in `fossic/Cargo.toml` but is
empty. No trait is declared, no `OpenOptions::similarity_search` field exists.

**Why it was necessary:** The v1 implementation prioritized the core event sourcing
primitives. Similarity search was explicitly listed as "designed-for, not implemented"
in the spec's non-goals section. The feature flag was a placeholder.

**Known cost:** Consumers enabling `--features similarity` get nothing. bons.ai's planned
semantic nearest-neighbor queries on idea text cannot use the extension point. The spec's
architectural promise is invisible to downstream implementors.

**Trigger:** When bons.ai requests vector search integration AND a similarity backend
(sqlite-vec or ChromaDB) is ready to implement against the trait. The trait stub (the
`pub trait` declaration + `Store::similarity_query` returning `NotImplemented`) should
ship before consumers need it — so they can write against the interface even while the
implementation is pending.

**Evidence:** `fossic/Cargo.toml` — `similarity = []` feature flag, no associated code.
`fossic/src/lib.rs` — no `SimilaritySearchProvider` in the public API. TIDYUP survey D1.

---
id: TD-005
type: tech_debt
status: resolved
pass_opened: v0.10.0.u
pass_resolved: v0.10.0.u
severity: LOW
---

### ~~TD-005 — fossic-py had no CCE conformance harness (binding parity gap)~~

> **Resolved in v0.10.0.u** — Added `cce_encode_value`, `cce_encode_bytes_raw`,
> `cce_encode_f64_bits` to fossic-py (State B: encoder was internal to `append()`).
> Created `fossic-py/tests/test_cce_vectors.py`: 29 vectors pass, 1 skipped
> (`expected_hex: null`, same as Rust harness). See blast-radius/pass-10.0.u.md.

<details>
<summary>Original entry</summary>

**What it is:** The Rust core has `tests/cce_vectors.rs` that verifies the CCE encoder
against all canonical vectors in `cce-test-vectors.json`. The Python binding had no
equivalent harness — any drift in fossic-py's CCE encoding path would go undetected.

**Why it was necessary:** The Python binding does CCE encoding internally inside
`append()`, but the encoder was never exposed to Python callers or test code.

**Known cost:** Python binding CCE divergence could produce silent wrong event IDs,
undetectable without comparing with the Rust core on the same input.

**Trigger:** Add Python harness in the same pass that adds it, i.e. now.

**Evidence:** `fossic-py/src/lib.rs` — no `cce_encode_*` functions registered before
v0.10.0.u. `tests/cce_vectors.rs` exists as the Rust reference.

</details>
