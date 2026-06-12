# ADR-002: Event Fabric via ES Toolkit, Not NATS

**Status:** Accepted
**Date:** 2026-06-11

---

## Context and Problem Statement

Lattica unifies multiple independent modules — LumaWeave, Cerebra, Policy Scout, Rhyzome, bons.ai, Bo — into a single platform runtime. For the "Reflective Twin Architecture" to function, modules must be able to observe each other's activity in real time. Graph A (canonical snapshot) and Graph B (live state) must be kept synchronized through a diff layer that carries semantic meaning: not "line 42 changed" but "Agent investigating file X," "Policy violation detected on tool call Y," "Repair strategy Z selected and failed."

A conventional pub/sub event bus satisfies the delivery requirement but nothing more. The reflective twin vision requires the diff layer to be a first-class artifact — queryable, replayable, branchable, and persistent. The system must be able to answer questions like: "What was the state of the knowledge graph at 14:32:07 before the repair agent modified it?" and "If we had taken branch B instead of branch A at that decision point, what would the graph look like now?" Delivery alone cannot answer those questions.

---

## Decision Drivers

- **The diff layer needs branchable history.** Counterfactual exploration — "what if the agent had chosen strategy B?" — requires the ability to fork an event stream at any point and project a divergent future. A message broker that delivers and forgets cannot model this.
- **Time-travel viewer requires an immutable log.** The Phase 6 scrubbable timeline UI must be able to reconstruct any past state by replaying events from the beginning of a stream (or from a snapshot). This requires every event to be durably stored and indexed.
- **LLM agent debugging requires deterministic replay.** Tool call inputs and outputs must be stored alongside the events that record them. Rerunning a failed agent trace means feeding it the exact same tool responses — not re-issuing live calls. This requires the event log to be the system of record for tool outputs, not a side channel.
- **Local-first, single machine, no broker daemon if avoidable.** The platform runs on one developer's workstation. Adding a broker process (NATS server, Redis server) adds a startup dependency, a failure mode, and operational overhead that yields no benefit at this scale. A SQLite-backed library embedded in each process is simpler and more reliable.
- **Portfolio signal.** A well-engineered local-first event sourcing library with Rust core, cross-language bindings, branchable history, and a time-travel viewer is a publishable artifact in its own right. The implementation cost is recoverable as a standalone open-source project (lattica-es).
- **Constraint design principle.** The governing philosophy of the platform is to make mistakes structurally impossible, not merely monitored. An append-only immutable log enforces the invariant that history cannot be rewritten. NATS provides no such structural guarantee.

---

## Considered Options

1. **NATS** — lightweight message broker, sub-millisecond pub/sub, Go single binary, JetStream persistence add-on
2. **Redis pub/sub** — widely understood, feature-rich, persistent with AOF/RDB, heavier operational footprint
3. **SQLite append-only polling** — what Cerebra (`inspector_events` table) and Policy Scout (`audit.db`) already do; zero new infrastructure
4. **ES toolkit (lattica-es)** — Rust core with rusqlite + blake3, PyO3 Python bindings, napi-rs TypeScript bindings, SQLite-backed, branchable history, reactive subscriptions, snapshot optimization

---

## Decision Outcome

**Chosen option: ES toolkit (lattica-es)**

NATS delivers events. The ES toolkit delivers events and remembers them, versions them, branches them, and makes them replayable. The reflective twin architecture cannot be built on delivery alone.

More precisely: NATS with JetStream can be configured to persist messages, but persistence in JetStream is a retention policy (time-based or count-based) layered onto a fundamentally delivery-oriented system. It does not provide branchable history, content-addressed event identity, synchronous pure reducers enforced at the type level, or a snapshot model that is structurally optional but always sufficient for correctness. These are not features that can be bolted onto NATS — they require a different foundational model.

The ES toolkit is built on three principles that NATS cannot provide:

1. **Append-only immutability as a structural invariant.** Every event gets a blake3 content-addressed ID derived from `blake3(type + payload + causation_id)`. The ID is a commitment — if you have the ID, you can verify you have the exact event. This is the substrate for deterministic replay.

