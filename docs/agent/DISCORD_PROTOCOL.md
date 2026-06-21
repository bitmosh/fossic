# Discord Protocol — fossic

All agent-to-developer approvals and reports go through Discord via the **MCP server only**
(never raw HTTP). This document is the single authoritative reference for channel IDs, message
formats, gate sequencing, and the blog bumper integration.

---

## Channel IDs (canonical — never guess these)

| Channel | ID | Purpose |
|---|---|---|
| `#approve-this` | `1506441138612080680` | Approval gates — merge, bump+push, destructive git |
| `#current-task` | `1506440945128701955` | START / END / in-progress status |
| `#changelog` | `1509728570367283250` | PASS COMPLETE reports (blog bumper reads this) |
| `#notifications` | `1506441052826107964` | Low-priority automated notices |
| `#brainstorm` | `1506441106869583932` | Design discussion |

---

## Per-pass flow (complete sequence)

```
1.  Confirm MCP connected                → HALT if not
2.  Brief START to #current-task
3.  Work + verify (typecheck, tests)
4.  Brief END to #current-task
5.  MERGE GATE → ping #approve-this      → wait for approval
6.  git commit (commit 1: substantive changes)
7.  git commit (commit 2: blast-radius + pass-complete files)
8.  Post PASS COMPLETE to #changelog     → use final form with real commit SHA
9.  Run: node bin/bumper.js bump --dry   → include output in step 10 message
10. BUMP+PUSH GATE → ping #approve-this  → wait for approval
11. Run: node bin/bumper.js bump         → live blog post
12. git push
```

**Two-commit SHA pattern:** commit 2 blast-radius frontmatter references commit 1's SHA.
Never amend commit 1 after the fact — that changes its SHA and breaks the reference.

---

## Gate messages

### MERGE GATE (#approve-this)

Post before committing. Include:
- Pass version and one-line summary of what shipped
- Test results (verbatim counts)
- The PASS COMPLETE draft (with `Commit: [PENDING_SHA]` placeholder — SHA not yet known)

```
MERGE GATE — vX.Y.Z

[one sentence: what this pass does]

Tests: N passed · M failed · K skipped

── PASS COMPLETE DRAFT ──────────────────────────────────────
[paste draft form from §PASS COMPLETE format below]
─────────────────────────────────────────────────────────────
```

### BUMP+PUSH GATE (#approve-this)

Post after PASS COMPLETE is in #changelog. Include the full `bumper bump --dry` output
so the developer can see exactly what will publish before approving.

```
BUMP+PUSH GATE — vX.Y.Z

bumper --dry output:
[paste full dry-run output here]
```

---

## PASS COMPLETE format

This is the format that blog.bumper parses. Every character in the header and bullets
is load-bearing. **Do not reconstruct this from memory — copy-paste the template.**

### Draft form (used in MERGE GATE message — SHA not yet known)

```
── PASS COMPLETE · vX.Y.Z · YYYY-MM-DD ──────────────────────

Title: [4–8 word blog-suitable title — not a commit subject]
Summary: [one sentence, 20–280 chars]
Project: fossic

Highlights:
· [concrete behavioral change — what changed, not which file]
· [concrete behavioral change]
· [concrete behavioral change]

Learnings:
· [optional methodology/architecture insight]

Commit: [PENDING_SHA]
Tests: [N] passed · [M] failed · [K] skipped
Branch: clean
```

### Final form (posted to #changelog after commit — use real 7-char SHA)

```
── PASS COMPLETE · vX.Y.Z · YYYY-MM-DD ──────────────────────

Title: [4–8 word blog-suitable title — not a commit subject]
Summary: [one sentence, 20–280 chars]
Project: fossic

Highlights:
· [concrete behavioral change — what changed, not which file]
· [concrete behavioral change]
· [concrete behavioral change]

Learnings:
· [optional methodology/architecture insight]

Commit: [7-char merge SHA]
Tests: [N] passed · [M] failed · [K] skipped
Branch: clean
```

### Canonical example (CRITICAL — follow this shape exactly)

This is the gold standard for what a well-formed fossic PASS COMPLETE looks like.
Match this shape: concrete summary, behavior-focused highlights, one tight learning.

