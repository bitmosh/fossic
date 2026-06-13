---
title: Version Convention — Forward and Descending-Letter Passes
---

# Version Convention

## Format strictness

Descending-letter cleanup passes use the format `v<major>.<minor>.<patch><letter>` with
**no dot** between the patch number and the letter:

```
Correct:   v0.10.0t   v0.10.0u   v0.10.0s
Incorrect: v0.10.0.t  v0.10.0.u  v0.10.0.s
```

This format is parser-load-bearing for Blog Bumper integration. Convention drift produces
parse failures and silent post-skips in the changelog pipeline.

Every pass prompt that generates either a commit version or a PASS COMPLETE message must
paste the canonical version format verbatim rather than reconstructing it from prior
context. Multiple format drifts were observed on 2026-06-12 when prompts allowed
reconstruction — the dot was reintroduced on two consecutive passes before correction.

This strictness applies to descending-letter passes only. Forward-versioned passes
(`v0.10.0`, `v0.10.1`, `v0.11.0`) follow standard semantic versioning and are not
subject to this rule.

---

fossic uses two versioning tracks that run in parallel: forward versioning for
load-bearing work and descending-letter versioning for non-load-bearing cleanup.

---

## Forward versioning — load-bearing passes

Work passes that deliver user-visible behavior increment the version normally:

```
v0.9.0  → v0.10.0 → v0.11.0 → v0.12.0
```

A "load-bearing" pass is one where you would describe the change to a user as
*"we made X better"* or *"fossic now does Y."* Examples:

- Pass 10 (DynReducer snapshot caching): "read_state now uses snapshot caching;
  p99 latency improved by ~10x on high-event-count streams." → `v0.10.0`
- Pass 11 (Threading model spec fix + tilde expansion): "spec and binding behavior
  now agree on tilde path expansion; threading model documented correctly." → `v0.11.0`
- Pass 3 (CCE): "events now have content-addressed IDs; identical events across
  streams deduplicate." → `v0.3.0`

---

## Descending-letter versioning — cleanup passes

Non-load-bearing passes take a descending letter suffix on the current forward version,
counting DOWN from `z`:

```
v0.11.0  →  v0.11.z  →  v0.11.y  →  v0.11.x  →  v0.12.0
```

A "non-load-bearing" pass is one where you would describe the change to a user as
*"we cleaned up internally"* or *"docs got better."* Examples:

- This pass (Aseptic bootstrap): docs-only addition, no behavior change. → `v0.10.x`
- A polish pass fixing docstrings and test helper duplication. → `v0.11.z`
- A supervisor pass cleaning up stale TIDYUP findings. → `v0.11.y`

**The load-bearing test:** Ask — *"would you describe this fix to a user as 'we made X better'?"*
- Yes → forward versioning.
- No, it's internal cleanup → descending letter.

When in doubt, prefer forward versioning. Descending letters are for passes where even
the most engaged user would correctly say "that doesn't affect me."

---

## Cadence norms

Three to five descending-letter passes between forward versions is normal. This is evidence
of a healthy cleanup habit: load-bearing work happens, then polish and docs catch up.

**More than five descending letters** between forward versions is a soft pressure signal
that the upstream development cadence has slowed or that cleanup is being over-prioritized
relative to load-bearing work. Not an error — just a signal to notice.

**Zero descending letters** for several forward versions in a row is a signal that cleanup
debt is accumulating. The living reports are the instrument for tracking this.

---

## Pass numbering vs. version numbers

Pass numbers (Pass 1, Pass 8.5, Pass 8.6, Pass 10) are conversation-session identifiers,
not version numbers. A single pass may bump the version once. Passes within the same
conversation context inherit the same base version. The mapping:

| Pass | Version |
|---|---|
| Pass 1 | v0.1.0 |
| Pass 2 | v0.2.0 |
| Pass 3 | v0.3.0 |
| Pass 4 | v0.4.0 |
| Pass 5 | v0.5.0 |
| Pass 6 | v0.6.0 |
| Pass 7 | v0.7.0 |
| Pass 8 | v0.8.0 |
| Pass 8.5 | v0.8.1 |
| Pass 8.6 | v0.8.6 |
| Pass 9 | v0.9.0 |
| Pass 10 | v0.10.0 |
| Pass 11 | v0.11.0 |
| This pass (Aseptic bootstrap) | v0.10.x |

The Aseptic bootstrap (`v0.10.x`) is a descending-letter cleanup pass anchored to `v0.10.0`
rather than `v0.11.0` because its content documents the state of the system through Pass 10.
Pass 11 is concurrent; they do not conflict.

---

## PASS COMPLETE version field

The `v<version>` in the PASS COMPLETE Discord message is the version assigned to the pass.
For descending-letter passes, use the full version including the letter: `v0.10.x`. The
blog.bumper parser accepts the optional trailing `[a-z]` suffix on version numbers.
