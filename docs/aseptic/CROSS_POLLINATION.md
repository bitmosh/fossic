---
title: Cross-Pollination — Per-Pass Adjacent-Project Notification Specification
---

# Cross-Pollination — Per-Pass Adjacent-Project Notification Specification

The cross-pollination report is generated when a pass has meaningful impact on adjacent
projects. It translates the blast-radius into actionable notifications per adjacent consumer —
pre-drafted so the human can approve and forward them without having to synthesize
the impact themselves.

---

## Location

```
docs/aseptic/cross-pollination/pass-NN.md
docs/aseptic/cross-pollination/pass-N.M.md
```

A cross-pollination file is only created when at least one adjacent project has an impact
entry. Passes with zero adjacent-project impact produce no file (document this in the
blast-radius file under "Living report updates").

---

## fossic's adjacent projects

| Project | Language | Consumes fossic via | Primary consumer surface |
|---|---|---|---|
| cerebra | Python | `fossic-py` | `register_reducer`, `read_state`, `take_snapshot`, subscriptions |
| policy-scout | Python | `fossic-py` | `append`, `read_range`, `register_payload_transform` |
| lumaweave | TypeScript | `fossic-node` + Tauri IPC | subscriptions (Graph B), event read, Tauri commands |
| bo (discord-bot) | Python | `fossic-py` | `append`, `read_range` (agent trace logging) |
| ai-stack | Docker/Python | `fossic-py` | indirect (through cerebra) |
| rhyzome | Python | `fossic-py` | branches, `create_branch`, `promote_branch` (benched) |
| bons.ai | Python | `fossic-py` | `indexed_tags`, aggregation, `SimilaritySearchProvider` (benched) |

---

## Format

```markdown
---
pass: N or N.M
version: vX.Y.Z
date: YYYY-MM-DD
impacts: [cerebra, policy-scout]  # only those with entries
---

# Cross-Pollination — Pass N (vX.Y.Z)

## cerebra

**Severity:** BLOCKING | NEEDS-AWARENESS | FYI

**What changed:** One paragraph. Specific enough that a cerebra maintainer can
assess impact without reading the full blast-radius.

**Action required:** One of:
- No action needed — behavior unchanged; this is informational.
- Verify: `[specific command or test to run]`
- Update: `[specific file or call site to change]`

**Advocate-agent message:**
> [Pre-drafted message in the voice of an advocate agent posting to cerebra's
> equivalent of #current-task. Includes: what changed in fossic, what specifically
> to check, and whether a response is needed. Should be copy-pasteable with minimal
> editing.]

---

## policy-scout

[same structure]
```

---

## Severity definitions

**BLOCKING** — the adjacent project cannot safely use its current fossic version after
this pass. Examples: breaking API change, correctness fix that changes output, behavior
change that would silently produce wrong state.

**NEEDS-AWARENESS** — the adjacent project should know about this change and may need to
update, but can continue operating on the current version in the meantime. Examples:
new API that solves a known workaround, performance improvement that requires a snapshot
cadence update, deprecation notice.

**FYI** — informational only. No action needed; the consumer's behavior is unchanged.
Examples: internal refactoring, doc update, a new API the consumer doesn't use.

---

## Pre-drafted advocate-agent message format

The message should be written in the voice of an agent briefing the adjacent project's
team — not a changelog entry. It should:

1. Open with the version and one-sentence description of what changed
2. State the specific impact for this project (file, call site, or behavior)
3. State the action required or confirm none is needed
4. Offer to coordinate if the impact is BLOCKING

```
fossic vX.Y.Z shipped. [One sentence description of the change.]

Impact for [project]: [specific impact — API change, behavior change, or new feature].

[If action required:] Action needed at [file/call site]: [what to change].
[If no action:] No changes needed — your existing usage is unaffected.

Severity: [BLOCKING | NEEDS-AWARENESS | FYI]. [If BLOCKING, add:] Coordinate before
your next merge that touches fossic calls.
```

---

## Deciding whether a pass produces a cross-pollination file

A pass produces a cross-pollination file if the blast-radius includes any of:
- A breaking API change
- A behavior change affecting shared interfaces (IPC contracts, event schemas, API signatures)
- A new API that solves a known workaround in an adjacent project
- A deprecation or removal

A pass does NOT produce a cross-pollination file if the blast-radius includes only:
- Internal refactoring with identical external behavior
- Doc and test updates
- New features that no adjacent project uses

When in doubt, produce the file with FYI entries. A redundant notification is better than
a missed one.
