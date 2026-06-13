---
title: Polish Debt — Living Report
last_reviewed: v0.10.v
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
