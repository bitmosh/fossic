---
title: Polish Debt — Living Report
last_reviewed: v1.0.0w
---

# Polish Debt — Living Report

Correct but feels-wrong. Mechanical to fix; no design discussion required.
See `LIVING_REPORTS.md` for entry format and resolution conventions.

---

---
id: PD-001
type: polish_debt
status: resolved
pass_opened: v0.9.0
pass_resolved: v0.10.v
---

### ~~PD-001 — Tilde expansion spec examples inconsistent with binding behavior~~

> **Resolved in v0.10.v** — Spec §4.2 and §4.3 examples already showed the correct
> tilde-path pattern (no `os.path.expanduser`). Added explanatory paragraph in both
> sections documenting that `Store.open` uses `shellexpand` at the binding boundary.
> TIDYUP K1 survey note superseded by the paragraph. See blast-radius/pass-10.v.md.

<details>
<summary>Original entry</summary>

**What it is:** Pass 9 implemented tilde expansion in the Python binding's `Store.open()`
method. The spec examples in `FOSSIC_V1_SPEC.md` §4.2 and §4.3 still show the
`consumer-handles-tilde` pattern (`os.path.expanduser("~/.fossic/store.db")`), which
contradicts the binding behavior (binding handles tilde). TIDYUP survey K1 independently
flagged the opposite: "no binding performs tilde expansion" — this needs verification
against the current code before resolving.

**Where:** `docs/implement/FOSSIC_V1_SPEC.md` §4.2 (Python quick-start example) and
§4.3 (Node quick-start example).

**Fix:** (1) Verify current behavior by reading `fossic-py/src/store.rs` open() path.
(2) If binding handles tilde: update spec examples to use plain `~/.fossic/store.db`
and add a note "tilde expansion is performed by the binding." (3) If binding does NOT
handle tilde: change spec examples to use `os.path.expanduser()` and document that
tilde expansion is the consumer's responsibility in all binding READMEs.

</details>

---

---
id: PD-002
type: polish_debt
status: resolved
pass_opened: v0.10.x
pass_resolved: v0.10.v
---

### ~~PD-002 — Spec §8 doesn't document the list_branches / main convention~~

> **Resolved in v0.10.v** — Added "Default branch convention" subsection to §8.
> Also added "BranchInfo shape" subsection documenting all fields. See blast-radius/pass-10.v.md.

<details>
<summary>Original entry</summary>

**What it is:** `Store::list_branches` returns only explicitly-created diverged branches.
The implicit `main` trunk is never stored in the `branches` table and does not appear in
`list_branches` results. This is the correct behavior (fixed in Pass 8.6 tests), but the
spec section on branches doesn't articulate it.

**Where:** `docs/implement/FOSSIC_V1_SPEC.md` §8 (Branches).

**Fix:** Add one paragraph to §8 clarifying: "The implicit `main` trunk is not stored
in the `branches` table. `list_branches` returns an empty list for a stream that exists
but has no diverged branches. Callers should not treat an empty `list_branches` as an
indication that the stream has no events or that `main` doesn't exist."

</details>

---

---
id: PD-003
type: polish_debt
status: resolved
pass_opened: v0.8.6
pass_resolved: v0.10.v
---

### ~~PD-003 — BranchInfo field naming discrepancy (spec vs binding)~~

> **Resolved in v0.10.v** — Verified: spec had no explicit BranchInfo field list (so there
> was no conflict). Added canonical BranchInfo shape table to §8, documenting `.id` and
> `.lifecycle` as the correct field names. Implementation (`fossic-py/src/types.rs`) confirmed
> as ground truth. See blast-radius/pass-10.v.md.

<details>
<summary>Original entry</summary>

**What it is:** `BranchInfo` in the Python binding exposes `.id` (not `.branch_id`) and
`.lifecycle` (not `.status`). This was discovered in Pass 8.6 when test_branches.py used
the wrong field names. Needs verification: does the spec use different field names?

**Where:** `docs/implement/FOSSIC_V1_SPEC.md` — any section that references `BranchInfo`
field names. `fossic-py/src/types.rs` — the PyO3 struct definition.

**Fix:** (1) Grep spec for `branch_id` and `status` references in `BranchInfo` context.
(2) If spec uses different names: update spec to match the binding's actual field names
(`.id`, `.lifecycle`). (3) If consistent: no action, close this entry.

</details>

---

---
id: PD-004
type: polish_debt
status: resolved
pass_opened: v0.8.6
pass_resolved: v0.10.v
---

### ~~PD-004 — `register_upcaster` missing docstring in fossic-py `__init__.py`~~

> **Resolved in v0.10.v** — Added full docstring to `Store.register_upcaster` in
> `fossic-py/python/fossic/__init__.py`: signature `(payload: dict) -> dict`, chains
> at read time, stored events keep original identity, chain gap raises `UpcasterChainGapError`.
> See blast-radius/pass-10.v.md.

<details>
<summary>Original entry</summary>

**What it is:** `register_payload_transform` received a detailed docstring in Pass 8.6
(callable signature, when transforms fire, append-time vs read-time behavior). The
adjacent `register_upcaster` method has no equivalent docstring. The same detail is
warranted: which direction does the chain run, what's the expected signature, what
happens on a chain gap.

**Where:** `fossic-py/python/fossic/__init__.py` — `Store.register_upcaster` method
(currently a one-line pass-through with no docstring).

