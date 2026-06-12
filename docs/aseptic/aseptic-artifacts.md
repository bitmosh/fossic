# Aseptic — Notes (Artifacts and Operations)

**Status:** Bench notes, June 2026. Not a spec.
**Companion:** `aseptic-notes.md` (the conceptual frame)

This doc captures the operational artifacts and the rules that govern them. When Aseptic becomes its own project, these notes are the seed material for the formal specs of LIVING_REPORTS, BLAST_RADIUS, CROSS_POLLINATION, PASS_REPORTING, and ADR_FORMAT.

---

## The five artifacts

### Living reports (accumulate across passes)

**`TECH_DEBT.md`** — Functional but known-bad implementation choices.

Each entry captures a deliberate deferral, architectural shortcut with known cost, or implementation that bypasses a structural principle for pragmatic reasons. Entries always have a triggering condition for cleanup. Without a trigger, debt becomes wallpaper.

Example shape:
```
## DynReducer snapshot caching for foreign-language reducers
Status: open
Trigger: any Python reducer stream exceeds 1k events in production
Cost estimate: ~30 min
Context: Pass 5 (v0.5.0) shipped Python reducers that full-replay
on every read_state. The Rust core's compile-time Reducer trait
can't bridge to Python directly. A DynReducer trait would enable
snapshot caching but was deferred to keep v1 binding scope tight.
Resolution: TBD
```

**`POLISH_DEBT.md`** — Correct but feels-wrong.

Naming inconsistencies, doc gaps, README staleness, test helper duplication, file organization that grew organically. Mechanical to fix; doesn't require design discussion. Separated from tech debt because the cleanup cadence is different.

Example shape:
```
## Tilde expansion spec examples inconsistent with binding behavior
Status: open
Pass observed: Pass 11 (v0.11.0)
Context: Pass 9 implemented binding-handles-tilde via shellexpand.
Pass 11 documented spec examples showing consumer-handles-tilde
(os.path.expanduser). Both are technically correct but inconsistent
with the design decision.
Fix: Update FOSSIC_V1_SPEC.md §4.2 and §4.3 examples to show
`Store.open("~/.fossic/store.db")` directly, with comment noting
"tilde expanded by the binding."
```

**`DEVIATION.md`** — Where implementation diverged from spec/ADR.

The most important of the three. Not a failure log — an information log. A deviation is when the agent decided that following the spec literally would be wrong, and made a different choice with justification.

Example shape:
```
## napi-rs cannot bind Symbol.asyncIterator via js_name attribute
Pass: Pass 8.5 (v0.8.1)
Spec said: §4.3 — "subscriptions surface as AsyncIterable"
Implementation did: JS wrapper class in index.js exposes
[Symbol.asyncIterator]() and [Symbol.asyncDispose](), delegating
to internal raw_next() and raw_dispose() methods on the napi-rs
class.
Why: napi-rs 2.x's #[napi(js_name = "[Symbol.asyncIterator]")]
attribute treats the value as a literal string. The well-known
symbol cannot be expressed via the js_name attribute. The JS
wrapper pattern is the standard mitigation (used by better-sqlite3
and others).
Spec update needed: YES — §4.3 should document the JS wrapper
pattern as the actual mechanism.
```

Each deviation has: what the spec said, what was done, why the divergence was necessary, and whether a spec update is needed.

Resolution convention for all three living reports: when an entry is resolved, it gets a strikethrough heading with the resolving commit reference and the resolution pass version. Don't delete — the history of "we knew, we fixed" is preserved.

### Per-pass artifacts (don't accumulate)

**`BLAST_RADIUS/pass-NN.md`** — One file per pass, structured.

Describes what *this specific change* touched. Inputs are: files modified/created/deleted (with line counts), public APIs added/modified/removed, database schema changes, configuration changes, dependency additions/version bumps, behavior changes consumers would notice.

The blast-radius file is the input for the PASS COMPLETE Discord message (Blog Bumper). Highlights and learnings in the PASS COMPLETE are a curated summary of the blast radius.

The blast-radius file is also the input for CROSS_POLLINATION derivation.

**`CROSS_POLLINATION/pass-NN.md`** — One file per pass, per-adjacent-project sections.

