# Fossic — Development History

*This document traces how fossic got to its current state. It is organized by
inflection points — the moments where direction changed — rather than strictly
by date. Every claim cites a source artifact in the archive. Read alongside
`docs/adr/` for the original decision rationale.*

---

## 1. The Platform Decision That Made fossic Necessary

Fossic was not conceived as a library. It was conceived as infrastructure.

In mid-2026, development was underway on Lattica — a platform intended to unify
several independent modules (LumaWeave, Cerebra, Policy Scout, ai-stack, fossic)
into a "Reflective Twin Architecture": two graphs of system state kept
synchronized through a semantically meaningful diff layer. The diff layer would
carry not just file changes but agent activity: `Agent investigating file X`,
`Policy violation detected on tool call Y`, `Inference session Z started with
model M`. For this to work, the event fabric could not merely deliver messages —
it had to remember them, version them, branch them, and replay them
deterministically.

NATS was the obvious candidate and was rejected. The full analysis is in
[ADR-002](adr/ADR-002-es-toolkit-over-nats.md). The short version: NATS
delivers and forgets. JetStream adds retention, but retention is a policy layered
onto a delivery system — it provides no content-addressed event identity, no
branchable stream model, no snapshot mechanism, no reducer contract. The
reflective twin requires the ability to answer "what was the state of the
knowledge graph at 14:32:07?" and "if the agent had taken strategy B instead of
A at that decision point, what would the graph look like now?" NATS cannot answer
either question. Building event-sourcing semantics on top of NATS would mean
writing the library anyway.

The architectural decision was to build it directly: a Rust core backed by a
single SQLite file in WAL mode, with PyO3 bindings for the Python modules
(Cerebra, Policy Scout, ai-stack) and napi-rs bindings for the TypeScript
frontend (LumaWeave). No daemon, no port, no separate server process. The Tauri
backend would import the crate directly. At this stage the library was called
`lattica-es` and scoped to Phase 6 of the Lattica plan, with Phases 1–5 expected
to proceed without it. It became apparent that this was useful enough to build
immediately.

*Sources: [ADR-002](adr/ADR-002-es-toolkit-over-nats.md),
[archive/…/docs/DESIGN.md](../archive/2026-06-21-pre-cleanup/docs/DESIGN.md),
[archive/…/docs/PHASES.md](../archive/2026-06-21-pre-cleanup/docs/PHASES.md)*

---

## 2. The Day-One Sprint: v0.1 through v0.9

Everything from pass 1 to pass 9 happened on **2026-06-12** — nine foundational
passes in a single day, each building on the last.

**v0.1.0** established the core: the `events`, `streams`, and `meta` SQLite
tables; `append` with CCE hash deduplication; `read_range`; typed errors. The CCE
BLAKE3 hash test vectors were committed alongside the implementation — 12
deterministic fixtures that any language binding must reproduce exactly.

**v0.2.0** added branches (a `branches` table with `parent_id` and
`parent_version`, no event copying), snapshots (stored in the same SQLite file,
used as starting points for `read_state`), and glob-pattern reducer registration.
The branch model's key property — that branching is cheap because branches share
event storage — was established here.

**v0.3.0** added subscriptions: `Synchronous` mode (callback fires while the
write lock is held) and `PostCommit` mode (callback fires on a dedicated thread
after commit, through a bounded channel). The WAL watcher was introduced — a
background thread using the `notify` crate to detect new WAL frames and deliver
events to cross-process subscribers. The threading model was `std::thread` and
`crossbeam-channel` throughout; no async runtime.

**v0.4.0** added cross-stream queries, upcasters (a chain of migration functions
per `(event_type, from_version)`), payload transforms applied at append time, and
the first cursor model for pagination.

**v0.5.0–0.9.0** added the language binding surfaces: PyO3 Python bindings
(v0.5.0), the Tauri IPC companion crate (v0.6.0), CI pipeline and wheel builder
(v0.7.0), napi-rs Node.js bindings with TypeScript types (v0.8.0), and glob
subscription improvements with tilde path expansion (v0.9.0).

By the end of 2026-06-12 the library had a full four-language surface, a CI
pipeline, and test coverage across all major subsystems.

*Sources: blast-radius artifacts
[pass-01](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-01.md)
through
[pass-09](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-09.md)*

---

## 3. The Spec Divergence Crisis

A tidy-up survey conducted on **2026-06-12** found that the spec had diverged
significantly from the code in several places.

The most serious divergence: the spec's threading model section (§14) described
fossic as using Tokio internally — a Tokio dispatcher thread, a Tokio
file-watcher task, even an `OpenOptions::tokio_handle` field for callers to pass
in a Tokio runtime handle. None of this was real. The core had always used
`std::thread` and `crossbeam-channel`. Any Tauri consumer who followed the spec's
guidance to configure `tokio_handle` would have gotten a compile error.

