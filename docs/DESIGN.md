# Lattica — Platform Design Document

**Status:** Living document — authoritative platform reference
**Last updated:** 2026-06-11
**Governed by:** ADR-001 through ADR-008

---

## 1. Vision

### The Reflective Twin Architecture

Software development environments have always been passive. You write code; the IDE reports errors. You push a commit; CI reports failures. You deploy; monitoring reports anomalies. Every tool in the chain is a one-directional reporter. The developer is the only entity that maintains a mental model of the whole system across time.

Lattica inverts this. The platform models itself continuously — not as documentation, not as a static diagram, not as a README that drifts from reality within weeks, but as a live graph that captures what the system believes about itself at every moment.

This is the **Reflective Twin Architecture**: two graphs, one diff layer.

**Graph A — Canonical Snapshot**

The last known coherent state. Stable, versioned, reproducible. This graph says: here is what we know to be true, what has been validated, what the evaluation baselines confirm. It represents the epistemic floor — the minimum the system will always be able to recover to. Graph A is never "current"; it is "confirmed."

**Graph B — Live State**

Constantly evolving. Current agent activity, current execution state, current Cerebra observations, current Policy Scout findings. This graph has noise. It has uncertainty. It has contradictions that have not yet been resolved. It is the frontier. Graph B is always "current" and never "confirmed."

**The Diff Layer**

The diff layer is not a line diff. It is not a git delta. It is a stream of semantic events:

- `Agent investigating` — a file is under active examination
- `Agent proposes modification` — a repair strategy has been selected
- `Policy violation detected` — governance has flagged an action
- `Repair pending` — a modification is queued, awaiting gate
- `Consensus reached` — Graph A and Graph B agree on a subgraph

These are changes with meaning. The diff layer is implemented by the ES toolkit (lattica-es), which provides not just delivery but immutable log, branchable history, and time-travel replay. This is why NATS was rejected (ADR-002): NATS delivers and forgets. The reflective twin requires memory of every state transition.

LumaWeave renders both graphs simultaneously. The visual layer is the interface to the twin.

### The Organs of an Organism

Each module in the portfolio existed independently. Cerebra was a knowledge management experiment. Policy Scout was a governance daemon. Rhyzome was a code repair agent. bons.ai was a multi-agent cognitive experiment. They were built at different times, in different languages, with different internal conventions, for different immediate purposes.

The novel insight is this: **these are not separate projects. They are organs.**

An organism does not coordinate its liver and lungs through external APIs. They share a circulatory system, a nervous system, a hormonal signaling fabric. Information flows between them in a common medium. The heart does not poll the lungs for oxygen status on a timer; it receives continuous feedback through shared physiology.

Lattica is the body. The modules are the organs. The event fabric (ES toolkit) is the circulatory system. The memory layer (Cerebra) is the hippocampus. The governance daemon (Policy Scout) is the immune system — it does not evaluate content, it detects structural violations. The inference layer (ai-stack) is metabolic energy. LumaWeave is the sensory cortex — the part that makes everything visible.

What emerges when organs communicate through a common runtime, memory model, event fabric, and visual language is something that no individual module could produce: **a system that can observe its own cognition**.

### The Software Cognition Platform

Long-term, Lattica is a platform where agents are visually active in the graph. When Claude Code is investigating a bug, you see which files are highlighted. When Rhyzome is attempting a repair, you see the strategy nodes light up and the policy gate edges appear. When Cerebra retrieves a memory to answer a query, the retrieval trace appears as a subgraph overlay, showing exactly which knowledge fragments contributed to the answer and with what confidence scores.

This is not a monitoring dashboard. Dashboards are static summaries of recorded state. This is a live visual language for software cognition — you are watching the system think.

The custom-definition layer is what makes it extensible: users can define their own color coding and reactive animated elements. The registry architecture (already present in LumaWeave) makes this possible without modifying core. The node program registry, the gwells physics dialect registry, the theme target registry — all of these are extensibility surfaces that were built before the vision was fully articulated. They were the right instinct.

### What Makes This Novel

It is not the individual modules. Vector memory stores exist. Code repair agents exist. Governance daemons exist. Policy-as-code frameworks exist. Graph visualization tools exist. Event sourcing libraries exist.

The novel thing is the **integration architecture**:

1. A single visual language (LumaWeave) that renders any module's state as a typed node/edge network
2. An event fabric (ES toolkit) that gives every cross-module state change immutable history and semantic type
3. A memory layer (Cerebra) that is not a log but a knowledge graph — with retrieval quality CI-gated against regression baselines
4. A governance layer (Policy Scout) that gates every agent's filesystem side effect, structurally, before it happens
5. A physics engine (gwells) where graph layout is not cosmetic but epistemically meaningful — repulsion forces encode risk scores, cluster seeds encode knowledge domain

No individual module is the invention. The invention is the combination, and the architectural discipline that makes the combination coherent.

---

## 2. Governing Philosophy

### Constraint Design

The developer's background is warehousing logistics. In a warehouse, a misplaced pallet is not a software bug — it is a physical object in the wrong location that causes cascading operational failures. The instinct developed in that environment is: **do not build systems that detect mistakes after the fact. Build systems where the mistake is structurally impossible.**

A pallet that cannot fit in the wrong location cannot be misplaced there. A shelf labeled only for a specific SKU category enforces correct placement without monitoring. The system's geometry is the policy.

This principle — **constraint design** — governs every architectural decision in Lattica.

### Structural Enforcement vs. Monitoring

Every invariant in the system falls into one of two categories:

**Structurally enforced** — the architecture makes violation impossible at compile time, schema time, or type-system time. The constraint is geometry. No monitoring required.

**Monitored** — the architecture makes violation detectable at runtime. This is the fallback when structural enforcement is too expensive or impossible. Monitoring is always a weaker guarantee than structural enforcement.

The design discipline is: **push invariants from monitored to structurally enforced at every opportunity.** Accept monitored invariants only when structural enforcement has a prohibitive cost.

### Current Codebase Examples

**Policy Scout tighten-only YAML overrides**

Policy Scout maintains YAML override registries. The schema enforces that overrides can only reduce permitted scope — you can narrow what an agent is allowed to do, never expand it beyond the baseline. The word "tighten-only" is not a convention or a guideline documented somewhere. It is a validation step that rejects any override file that would expand permissions. Attempting to write a permissive override is not a policy violation you might catch in review — it is a schema error that fails immediately.

**Registry-driven architecture with zero-core-change extensibility**

LumaWeave has 14+ Map/array registries with `register()` + `subscribe()` methods. Adding a new source adapter, a new physics dialect, a new tile section, a new command — none of these require modifying core files. The registration surface is the extension mechanism. This makes it structurally impossible to add a new module type without going through the registry. If you bypass the registry, the module does not exist to the system. The registry is not a convention; it is a hard dependency path.