Describes what changed in this project that adjacent projects need to know about. For each adjacent project potentially affected:
- What changed that affects them
- Severity (BLOCKING / NEEDS-AWARENESS / FYI)
- Suggested user action — pre-drafted message text the user can copy-paste to brief that project's advocate agent, or a verification command, or "no action needed"

Cross-pollination is separated from blast-radius because the audiences differ. Blast radius is "what this project did." Cross-pollination is "what adjacent projects should do about it."

For fossic Pass 9 specifically, the cross-pollination doc would have entries for: Cerebra (subscribe API changed, list_branches semantics for main), Policy Scout (subscribe API changed), LumaWeave (glob patterns now in IPC), Bo / ai-stack (FYI only).

---

## The version convention

Work passes increment normally: `v0.9.0`, `v0.10.0`, `v0.11.0`.

Cleanup passes for non-load-bearing debt count down from `z`: `v0.11.z`, `v0.11.y`, `v0.11.x`.

The load-bearing test: if you'd describe the fix to a user as "we made X better," it's load-bearing and uses forward versioning. If you'd describe it as "we cleaned up internally," it's non-load-bearing and uses descending letters.

Pass 8.5 in fossic's history (walk_causation truncation, Symbol binding correctness, concurrent-next races) was load-bearing — those affected user-visible behavior. Forward versioning was correct: `v0.8.1`.

A polish pass that consolidated three `unique_ev()` helpers and renamed `fossic.Purged` to `Purged` would have been non-load-bearing — descending letter would have been correct: `v0.4.z` or similar.