```
── PASS COMPLETE · v1.0.1 · 2026-06-20 ──────────────────────

Title: Glob patterns in ReadQuery across all bindings
Summary: ReadQuery now accepts ** stream globs in all four layers; patterns resolve to matching stream IDs at subscription seed time rather than being passed as literal exact-match strings.
Project: fossic

Highlights:
· ReadQuery("cerebra/agent-trace/**") correctly matches all session-namespaced streams at subscribe time
· Glob resolution runs once at seed, not per-event — no pattern-matching overhead on the hot path
· fossic-py, fossic-node, and fossic-tauri all updated; existing exact-stream callers are unaffected

Learnings:
· NW §9.5 was a real behavioral gap, not a spec gap — exact-match-only was never documented as intentional

Commit: a3f9c12
Tests: 238 passed · 0 failed · 2 skipped
Branch: clean
```

**What makes this good:**
- Summary is one sentence, 196 chars, concrete — says what changed and why it matters
- Highlights describe call-site behavior and consumer impact, not filenames
- Third highlight proactively tells adjacent consumers they don't need to change anything
- Learning earns its place by closing a named open coordination item
- Total message ~650 chars — well under the 1800-char hard limit

---

## Field rules

| Field | Required | Rule |
|---|---|---|
| Header delimiter | **YES** | `── PASS COMPLETE · vX.Y.Z · YYYY-MM-DD ──────────────────────` — exact characters, see below |
| `Title:` | **YES** | 4–8 words, blog-suitable. Not a commit message subject. |
| `Summary:` | **YES** | One sentence, **20–280 chars** (fossic limit; bumper contract allows 300 — we self-limit for safety). Too short fails schema validation, not parsing. |
| `Project:` | **YES** | Always `fossic` for this repo. Must match enrolled registry name. |
| `Highlights:` | **YES** | 3–5 `·` bullets. Behavior changes only — not file names, not commit subjects. |
| `Learnings:` | no | `·` bullets. Captured as hidden commentary block; not rendered on the page. |
| `Commit:` | **YES** | Exactly 7 hex chars `[0-9a-f]{7}`. Idempotency key — same SHA = skip. Use `[PENDING_SHA]` in draft form only. |
| `Tests:` | no | Observability only — logged to debug channel, not written to post. |
| `Branch:` | no | Observability only. |
| Header date | YES | Decorative for the parser. Actual post date = Discord message server timestamp. |
| `Project:` omission | — | Would fall back to bumper config default module — but for fossic always include it. |

**Total message length: ≤ 1800 characters.**

---

## Character constraints (parser-load-bearing)

These exact Unicode code points must be used. Wrong characters cause silent parse failures
or skipped posts with no error.

| Symbol | Unicode | Where used | Common wrong substitution |
|---|---|---|---|
| `──` | U+2500 BOX DRAWINGS LIGHT HORIZONTAL | Header delimiter | U+002D HYPHEN-MINUS `-` |
| `·` | U+00B7 MIDDLE DOT | Field bullets | U+002E FULL STOP `.`, U+002D `-` |

**Never reconstruct the header from memory.** Always copy-paste from this document or from
`docs/aseptic/PASS_REPORTING.md`. The dot-less version letter (`v1.0.0z` not `v1.0.0.z`) and
the exact box-drawing characters have both been lost to reconstruction at least twice.

---

## Version format (also parser-load-bearing)

```
Correct:   v1.0.0z    v1.0.0y    v1.0.0aa
Incorrect: v1.0.0.z   v1.0.0.y   v1.0.0.aa
```

The dot between patch number and letter is **forbidden**. This drives the slug and version
fields in the published post. A dotted form produces a malformed slug or skipped post.

---

## Blog bumper CLI reference

```bash
# At BUMP+PUSH GATE — run before posting gate message:
node bin/bumper.js bump --dry

# After approval:
node bin/bumper.js bump

# Enroll fossic (one-time setup):
node bin/bumper.js project-add fossic --path ~/Projects/fossic

# Target specific message (bypass buffer):
node bin/bumper.js bump --msg <discord-message-id>
```

`buffer = 1` (default): bumper reads the second-most-recent #changelog message, giving
a one-message correction window. To correct a bad report: post a new corrected one — the
bad one moves to buffer position, the corrected one becomes latest (unread by bumper).

Full reference: `docs/agent/CHANGELOG_CONTRACT.md`

---

## What NOT to ping

Do not ping `#approve-this` for: reads, typechecks, test runs, diagnostics, in-progress
edits not yet being committed.

If unsure whether something needs a gate ping: ask in `#current-task`.

---

*References: `docs/aseptic/PASS_REPORTING.md` (internal pass report format) ·
`docs/agent/CHANGELOG_CONTRACT.md` (bumper contract, local copy) ·
`~/Projects/public/blog.bumper-public/docs/` (bumper source docs)*