**eval-core regression baselines**

eval-core CI gates ensure that retrieval quality scores cannot regress below established baselines. This is not a "please don't break the evals" note in a README. It is a CI failure that blocks merge. The baseline is not a soft target; it is a hard floor enforced by the pipeline.

**ES toolkit pure+synchronous reducers**

Reducers in the ES toolkit are typed to be pure and synchronous at the type level. The type system rejects an async reducer. You cannot write a reducer that performs I/O and have it compile. This is the strongest form of constraint design — the invariant exists before runtime.

**Forward-only database migrations**

LumaWeave's Zustand store has 90+ versioned migrations. They go in one direction. There is no rollback path, by design. An old migration is never modified. This makes the schema history an append-only log — the same principle that governs the ES toolkit's event store.

### Applying Constraint Design Going Forward

When evaluating any architectural decision, the first question is: **what invariants does this design need to maintain, and how is each one enforced?**

For each invariant, classify it:
- Can the type system enforce this? If yes, enforce it there.
- Can the schema enforce this? If the type system can't, use schema validation.
- Can the module boundary enforce this (i.e., the only path to violate it goes through a controlled interface)? If so, make the interface the gate.
- If none of the above, monitor it — but document that it is a monitored invariant, and track it as a future candidate for structural enforcement.

The governance philosophy is not perfectionism. Monitored invariants are acceptable. The discipline is never to confuse a monitored invariant for a structural one, and never to stop looking for opportunities to promote monitoring to structure.

---

## 3. Module Map

### Summary Table

| Module | Purpose | Language/Stack | State | Role in Lattica |
|---|---|---|---|---|
| LumaWeave | Graph visualization workbench | TypeScript, React 19, Tauri 2, Sigma.js, Zustand | Production, active | Primary graph module; Lattica's visual cortex |
| Cerebra | Memory and knowledge layer | Python 3.12+, SQLite WAL + FTS5 + vector embeddings | Active development | Hippocampus; long-term memory and retrieval |
| Policy Scout | Governance and safety daemon | Python 3.12+, SQLite, YAML registries, Tauri 2 dashboard | Production, active | Immune system; structural gate on all agent side effects |
| ai-stack | Inference infrastructure | Ollama, LiteLLM, Open WebUI, Docker Compose | Operational | Metabolic layer; GPU compute and model routing |
| discord-bot (Bo) | Communications interface | Python 3.11+, discord.py | Operational | Peripheral nervous system; human-in-the-loop channel |
| Rhyzome | Code repair agent | Python 3.12, Qwen2.5-Coder:14b + Llama3.1 | Operational, 87.5% benchmark | Motor cortex; executes targeted repairs |
| bons.ai / AI-lab | Multi-agent cognitive system | Python 3.12, three-agent loop | Experimental | Prefrontal cognition; hypothesis generation and evaluation |
| gwells | n-body physics engine | TypeScript, embedded in LumaWeave | Production, embedded | Spatial reasoning engine for graph layout |
| eval-core | Shared evaluation infrastructure | TypeScript, stdlib only, zero deps | Extracted, standalone | Measurement substrate; CI regression floor |
| lattica-es | Event sourcing library | Rust core, PyO3, napi-rs/WASM | Design phase | Circulatory system; shared event fabric and history |
| LumaShell patterns | Absorbed UX design patterns | N/A (benched project) | Absorbed, not running | Design vocabulary; four patterns incorporated into Lattica UX |

### Module Descriptions

**LumaWeave — Graph Module**

LumaWeave is the visual center of Lattica. Its job is to render any module's state as a typed node/edge network. It is not a generic graph viewer — it has semantic knowledge of source adapters, live transport, gwells physics, and the tile/panel layout system. The registry architecture (14+ registries) makes it extensible without touching core. As Lattica's primary graph module, it renders both the canonical snapshot (Graph A) and the live state (Graph B), and it is the surface through which the reflective twin becomes visible. Decision ADR-001 establishes that Lattica IS LumaWeave extended — the LumaWeave codebase becomes Lattica's codebase. LumaWeave's identity persists as the graph module; it does not dissolve.

**Cerebra — Memory and Knowledge Layer**

Cerebra is a local-first cognitive runtime. It maintains a SQLite database with WAL mode and FTS5 full-text search, vector embeddings via mxbai-embed-large-v1, and a LoRA training pipeline via Unsloth on the RTX 4070 SUPER. Its primary function in Lattica is to serve as the platform's long-term memory: ingesting observations, conversations, code artifacts, and events, and returning relevant context on demand. The current integration path is shell-out (ADR-005: `cerebra context --format json`), with a Unix domain socket daemon planned for Phase 7 to eliminate the 10-second cold-start. Cerebra's knowledge graph will become a LumaWeave source adapter in Phase 4, making memory structure directly visible.

**Policy Scout — Governance Daemon**

Policy Scout is the platform's immune system. Its governance scope is deliberately bounded (ADR-004): shell commands, package installations, file mutations, and HITL agent gates. LLM content evaluation is explicitly out of scope. This boundary is not arbitrary — it reflects the constraint design principle applied to governance: define the perimeter of structural enforcement precisely, so that everything within it is truly enforced, rather than having a vague boundary monitored poorly. Policy Scout's tighten-only YAML overrides, PreToolUse hook for Claude Code agents, and MCP `policy_scout_check` for Lattica agent dispatch are the three integration surfaces. Its audit JSONL becomes LumaWeave's first live source adapter in Phase 2.

**ai-stack — Inference Layer**

The ai-stack is Docker-composed Ollama + LiteLLM gateway + Open WebUI running on a local RTX 4070 SUPER. It is the platform's compute substrate for all local inference. In Phase 1, it gains Prometheus metrics export (via textfile collector and GPU poller), which become the first Lattica status panel data source. The ai-stack is the only module with no plans for graph visualization of its own state — its contribution to the platform is energy (compute), not cognition. Its observability surface (Prometheus) is consumed by other modules.

**discord-bot (Bo) — Communications Interface**

Bo is a Python discord.py bot that routes messages to local Qwen via LiteLLM, with a three-tier resilience pipeline and rolling conversation memory. In the current architecture, Bo is peripheral — it is a human-accessible channel into the platform's inference layer. Its Phase 9 transformation is significant: Bo's `gather_context()` function is replaced with the Cerebra session API, grounding Bo's responses in the platform's shared memory model rather than its own local rolling buffer. This makes Bo a window into Cerebra rather than a standalone chatbot.

**Rhyzome — Code Repair Agent**

Rhyzome is a Python-based code repair agent with an 87.5% benchmark pass rate, using Qwen2.5-Coder:14b and Llama3.1 with AST-based semantic gates. Its role in Lattica is as the motor cortex: when a repair is needed, Rhyzome is dispatched, executes against Policy Scout's governance gate, and emits structured events that LumaWeave renders as visible agent activity. Phase 8 adds event emission (`file_inspected`, `repair_attempted`, `strategy_selected`, `outcome`) and full Policy Scout HITL gate wiring for all filesystem side effects. The visual manifestation — files under investigation highlighted in the graph, repair strategy nodes, policy gate edges — is a primary demonstration of the reflective twin.