**Fix:** Add a docstring explaining: upcaster signature is `(payload: dict) -> dict`
(NOT `(event_type, payload)`); chain runs sequentially from `from_version` to the
highest registered `to_version`; a gap in the chain (1→2, 3→4 but no 2→3) raises
`UpcasterChainGapError` at read time.

</details>

---
id: PD-005
type: polish_debt
status: resolved
pass_opened: v0.10.0t
pass_resolved: v1.0.0t
severity: LOW
---

### ~~PD-005 — Extension event type naming convention inconsistency (snake_case vs PascalCase)~~

> **Resolved in v1.0.0t** — Added one sentence to `AGENT_TRACE_VOCABULARY.md` §9: "Choose the casing that matches the consumer codebase's idiom — `snake_case` for Rust consumers (rhyzome, bons.ai), PascalCase for Python consumers (Cerebra)." No code changes. See blast-radius/pass-1.0.0t.md.

<details>
<summary>Original entry</summary>

**What it is:** Rhyzome and bons.ai extension event types use `snake_case` naming (e.g., `strategy_selected`, `bandit_arm_selected`). Cerebra extension event types use `PascalCase` (e.g., `SessionOpened`, `ClutchDecisionMade`). Both conventions work — fossic core is agnostic to event type name casing — but the inconsistency is visible to readers of AGENT_TRACE_VOCABULARY.md and creates a question about which convention new consumers should follow.

**Where:** `docs/implement/AGENT_TRACE_VOCABULARY.md` §9 ("Adding new event types") acknowledges both conventions exist but doesn't make a recommendation.

**Fix:** Add one sentence to §9 recommending a convention for new consumers: either standardize on `snake_case` (matching fossic core's convention for the standard five types) or accept `PascalCase` (Cerebra's convention, common in event sourcing). If consensus is `snake_case`, note that Cerebra's PascalCase names are grandfathered. No code changes — docs-only.

</details>


---
id: PD-007
type: polish_debt
status: open
pass_opened: v0.10.0s
severity: LOW
---

### PD-007 — blake3 Python availability gap in CCE conformance harness

**What it is:** The CCE conformance harness (`fossic-py/tests/test_cce_vectors.py`) verifies encoder byte-identity using `cce_encode_value`, `cce_encode_bytes_raw`, and `cce_encode_f64_bits`. It does NOT verify `event_id` derivation because blake3 is not available as a Python package in the test environment. Full event identity (CCE bytes → blake3 → event_id) is only tested at the Rust level.

**Surfaced:** v0.10.0u PASS COMPLETE Learnings — "blake3 not available as a Python package; event_id verification skipped."

**Known cost:** Any drift in the fossic-py event_id derivation path (the blake3 call in the Rust append path that Python consumers trigger) would not be caught by the Python-level harness. The Rust-level tests do cover this; the gap is in Python-level observability.

**Resolution options:**
1. Add `blake3` as a fossic-py test dependency (available on PyPI under the `blake3` package name).
2. Expose `compute_event_id(cce_bytes: bytes) -> bytes` from fossic-py via PyO3; routes through fossic's own blake3 implementation in Rust and avoids a Python-side blake3 dependency.

**Recommendation:** Option 2 — cleaner architecture; the harness tests fossic's complete `event_id` derivation (CCE + blake3) through the same code path production callers use, rather than reimplementing blake3 in Python.

**Trigger:** v1.0.0 polish pass, or when a consumer reports an unexpected `event_id` mismatch.


---
id: PD-008
type: polish_debt
status: resolved
pass_opened: v0.10.0r
pass_resolved: v0.10.0r
severity: LOW
---

### ~~PD-008 — No canonical test invocation documented~~

> **Resolved in v0.10.0r** — `just test` is the canonical command; documented in
> `README.md` with first-run timing, per-binding variants, and no-just fallback.
> See blast-radius/pass-10.0r.md.

<details>
<summary>Original entry</summary>

**What it is:** `README.md` documented `cargo test --all-features` (Rust only) and
`cargo test --test cce_vectors -- --nocapture` as the test commands. No mention of
Python or Node test invocation. No per-binding shortcuts. No first-run setup guidance.

**Where:** `README.md` §Tests section.

**Fix:** Add `just test` as the canonical command with a short description, note on
first-run cost, per-binding variants (`just test-rust`, `just test-py`, `just test-node`),
and a no-just fallback block for contributors without `just` installed.

</details>

---
id: PD-009
type: polish_debt
status: resolved
pass_opened: v1.0.0w
pass_resolved: v1.0.0z
severity: LOW
---

### ~~PD-009 — `PoolExhausted` not covered by integration tests~~

**What it is:** `Error::PoolExhausted` (returned after 30s `recv_timeout` when all pool connections are busy) had no integration test. Triggering it required either holding all pool connections for 30 seconds or a configurable shorter timeout — neither was in `OpenOptions`.

> **Resolved in v1.0.0z** — Added `OpenOptions::read_pool_timeout_ms: u64` (default 30_000). Added `fossic::test-helpers` feature exposing `Store::_test_hold_read_conn(hold_ms)`. Added `pool_exhausted_returns_error` test in `tests/read_pool.rs`: pool_size 1, timeout 50ms, connection held 200ms → `Error::PoolExhausted { pool_size: 1, timeout_ms: 50 }`. See blast-radius/pass-1.0.0z.md.
