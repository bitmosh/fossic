# Aseptic — Notes (Frame)

**Status:** Bench notes, June 2026. Not a spec. Preserves the conceptual frame for a future methodology project.
**Companion:** `aseptic-artifacts.md`

---

## What Aseptic is

A methodology for multi-agent code execution that treats coordination drift as a *contamination problem* — something prevented through continuous discipline at the work boundary, not cleaned up retrospectively after damage accumulates.

The name `aseptic` is the metaphor: medical-grade sterile technique applied to agent coordination. The discipline is continuous (every pass maintains the instruments) rather than periodic (deep clean after the mess).

Name available on crates.io, npm, PyPI as of June 2026. Lean medical-clinical, which is a feature for technical contexts.

---

## The problem it addresses

Parallel agent execution against shared specifications produces six recurring failure modes:

1. **Convention drift.** Multiple agents independently invent multiple solutions to the same micro-problem (three different `unique_ev()` helpers, three different error message conventions, three different test fixture patterns). Each works in isolation; together they're inconsistent.

2. **Spec-as-aspiration.** Aspirational language in the spec produces plausible-looking but wrong code. The agent invents a syntactically-valid attribute that doesn't actually do what the spec required, and no test in the agent's scope catches it.

3. **Latent integration bugs.** A pass completes 100% green in its own scope, but the green outcome masks issues that only surface when downstream work runs against it. Parallel agents can produce clean-looking outcomes while hiding integration failures.

4. **Partial invariant implementation.** An agent discovers a load-bearing invariant during a pass and fixes the case it saw, but doesn't generalize the invariant to other places it applies. Future agents reintroduce the bug.

5. **ADR drift.** Two agents read the same ADR at different times after amendments and produce divergent code against what they each believed was the canonical spec.

6. **Silent shortcuts.** An agent encounters ambiguity, makes a plausible choice silently, and the choice is wrong. The wrongness is undetectable until much later because the failure mode is invisible (e.g., a hung Promise that never resolves rather than throwing).

The traditional response — survey-after-arc retrospectives — catches some of these but only after they've ossified. Aseptic's premise: catch them per-pass, structurally, through continuous instruments.

---

## Core conviction

**Constraints survive parallel execution. Intentions don't.**

A constraint is enforceable at the boundary of an agent's work. An intention requires alignment of judgment across agents. Two agents executing in isolation form different judgments and have no way to learn what the other decided.

ADRs in the multi-agent era stop being communication of intent and become the actual coordination mechanism. They have to be *specific*, *enforceable*, *testable*, and *boundary-aware* in ways that traditional ADRs aren't required to be. The document does more work than it used to.

This generalizes: any artifact intended to coordinate multiple agents needs to be enforceable at the boundary of each agent's work. Vague guidance fails silently and produces drift.

---

## The four moves that make the system work

1. **Specific, enforceable specifications.** ADRs use the format in `aseptic-artifacts.md`. The agent-facing section is strictly enforceable; the human-facing section remains conversational.

2. **Continuous instruments, not retrospective surveys.** Three living reports (`TECH_DEBT.md`, `POLISH_DEBT.md`, `DEVIATION.md`) accumulate per-pass entries. Plus one per-pass artifact (`BLAST_RADIUS`) and one cross-project artifact (`CROSS_POLLINATION`). Five artifacts total.

3. **Supervisor passes between batches, not after the arc.** The supervisor reads the living reports as first input, cross-checks against git diff for the integrity loop, and proposes targeted cleanup *before* the next parallel batch starts. Cleanup triggers off report state, not human noticing.

4. **Fail-loudly defaults.** When an agent encounters ambiguity, the correct behavior is to refuse and surface, not to make a plausible choice silently. Every "plausible choice silently made" is a landmine; every "agent refused and surfaced" is recoverable.

---

## The version convention

Work passes increment normally: `v0.9.0`, `v0.10.0`, `v0.11.0`.

Cleanup passes for non-load-bearing debt count down from `z`: `v0.11.z`, `v0.11.y`, `v0.11.x`. The descending letter encodes "this is debt resolution, not forward progress." Three to five letters of runway is normal; consuming more is a signal the upstream cadence is wrong (acts as a soft pressure indicator).

The load-bearing distinction: if you'd describe a fix to a user as "we made X better," it's load-bearing and uses forward versioning. If you'd describe it as "we cleaned up internally," it's non-load-bearing and uses descending letters.

