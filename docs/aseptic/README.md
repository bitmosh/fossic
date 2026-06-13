---
title: Aseptic Methodology — fossic Working Copy
status: live
version: v0.10.0q
---

# Aseptic — fossic Working Files

Aseptic is a methodology for multi-agent code execution that treats coordination
drift as a contamination problem — prevented through continuous discipline at the
work boundary, not cleaned up retrospectively after damage accumulates. The core
instrument is three accumulating living reports, maintained by every passing agent,
that make the project's known debt and divergence legible at a glance.

> **This is fossic's working copy of an in-development methodology.** The canonical
> version will move to its own project once a second reference implementation exists.
> Fossic is the first project using Aseptic as a live discipline (rather than
> retrospective application). The conventions here may evolve; updates to methodology
> itself go in this file tree, not in external docs.

---

## File structure

| File | Purpose |
|---|---|
| `README.md` | This file — entry point and structure map |
| `INTRODUCTION.md` | The "why" — failure modes, core conviction, the four moves |
| `LIVING_REPORTS.md` | Spec for the three accumulating reports: format, what goes in, resolution |
| `TECH_DEBT.md` | **Living report** — functional but known-bad implementation choices |
| `POLISH_DEBT.md` | **Living report** — correct but feels-wrong; mechanical to fix |
| `DEVIATION.md` | **Living report** — where implementation diverged from spec or ADR |
| `BLAST_RADIUS.md` | Spec for the per-pass blast radius artifact |
| `CROSS_POLLINATION.md` | Spec for the per-pass adjacent-project notification artifact |
| `ADR_FORMAT.md` | The agent-friendly ADR template (parallel-execution-safe) |
| `PASS_REPORTING.md` | Structured pass report format; PASS COMPLETE integration |
| `SUPERVISOR_PROTOCOL.md` | What a supervisor pass does; trigger conditions and process |
| `AGENT_BRIEFING.md` | Copy-pasteable prompt fragment for participating agents |
| `VERSION_CONVENTION.md` | Forward versioning vs. descending-letter cleanup passes |
| `blast-radius/` | One file per pass — `pass-NN.md` or `pass-N.M.md` |
| `cross-pollination/` | Per-pass adjacent-project impact, when impacts exist |

---

## The three living reports at a glance

- **[TECH_DEBT.md](TECH_DEBT.md)** — things that work but have a known cost: architectural shortcuts,
  deliberate deferrals, implementations that bypass structural principles for pragmatic reasons.
  Every entry has a trigger condition for when it becomes worth addressing.

- **[POLISH_DEBT.md](POLISH_DEBT.md)** — things that are correct but imprecise: naming inconsistencies,
  doc gaps, test helper duplication, file organization that grew organically. Mechanical to fix;
  no design discussion required.

- **[DEVIATION.md](DEVIATION.md)** — information log of where implementation diverged from spec or ADR.
  Not a failure log — deviations are often correct responses to discovered constraints. Each entry
  records what spec said, what happened, why, and whether the spec should catch up.

---

## Blast radius and cross-pollination

Every pass produces a `blast-radius/pass-NN.md` at completion. Passes with meaningful
adjacent-project impact also produce a `cross-pollination/pass-NN.md`. These feed the
PASS COMPLETE message and inform supervisor passes.

Retroactive files for Passes 1–11 are in `blast-radius/` and `cross-pollination/`.
Headers were realigned to real SHAs and dates in Pass v0.10.w.

---

## Entry points by role

| You are… | Start here |
|---|---|
| An agent starting a new pass | [AGENT_BRIEFING.md](AGENT_BRIEFING.md) |
| A supervisor conducting a review | [SUPERVISOR_PROTOCOL.md](SUPERVISOR_PROTOCOL.md) |
| Authoring a new ADR | [ADR_FORMAT.md](ADR_FORMAT.md) |
| Writing a pass report | [PASS_REPORTING.md](PASS_REPORTING.md) |
| Understanding the methodology | [INTRODUCTION.md](INTRODUCTION.md) |