**bons.ai / AI-lab — Multi-Agent Cognitive System**

bons.ai implements a three-agent cognitive loop (generator, evaluator, mutator) with reinforcement learning, bandit algorithms, and ChromaDB for semantic memory. It represents the platform's most experimental cognitive architecture — the prefrontal reasoning layer. In Phase 8, the three-agent cycle becomes visible in LumaWeave through mutation genealogy tracked via the ES toolkit's `Mutated` events. The generator-evaluator-mutator loop, rendered as a graph, becomes one of the most visually compelling demonstrations of the platform.

**gwells — Physics Engine**

gwells is a custom n-body physics engine embedded in LumaWeave. It provides `radial-backbone` and `parallel-spines` layout dialects, with seed functions (phyllotaxis spiral, recursive fern-frond), NaN-guard layers, and a pluggable dialect registry. Its significance in the platform is underappreciated: graph layout in Lattica is not cosmetic. Repulsion forces encode risk scores. Cluster seeds encode knowledge domain membership. The spatial arrangement of nodes is an epistemically meaningful signal. gwells is designed for eventual extraction as a standalone npm package, which would be the first externally distributable Lattica subsystem.

**eval-core — Evaluation Infrastructure**

eval-core is a standalone TypeScript package at `lattica/eval-core/`, under 300 lines, with stdlib only and zero runtime dependencies. Its job is to provide shared evaluation primitives — pass rates, regression baseline comparison, CI gate conditions — that every module can use without taking a dependency on each other. The no-runtime-deps constraint is structural: it prevents eval-core from becoming an integration point that pulls in transitive dependencies and creates version conflicts. eval-core is the platform's measurement substrate, and its minimalism is a feature.

**lattica-es — Event Sourcing Toolkit**

lattica-es is the platform's event fabric, implemented as a Rust core with PyO3 Python bindings and napi-rs TypeScript bindings. Its three-table SQLite schema (events, snapshots, branches) provides immutable log, branchable history, and content-addressed IDs via blake3. The reactive subscription system fires on every append. Reducers are pure and synchronous at the type level — async reducers cannot compile. The agent trace adapter standardizes event types across all modules (`llm_call`, `tool_call`, `tool_result`, `reasoning_step`) and exports to OTel GenAI span format. Existing event stores in Cerebra and Policy Scout receive cross-language read adapters rather than replacement, respecting the constraint design principle: don't destroy existing invariants in pursuit of purity.

**LumaShell — Absorbed Patterns**

LumaShell was a Go/Bubble Tea terminal workspace, now benched. Rather than losing the design work, four UX patterns were extracted and absorbed into Lattica's design vocabulary (ADR-007). These patterns are documented fully in Section 7.

---

## 4. Platform Architecture

### The Module System

Lattica's module system is an extension of LumaWeave's existing registry architecture. The core insight is that the registry pattern that already governs LumaWeave's internal extensibility is the right pattern for platform-level module management as well. There is no "new" module system to build — there is the existing registry pattern, applied at a larger scope.

A Lattica module is a unit that:
1. Registers itself with the platform module registry at activation time
2. Declares its IPC contract shape (what Tauri commands it exposes, what events it emits, what events it consumes)
3. Provides at least one integration surface from the three-layer integration architecture (graph data, event stream, or governance gate)
4. Deactivates cleanly without requiring platform restart

The module registry follows the same `register()` + `subscribe()` pattern as all other LumaWeave registries. This means module discovery, activation state, and capability queries are available to any component in the system through the same mechanism that queries tile sections, physics dialects, or source adapters.

### The Module Lifecycle

**Registration**

A module registers at platform boot or on-demand activation. Registration provides: module ID, display name, IPC surface descriptor, declared event types (emitted and consumed), health check endpoint, and shutdown callback. Registration is synchronous. A module that fails to register in under 100ms is treated as unavailable.

**Activation**

Activation differs from registration. Registration is declaration; activation is connection. At activation, the module establishes its Tauri IPC channels, subscribes to ES toolkit streams it declared as consumed, and signals readiness. Activation is async and may fail. Failed activation leaves the module in a `degraded` state — visible in the graph, but with a distinct visual indicator, rather than silently absent.

**IPC Contract**

The IPC contract shape is defined at registration time and is immutable for the module's lifetime. This is a structural constraint — a module cannot add new commands at runtime. The contract is the module's public surface area, and it is fixed. Changes to the contract require re-registration (i.e., module restart). This prevents contract drift, where a module's actual capabilities diverge from its declared surface.

**Deactivation**

Modules deactivate via their registered shutdown callback. Deactivation is a two-phase protocol: `draining` (stop accepting new requests, complete in-flight work) then `stopped` (release resources, close IPC channels). The module registry maintains this state. A module in `draining` is still visible in the graph, with a visual indicator distinct from both `active` and `failed`.

### The Three Integration Layers

**Layer 1: Graph Data (Source Adapters)**

Any module that wants its state visible in LumaWeave implements a source adapter. The source adapter interface is already defined in LumaWeave (5 existing adapters: self-graph, markdown-vault, cytoscape-json, package-dependency, csv-edge-list). The `transport:"live"` field is seeded in the type system but not yet implemented — Phase 2 implements it for the first time via Policy Scout's audit JSONL watcher. The `database-schema` slot is declared with an empty `LoaderFn` — Phase 4 implements it for Cerebra's SQLite.

Source adapters are the primary mechanism by which module state becomes visible. A module without a source adapter exists in the platform but is invisible in the graph. Invisibility is not the same as absence — the module registry knows it exists and it participates in IPC — but invisibility means the developer loses the primary observability surface.

**Layer 2: Event Stream (ES Toolkit)**

Any module that wants its state changes to be part of the semantic diff layer emits events through the ES toolkit. Event emission is not mandatory — a module can be a consumer-only participant, reading the event stream without emitting. But a module that neither emits nor consumes events is not participating in the reflective twin.

The ES toolkit's cross-language bindings (PyO3 for Python modules, napi-rs for TypeScript modules) mean that Cerebra, Policy Scout, Rhyzome, bons.ai, and LumaWeave all emit to the same SQLite-backed event store using native language bindings with no serialization overhead beyond msgpack payload encoding.

**Layer 3: Governance Gate (Policy Scout)**

Any agent-dispatched action that modifies the filesystem, installs packages, or executes shell commands must pass through the Policy Scout governance gate. This is the immune system layer. The PreToolUse hook for Claude Code and the MCP `policy_scout_check` for Lattica's own agent dispatch are the two integration points.

