---
title: Supervisor Protocol — What a Supervisor Pass Does
---

# Supervisor Protocol

A supervisor pass is a dedicated read-only review that runs between parallel agent batches
(or when a trigger condition fires). It reads the accumulating instruments, cross-checks
against git history, classifies findings that per-pass agents missed, and produces a
structured report for human review. It does not execute any fixes. It halts and hands
off.

The FOSSIC_TIDYUP_SURVEY.md is the canonical example of what a retroactive supervisor pass
looks like. Living-report-based supervisor passes are faster (the reports do the accumulation
work) and more targeted (they report the delta since the last supervisor pass, not the
full history).

---

## Trigger conditions

A supervisor pass is warranted when any of the following fire:

| Trigger | Condition |
|---|---|
| Length threshold | Any living report exceeds 600 lines |
| Deviation flag | Any DEVIATION entry has `status: OPEN — spec should be updated` and is more than two forward versions old without action |
| Batch boundary | A parallel agent batch has completed (recommended between every batch) |
| Periodic | No supervisor pass has run in the last five forward versions |
| Manual | Developer or planning Claude requests one explicitly |

---

## Inputs

The supervisor pass reads, in order:

1. `docs/aseptic/TECH_DEBT.md` — all open entries
2. `docs/aseptic/POLISH_DEBT.md` — all open entries
3. `docs/aseptic/DEVIATION.md` — all open entries
4. `blast-radius/pass-NN.md` files since the last supervisor pass
5. `cross-pollination/pass-NN.md` files since the last supervisor pass
6. Git log since the last supervisor pass (to cross-check blast-radius against actual commits)

The supervisor pass does NOT need to read full source files unless a specific integrity
check requires it (see Integrity loop below).

---

## Process

### Phase 1 — Read and inventory

Read all inputs. Build a working inventory:
- Open TECH_DEBT entries: how many, which are oldest, which have missed trigger conditions
- Open POLISH_DEBT entries: how many, are any trivially closable in a cleanup batch
- Open DEVIATION entries: which are waiting for spec updates, which for implementation
- Blast-radius cross-check: do the blast-radius files account for all modified files in
  the git log since the last supervisor pass?

### Phase 2 — Integrity loop

For each blast-radius file in scope:
1. Check that "Living report updates" section is present (no silent omissions)
2. Spot-check two or three API changes against the actual source file (read the file)
3. Verify that any DEVIATION entries flagged `OPEN — implementation should catch up`
   haven't been silently closed in the code without a report entry

Flag any inconsistency found.

### Phase 3 — Spec coherence verification

For each DEVIATION entry with `status: OPEN — spec should be updated`:
1. Confirm the spec section still says what the entry claims it says
2. Confirm the implementation still does what the entry claims it does
3. If either has changed (stale deviation entry), flag for update

### Phase 4 — New findings

Look for findings that per-pass agents missed. Sources:
- Blast-radius files that mention APIs without corresponding DEVIATION entries
- POLISH_DEBT entries that have grown into TECH_DEBT (mechanical fix now requires design)
- Patterns across multiple blast-radius files (consistent debt accumulation)

### Phase 5 — Report

Produce `SUPERVISOR_REPORT.md` (in `docs/aseptic/`, not committed until reviewed):

```markdown
# Supervisor Report — [date]

**Passes reviewed:** Pass N through Pass M (vX.Y.Z through vA.B.C)
**Previous supervisor pass:** [date or "none — first pass"]

## Summary

[2-3 sentences: overall system health, key findings count, urgency]

## Integrity findings

[Blast-radius inconsistencies, silent omissions, stale entries found during integrity loop]

## New findings (missed by per-pass agents)

[Classification: TECH_DEBT | POLISH_DEBT | DEVIATION, with proposed entry text]

## Stale entries to update or close

[Entries in living reports that are no longer accurate]

## Recommended cleanup batch

[Prioritized list of what a cleanup pass should address; estimated effort]

## Halt — awaiting human review

This report is complete. No changes have been made. The recommended cleanup batch
requires human approval before any agent executes it.
```

---

## The halt discipline

A supervisor pass **halts before executing any fix**. This is non-negotiable.

The supervisor pass is a diagnostic instrument. Its output is a structured report for
human review. The human decides which findings to act on, which to defer, and whether
to execute the cleanup batch as a whole or in parts.

This discipline exists because supervisor findings sometimes reveal that an assumption
the whole system was built on is wrong. Executing fixes from a supervisor pass without
review can propagate a wrong assumption into more code. The halt creates a review gate
at the point of maximum leverage.

---

## What the supervisor pass is NOT

- **Not a code review pass.** The supervisor reads living reports and blast-radius files,
  not source code in detail. It spot-checks for integrity; it doesn't do comprehensive
  code review.
- **Not a fixing pass.** Supervisor passes produce reports; cleanup passes execute fixes.
  The supervisor's job is to see clearly; it doesn't touch code.
- **Not triggered by every pass.** The trigger conditions above define when it's warranted.
  Running a supervisor pass after every single pass would create overhead that erodes the
  discipline. Batch boundaries and length thresholds are the natural cadence.
