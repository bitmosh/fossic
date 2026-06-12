---
title: Pass Reporting — Structured Format
---

# Pass Reporting — Structured Format

The structured pass report is the internal completion artifact for every fossic pass.
It is written before the PASS COMPLETE Discord message — the Discord message is derived
from it, not the other way around.

---

## Required sections

```markdown
# Pass Report — Pass N (vX.Y.Z)

**Date:** YYYY-MM-DD
**Version:** vX.Y.Z
**Type:** load-bearing | cleanup (descending-letter)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| [what was specified] | DONE / PARTIAL / SKIPPED | reason if not DONE |

If partial or skipped: explain what remains and whether it's a STOP or a
deliberate deferral with a tracking entry.

---

## 2. Test results

```
[paste verbatim output of the test run — do not summarize]
```

Or if tests were not run: explain why (docs-only pass, no runnable tests affected).

---

## 3. Files touched

Reference the blast-radius file: `docs/aseptic/blast-radius/pass-N.md`

Summary here:
- Modified: [count] files
- Created: [count] files  
- Deleted: [count] files

Key files: [list the 3-5 most significant]

---

## 4. API changes

Reference the blast-radius file for full list. Highlight here:
- Breaking changes: [none | list]
- New public APIs: [none | list]
- Removals: [none | list]

---

## 5. Living report updates

### New entries this pass

- TECH_DEBT: [none | TD-NNN (title)]
- POLISH_DEBT: [none | PD-NNN (title)]
- DEVIATION: [none | DV-NNN (title)]

**If no new entries:** "No new entries this pass." (explicit confirmation required)

### Entries resolved this pass

- [none | ID — title — resolution summary]

---

## 6. Adjacent project impact

Reference the cross-pollination file if one was produced: `docs/aseptic/cross-pollination/pass-N.md`

If no cross-pollination file: "No adjacent-project impact this pass."

If cross-pollination file exists: list impacted projects and severity.

---

## 7. PASS COMPLETE message ready

The following is ready to post to #changelog:

```
── PASS COMPLETE · vX.Y.Z · YYYY-MM-DD ──────────────────────

Title: [4–8 word blog-suitable title]
Summary: [one sentence, 20–300 chars]
Project: fossic

Highlights:
· [concrete behavioral change — derived from blast-radius]
· [concrete behavioral change]
· [concrete behavioral change]

Learnings:
· [optional methodology/architecture insight]

Commit: [7-char merge SHA]
Tests: [N] passed · [M] failed · [K] skipped
Branch: clean
```
```

---

## The "no new entries" requirement — why it's structural

Section 5 requires an explicit "No new entries this pass" confirmation when there are
no living report updates. This is not optional.

The failure mode it prevents: an agent that made no living report updates is
indistinguishable from an agent that didn't check — unless absence is made explicit.
A pass report that simply omits Section 5 reads as "nothing to report here" but could
equally mean "I forgot to check."

Forced explicit confirmation converts the ambiguity into a clear signal: this agent
checked the living reports, found nothing new to add, and confirmed that explicitly.
If a future supervisor pass finds a debt that this pass should have caught, the gap
is visible: the agent said "no new entries" when there was one.

---

## Deviations from this format

If a pass has a good reason to omit or modify a section, document that in the report:

```markdown
## 2. Test results

Not applicable: this is a docs-only pass (v0.10.x). No source files were modified;
no test suite was run.
```

Omitting a section silently is not acceptable.