Same change, different framing, different version treatment. The version stream becomes legible at a glance: forward versions = features and load-bearing fixes; descending letters = internal hygiene.

---

## What Aseptic explicitly is not

- **Not a process-heavy methodology.** Each living-report entry should be 1-3 sentences. Each blast-radius file is a structured snapshot. The discipline is lightweight per-pass; the value comes from continuity, not depth.

- **Not a replacement for spec discipline.** Aseptic assumes the specs already exist and are well-written. It addresses the gap between "spec is good" and "multi-agent execution against the spec stays clean." If the spec is bad, Aseptic won't save you.

- **Not automation.** The MCP server (future-state) makes the instruments easier to maintain, but the discipline is human-and-agent-driven. Aseptic is a contract, not a tool.

- **Not universally applicable.** Works well for projects with clean module boundaries, well-understood domains, and ADR-friendly decision structure. Likely struggles with projects where "taste" or "ergonomics" are primary inputs (UI, business logic with subjective interfaces).

---

## Origin and stress test

Originated from fossic's June 2026 build — a Rust event sourcing library with Python, Node, and Tauri bindings, built across ~11 parallel agent passes in roughly one calendar day. Aseptic's patterns are the retrospective extraction of what worked and what would have caught the issues that surfaced.

Specific fossic moments that motivated specific patterns:

- **The double-delivery race in Pass 8 Track B** motivated the cursor-ownership invariant pattern in `LIVING_REPORTS.md`. A load-bearing invariant discovered during implementation and not captured anywhere persistent. Future agents would have reintroduced it.

- **The napi-rs Symbol-binding bug in Pass 6** motivated the "fail loudly" principle and the spec-as-aspiration failure mode. Aspirational language ("subscriptions surface as AsyncIterable") produced a plausible-but-wrong implementation that wasn't caught until three passes later.

- **The CCE-dedup test pattern rediscovery in Passes 2-4** motivated the shared-utility pattern in ADRs and the polish-debt cleanup cadence. Three different `unique_ev()` implementations across three independent agents, none knowing about the others.

- **The Pass 11 path-dep fix** motivated the supervisor-between-batches cadence. Latent integration debt that no individual pass could see, surfaced only by a structural review.

- **The Pass 9 / Pass 11 tilde-expansion documentation drift** motivated the cross-pollination doc. Pass 9 implemented binding-handles-tilde; Pass 11 documented spec examples showing consumer-handles-tilde. Not wrong, but inconsistent. Both passes were "correct" individually.

These motivating examples are why Aseptic exists. They should be sidebar case studies in the eventual methodology docs.

---

## Future-state shape (when Aseptic becomes its own project)

Approximate doc structure when it's time to extract:

1. `README.md` — start here, methodology overview
2. `INTRODUCTION.md` — the framing manifesto (this doc, expanded)
3. `ADR_FORMAT.md` — spec format that works for agents
4. `LIVING_REPORTS.md` — TECH_DEBT, POLISH_DEBT, DEVIATION specs
5. `BLAST_RADIUS.md` — per-pass blast radius format
6. `CROSS_POLLINATION.md` — adjacent-project notification format
7. `PASS_REPORTING.md` — internal structured pass report format (feeds 4, 5, 6)
8. `PARALLEL_EXECUTION.md` — orchestration patterns, parallel-safety rules, version conventions
9. `SUPERVISOR_PROTOCOL.md` — what a supervisor pass does, integrity loop spec
10. `AGENT_BRIEFING.md` — canonical system-prompt fragment for participating agents
11. `MCP_SERVER.md` — future-state tool spec

Plus working notes (this file and the artifacts file) as historical context for design choices.

---

## When to bootstrap Aseptic for real

Two conditions:

1. **A second real reference implementation exists.** Fossic alone is not enough data to extract a generalizable methodology. The second project that needs Aseptic — could be a Lattica integration, could be something fresh — provides the comparative basis. Two reference implementations is the minimum to spec honestly.

2. **The current project's work is at a natural pause.** Aseptic-the-project is itself a project. It needs design attention and parallel-agent work of its own. Starting it mid-fossic would split focus poorly.

Until both conditions hold: keep these two notes files, apply Aseptic patterns informally to ongoing work, capture motivating examples as they happen.
