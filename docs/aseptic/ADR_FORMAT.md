---
title: ADR Format — Agent-Friendly Template
---

# ADR Format — Agent-Friendly Template

## Namespace

Fossic-specific ADRs use the **ADR-F-NNN** prefix (e.g., ADR-F-001, ADR-F-002). This
namespace is distinct from the platform ADRs in `docs/adr/` (ADR-001 through ADR-008),
which document Lattica's cross-project architectural decisions.

The split exists because fossic is a publishable library; its architectural decisions
should live in fossic's own decision stream, not be conflated with consumer-project
decisions. Platform ADRs describe how Lattica uses fossic; fossic ADRs describe how
fossic is built internally.

Fossic-specific ADRs live in `docs/aseptic/adr/` (create the directory when the first
ADR is authored). Platform ADRs continue to live in `docs/adr/`. Use zero-padded
three-digit numbering (ADR-F-001, not ADR-F-1) to allow up to 999 fossic ADRs.

---

The format that survives parallel execution. Conventional ADRs are written for humans:
narrative context, trade-off analysis, a decision sentence. Aseptic ADRs layer two sections
on top: **agent-facing** (strictly enforceable) and **human-facing** (conversational).

The agent-facing sections contain only specific, testable statements. An agent reading
`Constraints` should be able to write a test that verifies each statement. An agent
reading `Boundaries` should know exactly which files it may and may not modify. An agent
reading `Failure-mode preference` should know what to do when implementation hits ambiguity.

The human-facing sections (`Context`, `Consequences`) are for understanding and rationale.
They are not action-items; they are not instructions. Agents may read them for background.
They must not modify decisions based on human-facing sections alone.

---

## Template

```markdown
# ADR-N: Title

**Status:** Draft | Accepted | Superseded by ADR-M
**Date:** YYYY-MM-DD
**Version:** vX.Y.Z (the version at which this decision was accepted)

---

## Decision

One sentence. What was decided, stated affirmatively.

## Constraints (enforceable)

Specific, testable statements. Each constraint should be expressible as a property
test or a type invariant. Vague constraints ("should be robust") are not constraints.

- `[constraint statement]` — testability note (e.g., "verified by test X in tests/Y.rs")
- `[constraint statement]` — testability note

## Boundaries (parallel-execution-safe)

Files this decision permits modification of:
- `[file or glob pattern]` — permitted scope

Files this decision PROHIBITS modification of (without revisiting this ADR):
- `[file or glob pattern]` — rationale in one clause

Other ADRs this decision depends on:
- ADR-N (vX.Y.Z) — how this decision relies on it

## Invariants (testable)

Property tests or assertions that must hold. Link to the test file if the test exists.

- **[invariant name]:** `[testable statement]` → `[test reference or "test needed"]`

## Failure-mode preference

When implementation hits ambiguity, prefer: **[loud failure | explicit refusal |
well-defined fallback]**

Justification: one sentence.

---

## Context (for humans)

Conversational rationale for why this decision was made. Trade-offs considered.
What would happen if a different choice were made.

## Consequences

What downstream work this decision enables. What it constrains. What debt it creates.
```

---

## Example ADR — adapted from fossic invariant §16.4

```markdown
# ADR-F-001: Snapshots Are Never Authoritative

**Status:** Accepted
**Date:** 2026-05-01
**Version:** v0.5.0

---

## Decision

Snapshots in fossic are a read-path optimization only; the event log is always
the source of truth, and snapshots may be discarded without consequence beyond
replay cost.

## Constraints (enforceable)

- No API surface may make a `read_state` call that can return a different result
  depending on whether a snapshot exists for the stream. — verified by invariant
  test in `fossic/tests/snapshots.rs` (see Invariants below; test needed).
- `take_snapshot` must never write to the `events` table. — structural (no such
  code path exists in `snapshots.rs`).
- `gc_orphaned_snapshots` must never modify the `events` table. — structural.

## Boundaries (parallel-execution-safe)

Files this decision permits modification of:
- `fossic/src/snapshots.rs` — snapshot write, read, gc logic
- `fossic/src/reducers.rs` — reducer registration and state fold
- `fossic/tests/snapshots.rs` — snapshot behavior tests

Files this decision PROHIBITS modification of (without revisiting this ADR):
- `fossic/src/store.rs` (append path) — the append path must not consult snapshots;
  doing so would couple write correctness to snapshot state

Other ADRs this decision depends on:
- ADR-F-002 (v0.3.0) — CCE identity: snapshot correctness depends on event identity
  being content-addressed (a snapshot at version N reflects the exact CCE-addressed
  events 1…N, not a mutable sequence)

## Invariants (testable)

- **Snapshot equivalence:** `read_state(stream, no_snapshot) == read_state(stream, with_snapshot)`
  for any event sequence → `fossic/tests/snapshots.rs:test_snapshot_equivalence` (needed)
- **Snapshot GC safety:** after `gc_orphaned_snapshots()`, `read_state` returns the same
  value as before → `fossic/tests/snapshots.rs:test_gc_does_not_affect_state` (needed)

## Failure-mode preference

When implementation hits ambiguity, prefer: **loud failure**

Justification: a silent correctness divergence between snapshot-path and no-snapshot-path
is far harder to debug than an explicit error at the point of inconsistency.

---

## Context (for humans)

The decision was driven by Cerebra's use case: thousands of lattice-node streams, each
with potentially thousands of events. Snapshot caching is required for read performance
at that scale. But making snapshots authoritative would mean that snapshot corruption
(SQLite file partial write, concurrent writer bug) could produce incorrect aggregate state
without any error signal. The append-only event log is guaranteed correct by the CCE
identity property; snapshots are derived from it. Keeping the log as the sole source of
truth means any corruption in the snapshot layer is detectable (by replaying from the log
and comparing) and recoverable (by deleting snapshots and replaying).

## Consequences

Enables: snapshot caching as a pure optimization; `gc_orphaned_snapshots` as a safe
housekeeping operation; snapshot deletion for debugging without data loss.

Constrains: snapshots cannot carry state that cannot be derived from events (no out-of-band
metadata in snapshot blobs). Cross-language snapshot portability is explicitly out of scope
for v1 because the state format is language-bound (msgpack encoding produced by the
language-specific reducer).

Creates debt: the Python DynReducer bridge currently replays all events (no snapshot caching
via the Rust path) pending a public DynReducer trait. This is tracked as TD-001.
```
