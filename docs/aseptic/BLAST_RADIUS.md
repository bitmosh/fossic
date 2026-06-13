---
title: Blast Radius — Per-Pass Artifact Specification
---

# Blast Radius — Per-Pass Artifact Specification

The blast radius report is generated at the completion of every pass. It is a structured
inventory of everything the pass touched. It feeds the PASS COMPLETE Discord message (as
the source for Highlights) and the cross-pollination report (which asks: "given what
changed, what do adjacent projects need to know?").

---

## Location

```
docs/aseptic/blast-radius/pass-NN.md      # whole-number passes
docs/aseptic/blast-radius/pass-N.M.md     # decimal passes (8.5, 8.6, etc.)
```

One file per pass. Generated at pass completion before the PASS COMPLETE message is drafted.

---

## Format

```markdown
---
pass: N or N.M
version: vX.Y.Z or vX.Y.Za
date: YYYY-MM-DD
summary: one sentence
---

# Blast Radius — Pass N (vX.Y.Z)

## Files

### Modified
- `path/to/file.rs` — what changed (one clause)
- `path/to/file.py` — what changed

### Created
- `path/to/new_file.rs` — purpose

### Deleted
- `path/to/removed_file.rs` — why removed

---

## Public APIs

### Added
- `Store::new_method(arg: Type) -> ReturnType` — what it does
- `NewType` — what it is

### Modified (breaking)
- `Store::changed_method` — what changed, what callers must update

### Modified (non-breaking)
- `Store::extended_method` — what was added (additive only)

### Removed
- `Store::removed_method` — why removed; migration path if any

---

## Schema changes

- `events` table: added column `new_column TEXT` — purpose
- `branches` table: added index `idx_...` — query it enables

If no schema changes: "None."

---

## Configuration changes

- `OpenOptions::new_field` added — default value, behavior when omitted
- `OpenOptions::removed_field` removed — what to use instead

If no config changes: "None."

---

## Dependency changes

- Added: `crate-name = "1.2"` in `fossic-py/Cargo.toml` — purpose
- Removed: `old-crate` — why

If no dependency changes: "None."

---

## Behavior changes

Changes in observable behavior (not API surface changes). Include:
- Correctness fixes that change output
- Performance characteristic changes
- Error message changes
- Default behavior changes

- `Store::read_state`: now starts from most recent snapshot instead of replaying
  all events. Output is identical; performance is O(events since snapshot) not O(all events).

If no behavior changes: "None."

---

## Living report updates

Entries added to living reports this pass:

- TECH_DEBT: TD-004 (new entry) — SimilaritySearchProvider trait stub
- POLISH_DEBT: (none)
- DEVIATION: DV-003 — purge_event tombstone semantics

Entries resolved this pass:

- DEVIATION: DV-001 resolved — Symbol.asyncIterator via wrapper class (Pass 8.5)

If no updates: confirm explicitly — "No new entries this pass. No entries resolved."
```

---

## The "no new entries" confirmation requirement

The living report updates section is **required** even when empty. An agent that made no
living report updates must write:

```markdown
## Living report updates

No new entries this pass. No entries resolved.
```

This is the structural safeguard against empty-report-by-omission. An agent that didn't
notice anything looks identical to an agent that didn't check — unless absence is made
explicit.

---

## Retroactive files

Blast-radius files generated before Aseptic was live in this project may include
"(retroactive — Aseptic not yet active)" or similar markers in headers and section
labels. These markers indicate the file was reconstructed from project history rather
than filed at pass-completion time, and have lower confidence than live-filed reports.

Retroactive files SHOULD still follow the canonical format above where possible. Cases
where canonical fields cannot be reconstructed from history should be marked "(unknown)"
or "(approximate)" rather than fabricated.

Future passes file live and do not use the retroactive marker. The presence of the
marker on a file means: this is historical reference material, treat with appropriate
epistemic humility.

**Retroactive commit walkthrough note:** In fossic, the v0.10.w realignment pass
updated retroactive files to include real SHAs and commit dates from the retroactive
git walkthrough. The Files sections in those files reflect what was actually in the
corresponding git commit — which may differ from the features the pass delivered,
because source files were committed at their post-v0.10.x final state in the earliest
retroactive commit that introduced them. Where this divergence exists, the blast-radius
file notes it explicitly.