2. **Branchable streams with shared storage.** The `branches` table records `(id, parent_id, parent_version)`. A branch does not copy events — it begins appending from `parent_version + 1` and the query layer handles fan-out transparently. This makes counterfactual exploration cheap: fork at any version, project a divergent state, compare against the canonical branch.

3. **Reducers are pure and synchronous, enforced at the type level.** No I/O, no promises, no side effects inside a reducer. State at any version is a pure function of events from the beginning of the stream (or from the nearest snapshot). This is what makes time-travel correct rather than merely approximate.

The time-travel viewer (Phase 6, scrubbable HTML canvas UI embedded in LumaWeave) depends on all three of these properties. An agent trace is replayed by feeding the stored tool outputs from the event log back to the reducer — no live calls are re-issued, no network state is needed. This is deterministic replay in the strict sense.

---

### Positive Consequences

- **Branchable history for counterfactual exploration.** Fork any stream at any version and project a divergent future without copying event data.
- **Immutable audit trail per module.** Every agent action, policy decision, memory retrieval, and repair attempt is permanently recorded with a content-addressed ID. History cannot be rewritten.
- **Time-travel viewer.** The Phase 6 scrubbable timeline UI can reconstruct any past state with guaranteed correctness because reducers are pure and every event is stored.
- **Deterministic agent trace replay.** Failed or anomalous agent runs can be replayed exactly by feeding stored tool outputs back through the same reducer. No live re-execution needed.
- **No broker daemon.** Each module embeds the ES toolkit and writes to its own SQLite file. No server process to start, no port to allocate, no failure mode from a crashed broker.
- **Single SQLite file per module.** Operationally simple: the event log for any module is one file on disk, readable with standard SQLite tooling, copyable, backupable, inspectable.
- **Cross-language without protocol overhead.** Rust core is the single canonical implementation. PyO3 gives Python modules (Cerebra, Policy Scout, Rhyzome, bons.ai, Bo) direct bindings with no serialization round-trip. napi-rs gives the TypeScript frontend and Node tooling direct bindings. The Tauri backend uses the crate directly. Every language sees the same types.
- **Portfolio artifact.** lattica-es is a self-contained publishable library. The implementation cost is not sunk — it is recoverable as open-source.
- **Constraint design alignment.** Append-only immutability enforces the invariant structurally. The diff layer cannot silently drop or rewrite events. This is a structural guarantee, not a monitoring policy.

---

### Negative Consequences / Risks

- **ES toolkit does not exist yet.** lattica-es must be designed and built in Phase 6. NATS can be installed in minutes. The implementation cost is real and non-trivial: Rust core with rusqlite, blake3, and reactive subscriptions; PyO3 bindings with lifetime management; napi-rs bindings with async bridge; comprehensive test coverage for the branchable history model.
- **Phase 6 is a prerequisite for the full diff layer.** Phases 1–5 work without it (they use polling and existing event stores). But the cross-module event timeline, the time-travel viewer, and deterministic agent replay all block on Phase 6. If ES toolkit development runs long, Phase 7+ may slip.
- **Higher implementation complexity than pub/sub.** Event sourcing requires discipline from every module author: events must be designed as facts (past tense, immutable), reducers must be kept pure, command/event separation must be maintained. These invariants are easy to violate and hard to recover from once a stream is populated.
- **SQLite contention under high write volume.** SQLite's WAL mode handles concurrent reads well but serializes writes. At the throughput levels of a single developer's workstation this is not a concern, but it would be a ceiling if the platform ever scaled to multi-user or high-frequency event emission.
- **No built-in network transport.** NATS is built for distribution. The ES toolkit is local-only in v1. If the platform ever needs to stream events to a remote observer (a cloud dashboard, a second device), a transport layer would need to be added. The CRDT sync via Loro (Phase 2 add-on) addresses the multi-device case but is explicitly out of v1 scope.

---

## Pros and Cons of Discarded Options

### NATS

**What it does well.** NATS is genuinely excellent at what it was designed for: sub-millisecond pub/sub delivery, lightweight single binary (no external dependencies), simple subject-based routing, JetStream for at-least-once delivery with acknowledgment. For a microservices deployment where many processes on many machines need to exchange events with low latency and minimal operational overhead, NATS is a strong default.

