---
title: Agent Briefing — System-Prompt Fragment
---

# Agent Briefing

The following is a copy-pasteable prompt fragment for any agent participating in
an Aseptic-instrumented fossic pass. Include it at the start of the agent's system
prompt or task briefing.

---

```
## Aseptic discipline — fossic

You are working in a fossic pass under Aseptic discipline. Before writing any code,
read these three files:

  docs/aseptic/TECH_DEBT.md      — functional but known-bad implementation choices
  docs/aseptic/POLISH_DEBT.md    — correct but feels-wrong; mechanical to fix
  docs/aseptic/DEVIATION.md      — where implementation diverged from spec or ADR

These are the living reports. They are your map of known debt and divergence. An open
TECH_DEBT entry may explain why code looks the way it does. A DEVIATION entry tells you
that the spec and the code disagree — trust the code, not the spec, unless the entry says
otherwise.

### At pass completion, before posting PASS COMPLETE:

**1. Update the living reports.** For every finding you encountered during this pass:
- Is it functional but known-bad? → add a TECH_DEBT entry (ID: TD-NNN)
- Is it correct but imprecise? → add a POLISH_DEBT entry (ID: PD-NNN)
- Does implementation diverge from spec/ADR? → add a DEVIATION entry (ID: DV-NNN)
- Is an existing open entry now resolved? → mark it resolved (strikethrough + resolved block)

**2. Write the blast-radius file.** Create `docs/aseptic/blast-radius/pass-NN.md`
using the format in `docs/aseptic/BLAST_RADIUS.md`. The "Living report updates"
section is required even if empty — write "No new entries this pass. No entries resolved."

**3. Write the cross-pollination file if warranted.** If the blast-radius includes
breaking API changes, behavior changes, or new APIs that adjacent projects use, create
`docs/aseptic/cross-pollination/pass-NN.md` using the format in
`docs/aseptic/CROSS_POLLINATION.md`. fossic's adjacent projects are: cerebra,
policy-scout, lumaweave, bo, ai-stack.

**4. Write the pass report.** Use the format in `docs/aseptic/PASS_REPORTING.md`.
The "no new entries" confirmation in section 5 is required and must be explicit.

### Fail-loudly defaults

When implementation hits ambiguity, prefer a loud, explicit error over a silent fallback.
fossic uses this pattern throughout: `purge_event` requires a specific confirmation string,
`declare_stream` is required before `append`, `register_reducer` validates the reducer
protocol at registration time. When adding new APIs, follow this pattern.

### Deviation surfacing convention

If you discover during this pass that the spec says one thing and the code does another:
- Do NOT silently align the code to the spec (the code is likely correct)
- Do NOT silently align the spec to the code (spec changes need human review)
- DO add a DEVIATION entry with `status: OPEN — spec should be updated` or
  `status: OPEN — implementation should catch up`, as appropriate
- DO surface the deviation in your pass report (Section 4 or 5)

### Reading project state

The living reports are additive and resolved entries are preserved. To understand the
current state of a debt area:
- All entries without a `pass_resolved` value are open
- Entries with a strikethrough heading are resolved — read them for history, not for
  current state
- Entry severity (TECH_DEBT, DEVIATION) reflects risk as assessed at the time of writing;
  re-assess if circumstances changed

### Version to use for this pass

The VERSION_CONVENTION file specifies whether this pass is load-bearing (forward version:
v0.N+1.0) or cleanup (descending letter: v0.N.z, v0.N.y, etc.). The task prompt will
specify. When in doubt: load-bearing work that a user would notice → forward version;
cleanup that users wouldn't notice → descending letter.
```
