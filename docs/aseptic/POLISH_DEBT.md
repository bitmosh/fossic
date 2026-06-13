---
title: Polish Debt — Living Report
last_reviewed: v0.10.w
---

# Polish Debt — Living Report

Correct but feels-wrong. Mechanical to fix; no design discussion required.
See `LIVING_REPORTS.md` for entry format and resolution conventions.

---

---
id: PD-001
type: polish_debt
status: open
pass_opened: v0.9.0
---

### PD-001 — Tilde expansion spec examples inconsistent with binding behavior

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

---

---
id: PD-002
type: polish_debt
status: open
pass_opened: v0.10.x
---

### PD-002 — Spec §8 doesn't document the list_branches / main convention

**What it is:** `Store::list_branches` returns only explicitly-created diverged branches.
The implicit `main` trunk is never stored in the `branches` table and does not appear in
`list_branches` results. This is the correct behavior (fixed in Pass 8.6 tests), but the
spec section on branches doesn't articulate it.

**Where:** `docs/implement/FOSSIC_V1_SPEC.md` §8 (Branches).

**Fix:** Add one paragraph to §8 clarifying: "The implicit `main` trunk is not stored
in the `branches` table. `list_branches` returns an empty list for a stream that exists
but has no diverged branches. Callers should not treat an empty `list_branches` as an
indication that the stream has no events or that `main` doesn't exist."

---

---
id: PD-003
type: polish_debt
status: open
pass_opened: v0.8.6
---

### PD-003 — BranchInfo field naming discrepancy (spec vs binding)

**What it is:** `BranchInfo` in the Python binding exposes `.id` (not `.branch_id`) and
`.lifecycle` (not `.status`). This was discovered in Pass 8.6 when test_branches.py used
the wrong field names. Needs verification: does the spec use different field names?

**Where:** `docs/implement/FOSSIC_V1_SPEC.md` — any section that references `BranchInfo`
field names. `fossic-py/src/types.rs` — the PyO3 struct definition.

**Fix:** (1) Grep spec for `branch_id` and `status` references in `BranchInfo` context.
(2) If spec uses different names: update spec to match the binding's actual field names
(`.id`, `.lifecycle`). (3) If consistent: no action, close this entry.

---

---
id: PD-004
type: polish_debt
status: open
pass_opened: v0.8.6
---

### PD-004 — `register_upcaster` missing docstring in fossic-py `__init__.py`

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