This layer is not optional for agents. A module can declare it has no agents (and therefore needs no governance gate), but a module with agents that bypass the governance gate is an architectural violation, not a configuration choice.

### The Rust Backend

Tauri 2's Rust backend has five primary responsibilities in Lattica:

1. **IPC routing** — mediating all cross-module communication that crosses the Tauri boundary. The Rust backend is the only process that can hold open connections to all modules simultaneously.
2. **ES toolkit host** — the SQLite-backed event store runs in the Rust backend process. This gives it access to the same process lifetime as the Tauri shell, ensuring events are not lost on webview reload.
3. **Filesystem gate** — the lowest-level Policy Scout integration point. File mutations requested through IPC are validated against Policy Scout before execution.
4. **Health monitoring** — the Rust backend polls module health check endpoints and updates registry state. Module `degraded` and `stopped` states are driven by the Rust backend.
5. **Native capability host** — GPU queries, OS-level metrics, and other capabilities that require native code live in the Rust backend and are exposed via Tauri commands.

The Rust backend is deliberately thin on business logic. It routes, gates, monitors, and hosts. It does not reason, remember, or decide. All cognitive behavior lives in the modules.

### Monorepo Structure (ADR-006)

```
lattica/
  apps/
    desktop/          # Tauri 2 shell (extends LumaWeave)
  packages/
    lumaweave/        # Graph module (was the LumaWeave repo root)
    eval-core/        # Standalone evaluation package
    lattica-es/       # ES toolkit (Rust + TS bindings)
    gwells/           # Physics engine (npm-extractable)
  python/
    cerebra/          # Memory layer (uv workspace)
    policy-scout/     # Governance daemon (uv workspace)
    rhyzome/          # Code repair agent (uv workspace)
    bons-ai/          # Multi-agent system (uv workspace)
    discord-bot/      # Bo (uv workspace)
  infra/
    ai-stack/         # Docker Compose (Ollama + LiteLLM + Grafana)
  docs/               # All architectural documentation
```

pnpm workspaces govern TypeScript/Rust packages. uv workspaces govern Python packages. The two workspace systems are independent — they share no tooling. This is intentional: forcing a unified workspace manager across language ecosystems creates more problems than it solves.

---

## 5. Integration Topology

### The Single-Machine Constraint

Lattica is a local-first platform. All modules run on a single machine. There is no distributed system, no network partition problem, no eventual consistency requirement between modules. This is not a limitation to be engineered around — it is a first-class architectural property that enables guarantees that distributed systems cannot provide.

Specifically: the event fabric is a single SQLite file. All modules write to the same file through the ES toolkit bindings. SQLite's WAL mode gives concurrent readers and a single writer. There is no message broker, no queue, no broker failure mode. The event log is just a file. It can be backed up with `cp`. It can be inspected with any SQLite tool. Its integrity can be verified with blake3 content-addressed IDs.

This is constraint design applied to infrastructure: the topology makes certain failure modes structurally impossible.

### Integration Topology Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        LATTICA DESKTOP APP                          │
│                      (Tauri 2 / Rust backend)                       │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                   LUMAWEAVE (Graph Module)                     │ │
│  │                                                                │ │
│  │   Graph A (Canonical) ──────────── Graph B (Live State)       │ │
│  │         │                                  │                  │ │
│  │         └──────────── Diff Layer ──────────┘                  │ │
│  │                    (ES Toolkit events)                         │ │
│  │                                                                │ │
│  │  Source Adapters:                                              │ │
│  │    ├── self-graph (existing)                                   │ │
│  │    ├── policy-scout-audit (Phase 2, transport:live)            │ │
│  │    ├── cerebra-knowledge (Phase 4, transport:database-schema)  │ │
│  │    ├── ai-stack-metrics (Phase 4, transport:prometheus)        │ │
│  │    └── rhyzome-repair (Phase 8, transport:event-stream)        │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                              │                                      │
│                    Tauri IPC │ Commands + Events                    │
│                              │                                      │
│  ┌─────────────┐  ┌──────────┴──────────┐  ┌─────────────────────┐ │
│  │  POLICY     │  │    ES TOOLKIT       │  │  MODULE REGISTRY    │ │
│  │  SCOUT      │  │    (Rust core)      │  │  (Rust backend)     │ │
│  │             │  │                     │  │                     │ │
│  │  PreToolUse │  │  events.sqlite      │  │  Health monitor     │ │
│  │  hook       │  │  ├── events         │  │  IPC routing        │ │
│  │  MCP gate   │  │  ├── snapshots      │  │  Lifecycle state    │ │
│  │  YAML rules │  │  └── branches       │  │                     │ │
│  └──────┬──────┘  └──────────┬──────────┘  └─────────────────────┘ │
│         │                    │                                      │
└─────────┼────────────────────┼──────────────────────────────────────┘
          │                    │
          │   (Unix sockets / shell-out / Prometheus HTTP)
          │                    │
┌─────────┴────────────────────┴──────────────────────────────────────┐
│                      LOCAL PROCESS SPACE                             │
│                                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐   │
│  │   CEREBRA    │  │   RHYZOME    │  │        AI-STACK          │   │
│  │              │  │              │  │                          │   │
│  │  SQLite WAL  │  │  Repair      │  │  Ollama (GPU)            │   │
│  │  FTS5        │  │  agent       │  │  LiteLLM gateway         │   │
│  │  Embeddings  │  │  AST gates   │  │  Prometheus metrics      │   │
│  │              │  │  87.5% bench │  │  Grafana dashboard       │   │
│  │  Phase 5:    │  │  Phase 8:    │  │  Phase 1:                │   │
│  │  cerebra     │  │  emits ES    │  │  GPU VRAM → textfile     │   │
│  │  serve (UDS) │  │  events      │  │  → Lattica status panel  │   │
│  └──────┬───────┘  └──────┬───────┘  └────────────┬─────────────┘  │
│         │                 │                        │                │
│  ┌──────┴───────┐  ┌──────┴───────┐  ┌────────────┴─────────────┐  │
│  │   BONS.AI    │  │   DISCORD    │  │         EVAL-CORE        │  │
│  │              │  │   BOT (Bo)   │  │                          │  │
│  │  Gen/Eval/   │  │              │  │  Standalone package      │  │
│  │  Mutator     │  │  Phase 9:    │  │  CI regression floors    │  │
│  │  RL/bandit   │  │  → Cerebra   │  │  Zero runtime deps       │  │
│  │  ChromaDB    │  │  session API │  │                          │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

### Data Flow: Cerebra → LumaWeave

Current (Phases 0–3): Cerebra is accessed via shell-out. `cerebra context --format json` is called from Tauri's Rust backend, stdout is parsed, and the result is displayed in the Cerebra status panel. No graph data.