Three to five descending letters between forward versions is normal. Consuming more is a soft signal the upstream cadence is wrong (debt is accumulating faster than it's being addressed).

---

## ADR template for parallel-agent execution

The format that survives parallel execution:

```
# ADR-N: <Title>

## Decision
<One sentence. The thing being decided.>

## Constraints (enforceable)
- <Specific, testable. "Type X must implement trait Y." "API method
  Z returns Result<T, FossicError>.">

## Boundaries (parallel-execution-safe)
- Files this decision permits modification of: <explicit list>
- Files this decision PROHIBITS modification of: <explicit list>
- Other ADRs this decision depends on: <list with version refs>

## Invariants (testable)
- <Property tests or assertions that must hold after this decision
  is implemented. Each invariant has a name and a test that exercises it.>

## Failure-mode preference
- When implementation hits ambiguity, prefer: <"loud failure" /
  "explicit refusal" / "well-defined fallback">. Justification: <one sentence>.

## Context (for humans)
<Why this decision was made. Tradeoffs considered. What this replaces.>

## Consequences
<What downstream work this enables or constrains.>
```

The top sections (Decision through Failure-mode preference) are what parallel agents read. The bottom sections are conversational context for humans. The discipline is making the agent-facing section strictly enforceable while the human-facing section remains conversational.

---

## Pass reporting structure

Every pass produces a structured report that feeds the artifacts. Required sections:

1. **Deliverables status** — per-deliverable: completed / partial / blocked, with file/line citations
2. **Test results** — counts per artifact (core, bindings, etc.), pass/fail/skip
3. **Files touched** — modified / created / deleted with line counts (feeds BLAST_RADIUS)
4. **API changes** — public surface added/modified/removed (feeds BLAST_RADIUS and CROSS_POLLINATION)
5. **Living report updates** — what was appended to TECH_DEBT / POLISH_DEBT / DEVIATION this pass, or explicit "no new entries" confirmation
6. **Adjacent project impact** — which adjacent projects are affected, severity, suggested action (feeds CROSS_POLLINATION)
7. **PASS COMPLETE template** — pre-filled Discord message ready for posting

The "no new entries this pass" confirmation requirement is structural. An agent that didn't notice anything looks identical to one that didn't check. Forced explicit confirmation prevents the empty-report-by-omission failure mode.

---

## Supervisor pass

Runs between parallel batches, not after the arc closes. Cheap relative to retrospective surveys because the living reports already surfaced most issues.

Supervisor's first input is the four living reports plus the per-pass blast-radius and cross-pollination files since the last supervisor pass.

Supervisor's responsibilities:

1. **Read the living reports.** Identify open entries that have accumulated since the last supervisor pass.
2. **Run the integrity loop.** Fetch git diffs for the period and cross-check against blast-radius files. Flag any change not represented in a blast-radius. This catches the "no new entries" failure mode where an agent silently shipped something without recording it.
3. **Verify spec coherence.** Cross-check DEVIATION.md entries against spec docs. Any deviation marked "spec update needed" should result in either a follow-up spec update task or an acknowledged decision to keep the spec as-is.
4. **Cross-check living reports against actual code state.** Find code patterns that should be in TECH_DEBT or POLISH_DEBT but weren't logged.
5. **Identify findings the per-pass agents missed.** Convention drift across agents, partial invariant implementations, latent integration issues.
6. **Produce `SUPERVISOR_REPORT.md`** — severity-classified findings with proposed cleanup tasks.
7. **Halt before executing.** The user reviews the supervisor report and approves the cleanup batch before any code changes happen.

The supervisor pass is itself an agent execution. It follows Aseptic discipline: produces its own blast-radius report, can surface deviations from its prompt, etc.

---

## Parallel-safety rules

When running N agents in parallel against the same project:

1. **Freeze the spec before launching.** Declare "you are all working against spec version M." Any agent that wants to modify the spec surfaces a proposal back rather than editing unilaterally. After the batch lands, spec gets reconciled and the next batch starts.

2. **Explicit file boundaries per agent.** Every parallel agent's prompt includes `# Files NOT to touch` listing what other parallel agents own. Not "be careful" — explicit lists.

3. **Designate shared-file owners.** Files like `Cargo.toml`, `lib.rs` re-exports, central `impl` blocks get edited by multiple agents. Pick one agent per batch to own all such edits; others surface their additions via the report.

4. **Write the test before launching.** If an ADR has an invariant, the property test that exercises it should already exist when agents start. Each agent runs the suite. No agent can claim "I implemented the spec" while violating an invariant another agent's test exercises.

5. **Run a supervisor pass between batches.** Not optional. Catches drift before the next batch builds on it.

---

## Failure modes and their mitigations

Captured here as a quick reference. Each was directly observed during fossic's build.

| Failure mode | Mitigation in Aseptic |
|---|---|
| Convention drift | Shared-utility specifications in ADRs; supervisor catches drift between batches; tidy-up passes consolidate |
| Spec-as-aspiration | Specific enforceable language in ADRs; "fail loudly" default; deviations surfaced not hidden |
| Latent integration bugs | Supervisor pass; cross-pollination doc surfaces breaking changes to adjacent projects |
| Partial invariant implementation | Invariants section in ADRs; property tests; supervisor cross-checks |
| ADR drift | Spec freeze before parallel batches; version refs in ADR boundaries |
| Silent shortcuts | "Fail loudly" default; structured pass report with mandatory updates; supervisor integrity loop |

---

## Open design questions

To resolve when Aseptic becomes its own project:

1. **Do the three living reports map cleanly to non-software projects?** Tested mentally against fossic; haven't tested against a documentation project or research methodology. May need a fourth report category for non-code work.

2. **How do agents handle the supervisor pass's findings?** Is there a "supervisor said do X" deliverable that gets folded into the next pass, or does it become its own cleanup batch? Probably depends on severity; needs explicit rule.

3. **The MCP server's exact tool surface.** Should agents call structured tools like `aseptic.log_deviation(spec_ref, description, why)` instead of hand-writing markdown? Yes probably, but the structured-tool-vs-free-text tradeoff isn't fully worked out. Free text is easier to write; structured tools enable validation and cross-referencing.

4. **Schema evolution.** When the living-report formats need to evolve (v1 to v2), how do existing report entries migrate? Probably the same as fossic's upcaster pattern — entries carry a format_version, readers know how to upcast.

5. **Cross-project blast-radius integration.** Right now, "tell Cerebra about this change" is a human action with a pre-drafted message. Could a future Aseptic MCP server *push* the cross-pollination notes to enrolled adjacent projects' Aseptic instances automatically? Probably yes, but the trust and validation model isn't designed yet.

These are not blockers for capturing the methodology. They're items to think through when Aseptic graduates from notes to project.