**Why it falls short for the reflective twin vision.** NATS delivers events and, by default, forgets them. JetStream adds retention, but retention is a policy — not an immutable log. There is no content-addressed event identity, no branchable stream model, no snapshot mechanism, no reducer contract. "The state of the system at 14:32:07" is not a question JetStream is designed to answer. Reconstructing past state from a JetStream stream would require building an event sourcing layer on top of NATS — at which point NATS is providing pub/sub delivery infrastructure for a system that still needs to be written. The ES toolkit is that system, without the broker dependency.

Additionally, NATS requires a server process. On a single developer's workstation, starting `nats-server` before Lattica launches is pure operational overhead. If the server crashes, all event delivery stops. A SQLite-embedded library has no such failure mode.

### Redis pub/sub

**What it does well.** Redis is widely understood, has excellent client libraries in every language, and offers more features than NATS out of the box: key-value store, sorted sets, streams (XADD/XREAD), persistence via AOF or RDB snapshots. Redis Streams are closer to an event log than NATS subjects — they have consumer groups, message IDs, and a read-from-offset API.

**Why it falls short.** Redis Streams are append-only but not content-addressed, not branchable, and not designed for pure-function state reconstruction. The operational footprint is heavier than NATS (Redis is not a message broker — it is a data structure server being used as one). The local-first constraint makes the broker-daemon model unattractive for the same reasons as NATS. Redis adds memory pressure (all active data in RAM) on a machine that is already running Ollama with a loaded 14b model. And like NATS, using Redis Streams as an event fabric would require building the event sourcing semantics on top — yielding the same conclusion: build the right abstraction directly.

### SQLite append-only polling (existing approach)

**What it does well.** This is already working. Cerebra writes to `inspector_events`. Policy Scout writes to `audit.db`. Both tables are append-only in practice. Any module can read them with standard SQLite queries. No new infrastructure, no new dependencies, no build work required. For the Phase 2 audit JSONL watcher (the first live source adapter in LumaWeave), polling an existing SQLite table is the correct approach — it costs nothing and delivers the feature.

**Why it doesn't scale to the full platform event timeline.** The existing event stores are module-local, schema-local, and semantically heterogeneous. `inspector_events` was designed for Cerebra's internal use; `audit.db` was designed for Policy Scout's audit trail. Neither was designed with a unified event vocabulary, content-addressed IDs, branchable history, or cross-language reducer contracts. Building the reflective twin's diff layer on top of raw polling against two incompatible schemas would require a translation layer that is essentially a worse version of the ES toolkit — without the structural guarantees.

Polling also has inherent latency (polling interval), cannot support reactive subscriptions (a subscriber is notified synchronously on every append), and has no natural model for branching or snapshotting. For the time-travel viewer and deterministic agent replay, polling is the wrong substrate.

**What happens instead.** The existing event stores are not replaced. Cross-language read adapters bridge `inspector_events` and `audit.db` into the unified ES toolkit event vocabulary. Cerebra and Policy Scout continue writing to their own stores; the ES toolkit read adapters surface those events in the unified timeline without migration cost. See "Relationship to Existing Event Stores" below.

---

## Language Architecture

The ES toolkit is implemented as a Rust crate (lattica-es) with cross-language bindings. This is not the only viable architecture — a TypeScript-first implementation on better-sqlite3 would also work — but it is the correct one for this platform for the following reasons.

**Rust core is the single canonical implementation.** The SQLite schema, the blake3 content-addressed ID derivation, the branching model, the snapshot logic, and the reactive subscription machinery are written once in Rust and exposed to all languages through bindings. There is no risk of subtle behavioral divergence between a "Python version" and a "TypeScript version" of the same reducer contract.

**PyO3 for Python modules.** Cerebra, Policy Scout, Rhyzome, bons.ai, and Bo are all Python 3.12+. PyO3 gives them synchronous and async bindings to the Rust core with no serialization overhead for in-process calls. The Python API surface is idiomatic Python — type hints, context managers, async generators for reactive subscriptions — but the implementation runs at Rust speed against the same SQLite file.