Phase 4: The `database-schema` source adapter slot (currently declared with empty `LoaderFn`) is implemented to read Cerebra's SQLite directly. The adapter reads the `memory_items` and `retrieval_traces` tables, constructs a graph where memory items are nodes and retrieval associations are edges, and feeds it into LumaWeave as a live source. The SKU D1 quadrant classification becomes the seed function for gwells cluster positioning — items in the same knowledge domain cluster spatially.

Phase 7 (Cerebra daemon): The shell-out is replaced by the Unix domain socket at `~/.cerebra/sockets/cerebra.sock`. The source adapter migrates from direct SQLite reads to the socket API. Response time drops from 10 seconds (cold start) to sub-100ms (model loaded in daemon).

### Data Flow: Policy Scout → LumaWeave

Phase 2 implements the first `transport:"live"` source adapter. Policy Scout writes structured JSONL to its audit log on every governance decision. The live adapter tails this file (via Rust `notify` crate watching filesystem events, exposed through Tauri IPC to the webview). Each audit entry becomes a LumaWeave graph event.

The graph semantics are specific: `request_id` chains construct DAGs (a sequence of policy decisions with the same `request_id` are causally linked edges). `risk_score` drives gwells repulsion forces — high-risk policy events push nodes apart spatially, making risk visible as graph density. This is constraint design in visualization: you cannot have a high-risk cluster that looks calm. The physics make it impossible.

### Data Flow: ai-stack → LumaWeave

Phase 1 adds a Prometheus textfile collector to the ai-stack. The Ollama `/api/ps` endpoint is polled for GPU utilization and VRAM consumption. This is written to a textfile that Prometheus scrapes. Grafana reads from Prometheus for the ai-stack dashboard.

In Phase 1, the Lattica ai-stack status panel reads directly from Prometheus's HTTP query API (not from Grafana). This is the first Prometheus-to-LumaWeave data path. Phase 4 formalizes this as a `prometheus` transport type in the source adapter registry — the same extensibility surface that handles file-based and database adapters handles time-series metrics.

### Data Flow: Rhyzome → LumaWeave

Phase 8 adds structured event emission to Rhyzome. Currently, Rhyzome logs to console and returns a result. In Phase 8, every significant internal state change emits an ES toolkit event: `file_inspected` (Rhyzome has opened a file for analysis), `repair_attempted` (a strategy has been selected and is being applied), `strategy_selected` (AST-based gate has approved a specific repair path), `outcome` (repair succeeded or failed with classification).

These events flow through the ES toolkit's SQLite store. LumaWeave's event-stream source adapter (Phase 8) reads them and constructs an overlay graph: the files Rhyzome is currently investigating are highlighted as active nodes, repair strategy selection appears as new nodes with weighted edges to the target file, Policy Scout gate outcomes appear as edge annotations. The developer sees Rhyzome working.

---

## 6. The Reflective Twin

### Canonical vs. Live

The distinction between Graph A (canonical) and Graph B (live) is not just a visualization choice. It reflects a deep epistemological principle about what the system can and cannot claim to know.

**Graph A** represents claims that have survived validation. A node in Graph A has been observed, evaluated, and confirmed. Its edges have been validated as accurate. Its metadata (labels, types, attributes) have been checked against ground truth. Graph A does not change without an explicit promotion event — a semantic event that says "this live observation has been confirmed and incorporated into the canonical record."

**Graph B** represents current observations. It changes continuously. A node in Graph B might be tentative, might be contradicted by another node in Graph B, might be stale (observed 5 seconds ago but not yet invalidated). Graph B has high information density and low confidence density. It is the raw signal before filtering.

The developer interacts with both simultaneously. LumaWeave renders them as overlapping or side-by-side views, with visual differentiation (opacity, border style, animation cadence). A node that exists in both graphs is rendered with visual emphasis — it is confirmed live state, the highest-confidence category.

### Why the Diff Layer Has Semantic Meaning

A git diff tells you which bytes changed. The diff layer in Lattica tells you why they changed and with what intent.

`Agent investigating` means: an agent has opened this node for examination. The system does not know yet what the agent will do. This is a liminal state.

`Agent proposes modification` means: the agent has formed a hypothesis and is proposing a change. The hypothesis is now visible. It can be evaluated before execution.

`Policy violation detected` means: the proposed change has been flagged by the immune system. The system cannot proceed without either HITL resolution or policy override. This is a hard gate.

`Repair pending` means: a modification has been approved by Policy Scout and is queued. The system is committed to executing it. This is the last point at which human intervention is structurally possible before execution.

`Consensus reached` means: Graph A and Graph B have agreed on a subgraph. The live observation has been validated and promoted. The canonical record has been updated.

These events are typed, versioned, content-addressed, and stored in the ES toolkit's immutable log. They cannot be deleted. They can be queried, replayed, and analyzed. The history of the system's cognition is always available.

### What Visual Agent Activity Looks Like

When Rhyzome is investigating a bug in Phase 8:

1. The file nodes that Rhyzome has opened appear highlighted with a distinctive pulse animation (customizable via theme targets and node program registry — no hardcoded styles)
2. As Rhyzome's AST analysis proceeds, dependency edges to affected functions appear in Graph B as new edges with `investigating` type
3. When Rhyzome selects a repair strategy, a new strategy node appears connected to the target file node — this is the proposal, visible before execution
4. The Policy Scout gate edge appears connecting the strategy node to the governance node — the approval chain is visible
5. On approval, the repair node changes state and execution begins — the edge to the target file animates (direction indicates write direction)
6. On completion, an `outcome` node appears with success/failure classification and confidence score
7. If the repair succeeds and is validated, a promotion event moves the affected subgraph from Graph B into Graph A

The developer watches this happen in real time. They can pause at any step (via the HITL gate). They can rewind via the ES toolkit's time-travel viewer. They can branch at any decision point and replay with a different policy configuration.

### Counterfactual Exploration via ES Toolkit Branching

The ES toolkit's branches table (`id`, `parent_id`, `parent_version`) enables a capability that no flat event log provides: counterfactual history.

Scenario: Policy Scout blocked a Rhyzome repair attempt because the file mutation exceeded the permitted scope. The developer wants to know: what would have happened if the policy had allowed it?

1. Create a branch at the `Policy violation detected` event
2. On the branch, inject a synthetic `Policy approved` event with the hypothetical permission
3. Replay subsequent events on the branch, allowing the repair to proceed
4. Observe the `outcome` event on the branch
5. Discard the branch (events are shared storage with the parent branch — no copying)

This is not simulation. The actual repair logic runs against the actual codebase state at the time of the branch point. The branch is a real execution with counterfactual governance configuration.

This capability is what makes the ES toolkit's design requirements non-negotiable. A message bus delivers. A log records. Only an event sourcing system with branching history enables replay and counterfactual exploration. The NATS rejection (ADR-002) was correct.

### Evolution Path: v1 → v2 → Cognitive OS

**Phase 5 (Reflective Twin v1) — Polling-based**

