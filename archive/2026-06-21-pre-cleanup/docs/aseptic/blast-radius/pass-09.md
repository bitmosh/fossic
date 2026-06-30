---
pass: 9
version: v0.9.0
sha: a10f7b3
date: 2026-06-12
summary: Glob subscriptions, tilde expansion, cursor alignment; platform ADRs and phase definitions
---

# Blast Radius — Pass 9 (v0.9.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Header and Files section updated; content below reflects what the pass delivered.

## Files

### Created (actual commit a10f7b3)
- `docs/PHASES.md` — platform phase definitions (Phase 0–12) with exit criteria
- `docs/adr/ADR-001-lattica-extends-lumaweave.md` — platform ADR: Lattica extends LumaWeave
- `docs/adr/ADR-002-es-toolkit-over-nats.md` — platform ADR: ES toolkit choice
- `docs/adr/ADR-003-eval-core-standalone.md` — platform ADR: eval-core standalone
- `docs/adr/ADR-004-policy-scout-governance-scope.md` — platform ADR: Policy Scout scope
- `docs/adr/ADR-005-cerebra-api-surface.md` — platform ADR: Cerebra API surface
- `docs/adr/ADR-006-monorepo-structure.md` — platform ADR: pnpm/uv monorepo
- `docs/adr/ADR-007-lumashell-pattern-absorption.md` — platform ADR: LumaShell absorption
- `docs/adr/ADR-008-phase-12-research-exploration.md` — platform ADR: Phase 12 scope
- `docs/aseptic/blast-radius/pass-09.md` — this file (retroactive blast-radius artifact)
- `docs/aseptic/cross-pollination/pass-09.md` — consumer notification for tilde expansion

> **Note (retroactive commit walkthrough):** Source code changes for this pass
> (`fossic-py/src/store.rs` tilde expansion, `src/subscriptions.rs` glob pattern
> support, `fossic-node/src/store.rs` cursor alignment fix) are not represented in
> the git history. The retroactive commit walkthrough committed those source files at
> their post-v0.10.x final state in the v0.3.0, v0.5.0, and v0.8.0 commits respectively.
> The behavioral and API changes described below are accurate to what this pass delivered.

---

## Public APIs

### Modified (non-breaking)
- `Store.open(path)` (Python) — tilde in path is now expanded by the binding before
  opening the SQLite file. `~/.fossic/store.db` opens correctly without consumer calling
  `os.path.expanduser()`.

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

- Python `Store.open("~/.fossic/store.db")` now opens at the home-relative path rather
  than creating a store at a literal path starting with `~`.

---

## Living report updates

New entries:
- POLISH_DEBT: PD-001 — tilde expansion spec examples inconsistent with binding behavior
  (spec examples still show `os.path.expanduser()` after binding was updated to handle tilde)
- DEVIATION: DV-003 — spec §14 Tokio threading model (if this is when the Tokio section
  was written — retroactive estimate)

No entries resolved.