A second gap: the Python binding had no snapshot caching. Every `read_state` call
did a full replay from version 0 — the spec implied snapshot-based reads, but the
implementation was a pure-Python fold loop. The Node.js binding had no reducer or
`read_state` surface at all. And `SimilaritySearchProvider`, which the spec had
described as an extension point, did not exist in the code.

The v0.10.x pass cluster addressed each of these in sequence: the `DynReducer`
trait was added to expose snapshot primitives for cross-language reducers; Python
snapshot caching was wired through PyO3; the threading model section was rewritten
to describe the actual `std::thread` + `crossbeam-channel` model;
`SimilaritySearchProvider` was declared as a stub trait. The root cause: the spec
was written before the code, and the implementation outpaced the spec update loop
during the sprint. The aseptic methodology (blast-radius artifacts, living
reports, source-verified documentation) was introduced precisely to prevent this
pattern from recurring.

*Sources:
[archive/…/docs/FOSSIC_TIDYUP_SURVEY.md](../archive/2026-06-21-pre-cleanup/docs/FOSSIC_TIDYUP_SURVEY.md),
blast-radius
[pass-10](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-10.md),
[pass-11](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-11.md)*

---

## 4. The v1.0-rc.1 Prep Window and Aseptic Bootstrap

The passes from `v0.10.0n` through `v1.0.0z` (2026-06-13 through 2026-06-16)
covered several separate concerns running in parallel.

**The aseptic methodology.** Pass `v0.10.x` introduced the aseptic working
framework: every pass now produces a blast-radius artifact (scope manifest),
living reports (tech debt, polish debt) are continuously maintained, and
cross-project relay documents track inter-module interface agreements. Retroactive
blast-radius files were written for all passes 1–9.

**The agent trace vocabulary.** `AGENT_TRACE_VOCABULARY.md` was introduced at
`v1.0.0n` and extended through several subsequent passes. It defines the standard
event types fossic ships for recording LLM tool calls, reasoning steps, and
Cerebra cognitive events — the vocabulary that makes fossic useful as an agent
observability substrate, not just a generic event log.

**The connection pool.** Pass `v1.0.0w` (2026-06-16) introduced a
`crossbeam-channel`-based read connection pool. Prior to this, concurrent reads
were serialized on the single write connection during read-lock windows — they did
not block writes, but they blocked each other. The pool (configurable size,
defaults to 4) eliminated this: concurrent readers each hold their own connection,
the write path is never contended by read traffic. Pass `v1.0.0z` added a
`read_pool_timeout_ms` option and a `PoolExhausted` error for callers that need to
detect contention.

*Sources:
[archive/…/pass-1.0.0n.md](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.0.0n.md),
[archive/…/pass-1.0.0w.md](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.0.0w.md),
[archive/…/pass-1.0.0z.md](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.0.0z.md)*

---

## 5. The Bounded Resource API: v1.1.x

The v1.1.x sprint (2026-06-20 through 2026-06-21, nine passes) was the largest
architectural expansion since the day-one sprint. It was driven by Lattica
integration requirements: as Cerebra and ai-stack were wired against fossic at
scale, the unbounded read APIs became a concrete OOM risk rather than a
theoretical one.

**v1.1.0** laid the foundation: the `ReadOutcome<T>` enum (`Complete(T)` vs
`Truncated { data, cursor, reason }`), `TruncationCursor` (opaque bytes, not
interpretable by callers), and dispatch-channel observability
(`dispatch_channel_pressure`, `dispatch_channel_high_water_mark`).

**v1.1.1** introduced `SystemStreamWriter` — a dedicated system-stream connection
separate from the write connection, establishing the pattern for substrate-internal
event emission. Events written to `_fossic/system` needed a path that could not
deadlock against application writes.

**v1.1.2–1.1.4** added the three bounded APIs: `read_range_bounded`,
`walk_causation_bounded` (with three sampling modes: `Exhaustive`,
`BreadthFirst { max_per_level }`, and `Adaptive { target_count }`), and
`aggregate_bounded`. Streaming iterators (`RangeIter`, `CorrelationIter`,
`CausationIter`) came in v1.1.5 — these release the read pool connection between
yields, so a pool of size 1 can serve concurrent readers while an iterator is
live.

**v1.1.6–1.1.8** propagated the bounded API surface to all three language
bindings. v1.1.9 was a documentation pass — all binding READMEs, the root README,
and the spec were updated to describe the new APIs, the cursor model, and the
`SamplingMode` table.

*Sources: blast-radius
[pass-1.1.0](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.1.0.md)
through
[pass-1.1.9](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.1.9.md)*

---

## 6. Snapshot Lifecycle Completion: v1.2.x and v1.3.x

The v1.2.x passes (2026-06-21) completed the snapshot lifecycle, which had been
partially implemented since v0.2.0 but lacked automatic policy and garbage
collection.