**napi-rs for TypeScript.** LumaWeave's frontend and the Lattica UI layer are TypeScript/React. napi-rs compiles the Rust core to a native Node addon. The TypeScript API surface is idiomatic TypeScript — async iterators for subscriptions, branded types for stream IDs and event versions, full type inference on reducers. The time-travel viewer and the cross-module event timeline panel in Lattica consume the ES toolkit through this binding.

**Tauri backend uses the crate directly.** The Tauri Rust backend is in the same workspace as lattica-es. It imports the crate with no FFI boundary, no serialization, no binding overhead. Tauri IPC commands that expose ES toolkit operations to the frontend are thin wrappers over direct crate calls.

This architecture means the lattica-es crate is the performance-critical, correctness-critical core, and all language surfaces are generated or thin. A bug found in the Rust core is fixed in one place and propagates to all language bindings automatically. A performance optimization in the Rust core benefits all consumers. The crate can be extracted and published independently of the Lattica monorepo.

A TypeScript-first implementation on better-sqlite3 would have been simpler to bootstrap but would have required a separate Python implementation (or a subprocess bridge with serialization overhead) for Python modules. Given that Python modules are the primary event emitters in Phases 2–3 (Policy Scout audit events, Cerebra knowledge events), a Python-first or Rust-first approach was necessary. Rust was chosen over Python-first because the crate is the most natural artifact for a publishable standalone library, and because the Tauri backend is already Rust.

---

## Relationship to Existing Event Stores

The ES toolkit does **not** replace Cerebra's `inspector_events` table or Policy Scout's `audit.db`. Both stores were designed for module-local purposes and continue to serve those purposes. Replacing them would require a migration with non-trivial risk and no benefit to the modules themselves — Cerebra and Policy Scout do not need branchable history internally.

What the ES toolkit provides instead is **cross-language read adapters** that surface both stores in the unified event vocabulary. The adapters:

- Map `inspector_events` rows to standard ES toolkit event types (`memory_retrieved`, `context_assembled`, `embedding_computed`, etc.)
- Map `audit.db` rows to standard ES toolkit event types (`policy_decision_made`, `tool_call_gated`, `hitl_gate_triggered`, etc.)
- Assign synthetic content-addressed IDs to historical events (based on the existing row content) so they participate correctly in the unified timeline
- Are read-only — they do not write back to the source stores

From the perspective of the cross-module event timeline in Lattica and the time-travel viewer, all events — whether they originated in lattica-es streams, Cerebra's inspector_events, or Policy Scout's audit.db — appear in a unified vocabulary with consistent types and IDs. The migration cost is zero because there is no migration: the source stores remain authoritative for their own modules, and the adapters provide a read view.

New modules built in Phase 6 and later (Rhyzome event emission, bons.ai three-agent cycle events, Bo conversation events) will write directly to lattica-es streams from the start, using the native PyO3 bindings. They have no legacy event store to bridge.

---

## Notes

- lattica-es is scoped to Phase 6. Phases 1–5 do not depend on it. The platform is not blocked on the ES toolkit until the cross-module event timeline, the time-travel viewer, and the diff layer substrate are required.
- The CRDT sync via Loro (multi-device, collaborative branching) is explicitly out of v1 scope. It is noted as a Phase 2 add-on to the ES toolkit schema design (the `branches` table shape accommodates it) but will not be implemented in Phase 6.
- "Snapshot optimization only, never required for correctness" is a hard invariant of the ES toolkit design. Any state that requires a snapshot to be present is a bug. Snapshots are written every N events by default as a performance optimization; recovery from a database that contains only the raw event log (no snapshots) must always be possible and must always produce the same result.
- The three-table SQLite schema (`events`, `snapshots`, `branches`) is the v1 schema. Schema version is tracked in the database; migrations are handled by the crate at open time. Consumers do not manage schema migrations.
- Agent trace adapter standard event types: `llm_call`, `tool_call`, `tool_result`, `reasoning_step`. OTel GenAI span export format is supported from Phase 6. The adapter design follows the emerging OpenTelemetry GenAI semantic conventions so that traces can be ingested by external observability tooling if desired.