The first working reflective twin uses polling. LumaWeave polls Cerebra's SQLite at a configured interval (1–5 seconds depending on activity mode). The live graph updates are batched per poll cycle. The canonical graph is updated on explicit user action or on a configured promotion schedule.

Working memory slots from Cerebra become graph nodes. Retrieval traces appear as subgraph overlays — when Cerebra answers a query, the contributing memory items light up as a connected subgraph for a configurable duration. Discord conversation threads appear as high-level nodes connected to the memory items they reference.

Phase 5 is deliberately limited. Polling introduces latency. The diff layer is not yet semantic — it is just "graph changed since last poll." But it is working, observable, and demonstrable.

**Phase 6 (Event Fabric) — Reactive**

Phase 6 replaces polling with the ES toolkit's reactive subscriptions. Every module state change that is structurally significant emits an event. LumaWeave subscribes to relevant event streams via the TS bindings and updates the graph on every append — no polling, no batch latency.

The diff layer becomes semantic. Events have types. The visualization layer can distinguish `Agent investigating` from `Policy violation detected` and render them differently without a general "graph changed" handler.

Phase 6 also adds the time-travel viewer — a scrubbable timeline embedded in LumaWeave's tile system. The developer can drag the timeline scrubber backward and watch the graph rewind to any historical state.

**Phase 12 (Cognitive OS / Reflective Twin v2) — Research**

Phase 12 is a research agenda, not an engineering roadmap (ADR-008). Each milestone is an experiment with a hypothesis and success criterion. The direction is:

- **Epistemic evolution graph**: tracking how the system's beliefs change over time, which beliefs are stable, which are volatile, which are contested
- **Meta-cognitive profiler**: measuring the quality of the system's reasoning processes, not just its outputs
- **Teacher-student distillation pipeline**: using Claude to review local model reasoning, generate corrections, and produce training signal for LoRA fine-tuning — the system improving its own inference quality

Phase 12 is where the platform becomes genuinely novel research infrastructure. Whether it succeeds as research is unknown. That is what makes it research.

---

## 7. LumaShell Pattern Absorptions

LumaShell was a Go/Bubble Tea terminal workspace that was benched when it became clear that a terminal UI was the wrong display medium for a graph-visual platform. The project was not a failure — it was a design exploration that produced four UX patterns worth preserving. ADR-007 documents the absorption of these patterns into Lattica's design vocabulary.

### Pattern 1: Breadcrumb Module Navigation

**LumaShell origin:** LumaShell's terminal UI navigated between functional areas using a breadcrumb path displayed in the header. The path reflected not just the current view but the history of how the user arrived there — not just "where am I" but "how did I get here."

**Lattica manifestation:** In Lattica's tile system, the active module context is displayed as a breadcrumb navigation element. When the developer drills into Cerebra's knowledge graph from the main platform overview, the breadcrumb records the path: `Lattica > Graph B > Cerebra Knowledge > Memory Item #4217`. This breadcrumb is not cosmetic — it corresponds to an actual navigation state that can be bookmarked (LumaWeave's existing `bookmarkRegistry`) and restored.

The breadcrumb pattern matters for the reflective twin because the developer frequently needs to navigate from a high-level system view into a specific module's detailed state and back out. The breadcrumb maintains orientation across scale changes that would otherwise be disorienting.

### Pattern 2: Config Hot-Reload

**LumaShell origin:** LumaShell watched its YAML config files for changes and hot-reloaded configuration without requiring restart. The reload was surfaced to the user as a transient status indicator, not a notification prompt.

**Lattica manifestation:** Lattica inherits this pattern at two levels:

First, Policy Scout's YAML rule files are watched by the Rust backend (via `notify` crate). Changes to policy configuration are hot-reloaded without restarting Policy Scout. The live graph in LumaWeave reflects the updated policy configuration within the next polling cycle or immediately via event emission (Phase 6+). A tighten-only validation step runs synchronously on the hot-reload path — a loosening change fails the validation and does not apply, with a visible error state in the governance panel.

Second, LumaWeave's own registry-driven architecture is inherently hot-reload-friendly. Source adapters, physics dialects, and tile sections can be re-registered without graph reset (the registry's `subscribe()` pattern notifies consumers of changes). This is not identical to LumaShell's file watching, but it is the same principle applied to the in-memory registry layer.

### Pattern 3: Atmosphere Layered Theming

**LumaShell origin:** LumaShell implemented theming as atmospheric layers — a base layer defining structural color relationships, overlaid by an atmosphere layer defining mood/context variations. The atmosphere layer could shift based on system state (high load, error conditions, quiescent operation) without changing the structural theme.

**Lattica manifestation:** LumaWeave already has a theme target registry (`themeTargetRegistry`) and Zustand-managed theme state. The atmosphere pattern extends this: a base structural theme is complemented by a contextual atmosphere layer that responds to system state. When Policy Scout is in a high-alert state (multiple recent violations), the atmosphere shifts to a cooler, higher-contrast palette. When Cerebra's retrieval confidence is high, the atmosphere warms. When Rhyzome is actively executing a repair, a subtle animation cadence activates across the graph that signals "system is acting."

The atmosphere layer is defined through the same theme target registry as structural tokens — no separate system. An atmosphere is a named set of theme token overrides that the registry applies as a layer on top of the structural base. Removing the atmosphere reverts to base without any state mutation.

### Pattern 4: 4-Mode Multi-Pass Panel Layout

**LumaShell origin:** LumaShell's terminal panel system supported four distinct operational modes, each with its own panel layout optimized for the workflow it supported: exploration mode (wide graph, narrow controls), analysis mode (split graph / detail), command mode (minimal graph, large command surface), and review mode (graph hidden, full document view). Switching modes was a single keystroke. The layout memory persisted the last state in each mode.

**Lattica manifestation:** Lattica's tile system (already present in LumaWeave as `tileSectionRegistry`) is extended with a named-mode layer. The four absorbed modes map to Lattica contexts:

- **Explore mode** — graph at 70% of panel space, source adapter sidebar, gwells physics controls minimized to icons. Default state for orientation and discovery.
- **Inspect mode** — graph at 50%, inspector spoke at 50%, full property detail visible. Activates automatically when a node is selected (can be overridden by explicit mode selection).
- **Agent mode** — graph at 60%, agent activity log at 30%, governance panel at 10%. Activates when any agent is dispatched. Optimized for watching the reflective twin in operation.
- **Review mode** — graph minimized or hidden, document/diff view at full width. Activates during code review or when navigating retrieval traces. ES toolkit timeline scrubber visible.

Mode transitions are animated (respecting the existing `motionSafetyRegistry` for users with motion sensitivity preferences). Each mode's panel layout state is persisted in `useSettingsStore` under a versioned key, consistent with the existing 90+ migration discipline.

---

## 8. Key Invariants