**v1.2.0** added `SnapshotPolicy` (an enum: `Manual`, `EveryNEvents`,
`StateAdaptive`) and wired auto-snapshot into `ReducerRegistry`. `EveryNEvents(n)`
causes the store to write a snapshot after every N appends on a stream,
eliminating the need for callers to manage snapshot cadence manually. **v1.2.1**
added `ReducerStateLarge` emission — the store writes a `_fossic/system` event
when a snapshot exceeds a configured byte threshold, enabling consumers to detect
runaway state growth. **v1.2.2** added `auto_gc_orphans` — a flag that GC's
orphaned snapshot rows at drop time.

The v1.3.x passes addressed integration-level issues: Python binding read-state
consistency, Node.js snapshot API surface.

*Sources: blast-radius
[pass-1.2.0](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.2.0.md)
through
[pass-1.3.1](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.3.1.md)*

---

## 7. Similarity Search and Substrate Visibility: v1.7.x

The v1.7.x passes (2026-06-21) opened two previously private substrate surfaces
and delivered the first concrete `SimilaritySearchProvider` implementation.

**v1.7.0** added the `fossic-similarity-hnsw` crate scaffold and made three
substrate types publicly accessible that had been `pub(crate)`:
`SystemStreamWriter`, `BackgroundExecutor`, and `TaskKind::Custom`. These are
intended for sibling crate authors who need to write system events or schedule
background tasks — the extension surface documented in
`docs/deep-dives/extension-patterns.md`.

**v1.7.1** delivered the full HNSW implementation: `HnswProvider` with
stream-pattern filtering, persistence to a sidecar file alongside the store, and
integration tests at Cerebra-scale vector counts (50k–500k). **v1.7.2–1.7.4**
added the Python binding, complete API documentation, and workspace wiring.

*Sources: blast-radius
[pass-1.7.0](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.7.0.md)
through
[pass-1.7.4](../archive/2026-06-21-pre-cleanup/docs/aseptic/blast-radius/pass-1.7.4.md)*

---

## 8. SR-10 and Panic Isolation: v1.8.1

The SR-10 reconnaissance report (2026-06-20) was the most systematic failure-mode
analysis the project had received. Seventeen findings were documented across two
parts: confirmed runtime behaviors (Part A) and design questions that had not been
explicitly resolved (Part B). The findings included disk-full error propagation,
WAL `synchronous=NORMAL` crash-window semantics, the absence of an advisory lock
preventing two-process open, causation cycle infeasibility (BLAKE3 makes strict
cycles structurally impossible — a finding that retroactively validated the
implementation's defense-in-depth), and thiserror dual-version risk in the
dependency tree.

Three Part A findings were actionable immediately and became v1.8.1:

- **A-6** (executor): `TaskKind::Custom` panics were not caught, permanently
  killing the background executor thread. `catch_unwind` + `AssertUnwindSafe`
  wrapping was added.
- **A-11** (reducers): reducer `apply_bytes` panics propagated through application
  threads. An `apply_reducer_guarded` helper was added wrapping every call site
  (7 in total), returning `Error::ReducerPanicked { stream_id, reducer_name,
  event_id_hex, panic_message }` instead of unwinding.
- **A-5** (subscriptions): synchronous subscriber panics were already caught and
  marked degraded, but the `SubscriptionDegraded` system event was not emitted for
  sync subscribers — only for PostCommit queue overflow. A `sync_degraded_tx`
  channel was added; the dispatcher drains it after each PostCommit fan-out to
  emit the system events after the write lock is released.

Fourteen SR-10 findings (A-1 through A-4, A-7 through A-10, A-12 through A-17)
remain documented in `docs/deep-dives/failure-modes.md` with no explicit triage
decisions. Six Part B design questions are also open. This is the project's
primary open-item register as of v1.8.1.

*Sources:
[docs/deep-dives/failure-modes.md](deep-dives/failure-modes.md),
[archive/…/fossic-recon-and-arch-opinion-2026-06-20.md](../archive/2026-06-21-pre-cleanup/docs/fossic-recon-and-arch-opinion-2026-06-20.md),
CHANGELOG.md v1.8.1 entry*

---

## Open Gaps (not fabricated)

The source docs identify the following gaps that are tracked but not yet resolved:

- The PyO3 bridge adds ~47μs per event replayed, making `read_state` slow for
  high-event-count streams despite snapshot caching (addressed partially by
  aggressive snapshot cadence; full resolution deferred).
- Aggregate `read_range_bounded` truncation does not produce a cursor — fold-resume
  requires injecting partial aggregator state, which the `Aggregate` trait does
  not yet support. Scoped to v1.2.x work.
- Pre-built wheel distribution was planned but has not shipped as of v1.8.1.
  Consumers still require Rust installed locally.
- SR-10 findings A-1 through A-4, A-7 through A-10, A-12 through A-17 are
  documented observations with no explicit deferred, accepted, or wontfix
  decisions recorded. Triage is open.
