---
title: Living Reports — Specification
---

# Living Reports — Specification

Three accumulating markdown files that every passing agent reads and updates. They are the
continuous instrument that replaces retrospective surveys. Each entry is written at the pass
that discovers the finding; each entry is resolved (never deleted) at the pass that closes it.

---

## General conventions

### Entry format

Every entry uses YAML frontmatter followed by a markdown body:

```markdown
---
id: TD-001
type: tech_debt  # or polish_debt, deviation
status: open     # open | resolved
pass_opened: v0.8.6
pass_resolved:   # blank until resolved
severity: MEDIUM # HIGH | MEDIUM | LOW (tech_debt, deviation) or omit (polish_debt)
---

### TD-001 — Short descriptive title

Body text describing the finding in detail. Enough context for an agent
that wasn't present for the original discovery.

**Trigger:** The condition under which this debt becomes worth addressing.

**Evidence:** Where to look to verify the finding is still accurate.
```

### Resolution convention

When a finding is closed, do NOT delete the entry. Mark it resolved:

```markdown
---
id: TD-001
type: tech_debt
status: resolved
pass_opened: v0.8.6
pass_resolved: v0.11.2
---

### ~~TD-001 — Short descriptive title~~

> **Resolved in v0.11.2** — commit `a3f8b21`. Trigger condition met:
> witness layer implemented and lattice reads crossed latency threshold.
> Rust-native reducer shipped. See blast-radius/pass-11.2.md.

<details>
<summary>Original entry (preserved for history)</summary>

... original body ...

</details>
```

Strikethrough the heading. Add a `> Resolved` block with the commit and resolution pass.
Collapse the original body inside `<details>`. The entry stays in the file, above all open
entries of the same section, in the order it was originally added.

### ID scheme

- Tech debt: `TD-NNN`
- Polish debt: `PD-NNN`
- Deviations: `DV-NNN`

IDs are assigned sequentially within each file. Never reuse an ID.

### Cleanup cadence triggers

Clean up (consolidate, remove truly stale entries, re-verify severities) when:
- The file exceeds 600 lines (supervisor pass automatically triggered at this threshold)
- A supervisor pass explicitly flags stale entries
- Three or more consecutive entries reference the same root cause (candidate for consolidation)

Cleanup means a dedicated polish pass, not inline editing during a feature pass.

---

## TECH_DEBT.md

**What goes in:** Functional but known-bad implementation choices. Deliberate deferrals.
Architectural shortcuts with a known cost. Implementations that bypass structural principles
for pragmatic reasons. The key test: *"does this work correctly today, and do we know
why it will need to change?"*

**What does NOT go in:** Things that don't work (those are bugs in the issue tracker).
Things that are just suboptimal style (those are polish debt). Things the spec says to
do differently but haven't been implemented yet (those are deviations).

*fossic example of what goes in:* The Python `read_state` implementation replays all
events from scratch because the Rust `BoxedReducer` trait wasn't public. It's correct
(returns the right state), it's known-bad (doesn't scale), and the cost of fixing it
requires an architectural change (making the DynReducer trait public).

*fossic example of what does NOT go in:* "The Node binding has no `reason` parameter on
`promote_branch`." That's a deviation (implementation differs from spec) not tech debt.
It doesn't work correctly; it silently drops data.

**Entry body structure:**

```markdown
**What it is:** One sentence description of the shortcut or deferral.

**Why it was necessary:** The constraint or pragmatic reason.

**Known cost:** What breaks, degrades, or becomes harder if this isn't addressed.

**Trigger:** The specific condition under which addressing this becomes justified.
  Format: "When [measurable condition] AND [measurable condition]."

**Evidence:** File + line reference, test output, or benchmark result that makes
  the debt visible.
```

---

## POLISH_DEBT.md

**What goes in:** Things that are correct but feel wrong. Naming inconsistencies.
Doc gaps — missing docstrings, stale README sections, spec examples that don't match
behavior. Test helper duplication. File organization that grew organically. The key
test: *"is this correct, and is it also slightly wrong in a way that will confuse
someone?"*

**What does NOT go in:** Architectural issues (tech debt). Things that are wrong in
a way that causes incorrect behavior (bugs). Things where fixing it requires a design
discussion (tech debt or a feature).

*fossic example of what goes in:* The `register_upcaster` method in `fossic-py/__init__.py`
has no docstring. `register_payload_transform` got one in Pass 8.6. The inconsistency is
mechanical to fix (add a docstring), correct (the method works), and slightly wrong
(API documentation is incomplete).

*fossic example of what does NOT go in:* "The system stream event types use three different
naming conventions." That's not purely cosmetic — it affects consumers querying the system
stream and requires a spec decision about which convention to adopt. That's tech debt or a
deviation, not polish.

**Entry body structure:**

```markdown
**What it is:** One sentence.

**Where:** File(s) and relevant section.

**Fix:** What the corrected state looks like. Should be specific enough that a
  future agent can execute the fix without design discussion.
```

Polish debt entries intentionally have less structure than tech debt entries — they're
meant to be a checklist that a cleanup pass burns down, not a design document.

---

## DEVIATION.md

**What goes in:** Where implementation diverged from spec or ADR. This is an information
log, not a failure log. Deviations are often correct responses to discovered constraints
(a spec that assumed Tokio; a napi-rs limitation that prevents direct Symbol binding).
The entry records the divergence factually so that future agents don't re-discover it.

**What does NOT go in:** Things where the spec is aspirational/vague and the implementation
made a reasonable choice (those are spec gaps, document them as POLISH_DEBT). Implementation
bugs that aren't spec-related. Performance issues (tech debt unless they're caused by
a spec-mandated constraint).

*fossic example of what goes in:* FOSSIC_V1_SPEC.md §4.3 showed subscription examples with
`stream_pattern="cerebra/lattice/*"`. The `SubscribeQuery` struct takes exact stream IDs only.
The spec said one thing; implementation shipped another; neither is strictly wrong — the spec
was aspirational. The deviation entry records the gap and whether the spec should catch up.

*fossic example of what does NOT go in:* "The DynReducer Python implementation is slow."
That's tech debt (correct but costly), not a deviation from spec.

**Entry body structure:**

```markdown
**Spec said:** Exact quote or specific reference (section + line where possible).

**Implementation did:** What actually shipped, with code reference.

**Why:** The constraint or discovery that made the spec's requirement impractical.

**Status:** One of:
  - `OPEN — spec should be updated` (deviation is correct; spec is stale)
  - `OPEN — implementation should catch up` (spec is correct; implementation is incomplete)
  - `RESOLVED — spec updated in vX.Y.Z` (spec corrected to match implementation)
  - `RESOLVED — implementation corrected in vX.Y.Z` (implementation corrected to match spec)

**Adjacent impact:** Which other modules or consumers are affected by the gap.
```