These invariants must never be violated. Each is derived from the constraint design philosophy and from hard lessons in the project history. They are listed in rough order from most structural (hardest to accidentally violate) to most behavioral (requires discipline to maintain).

### Invariant 1: Registries Over Hardcoded Lists

No enumeration of module types, source adapters, tile sections, physics dialects, node programs, commands, or any other extensibility dimension may be implemented as a hardcoded array or switch statement in core code.

**Structural enforcement:** The registry pattern (`Map<K, V>` with `register()` + `subscribe()`) is the only recognized mechanism for extensibility. Code review must reject any `const MODULE_TYPES = [...]` or `switch (type) { case 'cerebra': ... }` in core. If a new type appears that is not in the registry, it does not exist to the system.

**Why:** Hardcoded lists have been the source of multiple maintenance failures in LumaWeave's history. Registries make the extension surface explicit and discoverable without modifying core files. This is constraint design: the registry makes it structurally expensive to bypass.

### Invariant 2: Tighten-Only Policy Overrides

Policy Scout's YAML override files may only reduce permitted scope. No override may expand permissions beyond the compiled-in baseline.

**Structural enforcement:** The hot-reload path and the initial load path both run a `validate_tighten_only()` step that rejects any override that would expand permissions. This runs synchronously before the override is applied. A loosening override cannot be applied silently — it produces a schema validation error visible in the governance panel.

**Why:** The governance layer is the immune system. An immune system that can be weakened by configuration is not a structural guarantee — it is a monitoring system with an administrator override. The structural enforcement of tighten-only is what makes Policy Scout's governance a hard gate rather than a soft suggestion.

### Invariant 3: Pure and Synchronous ES Toolkit Reducers

All reducers in the ES toolkit are pure functions (no side effects, referentially transparent output given the same input) and synchronous (no async, no I/O, no promises).

**Structural enforcement:** The TypeScript type system enforces this at compile time. The Rust type system enforces it in the core. A reducer that returns a Promise does not compile. A reducer that takes a mutable reference to external state fails the type checker.

**Why:** Pure synchronous reducers make state reconstruction deterministic and fast. Snapshots are optimization only — correctness never depends on a snapshot being present (every state can be recomputed from events). This property enables the time-travel viewer and counterfactual branching without special-case handling.

### Invariant 4: Forward-Only Database Migrations

LumaWeave's Zustand store migrations, Cerebra's SQLite schema, Policy Scout's SQLite schema, and the ES toolkit's SQLite schema are all forward-only. No migration is ever deleted, modified, or reversed. Adding a new migration increments the version monotonically.

**Structural enforcement:** Migration functions are numbered. The migration runner checks that the stored version is never greater than the highest migration number and never skips a version. A schema at version N must have been produced by applying migrations 1 through N in order. Attempting to modify migration 47 to "fix" it breaks the invariant for any database that already applied migration 47.

**Why:** Schema history is a record of how the data model evolved. Modifying history creates divergence between the current code's expectation of "what migration 47 does" and what was actually applied to existing databases. Forward-only migrations with a sequential version number make this invariant structurally visible — the version number is a checksum of the applied migration sequence.

### Invariant 5: eval-core Has Zero Runtime Dependencies

The `lattica/eval-core/` package has no runtime dependencies beyond the standard library. It is under 300 lines. It exports evaluation primitives only.

**Structural enforcement:** CI checks `package.json`'s `dependencies` field (not `devDependencies`) and fails if it is non-empty. The 300-line limit is checked by a pre-commit hook.

**Why:** eval-core is imported by every module that has CI-gated regression baselines. If eval-core has runtime dependencies, every module that imports it inherits those dependencies transitively. The no-runtime-deps constraint prevents eval-core from becoming an integration surface that creates version conflicts or supply chain exposure.

### Invariant 6: Event Store Is Append-Only

The ES toolkit event store (`events` table in SQLite) accepts only INSERT operations on the events table. UPDATE and DELETE are not permitted by the Rust core's internal API. The only mutation permitted after insertion is to the `snapshots` table (which is an optimization surface) and the `branches` table (which tracks branch provenance, not event content).

**Structural enforcement:** The Rust core exposes no `update_event()` or `delete_event()` function. The only write path is `append()`. Content-addressed IDs (blake3 hash of type + payload + causation_id) make tampering detectable — a modified event produces a different content address, breaking the chain.

**Why:** Immutability is the foundation of the reflective twin. If events can be deleted or modified, the time-travel viewer cannot guarantee it is showing what actually happened. The counterfactual branching feature depends on the canonical branch being a true record. Append-only storage is the structural guarantee that makes historical analysis trustworthy.

### Invariant 7: Module IPC Contracts Are Immutable at Runtime

A module's declared IPC contract (the commands it exposes and the event types it emits/consumes) is fixed at registration time and cannot change while the module is active. Adding a new command requires re-registration (module restart).

**Structural enforcement:** The module registry does not expose an `update_contract()` method. Attempting to call a command that was not in the registration-time contract returns a typed error. The Rust IPC router rejects commands not declared in the registered contract.

**Why:** Contract drift — where a module's actual capabilities diverge from its declared surface area — is a debugging and reliability failure mode that the constraint eliminates. Consumers of a module can inspect the contract at registration time and trust it remains accurate. If the contract has changed, the module has restarted (and the restart is a visible event in the registry).

### Invariant 8: Source Adapters Never Modify Their Source

A source adapter's job is to read data and produce a graph representation. It may not write to, delete from, or modify the data source it adapts. All writes go through the module that owns the data source via IPC.

**Structural enforcement:** The source adapter interface in LumaWeave's type system exposes only `load()` and `subscribe()` functions. There is no `write()` function. An adapter that needs to write data has been implemented incorrectly and should be split: a read adapter (source adapter) and a write path (IPC command to the owning module).

**Why:** Bidirectional adapters create ambiguity about who owns the data and which copy is canonical. The Cerebra source adapter reads Cerebra's SQLite. It does not write to it. If LumaWeave needs to trigger a Cerebra operation (e.g., add a memory item), it sends an IPC command to the Cerebra module. The data path is unambiguous.

### Invariant 9: Governance Gate Covers All Agent Filesystem Side Effects

Any agent dispatched by Lattica that performs a filesystem mutation (write, delete, rename, chmod) must pass through the Policy Scout governance gate before execution. There is no exception for "small changes," "read-only operations," or "operations already approved by the developer at session start."

**Structural enforcement:** The PreToolUse hook is wired into Claude Code's tool execution path. The MCP `policy_scout_check` is called by Lattica's agent dispatch before any tool that could cause a filesystem mutation. Rhyzome's integration in Phase 8 requires that every repair attempt goes through the gate. bons.ai's mutations in Phase 8 are gated at the Lattica agent dispatch layer, not within bons.ai itself.

