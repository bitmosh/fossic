---
pass: 10.v
version: v0.10.v
date: 2026-06-12
summary: Spec clarification — purge semantics, branch conventions, BranchInfo fields, tilde expansion, upcaster docstring
---

# Blast Radius — Pass 10.v (v0.10.v)

> Live-filed blast-radius artifact.

## Files

### Modified

- `docs/implement/FOSSIC_V1_SPEC.md` — six targeted changes:
  1. §4.2: Added "Tilde expansion" paragraph after Python quickstart example documenting `shellexpand` at binding boundary; redundant `os.path.expanduser()` call documented as unnecessary.
  2. §4.3: Added "Tilde expansion" note after TypeScript quickstart example.
  3. §8: Added "BranchInfo shape" subsection — canonical field table: `.id`, `.stream_id`, `.parent_id`, `.parent_version`, `.description`, `.lifecycle`, `.created_at`, `.closed_at`, `.closed_reason`, `.alternatives()`. Note on `.id` vs `.branch_id` and `.lifecycle` vs `.status`.
  4. §8: Added "Default branch convention" subsection — `main` trunk not stored in `branches` table; empty `list_branches` result means no diverged branches, not no history; `read_range(..., branch="main")` always works.
  5. §9.3: Rewrote two sentences to clarify purge semantics: `Purged` audit event goes to `_fossic/system` (not original stream); after purge, `read_one` returns `None` and `read_range` skips the event. Removed misleading "consumers see the Purged event in the stream" framing.
  6. §16 invariant 8: Rewritten to state "Purge removes an event from the read path entirely" with explicit `read_one`/`read_range` behavior and audit-in-`_fossic/system` note.

- `fossic-py/python/fossic/__init__.py` — added docstring to `Store.register_upcaster`:
  callable signature `(payload: dict) -> dict`; fires at read time not write time; stored events keep original identity; chain gap raises `UpcasterChainGapError`; registration per `(event_type, from_version, to_version)` triple.

- `docs/aseptic/DEVIATION.md` — DV-002 resolved: status `open` → `resolved`, `pass_resolved: v0.10.v`, resolution summary + `<details>` wrapper for history. `last_reviewed: v0.10.v`.

- `docs/aseptic/POLISH_DEBT.md` — all four open entries resolved:
  - PD-001 resolved: tilde expansion paragraph added to spec; TIDYUP K1 note superseded.
  - PD-002 resolved: "Default branch convention" subsection added to §8.
  - PD-003 resolved: spec had no BranchInfo field list (no conflict); canonical table added.
  - PD-004 resolved: `register_upcaster` docstring added to `__init__.py`.
  `last_reviewed: v0.10.v`.

- `docs/aseptic/TECH_DEBT.md` — `last_reviewed: v0.10.v` (no new or resolved TD entries).

- `docs/aseptic/README.md` — `version: v0.10.w` → `v0.10.v`.

### Created

- `docs/aseptic/blast-radius/pass-10.v.md` — this file.
- `docs/aseptic/cross-pollination/pass-10.v.md` — adjacent-project impact notes (Cerebra FYI, Policy Scout FYI, LumaWeave NEEDS-AWARENESS on Rust vs binding tilde, Rhyzome FYI on BranchInfo fields).

---

## Public APIs

### Added

None — docs-only pass.

### Modified

None — no implementation changes.

---

## Schema changes

None.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

None — no production code changes. All changes are spec corrections and docstrings.

---

## Living report updates

### DEVIATION.md

Resolved:
- **DV-002** — `purge_event` removes events from read path (was OPEN, status → RESOLVED)
  Spec §9.3 and §16.8 now correctly describe read-path removal semantics and document
  that the `Purged` audit event is in `_fossic/system`.

### POLISH_DEBT.md

Resolved (all four open entries):
- **PD-001** — Tilde expansion spec examples (was OPEN, status → RESOLVED)
- **PD-002** — Spec §8 missing list_branches/main convention (was OPEN, status → RESOLVED)
- **PD-003** — BranchInfo field naming discrepancy (was OPEN, status → RESOLVED)
- **PD-004** — `register_upcaster` missing docstring (was OPEN, status → RESOLVED)

### TECH_DEBT.md

No new or resolved entries. `last_reviewed` bumped to v0.10.v.

---

## Pass notes

**PD-001 finding:** The spec examples were already correct at pass start — both §4.2 and §4.3
already showed bare tilde paths with `# tilde expanded by binding` inline comments. The PD-001
entry had been filed speculatively (referencing an `os.path.expanduser` pattern that no longer
existed in the spec). The fix was the explanatory paragraph, not an example correction.

**PD-003 finding:** The spec had no explicit BranchInfo field list at all before this pass —
there was no conflict to resolve, only an absence to fill. Both `fossic-py/src/types.rs`
(ground truth) and the `branches` table schema confirm `.id` and `.lifecycle` as the correct
names. Added the canonical table rather than just closing the entry.

**register_payload_transform verification:** docstring confirmed accurate — fires at append
time, signature `(event_type: str, payload: dict) -> dict`, stream-pattern based, no
`include_system` parameter. No changes needed.
