---
title: Aseptic — Conceptual Frame
---

# Aseptic — Why This Exists

## What Aseptic is

Aseptic is a working methodology for multi-agent code execution that borrows its
central metaphor from surgical sterile technique: contamination is prevented at the
work boundary, continuously, through practiced procedure — not detected after the
fact and cleaned up. In software terms, the "sterile field" is the shared understanding
between agents of what the system currently is: its invariants, its debt, its
divergences from spec. The contamination is any work that proceeds from a false model
of that shared state. Aseptic's instruments are the living reports, the blast radius
artifact, and the supervisor pass — tools for keeping the sterile field clean between
agent executions, not for reconstructing it afterward.

---

## The problem it addresses — six failure modes

Parallel agent execution surfaces failure modes that single-agent sequential work
mostly avoids. These are the six that Aseptic is specifically designed to prevent.

**1. Convention drift**
Naming conventions, code organization patterns, and API consistency erode incrementally
across passes. No single change is egregious; the aggregate is a codebase that looks
like three different projects.

*fossic example:* The system stream event types accumulated three inconsistent naming
conventions across passes — `fossic.Purged` (dot-namespaced), `SubscriptionDegraded`
(bare PascalCase), and `ShreddedStreamMarker` (bare PascalCase). Each pass followed
the pattern it found locally; no pass held the global view. TIDYUP survey finding J1.

**2. Spec-as-aspiration**
Specification documents drift from implementation as implementation decisions are made
under constraint. The spec remains as written; the code diverges; future agents use
the spec as their implementation guide and reproduce the wrong behavior.

*fossic example:* FOSSIC_V1_SPEC.md §14 described a Tokio-based threading model
including `OpenOptions::tokio_handle`. The core never used Tokio — it used `std::thread`
and `crossbeam-channel` throughout. Any agent implementing a Tauri integration by
following the spec would write code that doesn't compile. TIDYUP survey finding Issue 2;
resolved in Pass 11.

**3. Latent integration bugs**
A change to one component's API or behavior is implemented correctly in isolation but
breaks consumers in adjacent components. The break is latent because the adjacent
component's tests run in a separate context against a different version of the dependency.

*fossic example:* The `register_payload_transform` callable receives `(event_type, payload)`
— the first argument is the event type, not the stream ID. Tests in `test_transforms.py`
named the parameter `stream_id` and asserted stream ID values. The tests passed individually
but would have silently broken any consumer that relied on the actual argument identity.
Fixed in Pass 8.6.

**4. Partial invariant implementation**
An invariant is recognized and implemented in one binding but not propagated to others.
The system's correctness guarantee is only as strong as its weakest consumer.

*fossic example:* The Python binding has a full typed exception hierarchy (`StreamNotDeclaredError`,
`BranchNotFoundError`, etc.). The Node binding throws `GenericFailure` for everything.
TypeScript consumers cannot distinguish error types without string-parsing error messages.
TIDYUP survey finding A1.

**5. ADR drift**
Architectural decisions are recorded but their constraints and invariants aren't expressed
in a form that agents can verify. The ADR says "do X"; subsequent passes do Y; the ADR
is never updated because no agent is responsible for comparing implementation against it.

*fossic example:* fossic's specification guarantees `SimilaritySearchProvider` as a declared
extension point. The trait was never implemented. The feature flag exists in `Cargo.toml`
(`similarity = []`) but enables nothing. An agent enabling `--features similarity` gets
silence, not the expected stub. TIDYUP survey finding D1.

**6. Silent shortcuts**
An agent working under time or scope pressure takes an implementation shortcut without
recording it. The shortcut is functionally correct in the short term. It becomes a trap
when a future agent assumes the principled implementation exists and builds on top of it.

*fossic example:* `read_state` in the Python binding was implemented as a pure-Python
full-event replay (no snapshot caching) pending the DynReducer trait being made public
in the Rust core. A comment in `__init__.py` acknowledged this but it was easy to miss.
The benchmark (Pass 10 prereq) revealed the scaling cliff: p99 = 46.6ms on a 1000-event
stream with no snapshot. Addressed in Pass 10.

---

## Core conviction

**Constraints survive parallel execution. Intentions don't.**

An intention is: "we're trying to keep the error hierarchy consistent across bindings."
A constraint is: `register_reducer` must accept an object with `name: str`, `version: int`,
`state_schema_version: int`, `initial_state()`, and `apply(state, payload)` — validated
at registration time, hard error if any attribute is missing. The intention lives in a
conversation. The constraint lives in the code. A new agent inherits the code; it may
not have access to the conversation.

Aseptic works by converting intentions into constraints wherever possible, and by keeping
an explicit record of the intentions that couldn't be converted — the living reports.

---

## The four moves that make the system work

**1. Specific, enforceable specifications**
Spec sections that say "the system should handle errors gracefully" produce inconsistent
implementations. Sections that say "any error that is not a `FossicError` subclass must
not propagate to the caller; wrap in `StorageError` at the binding boundary" produce
consistent ones. The spec is only as useful as its testability.

**2. Continuous instruments, not retrospective surveys**
The FOSSIC_TIDYUP_SURVEY.md represents 31 findings accumulated over 11 passes. Every
one of those findings was visible at the pass that created it. The problem is that no
instrument captured it at the time. Living reports replace retrospective surveys by
making the act of noticing a finding part of every pass's completion criteria — not
a separate supervisor effort after damage has accumulated.

**3. Supervisor passes between batches**
A supervisor pass is a dedicated read-only review that runs between parallel agent batches.
It reads the four living reports plus the blast-radius artifacts, cross-checks against git
history, and classifies findings that per-pass agents missed. It halts before executing
any fix — the output is a structured report for human review. Supervisor passes are
themselves agent executions that follow Aseptic discipline.

**4. Fail-loudly defaults**
When implementation hits ambiguity, the right default is a loud error, not a silent fallback.
`purge_event` requires a specific confirmation string; omitting it raises `PurgeConfirmationError`
rather than silently succeeding. `declare_stream` is required before `append`; violating it raises
immediately. The pattern: surfaces missed invocations as failures rather than as hard-to-diagnose
data anomalies downstream.

---

## What Aseptic is explicitly NOT

- **Not process-heavy.** The instruments are three markdown files and a commit artifact.
  They require five minutes of attention at pass completion, not a separate planning session.

- **Not a replacement for spec discipline.** Living reports capture what the spec misses.
  They don't substitute for writing a good spec. A living report entry that says "the spec
  is silent on X" is a prompt to update the spec, not a permanent workaround.

- **Not automation.** Aseptic has no CI hooks, no enforcement scripts, no linters. It is
  a practice discipline. The instruments work because agents that read them find them useful
  — not because a system enforces compliance.

- **Not universally applicable.** Solo sequential work doesn't benefit much from Aseptic.
  The methodology exists for multi-agent parallel execution where shared-state coherence
  is the binding constraint. Single agents working in isolation rarely produce the failure
  modes Aseptic prevents.