**Why:** Session-level blanket approvals are a common failure mode in agent governance — they transform "the developer approved this class of action" into "the developer approved all actions." Structural per-operation gating makes this impossible. The cost is latency on each operation; the benefit is that "the governance system was active" is a verifiable claim, not a statement of intent.

### Invariant 10: Visual Changes Require Manual Verification

No visual change (layout, animation, interaction feel, color, spacing, motion, graph rendering) may be claimed "ready for review" based on automated test results alone. The developer must verify the change in `npm run dev` before any commit involving visual changes.

**Structural enforcement:** This invariant is in CLAUDE.md and enforced by protocol rather than by type system. The reason it is listed here is that it is a constraint design principle applied to the development process: make it impossible to silently commit a visual regression by requiring a structural gate (human visual inspection) before the commit gate.

**Why:** Playwright tests verify behavior, not appearance. A layout that is structurally correct (correct DOM, correct data, passing accessibility checks) can still be visually broken (overlapping elements, incorrect z-order, animation jank). The only test for visual correctness is visual inspection. Pretending automated tests cover this creates false confidence.

### Invariant 11: The ES Toolkit Existing Event Stores Are Adapted, Not Replaced

Cerebra's `inspector_events` table and Policy Scout's `audit.db` are live production data stores. When the ES toolkit is introduced in Phase 6, these stores receive cross-language read adapters. They are not migrated to, replaced by, or deprecated in favor of the ES toolkit's event store.

**Structural enforcement:** The Phase 6 scope description explicitly states "cross-language read bridge over existing stores — not replaced." Any Phase 6 implementation that proposes migrating or deprecating these stores should be treated as a scope violation.

**Why:** Each existing store has established consumers, established migration discipline, and established backup procedures. Replacing them introduces risk with no immediate benefit — the cross-language read adapter provides the integration value (ES toolkit reads can include events from these stores) without the migration risk. This is constraint design applied to integration: preserve existing invariants; don't destroy them in pursuit of architectural uniformity.

---

## 9. What Success Looks Like

### At Phase 5 Completion

The developer can open Lattica on their local machine and see:

**A live split-view graph** showing Graph A (confirmed knowledge, drawn with solid nodes and edges) and Graph B (live state, drawn with slightly translucent nodes and animated edges that pulse when recently updated).

**Cerebra's knowledge graph** visible as a source adapter in LumaWeave — memory items as nodes, retrieval associations as weighted edges, knowledge domains clustered by gwells physics seeds derived from the SKU D1 classification. The developer can click any memory node and see the full memory item text in the inspector spoke.

**Working memory slots** from Cerebra visible as a distinct node type — brighter, more prominent, connected to the memory items they currently reference. When Cerebra retrieves context for a query, the contributing memory items briefly illuminate as a subgraph overlay.

**Discord conversation threads** as graph nodes connecting to the memory items they reference. The developer can trace from a Discord message to the memory items it created.

**Policy Scout's audit panel** showing live governance decisions, with the risk score distribution visible in the graph (high-risk decisions push clusters apart spatially via gwells repulsion forces).

**ai-stack GPU panel** showing current VRAM utilization, active models, and inference load — the first place in the portfolio where GPU state is directly visible.

The portfolio claim at Phase 5: "I built a local-first platform where multiple AI systems — memory, governance, inference — are visible simultaneously in a live graph, with the graph layout encoding semantic meaning from each module."

### At Phase 12 (Speculative)

The developer can open Lattica and watch the system reason about a problem.

**Rhyzome is investigating a bug.** The graph shows which files are under investigation (animated pulse), which dependencies are being traversed (new edges appearing in Graph B), and which repair strategy has been proposed (a new node in Graph B, connected to the target file, with a weighted edge to the Policy Scout governance node showing the gate state).

**Cerebra is retrieving context.** The retrieval trace is visible as a subgraph overlay — which memory items were queried, which were returned, their confidence scores as edge weights. The developer watches the system decide what it knows about the problem.

**The ES toolkit timeline is scrubbed backward.** The developer rewinds 10 minutes and watches the sequence of events that led to the current state. They see when Rhyzome first flagged the bug, when Cerebra was queried for context, when the repair strategy was selected. They branch at the strategy selection and replay with an alternative policy configuration.

**The meta-cognitive profiler is running.** A panel in Lattica shows the quality metrics of the current reasoning session — which reasoning steps had high confidence, which were uncertain, which bons.ai mutations survived the evaluator. The training signal for the next LoRA fine-tuning cycle is being accumulated in real time.

The portfolio claim at Phase 12: "I built a software cognition platform where the system models itself, observes itself, and reasons about itself. The visual layer makes the cognition legible. The governance layer makes the agency safe. The memory layer makes the reasoning coherent across time. These are not separate tools that I built — they are organs of a single organism, and the organism is visible."

### The Interview Demonstration

The demonstration that lands in an interview is not Phase 12. It is Phase 5 or Phase 8, running locally, showing:

1. **Open Lattica.** The graph loads. LumaWeave renders the platform's own structure as a self-graph — the registries, the tile sections, the active source adapters. The developer explains: "This is the system looking at itself."

2. **Activate the Cerebra source adapter.** The knowledge graph appears. Clusters form as gwells physics positions memory items by domain. "This is six months of conversations, code sessions, and research, structured as a knowledge graph."

3. **Ask Cerebra a question.** The retrieval trace appears as an animated subgraph overlay — the memory items that contributed to the answer, connected by confidence-weighted edges. "This is the system showing me how it thought."

4. **Trigger a Rhyzome repair.** A file node lights up. Strategy nodes appear. The Policy Scout gate edge appears. The gate approves. The repair executes. The outcome node appears. "This is an agent working in the graph. You can see every decision it made and every gate it passed through."

5. **Scrub the timeline backward.** The graph rewinds. The repair undoes. The strategy nodes disappear. The file returns to pre-investigation state. "This is the full history of what happened, queryable and replayable."

**What makes this a portfolio differentiator:**

The claim is not "I know React" or "I can build a graph visualization." The claim is: "I built a system where the constraint architecture makes bad states structurally impossible, where every AI component is visible to every other component, where the history of cognition is preserved and replayable, and where the governance system is a structural gate not a monitoring system. And I can show you all of this running locally on my machine."

That is not a tutorial project. That is not a CRUD application. That is original systems architecture work, demonstrated live, with a clear governing philosophy that explains every design decision.

The candidate who can say "I rejected NATS because it forgets, and I needed the event store's history for the reflective twin to be coherent" in a systems design interview has demonstrated architectural depth that is rare at any level.

---

*End of DESIGN.md*

*Companion documents:*
- *`docs/LUMAWEAVE_NOW.md` — current version, active roadmap, known bugs*
- *`docs/agent/DISCORD_PROTOCOL.md` — approval gates and communication protocol*
- *`docs/canonical/` — domain reference documents*
- *`docs/agent/survival-manual/` — debugging deep-dives*