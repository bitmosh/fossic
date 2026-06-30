# Lattica — Phase Plan

## How to Read This Document

This document is a planning reference and confidence calibration tool. It covers all 12 phases of the Lattica platform build, from infrastructure bootstrap through speculative research. Each phase entry answers five questions: what are we trying to achieve, what does "done" look like concretely, what has to be true before we start, what has to be true before we finish, and what could go wrong.

**Confidence levels** reflect the probability that the phase's scope, approach, and timeline are approximately right — not whether the work is hard. HIGH means the design is settled, the unknowns are engineering (not architectural), and dependencies are under control. MEDIUM means at least one architectural question remains open or at least one external dependency is not yet stable. MEDIUM-LOW means multiple open questions exist and the approach may change materially as earlier phases resolve. LOW means the design is provisional. SPECULATIVE means this is a research agenda, not an engineering plan — milestones are experiments, not features.

**Phase sequencing** is mostly sequential, but several phases have meaningful parallel work within them (particularly Phases 3–5, where Cerebra development runs alongside LumaWeave integration work). Cross-phase dependencies are called out explicitly in Entry Criteria. The critical path runs: 0 → 1 → 2 → 3 → 4 → 5 → 7 → 9. Phase 6 (ES Toolkit) is a structural dependency for Phase 8 but can be developed partly in parallel with Phases 4 and 5. Phases 10 and 11 depend on Phase 8 but are otherwise independent of each other. Phase 12 has no hard prerequisites beyond the platform being sufficiently built that there is something to reason about.

**Locked decisions** referenced throughout are documented as ADRs (ADR-001 through ADR-008). The ADR record is the authoritative source for architectural rationale; this document references them by number without restating the full argument.

---

## Phase 0 — Lattica Platform Bootstrap [HIGH]

### Goals

Establish the structural foundation from which all subsequent work builds. The primary goal is to make LumaWeave's codebase the Lattica codebase — not by renaming files, but by introducing the platform scaffolding (monorepo layout, IPC contract skeleton, Rust backend extension points, module registry pattern) that future modules slot into. LumaWeave's identity is preserved as the graph module; the platform identity is added around it.

The secondary goal is to capture the four UX patterns from the benched LumaShell project as living design references before context is lost (ADR-007). These patterns — breadcrumb module navigation, config hot-reload, atmosphere layered theming, 4-mode multi-pass panel layout — are inputs to future UI work, not implemented features yet.

### Deliverables

- Tauri 2 project extended with Lattica identity: updated `tauri.conf.json`, window title, app identifier (`ai.lattica.app`)
- Monorepo root initialized: `pnpm-workspace.yaml` declaring TypeScript/Rust packages, `pyproject.toml` (uv workspace root) declaring Python packages, root `package.json` with workspace scripts (ADR-006)
- Module registry pattern documented and stubbed: `src/platform/moduleRegistry.ts` with `register()`, `get()`, `list()` — empty implementations, contract defined
- IPC contract skeleton: `src-tauri/src/ipc/` with one stub command per planned module (graph, cerebra, policy, ai-stack), typed with `serde` on the Rust side and `@tauri-apps/api/core` `invoke()` wrappers on the TS side. No real logic — shape only.
- Rust backend skeleton: `src-tauri/src/modules/` directory with one `mod.rs` per planned module, each exporting a stub command handler
- LumaShell patterns documented: `docs/design/lumaShell-absorbed-patterns.md` with concrete implementation notes for each of the four patterns, sufficient for a future implementer to work from without the original codebase
- Existing LumaWeave functionality fully preserved: all existing Playwright E2E tests pass, registry shape unchanged, existing source adapters unaffected

### Entry Criteria

- LumaWeave is in a passing state: `tsc --noEmit` clean, Playwright suite green, no known regressions
- Developer has confirmed the ADR-001 decision (Lattica IS LumaWeave extended) and the monorepo structure agreed in ADR-006
- LumaShell source is accessible for pattern extraction

### Exit Criteria

- `pnpm install` from monorepo root resolves without errors
- `cargo build` in `src-tauri/` succeeds with no warnings on the new stub commands
- `tsc --noEmit` clean across all TypeScript packages in the workspace
- All existing LumaWeave Playwright E2E tests pass without modification
- Module registry `register()` / `get()` / `list()` round-trip is tested (unit test, not E2E)
- IPC stubs are callable from the frontend: a smoke test invokes each stub command and receives the typed stub response
- LumaShell patterns doc is written and reviewed by developer
- `docs/design/lumaShell-absorbed-patterns.md` exists and covers all four patterns with enough specificity to implement from

### Key Risks

- **Monorepo tooling friction.** pnpm workspaces + uv workspaces coexisting in one repo is an unusual configuration. Hoisting rules, phantom dependencies, and Python/Rust build tool interactions may require more yak-shaving than expected.
- **Tauri version lock.** LumaWeave is already on Tauri 2. If any LumaWeave dependency is pinned to an incompatible Tauri 2 minor, the extension may surface conflicts immediately.
- **Preserving LumaWeave's test suite.** The existing Playwright suite has known configuration sensitivity (QA registry, controlSurfaceContractRegistry contract counts). The monorepo restructuring must not move or rename files that these tests reference by path.
- **Scope creep.** Phase 0 is explicitly infrastructure only. The temptation to begin implementing module logic (especially for ai-stack, which is tangible and motivating) should be resisted. The IPC shapes are stubs; nothing real runs yet.

### Confidence Rationale

HIGH. The work is additive scaffolding on a known codebase. No novel algorithms, no external service dependencies, no design questions that aren't already answered by the ADRs. The monorepo tooling risk is real but solvable within the phase. The main risk to confidence is scope creep, not technical uncertainty.

---

## Phase 1 — Infrastructure Baseline / ai-stack [HIGH]

### Goals

Make the GPU infrastructure observable. The ai-stack (Ollama + LiteLLM + Open WebUI) exists but has a reliability gap (LiteLLM restart policy not set to `unless-stopped`) and has no metrics surface. This phase fixes both: LiteLLM becomes reliably running after host reboots, and VRAM utilization becomes visible in the Lattica UI for the first time. Prometheus and Grafana are added to the docker-compose stack. An Ollama GPU poller feeds into Prometheus. Lattica gets its first real panel reading live infrastructure data.

### Deliverables

- `docker-compose.yml` updated: LiteLLM `restart: unless-stopped`, Prometheus and Grafana services added with persistence volumes
- `litellm-config.yaml` updated: all Granite, OLMo, and Qwen models wired in with correct model names and routing
- LiteLLM Prometheus callback configured: `success_callback: ["prometheus"]` in litellm-config, metrics endpoint verified reachable
- Ollama GPU poller: a small script (Python or shell, containerized or systemd unit) that calls `ollama /api/ps`, extracts VRAM usage per model, writes Prometheus textfile format to the node-exporter textfile collector directory. Runs on a 15-second interval.
- Prometheus scrape config: scrapes LiteLLM metrics endpoint + node-exporter (for the Ollama textfile)
- Grafana provisioning: at least one dashboard provisioned via JSON (LiteLLM request latency, token throughput, per-model VRAM usage)
- Lattica ai-stack panel: a new tile section (`AiStackPanel`) in LumaWeave's tile registry showing current GPU VRAM usage (per model loaded), LiteLLM request rate, and model list. Reads from Prometheus HTTP API via a Tauri IPC command. Updates on a 30-second poll.
- IPC command: `get_ai_stack_metrics` in Rust, calls Prometheus HTTP API, returns typed struct

### Entry Criteria

- Phase 0 complete (monorepo bootstrapped, IPC skeleton in place)
- ai-stack docker-compose is running on the developer's machine
- RTX 4070 SUPER is accessible to Ollama (confirmed working in existing setup)

### Exit Criteria

- `docker compose up -d` starts all services including Prometheus and Grafana without errors
- After a simulated host reboot (or `docker compose stop && docker compose up -d`), LiteLLM comes back up automatically
- Prometheus is scraping LiteLLM metrics: `curl localhost:9090/api/v1/targets` shows LiteLLM target in UP state
- Ollama GPU poller runs and its metrics appear in Prometheus within 30 seconds of a model being loaded
- Grafana dashboard loads and shows VRAM data (at least one non-null data point for a loaded model)
- Lattica ai-stack panel renders in the Tauri app, shows current VRAM usage for at least one loaded model, and refreshes without user action
- `tsc --noEmit` clean, all existing Playwright tests pass

### Key Risks

- **Prometheus textfile collector path.** The node-exporter textfile directory path varies by installation method (Docker-in-Docker, bare node-exporter, custom path). The poller must write to the correct location and node-exporter must be configured to scan it.
- **LiteLLM Prometheus callback version compatibility.** LiteLLM's Prometheus integration has had breaking changes across minor versions. The current installed version may need a specific callback name or config format.
- **CORS / network routing.** The Lattica Tauri app calling Prometheus HTTP API from a Rust backend command avoids CORS issues, but Docker network naming (bridge vs. host network mode) may require explicit service DNS names in the Rust code.
- **Ollama `/api/ps` availability.** This endpoint exists in Ollama but its schema is not formally versioned. A future Ollama update could change the response shape and silently break the poller.

### Confidence Rationale

HIGH. All components are known quantities: Prometheus, Grafana, Ollama, LiteLLM are all running. The work is configuration and a small integration script. The main uncertainty is version-compatibility friction, which is expected to be resolvable within the phase. No novel design decisions required.

---

## Phase 2 — Governance Integration / Policy Scout [HIGH]

### Goals

Wire Policy Scout into Lattica as a live governance layer. This phase has two distinct outputs that must both land: (1) Policy Scout runs as a Lattica module with its audit stream feeding into LumaWeave's graph in real time, and (2) the PreToolUse hook is wired so that Claude Code agents operating in this environment have their tool calls checked by Policy Scout before execution.

This phase also delivers LumaWeave's first real implementation of the `transport:"live"` source adapter path — seeded in the registry since early development but never exercised. The audit JSONL watcher is the inaugural live adapter.

### Deliverables

- Policy Scout daemon integrated as Lattica module: launches from Tauri, appears in module registry, status visible in Lattica
- Audit JSONL watcher source adapter: implements the `transport:"live"` path in LumaWeave's sourceAdapterRegistry. Watches Policy Scout's audit JSONL file, parses events, emits graph deltas in real time. Registered as `policy-scout-audit` adapter.
- Graph node schema for policy events: `request_id` as node ID, `risk_score` as a node attribute that drives gwells repulsion force, parent-child `request_id` chains form DAG edges
- gwells integration: repulsion force modifier reads `risk_score` node attribute. High-risk nodes push neighbors away. This is the first gwells physics parameter driven by live data rather than static config.
- PreToolUse hook: `.claude/hooks/pre-tool-use.sh` (or equivalent) that calls `mcp policy_scout_check` before each tool execution when operating in Lattica's agent environment
- MCP tool: `policy_scout_check` available to Lattica agent dispatch. Calls Policy Scout's check endpoint, returns allow/deny/warn with rationale.
- Live audit panel: tile section in Lattica showing recent audit events (timestamp, tool name, decision, risk score), auto-scrolling
- E2E coverage: Playwright test exercising the live adapter path — event written to JSONL → node appears in graph within timeout

### Entry Criteria

- Phase 0 complete
- Policy Scout is running and generating audit JSONL output on the developer's machine
- Policy Scout's scope (ADR-004: shell + packages + file mutations + HITL agent gates) is confirmed and the check endpoint API is stable

### Exit Criteria

- Policy Scout JSONL watcher adapter is registered in sourceAdapterRegistry and passes its own E2E test
- At least one live audit event is visible as a graph node in LumaWeave within 5 seconds of being written to the JSONL file
- `transport:"live"` path is exercised by at least one Playwright test (the test writes an event, asserts the node appears)
- `risk_score` attribute drives gwells repulsion: a Playwright test or manual verification confirms that nodes with `risk_score > 0.7` have larger repulsion radius than baseline nodes
- PreToolUse hook fires before Claude Code tool executions in the Lattica agent environment (manual verification: run a test agent operation, confirm audit log shows the pre-check entry)
- `mcp policy_scout_check` returns a typed response (allow/deny/warn + rationale) in under 200ms for a simple shell command check
- Live audit panel renders and updates in real time (manual verification)
- `tsc --noEmit` clean, all existing Playwright tests pass, new adapter test is green

### Key Risks

- **JSONL watcher reliability.** File watchers (fs.watch, inotify) have platform-specific quirks — event debouncing, missed renames if the JSONL is written atomically, inotify limits. The adapter must be robust to these.
- **DAG inference from request_id chains.** The plan is to infer parent-child relationships from request_id structure. If Policy Scout's actual request_id format doesn't encode parentage, the DAG construction logic needs a different approach.
- **gwells physics modifier.** Modifying gwells force parameters from live node attributes is a new capability. The gwells architecture may need a new extension point; this could surface as a larger change than anticipated.
- **PreToolUse hook deployment.** The hook must be in place in the developer's Claude Code environment. If the hook registration mechanism changes in a future Claude Code version, the hook silently stops firing.
- **Policy Scout API stability.** Policy Scout is an in-house project. If its check endpoint API changes between Phase 0 and Phase 2, the MCP tool needs updating.

### Confidence Rationale

HIGH. Policy Scout is a known, running system. The JSONL watcher pattern is straightforward. The novel elements (live source adapter, gwells risk-score modifier) are architectural firsts but are well-defined design problems within known constraints. The PreToolUse hook is the most operationally uncertain piece — depends on Claude Code hook mechanics that are external to the codebase.

---

## Phase 3 — Memory Foundation / Cerebra [HIGH]

### Goals

Establish Cerebra as a data source within Lattica and make its health visible. This phase focuses on two streams of work that can proceed partly in parallel: (1) populating Cerebra's vault via Discord ingest and surfacing Cerebra metrics through Prometheus, and (2) extracting eval-core from Policy Scout into a standalone shared package and gating Cerebra retrieval quality with it.

The Discord ingest adapter is explicitly separate from the bot swap (Phase 9). Its sole purpose here is vault population — getting content into Cerebra so that later phases have real data to visualize and reason over.

### Deliverables

- Discord ingest adapter for Cerebra: a Python script (or small service) that reads Discord channel history via the Discord API and ingests it into Cerebra's vault. Not a bot — a one-time or scheduled ingest pipeline. Handles deduplication (skips messages already ingested by `message_id`).
- Cerebra metrics CLI: `cerebra metrics --format prometheus` command that outputs Prometheus textfile format. Covers: vault document count, embedding count, last-ingest timestamp, retrieval trace count, mean retrieval score, top retrieval score.
- Prometheus textfile integration: Cerebra metrics are picked up by the same node-exporter textfile path established in Phase 1. Cerebra metrics appear in Grafana alongside ai-stack metrics.
- `top_score` and `mean_score` fields added to Cerebra's `retrieval_traces` table (or derived view). These are required by eval-core's regression baseline for retrieval quality CI.
- eval-core extraction: `lattica/eval-core/` package created. Lifted from Policy Scout with zero modification to logic. `stdlib only`, zero runtime deps (ADR-003). Package has its own `pyproject.toml`, is declared in uv workspace, and is importable as `from lattica.eval_core import ...` from any Python package in the monorepo.
- Cerebra retrieval quality CI: a test suite in `lattica/cerebra/tests/eval/` that uses eval-core to run retrieval quality checks. A baseline is established. CI fails if `mean_score` drops below the baseline by more than a configurable threshold.
- Cerebra status panel: tile section in Lattica showing vault document count, embedding count, last-ingest timestamp, and current mean retrieval score. Reads via Tauri IPC command (shell-out to `cerebra metrics` — ADR-005 Phase 0 approach, socket comes in Phase 7).

### Entry Criteria

- Phase 0 complete (monorepo with uv workspace structure)
- Cerebra is running on the developer's machine with a functioning SQLite database
- eval-core source exists in Policy Scout and is in a state suitable for extraction (no Policy Scout-specific logic entangled with the core evaluation primitives)
- Discord API credentials available for ingest adapter

### Exit Criteria

- Discord ingest adapter successfully ingests at least 100 messages into Cerebra's vault without duplicates on a second run
- `cerebra metrics --format prometheus` outputs valid Prometheus textfile format with all six specified metrics
- Cerebra metrics appear as non-null data points in Prometheus within 60 seconds of `cerebra metrics` being run by the textfile poller
- `lattica/eval-core/` passes `uv run pytest` with no external dependencies (verify: `pip install lattica-eval-core` in a fresh venv, import succeeds, run tests pass)
- Cerebra retrieval CI test suite establishes a baseline and a deliberate score degradation (e.g., emptying the vault) causes CI to fail
- `top_score` and `mean_score` are present in retrieval_traces and correctly populated for new retrieval operations
- Cerebra status panel renders in Lattica and shows current metrics (manual verification)
- `tsc --noEmit` clean, all existing Playwright tests pass

### Key Risks

- **eval-core entanglement.** The extraction assumes eval-core logic in Policy Scout is cleanly separable. If Policy Scout's eval logic imports Policy Scout internals, the extraction requires more refactoring than anticipated — potentially touching Policy Scout in ways that need their own testing.
- **Cerebra SQLite WAL + concurrent access.** The Discord ingest adapter and `cerebra metrics` CLI both open the SQLite database. WAL mode handles concurrent readers well but writer contention during ingest could cause locking issues if the CLI is called during a large ingest run.
- **Discord API rate limits.** Backfilling channel history is a rate-limited operation. The ingest adapter must implement proper rate-limit handling (backoff, retry with jitter) or a large backfill will fail partway through.
- **retrieval_traces schema change.** Adding `top_score` / `mean_score` to retrieval_traces requires a schema migration. If Cerebra is in active use, this migration must not corrupt existing trace records.
- **Cerebra "Phase 5" dependency.** Several later phases reference "Cerebra Phase 5" as a prerequisite (working memory slots, session API). This document treats Cerebra's own development phases as parallel work. Phase 3 Lattica work depends only on Cerebra's current stable API — `cerebra context --format json` and `cerebra metrics`.

### Confidence Rationale

HIGH. All components are running and their APIs are known. The eval-core extraction is mechanical. The Prometheus integration follows the same pattern established in Phase 1. The main risk — eval-core entanglement — is a scope problem, not a design problem; it either is or isn't entangled, and that's verifiable before the phase starts.

---

## Phase 4 — Visualization Bridge [MEDIUM]

### Goals

Make Cerebra's knowledge graph visible in LumaWeave. This is the first phase where LumaWeave renders data it didn't generate itself — a real-world knowledge graph from Cerebra's SQLite, not a synthetic fixture. The `database-schema` source adapter slot (declared in the registry, loader empty since day one) gets its first real implementation.

This phase also delivers the System Overview tile — a Zustand slice (`metricsStore.ts`) that aggregates health signals from ai-stack (Phase 1), Policy Scout (Phase 2), and Cerebra (Phase 3) into a single platform status view.

### Deliverables

- Cerebra SQLite → LumaWeave graph adapter: implements the `database-schema` source adapter slot. Connects to Cerebra's SQLite (read-only, WAL mode), queries nodes and edges from the knowledge graph tables, emits a graph in LumaWeave's internal format. Registered as `cerebra-knowledge-graph` adapter.
- SKU D1 quadrant as gwells cluster seed: the D1 quadrant (whatever Cerebra's primary document classification is) is used as a seed for gwells' radial-backbone layout. Documents in D1 cluster at the center; other document types orbit by semantic distance.
- Retrieval trace as subgraph overlay: when a Cerebra retrieval trace is selected in the audit panel, the nodes involved in that retrieval are highlighted as a subgraph overlay in LumaWeave (color + edge weight change, not a separate graph).
- `metricsStore.ts` Zustand slice: aggregates ai-stack GPU metrics, Policy Scout audit summary, and Cerebra vault health into a single store. Follows existing Zustand patterns in the codebase (versioned migrations, getState() pattern for callbacks).
- Extended `gwellsProbe`: `window.__lwGetGwellsState()` extended to expose current cluster seed configuration and active overlay ID, to support debugging the Cerebra integration.
- System Overview tile section: registered in tileSectionRegistry, renders a compact platform health dashboard (GPU VRAM, recent policy violations, vault document count, last retrieval score). Reads from `metricsStore`.
- LumaWeave fully declared as Lattica's graph module: `moduleRegistry.register('graph', LumaWeaveModule)` — the graph module is the first fully registered module in the platform registry.

### Entry Criteria

- Phase 1 complete (ai-stack metrics in Prometheus)
- Phase 2 complete (Policy Scout integrated, live adapter path working)
- Phase 3 complete (Cerebra metrics in Prometheus, eval-core extracted)
- Cerebra's SQLite schema for the knowledge graph is stable enough to build a read adapter against (schema changes during this phase would require adapter updates)

### Exit Criteria

- `cerebra-knowledge-graph` adapter is registered in sourceAdapterRegistry and loadable from the GraphSources tile section
- Loading the adapter renders a non-empty graph in LumaWeave from Cerebra's actual SQLite data (at least 10 nodes visible)
- D1 quadrant documents appear as a distinct gwells cluster (manual verification with `npm run dev`)
- Selecting a retrieval trace in the audit panel highlights the correct nodes in the graph (Playwright test or manual verification)
- `metricsStore.ts` is tested: unit tests cover slice initialization, update, and migration
- System Overview tile section renders with live data for all three modules (manual verification)
- `moduleRegistry.get('graph')` returns the LumaWeave module descriptor
- `tsc --noEmit` clean, all existing Playwright tests pass, new unit tests green

### Key Risks

- **Cerebra SQLite schema instability.** Cerebra is in active development. If its knowledge graph table schema changes between Phase 3 completion and Phase 4 implementation, the adapter must be updated. The adapter should be written against a versioned schema snapshot and include a schema version check at startup.
- **Graph size.** Cerebra's knowledge graph may be large (thousands of nodes). LumaWeave + Sigma.js can handle large graphs, but the initial load may be slow. The adapter may need a pagination or sampling strategy for the initial view.
- **gwells cluster seed API.** Using the D1 quadrant as a gwells cluster seed may require a new extension point in gwells (similar to the risk-score modifier in Phase 2). The gwells API may need a `setSeed(nodeId, weight)` function that doesn't exist yet.
- **Retrieval trace subgraph overlay.** LumaWeave doesn't currently have a "subgraph overlay" concept — highlighted subsets of the full graph. This is a new rendering mode. The implementation scope may be larger than it appears.
- **metricsStore migration.** Adding a new Zustand slice that aggregates data from multiple sources requires careful migration versioning. A schema mismatch in the persisted store can cause silent failures on first load after update.

### Confidence Rationale

MEDIUM. The component pieces are known, but this phase assembles them in ways that surface new requirements in each: gwells needs new extension points, LumaWeave needs a new rendering mode, Cerebra's schema must be stable. Any one of these could require a design conversation mid-phase. The phase is achievable but has more surface area than Phases 1–3.

---

## Phase 5 — Reflective Twin v1 [MEDIUM]

### Goals

Deliver the first live, dual-mode view of Cerebra's knowledge graph: Graph A (canonical snapshot) alongside Graph B (live state, polling-based). This is the inaugural implementation of the "Reflective Twin Architecture" — the platform's defining concept. In Phase 5, "live" means polling (SQLite WAL reads on a configurable interval); the event-fabric-driven real-time path comes in Phase 6.

This phase depends on Cerebra's own Phase 5 being complete — specifically, working memory slots (transient contextual nodes that Cerebra maintains during active reasoning sessions) must exist and be queryable.

### Deliverables

- b-mode (live) graph view in LumaWeave: a toggle or mode selector that switches between snapshot mode (Graph A, last known coherent state) and live mode (Graph B, polling Cerebra SQLite WAL at a configurable interval, default 5s).
- Working memory slot nodes: Cerebra's working memory slots appear as a distinct node type in Graph B. Visual treatment: different shape or color, labeled with slot key, showing current slot value as tooltip.
- Retrieval trace subgraph overlays (extended from Phase 4): in live mode, the most recent retrieval trace is automatically highlighted. Older traces fade out over time. The overlay history is configurable (default: last 5 traces).
- Index staleness indicators: nodes in Graph B show a staleness indicator (visual — muted color, clock icon) when their backing Cerebra entry has not been updated within a configurable threshold (default: 24h). Staleness threshold is a gwells physics parameter (stale nodes drift outward from the cluster center).
- Discord conversation threads as graph nodes: Discord thread objects (thread_id, channel, participant list, message count, last_activity) from Cerebra's vault appear as a node type in Graph B. Edges connect threads to the knowledge nodes they reference.
- Snapshot export: a UI action in LumaWeave that freezes the current Graph B state as a named Graph A snapshot (written to a versioned JSON file). This is the mechanism by which "last known coherent state" is established.
- Polling configuration: interval and staleness threshold configurable from the Appearance or GraphSources tile section (specific section TBD).

### Entry Criteria

- Phase 4 complete (Cerebra SQLite adapter working, metricsStore established)
- Cerebra Phase 5 complete externally: working memory slots must be queryable via `cerebra context --format json` or equivalent CLI output. If Cerebra Phase 5 is delayed, this Lattica phase cannot complete — it can be partially implemented (dual-mode view without working memory nodes) but cannot exit.
- Discord thread data is present in Cerebra's vault (seeded by Phase 3 ingest adapter)

### Exit Criteria

- Graph A / Graph B toggle renders two distinct views of the same underlying Cerebra data
- In Graph B, changing a Cerebra knowledge node (via CLI or direct SQLite write) causes the corresponding LumaWeave node to update within 2 polling intervals
- Working memory slot nodes appear in Graph B when Cerebra has active working memory (manual verification: run `cerebra context` with an active session, confirm nodes appear)
- At least 3 staleness indicators are visible in a graph with data older than the staleness threshold (manual verification)
- Discord thread nodes appear in Graph B and are connected to at least one knowledge node (requires Phase 3 Discord ingest data)
- Snapshot export produces a valid JSON file that can be loaded as Graph A without modification
- `tsc --noEmit` clean, all existing Playwright tests pass
- Manual verification checklist completed by developer (dual-mode toggle, staleness indicators, thread nodes, snapshot export)

### Key Risks

- **Cerebra Phase 5 delay.** This is the most significant risk. If Cerebra's working memory implementation slips, Lattica Phase 5 cannot fully exit. Mitigation: implement the dual-mode view and staleness indicators first (no Cerebra Phase 5 dependency), defer working memory nodes to a Phase 5b or roll them into Phase 7.
- **SQLite WAL polling contention.** Polling Cerebra's SQLite at 5-second intervals from LumaWeave (via Tauri IPC shell-out to `cerebra context`) means Cerebra's CLI is invoked frequently. If Cerebra has a slow startup time (10s cold-start noted in ADR-005), polling will be unusable at 5s intervals until Phase 7's daemon is in place.
- **Graph diffing.** Switching between Graph A and Graph B requires diffing the two graph states to animate transitions. A naive approach (redraw entire graph) will be jarring. The phase needs a simple graph diff algorithm (added nodes, removed nodes, changed attributes) to drive incremental updates.
- **Snapshot format versioning.** The snapshot JSON format needs to be versioned from day one. A snapshot created in Phase 5 must still be loadable in Phase 12. The format should include a schema version field.
- **Working memory node churn.** Working memory slots may change rapidly during active sessions. If slots are created and destroyed faster than the polling interval, the graph will appear unstable. A minimum display duration (e.g., show a slot for at least 10s even if Cerebra has evicted it) may be needed.

### Confidence Rationale

MEDIUM. The dual-mode view is well-designed and the polling approach is deliberately simple. But this phase has two hard external dependencies (Cerebra Phase 5, and the cold-start problem that Phase 7 will fix). The phase is achievable, but its exit criteria may need to be phased if Cerebra Phase 5 is delayed. The graph diffing problem is underspecified and may surface as a larger-than-expected implementation task.

---

## Phase 6 — Event Fabric / ES Toolkit [MEDIUM-LOW]

### Goals

Build the event sourcing infrastructure that underpins the Reflective Twin's diff layer — the semantic event fabric that distinguishes "Agent investigating node X" from "file X was written." This is the most architecturally novel phase in the plan. The ES toolkit must be built from scratch in three languages (Rust core, PyO3 Python bindings, napi-rs TypeScript bindings), and every module must be modified to emit events through it.

The polling-based live view from Phase 5 is replaced (or augmented) by event-driven updates. The time-travel viewer makes the entire event history scrubbable. The diff layer between Graph A and Graph B becomes a semantic event stream, not a file diff.

This phase is MEDIUM-LOW confidence because: (1) the ES toolkit is a greenfield build with language-boundary complexity, (2) the agent trace adapter requires integrating with external agent runtimes whose event schemas are not fully specified, and (3) the full scope of "every module emits events" expands with each module that is added.

### ES Toolkit Architecture (Deep Specification)

The toolkit is a SQLite-backed event store with the following properties:

**Schema (3 tables):**
- `events`: `(id TEXT PRIMARY KEY, stream_id TEXT, branch TEXT, version INTEGER, type TEXT, type_version INTEGER, payload BLOB, timestamp INTEGER, causation_id TEXT)`. `id` is blake3(type || payload || causation_id) — content-addressed, deterministic, collision-resistant. `payload` is msgpack-encoded. `branch` defaults to `"main"`. `version` is per-stream monotonically increasing.
- `snapshots`: `(stream_id TEXT, branch TEXT, version INTEGER, state BLOB, timestamp INTEGER)`. Optimization only — correctness never depends on snapshots. Created every N events (configurable, default 50). Reducer must produce the same result whether snapshots are used or not.
- `branches`: `(id TEXT PRIMARY KEY, parent_id TEXT, parent_version INTEGER)`. Branches share event storage with no copying. A branch starts at `parent_version` of `parent_id` and appends new events. Reading a branch replays from the branch point.

**Invariants enforced at the type level:**
- Reducers are pure and synchronous: `(State, Event) -> State`. No I/O. No async. No promises. If a reducer needs I/O, it must happen before or after — not inside.
- Snapshots are optimization only: the type system must make it impossible to write a reducer that produces different results with vs. without snapshots.
- Append is the only write operation: no updates, no deletes, no compaction (except explicit archival, out of Phase 6 scope).

**Reactive subscriptions:**
- TypeScript: `store.subscribe(streamId, handler)` — handler fires synchronously on every append to that stream. Handler receives the new event and current state.
- Python: context manager `with store.subscribe(stream_id) as events:` — async generator. Also a callback form for non-async usage.
- Rust: `store.subscribe(stream_id, |event, state| { ... })` — callback registered with the store.

**Content-addressed IDs:**
- `id = blake3(type_bytes || payload_bytes || causation_id_bytes)`. This makes event IDs deterministic: the same event submitted twice with the same causation_id produces the same ID, enabling idempotent replay.

**Branch mechanics:**
- Creating a branch: `store.branch(stream_id, from_version, new_branch_id)`. No data is copied.
- Reading a branch: replay events from `main` (or parent branch) up to `from_version`, then replay branch-specific events.
- Merging branches: not supported in Phase 6. Branches are append-only forks. CRDT sync via Loro is a Phase 2 add-on for future multi-device use.

### Deliverables

**ES Toolkit core (`lattica/es-toolkit/`):**
- `lattica-es-core` Rust crate: SQLite backend (rusqlite), blake3 IDs, 3-table schema, append, read, subscribe (callback), snapshot (create + read), branch (create + read). Full test suite.
- `lattica-es-python` package (PyO3 bindings): Python-idiomatic API (context managers, async generators, dataclasses for event types). Matches Rust semantics exactly. Test suite covering round-trip: write in Python, read in Rust; write in Rust, read in Python.
- `lattica-es-ts` package (napi-rs bindings): TypeScript-idiomatic API (promises, `AsyncIterable` subscriptions, typed event schemas). Type definitions generated from Rust types. Test suite covering round-trip with Python bindings.
- Reducer type enforcement: in TypeScript, `Reducer<State, Event>` type alias that makes async reducers a compile error. In Python, a runtime assertion that reducers return synchronously (no coroutine return). In Rust, enforced by trait bound.

**Cross-language read adapters:**
- `CerebraEventAdapter`: reads Cerebra's existing `inspector_events` table, wraps rows as ES toolkit `Event` objects with standard types, emits to a `cerebra:inspector` stream. Does NOT modify Cerebra's schema — read-only bridge.
- `PolicyScoutEventAdapter`: reads Policy Scout's `audit.db`, wraps audit records as ES toolkit events with standard types (`policy_check_requested`, `policy_decision_made`, `policy_violation_detected`), emits to `policy-scout:audit` stream. Read-only bridge.

**Agent trace adapter:**
- Standard event types defined: `llm_call` (model, prompt_tokens, completion_tokens, latency_ms), `tool_call` (tool_name, input_schema_hash, input_preview), `tool_result` (tool_name, output_preview, success), `reasoning_step` (step_type, content_preview).
- OTel GenAI span export: event streams with agent trace events can be exported as OpenTelemetry GenAI spans (OTLP/HTTP). The exporter is a separate process/script, not part of the core toolkit.
- Claude Code hook integration: a PreToolUse hook that emits `tool_call` events to the agent trace stream before each tool execution. Complements (not replaces) the Policy Scout hook from Phase 2.

**Module event emission:**
- LumaWeave: emits `graph_node_added`, `graph_node_removed`, `graph_edge_added`, `graph_edge_removed`, `source_adapter_loaded`, `layout_changed` events to a `lumaweave:graph` stream via the TypeScript bindings.
- Cerebra: emits `knowledge_ingested`, `retrieval_executed`, `working_memory_updated` events (requires Cerebra code changes — coordinated with Cerebra development).
- Policy Scout: emits `policy_check_requested`, `policy_decision_made` events (new emission; the existing audit.db is read-only bridged separately).
- ai-stack (Ollama poller): emits `model_loaded`, `model_unloaded`, `vram_pressure_detected` events.

**Time-travel viewer:**
- Scrubbable HTML canvas UI embedded in LumaWeave as a tile section (`EventTimelinePanel`).
- Timeline shows all streams as horizontal tracks. Events are dots on the track. Scrubbing to a point in time replays the graph state to that point.
- Diff view: selecting two events shows a semantic diff (added nodes/edges, changed attributes, policy decisions made, agent actions taken) — not a text diff.
- Filter controls: by stream, by event type, by time range.

**Diff layer:**
- Graph A (canonical snapshot from Phase 5) and Graph B (live state) now differ via the event stream, not polling.
- Graph A is the state at the last `snapshot_exported` event in the `lumaweave:graph` stream.
- Graph B is the current head of the stream.
- The diff between A and B is rendered in the time-travel viewer as the event sequence between the snapshot event and head.

**Cross-module event timeline in Lattica:**
- A unified timeline view in the Lattica System Overview tile (extended from Phase 4) showing events from all streams in chronological order. Clicking an event navigates to the relevant module panel.

### Entry Criteria

- Phases 1–5 complete (all modules integrated, dual-mode graph view working)
- Rust toolchain installed and building correctly in the monorepo (Phase 0 establishes this)
- napi-rs and PyO3 build tooling confirmed working in monorepo (may require Phase 0 follow-up work)
- Cerebra and Policy Scout APIs stable enough that cross-language read adapters can be written without chasing moving targets

### Exit Criteria

- `lattica-es-core` crate: `cargo test` green, all three tables created on init, append + read + subscribe + branch round-trip tested
- `lattica-es-python`: `uv run pytest` green, cross-language round-trip test passes (Python writes, Rust reads, Python reads what Rust wrote)
- `lattica-es-ts`: `pnpm test` green, TypeScript round-trip test passes, async `Reducer` type rejects async functions at compile time (`tsc --noEmit` fails on an intentionally async reducer)
- `CerebraEventAdapter` reads Cerebra's `inspector_events` and emits ≥ 1 event to the `cerebra:inspector` stream (requires Cerebra to have at least one inspector event)
- `PolicyScoutEventAdapter` reads Policy Scout's `audit.db` and emits ≥ 1 event to the `policy-scout:audit` stream
- LumaWeave emits `graph_node_added` events to `lumaweave:graph` stream: adding a node in LumaWeave causes an event to appear in the stream (Playwright test)
- Agent trace PreToolUse hook emits `tool_call` events: running a Claude Code tool call produces a corresponding event in the `agent-trace` stream (manual verification)
- Time-travel viewer renders and scrubbing replays graph state (manual verification)
- Diff view shows semantic events between two selected points in time (manual verification)
- `tsc --noEmit` clean across all TypeScript packages, `cargo build` clean, `uv run pytest` green

### Key Risks

- **napi-rs / PyO3 build complexity.** Cross-language bindings require native compilation. In CI and on the developer's machine, this is manageable. But the build toolchain setup (napi-rs CLI, PyO3 maturin, cross-compilation targets) has significant friction. The monorepo must be configured carefully to avoid build cache conflicts between Rust crates.
- **SQLite contention across languages.** Multiple processes (LumaWeave via napi-rs, Cerebra via PyO3, Policy Scout via PyO3) may open the same SQLite file simultaneously. WAL mode handles concurrent readers, but write serialization is critical. The toolkit must enforce single-writer semantics clearly.
- **Agent trace schema incompleteness.** The standard event types (`llm_call`, `tool_call`, etc.) are defined here, but real agent runtimes may emit information that doesn't fit the schema. The agent trace adapter needs a `metadata` escape hatch (msgpack blob) for untyped extension fields.
- **Time-travel viewer scope.** An HTML canvas scrubbable timeline is a non-trivial UI component. The implementation may take longer than anticipated. A fallback (table-based event log with a manual "replay to this point" button) should be defined as the minimum viable version.
- **Cerebra/Policy Scout code changes required.** Getting Cerebra and Policy Scout to emit events requires changes to those codebases. This is coordinated work — the ES toolkit API must be stable before those changes are made, or the implementations will need to be redone.
- **Event volume.** In a production-like scenario, LumaWeave alone might emit hundreds of events per second (every node/edge change). The time-travel viewer must handle high event volumes without performance degradation. A throttle or batch-write strategy may be needed.
- **Blake3 content-addressing edge cases.** The content-addressed ID scheme means that two events with the same type, payload, and causation_id produce the same ID. This is intentional for idempotent replay but could cause confusion if the same action is genuinely repeated (e.g., adding the same node twice). The append operation must handle ID collisions explicitly (either reject or treat as idempotent).

### Confidence Rationale

MEDIUM-LOW. This is the most architecturally novel phase. The ES toolkit core (SQLite + append + read + branch) is well-understood engineering. But the language binding build complexity, the cross-language contention model, the time-travel viewer scope, and the dependency on external codebase changes (Cerebra, Policy Scout) all introduce meaningful uncertainty. The approach is sound, but the phase is likely to surface design questions that require mid-phase decisions. The "read adapter" strategy for existing event stores (bridging rather than replacing) is the right call to avoid disruption, but the bridge implementations depend on schema details that may not be fully known until implementation begins.

What would raise confidence: completing the napi-rs and PyO3 build setup as a Phase 6 spike before the phase formally begins. What would lower confidence further: discovering that Cerebra's `inspector_events` schema doesn't map cleanly to the standard event types.

---

## Phase 7 — Cerebra Daemon [HIGH]

### Goals

Eliminate the 10-second cold-start penalty that makes Cerebra polling impractical in Phase 5. The `cerebra serve` daemon loads the embedding model once and keeps it resident, reducing response latency from ~10s to sub-100ms. LumaWeave and Lattica migrate from shell-out (ADR-005 Phase 0 approach) to Unix domain socket (ADR-005 Phase 7 approach).

This phase has high confidence because the design is fully specified in ADR-005 and the work is straightforward FastAPI + Unix socket engineering.

### Deliverables

- `cerebra serve` command: starts a FastAPI server on a Unix domain socket at `~/.cerebra/sockets/cerebra.sock`. Loads embedding model (mxbai-embed-large-v1) on startup, keeps it resident. Exposes HTTP endpoints over the socket: `POST /context`, `POST /retrieve`, `GET /metrics`, `GET /health`.
- Tauri IPC migration: `get_cerebra_context` and `get_cerebra_metrics` Rust commands migrated from `Command::new("cerebra")` shell-out to Unix socket HTTP calls via `hyper` or `ureq` over `UnixStream`.
- CLI thin client: `cerebra context` and `cerebra metrics` CLI commands become thin clients that connect to the daemon socket if running, fall back to direct execution if not running. Backward compatibility preserved.
- Daemon lifecycle management: `cerebra serve --daemon` flag to daemonize (writes PID file to `~/.cerebra/cerebra.pid`). `cerebra stop` sends SIGTERM. Tauri app launches the daemon on startup if not already running (checks socket path, starts if absent).
- Health check and reconnect: Lattica monitors daemon health via `GET /health` on the socket. If the daemon is not responding, Lattica falls back to shell-out and shows a "Daemon offline" indicator in the Cerebra status panel.
- Performance validation: `POST /context` response time measured and logged. Exit criterion requires sub-100ms for a warm request.

### Entry Criteria

- Phase 5 complete (LumaWeave is using shell-out to Cerebra, working memory nodes in graph)
- Cerebra Phase 5 complete (working memory slots, session API — required for the daemon to expose a session endpoint)
- `~/.cerebra/` directory structure agreed and documented

### Exit Criteria

- `cerebra serve` starts without errors and creates the socket at `~/.cerebra/sockets/cerebra.sock`
- `POST /context` on the warm daemon returns in under 100ms (measured with `time curl --unix-socket` or equivalent, averaged over 10 requests)
- LumaWeave's Cerebra status panel shows data without the 10-second delay experienced in Phase 5
- CLI backward compatibility: `cerebra context --format json` returns the same output whether the daemon is running or not
- Daemon lifecycle: Tauri app starts daemon on launch (if not running), daemon survives Tauri app restart (PID file check), `cerebra stop` terminates the daemon cleanly
- Health check fallback: manually killing the daemon causes Lattica to show "Daemon offline" and fall back to shell-out within one polling interval
- `tsc --noEmit` clean, all existing Playwright tests pass

### Key Risks

- **FastAPI on Unix domain socket.** FastAPI supports UDS via `uvicorn` with a `--uds` flag. This is a supported but less-common deployment mode; there may be edge cases with request timeout handling or connection pooling.
- **Rust hyper/ureq over UnixStream.** HTTP over Unix domain sockets from Rust requires explicit socket path configuration. The `ureq` crate has simpler UDS support than `hyper`; the choice of HTTP client in the Tauri backend should be evaluated at phase start.
- **Daemon startup race.** Tauri app launches the daemon and immediately tries to use it. The daemon must be ready before the first request. A simple retry loop with backoff is sufficient, but must not block the Tauri app's main thread.
- **Model loading time.** Even if the daemon is warm, the initial startup (loading mxbai-embed-large-v1) takes some time. The Cerebra status panel should show a "loading" state during daemon startup rather than erroring.

### Confidence Rationale

HIGH. The design is fully specified in ADR-005. FastAPI + Unix domain socket is a well-understood pattern. The Rust UDS HTTP client is the most uncertain element, but there are multiple library options and the problem is bounded. The 10-second cold-start problem is real and measured; the daemon approach directly addresses it.

---

## Phase 8 — Agent Runtime / Rhyzome + bons.ai [MEDIUM]

### Goals

Make agent activity visible in the graph and place it under governance. Rhyzome (code repair) and bons.ai (multi-agent cognitive system) are both running agents that make file-system changes. This phase wires them into the platform: they emit structured events (via Phase 6's ES toolkit), appear as visually active entities in LumaWeave, and have their file-system side effects gated by Policy Scout's HITL mechanism.

The result is the first instance of "agents visually active in the graph" — the long-term vision stated in the platform overview. Files under investigation are highlighted. Repair strategy nodes appear and dissolve. Policy Scout gate edges constrain agent traversal.

### Deliverables

- Rhyzome structured event emission: Rhyzome emits `file_inspected`, `repair_attempted`, `strategy_selected`, `outcome` events to its ES toolkit stream (`rhyzome:repair`). This requires changes to the Rhyzome codebase.
- Rhyzome in LumaWeave: files under Rhyzome investigation appear as highlighted nodes (pulsing border or color change, configurable via theme tokens). Repair strategy nodes appear as temporary nodes connected to the file node. Outcome nodes (success/failure) appear with appropriate visual treatment.
- Policy Scout gate edges: when a Rhyzome repair action hits a Policy Scout HITL gate, a visual edge appears between the repair strategy node and a Policy Scout gate node. The gate node shows the pending decision. On approval, the edge changes style (approved). On rejection, the repair strategy node dims.
- bons.ai three-agent cycle in graph: generator, evaluator, and mutator agents appear as persistent nodes in LumaWeave. During active cycles, edges appear between them showing the current cycle direction. Mutation genealogy is tracked via ES toolkit `Mutated` events.
- HITL gate for agent file-system side effects: all agent file-system write operations (Rhyzome repairs, bons.ai mutations) are gated by Policy Scout's HITL mechanism before execution. This is the first use of the PreToolUse hook from Phase 2 for agent-originated actions.
- Lattica agent dispatch: a panel in Lattica that shows active agents (Rhyzome, bons.ai), their current status, and allows the developer to trigger a Rhyzome repair run or a bons.ai cycle from the UI.
- Agent trace ES streams: both Rhyzome and bons.ai agent traces (from Phase 6's agent trace adapter) are visible in the time-travel viewer.

### Entry Criteria

- Phase 6 complete (ES toolkit built, agent trace adapter working)
- Phase 2 complete (Policy Scout HITL gate implemented)
- Rhyzome is in a runnable state on the developer's machine (87.5% benchmark pass rate confirmed)
- bons.ai three-agent loop is running and can be triggered programmatically

### Exit Criteria

- Rhyzome emitting events: starting a Rhyzome repair session causes ≥ 3 events to appear in the `rhyzome:repair` ES stream (manual verification with time-travel viewer)
- LumaWeave shows Rhyzome activity: at least one file node is highlighted as "under investigation" during an active Rhyzome session (manual verification)
- Policy Scout gate edges appear when a Rhyzome action hits a HITL gate (manual verification: trigger a repair that would modify a file, confirm gate edge appears before action proceeds)
- bons.ai cycle visible: starting a bons.ai cycle causes generator → evaluator → mutator edges to appear in LumaWeave (manual verification)
- Mutation genealogy: after ≥ 3 bons.ai cycles, the ES stream contains `Mutated` events with parent causation IDs forming a genealogy (verifiable via time-travel viewer)
- HITL gate blocks execution: manually denying a Policy Scout HITL gate causes the agent action to not execute (manual verification)
- Lattica agent dispatch panel renders and can trigger a Rhyzome run (manual verification)
- `tsc --noEmit` clean, all existing Playwright tests pass

### Key Risks

- **Rhyzome codebase instrumentation.** Adding ES toolkit event emission to Rhyzome requires changes to its internal structure. Rhyzome uses AST-based semantic gates; the instrumentation must not perturb its reasoning pipeline. The instrumentation should be additive (side-effecting event emission, not integrated into the repair logic).
- **bons.ai API stability.** bons.ai's three-agent loop may not have a clean programmatic API for triggering cycles from the Lattica dispatch panel. An adapter or wrapper may be needed.
- **HITL gate UX.** When a HITL gate fires, the developer must approve or deny before the agent can proceed. The current mechanism (Discord message in Phase 2) may be too slow for an interactive repair session. A Lattica in-UI approval widget may be needed — this is a larger scope addition.
- **Visual treatment coordination.** The agent visualization (pulsing borders, temporary nodes, gate edges) requires coordination between the ES event stream and LumaWeave's rendering layer. This is new territory: LumaWeave currently renders static graphs, not live agent activity. The rendering logic may be more complex than anticipated.
- **ES toolkit performance under agent event volume.** During an active Rhyzome session, events may arrive rapidly. The ES toolkit's subscription mechanism and LumaWeave's reactive rendering must handle event bursts without dropping events or causing UI jank.

### Confidence Rationale

MEDIUM. The agent integrations require changes to codebases (Rhyzome, bons.ai) that may have their own constraints. The HITL gate UX in a live repair session is underspecified and may require a scope addition. The visual treatment for agent activity is architecturally novel. Phase 6's ES toolkit is a hard prerequisite and is itself MEDIUM-LOW confidence; any delay in Phase 6 directly delays Phase 8.

---

## Phase 9 — Bo Memory Swap [MEDIUM]

### Goals

Replace Bo's (the Discord bot's) current `gather_context()` function with a Cerebra session API call. Bo currently maintains rolling conversation memory internally. After this phase, Bo's working memory is grounded in Cerebra's knowledge graph — the same graph that's visible in LumaWeave. Bo's conversations become part of the platform's memory model, not a siloed buffer.

This phase has a hard two-phase dependency: Phase 5 (Cerebra working memory slots queryable) and Phase 7 (Cerebra daemon, sub-100ms responses) must both be complete. Without Phase 7, Bo's response latency would increase by ~10 seconds per message, which is unacceptable for a chat interface.

### Deliverables

- `gather_context()` replacement: Bo's context assembly function is replaced with a call to the Cerebra daemon's `/context` endpoint (via Unix socket). The call assembles context from Cerebra's knowledge graph + working memory slots + recent retrieval traces, formatted for Bo's LiteLLM prompt.
- Working memory slot mapping: Bo's conversation state (recent messages, active topic, referenced entities) is mapped to Cerebra's working memory slot schema. At conversation start, slots are initialized. During conversation, slots are updated via Cerebra's session API.
- Context quality comparison: an eval-core test that compares context quality (using the retrieval quality metrics from Phase 3) between the old `gather_context()` output and the new Cerebra-backed output on a held-out set of historical conversations.
- Fallback path: if the Cerebra daemon is unavailable (socket connection refused), Bo falls back to the legacy `gather_context()` implementation with a logged warning. Bo never goes offline due to Cerebra unavailability.
- Response latency monitoring: the time from Discord message received to first LiteLLM token is logged for each message. A Prometheus metric (`bo_response_latency_seconds`) is added. Alert threshold: p95 > 3s.
- Cerebra session cleanup: Bo's conversation session is written to Cerebra's vault (as a Discord thread node, following the pattern from Phase 5) when a conversation ends or after a configurable idle timeout.

### Entry Criteria

- Phase 5 complete (Cerebra working memory slots implemented and queryable)
- Phase 7 complete (Cerebra daemon running, sub-100ms responses confirmed)
- Bo is running and handling Discord messages (existing functionality preserved)
- Cerebra's session API (for working memory slot initialization and update) is stable

### Exit Criteria

- Bo responds to a Discord message using Cerebra-backed context (manual verification: send a message to Bo, confirm `bo_response_latency_seconds` metric is recorded and context includes Cerebra retrieval)
- Working memory slots in Cerebra reflect Bo's current conversation state during an active session (manual verification: start a conversation with Bo, check LumaWeave's Graph B for working memory nodes)
- Fallback path works: stopping the Cerebra daemon causes Bo to fall back to legacy `gather_context()` within one request (manual verification, check logs for fallback warning)
- Response latency p95 < 3s in the Prometheus metric over a sample of ≥ 20 messages
- eval-core context quality comparison shows no regression from the legacy context (mean retrieval score ≥ baseline)
- Bo's conversation session is written to Cerebra's vault at conversation end (manual verification: end a conversation, check Cerebra vault for the Discord thread node)
- All existing Bo functionality preserved (model routing, resilience pipeline, memory capacity) — manual verification smoke test

### Key Risks

- **Working memory slot schema mismatch.** Bo's internal conversation state (rolling message buffer, active topic heuristics) may not map cleanly to Cerebra's working memory slot schema. Adaptation logic may be needed, and that adaptation may degrade context quality if done naively.
- **Response latency regression.** Even with the Phase 7 daemon, network round-trip to the Cerebra socket adds latency. If the socket is on the same machine, this should be negligible, but a slow Cerebra query (large vault, complex retrieval) could push p95 above the 3s threshold.
- **Context format incompatibility.** Cerebra's `/context` endpoint returns JSON. Bo's prompt construction expects a specific context format. The adapter must translate without losing information or inflating token count.
- **Conversation boundary detection.** Cerebra session cleanup requires detecting when a conversation "ends." Discord conversations don't have explicit end signals. An idle timeout is the fallback, but choosing the right value involves a tradeoff between Cerebra writes and stale session accumulation.

### Confidence Rationale

MEDIUM. The design is clear and the dependencies are well-understood. The main uncertainty is context quality — whether Cerebra-backed context is actually as good as or better than Bo's existing rolling memory. The eval-core quality comparison provides a safety net, but a quality regression would require a design iteration before the phase can exit. Phase 7's daemon is a hard prerequisite; any delay there delays this phase.

---

## Phase 10 — Evaluation Platform [MEDIUM]

### Goals

Make evaluation a first-class cross-module capability with a unified Lattica dashboard. Each module currently has ad-hoc quality checks (if any). This phase standardizes them all under eval-core (extracted in Phase 3), adds a Lattica dashboard showing pass rates and regression trends per module, and introduces cross-module correlation analysis (e.g., does a drop in Cerebra retrieval quality correlate with lower bons.ai mutation pass rates?).

LoRA training quality (from Cerebra's local training pipeline) is tracked for the first time, and model comparison tooling is added to the ai-stack panel.

### Deliverables

- eval-core adoption across all modules: Policy Scout, Cerebra, Rhyzome, bons.ai, and Bo each have a `tests/eval/` directory using eval-core. Each module's CI runs these tests and records pass rate + regression baseline.
- Lattica evaluation dashboard: a tile section (`EvalDashboard`) showing per-module pass rates, trend charts (7-day rolling average), and regression alerts. Reads from a shared `eval_results.db` SQLite file (written by each module's eval test suite, read by Lattica).
- LoRA training quality tracking: Cerebra's LoRA training runs write epoch loss, validation metrics, and dataset fingerprint to `eval_results.db` via eval-core. The Lattica dashboard shows training quality trends.
- Model comparison tooling: the ai-stack panel (Phase 1) is extended with a model comparison view — two models, same prompt, side-by-side outputs, token counts, latency. Comparison results are logged to `eval_results.db`.
- Cross-module correlation layer: a background job (runs nightly or on-demand) that computes pairwise correlations between module eval metrics. Output is a heatmap in the Lattica dashboard. Alerts if a correlation drops below a configurable threshold (e.g., Cerebra retrieval quality and Bo response quality are expected to be correlated; a decorrelation may indicate a Cerebra regression not caught by Cerebra's own eval).
- Regression gate in monorepo CI: a root-level CI step that reads `eval_results.db` and fails the build if any module's pass rate has dropped by more than a configurable threshold from its 7-day average.

### Entry Criteria

- Phase 3 complete (eval-core extracted as standalone package)
- Phase 8 complete (all modules producing ES events, agent activity visible)
- Phase 9 complete (Bo using Cerebra memory, Bo eval baseline established)
- All modules are instrumented enough to have meaningful eval metrics (pass rate, retrieval quality, repair success rate, mutation pass rate, LLM response quality)

### Exit Criteria

- All five modules (Policy Scout, Cerebra, Rhyzome, bons.ai, Bo) have a passing eval test suite using eval-core
- `eval_results.db` is populated with at least 7 days of eval results (or a seeded baseline if the platform hasn't been running that long)
- Lattica eval dashboard renders with per-module pass rates and at least one trend chart showing non-flat data
- LoRA training quality metrics appear in the dashboard (requires at least one training run)
- Model comparison tooling works: comparing two models on a sample prompt produces a logged result (manual verification)
- Cross-module correlation heatmap renders (even if all correlations are near 1.0 due to limited data)
- Root CI regression gate: deliberately degrading one module's eval metrics causes the root CI step to fail (test this in a branch before merging)
- `tsc --noEmit` clean, all existing Playwright tests pass

### Key Risks

- **eval_results.db write contention.** Multiple modules writing to the same SQLite file simultaneously (especially if CI runs module evals in parallel) requires WAL mode and explicit write serialization. eval-core must provide a writer that handles concurrent access gracefully.
- **Sparse data problem.** The cross-module correlation analysis requires enough historical data to be meaningful. In the first week of operation, correlations will be noisy. The dashboard must make the data sparsity visible rather than showing misleading correlations.
- **LoRA training integration.** Cerebra's LoRA training pipeline (via Unsloth) may not have a clean hook for writing training metrics to eval-core. This may require changes to the training scripts.
- **Model comparison scope.** "Side-by-side model comparison" is an open-ended feature. The minimum viable version is a fixed prompt, two model calls, logged output — no interactive UI. A more elaborate comparison UI is out of scope for this phase.
- **Eval metric definition.** "Pass rate" means different things for different modules. Rhyzome's pass rate is repair success rate. bons.ai's is mutation acceptance rate. Bo's is context quality score. These need to be defined and documented before the eval suites are written, or the dashboard will aggregate incommensurable numbers.

### Confidence Rationale

MEDIUM. The eval-core foundation is solid (extracted in Phase 3, proven in Phase 3's Cerebra quality CI). The cross-module aggregation and correlation analysis are new but straightforward data engineering. The main uncertainty is metric definition — if the modules don't agree on what "quality" means, the dashboard will be misleading. This is a design/coordination problem, not a technical one, and it should be resolved before implementation begins.

---

## Phase 11 — GPU Infrastructure / Training Pipeline [LOW]

### Goals

Centralize GPU resource management in Lattica. The RTX 4070 SUPER is shared between Ollama (inference), Cerebra's embedding model (resident in Phase 7 daemon), and LoRA training runs (periodic, GPU-intensive). Currently there is no coordination — a training run can evict loaded inference models unexpectedly. This phase adds allocation visibility, VRAM budget tracking, contention detection, and a training job dispatch interface.

### Deliverables

- Local AI cluster manager: a Lattica module that tracks GPU allocation across all consumers (Ollama models, Cerebra daemon, active training runs). Reads VRAM usage from the Phase 1 Prometheus metrics, maintains an allocation model.
- GPU allocation visibility: the Lattica ai-stack panel (Phase 1) is extended with an allocation timeline — which consumer owned how much VRAM at each point in time. Backed by Prometheus historical data.
- VRAM budget with contention detection: a configurable VRAM budget per consumer type (inference: X GB, embedding: Y GB, training: Z GB). If a new allocation would exceed total VRAM, a contention alert fires in Lattica and (optionally) a Discord notification via Bo.
- LoRA deployment through Lattica: a UI workflow for deploying a trained LoRA adapter (select base model, select adapter file, deploy to Ollama). Replaces the current manual process.
- Training job dispatch: a Lattica panel for dispatching Cerebra LoRA training runs. Configurable hyperparameters (epochs, learning rate, dataset). Dispatched jobs write their status to `eval_results.db` (integrating with Phase 10's eval platform).
- Contention detection logic: before dispatching a training job, Lattica checks current VRAM allocation. If insufficient free VRAM, the UI shows which consumers to evict (and offers to evict them) before dispatching.

### Entry Criteria

- Phase 1 complete (VRAM metrics in Prometheus)
- Phase 7 complete (Cerebra daemon resident, known VRAM footprint)
- Phase 10 complete (training quality metrics in eval_results.db)
- Developer has at least one complete LoRA training run to use as a reference for the deployment workflow

### Exit Criteria

- GPU allocation panel renders with current VRAM usage per consumer (manual verification)
- Contention detection fires when a training job dispatch would exceed VRAM budget (manual verification: set budget below current usage, try to dispatch a training job)
- LoRA deployment workflow completes end-to-end: select adapter, deploy to Ollama, confirm model is loadable (manual verification)
- Training job dispatch initiates a real training run and records status in `eval_results.db` (manual verification)
- Allocation timeline shows historical VRAM data from Prometheus (at least 24h of data)
- `tsc --noEmit` clean, all existing Playwright tests pass

### Key Risks

- **Ollama VRAM eviction behavior.** Ollama manages VRAM internally and does not expose an explicit eviction API. "Evicting" a model means unloading it, which Ollama does via `DELETE /api/delete` or by loading a different model. The eviction logic must work within Ollama's API constraints.
- **Training job isolation.** Dispatching a training run from Lattica means Lattica is responsible for process management (start, monitor, stop). If the training process crashes or hangs, Lattica must detect it and clean up. Process supervision logic is non-trivial.
- **VRAM budget accuracy.** VRAM usage from Prometheus (via the Phase 1 poller) is sampled, not real-time. A burst allocation (e.g., Ollama loading a new model during a training run) may not be detected in time to prevent contention.
- **LoRA deployment complexity.** Deploying a LoRA adapter to Ollama requires creating a custom Modelfile. The format and options depend on the base model and adapter type. The UI must handle at least the common cases (Llama3.1, Qwen2.5-Coder base models) without requiring manual Modelfile editing.

### Confidence Rationale

LOW. This phase is more speculative than Phases 1–9 because GPU resource management is inherently dependent on Ollama's internal behavior (which is not fully under the developer's control), and the training job dispatch involves process supervision complexity that is not trivial to get right. The design is reasonable, but the implementation details are underspecified. This phase should be approached after Phase 10 is stable, and its scope may be narrowed based on what's actually needed by then.

---

## Phase 12 — Cognitive OS / Reflective Twin v2 [SPECULATIVE — Research Agenda]

### How to Read This Phase

Phase 12 is not an engineering plan. It is a research agenda — a set of hypotheses about what becomes possible once the platform is fully operational (Phases 0–11 complete). Each milestone is an experiment with a stated hypothesis and a success criterion. The experiments are ordered by dependency, but many can run concurrently.

The "Cognitive OS" framing should be treated as an aspiration, not a specification. What constitutes success for Phase 12 will be defined by what the platform reveals about itself once it can observe its own reasoning processes. ADR-008 explicitly designates Phase 12 as a research exploration with experimental milestones, not engineering deliverables.

None of the following milestones should be committed to a release schedule. They are investigations.

---

### Milestone 12-A — Epistemic Evolution Graph

**Hypothesis:** Cerebra's knowledge graph changes in measurable ways over time as new information is ingested. These changes are not random — they cluster around topics of active investigation, decay around unused domains, and exhibit patterns that correlate with the developer's actual work cadence.

**Experiment:** Instrument the Cerebra knowledge graph with version snapshots (daily, using the Phase 5 snapshot export mechanism). After 30 days of operation, analyze the snapshot sequence: which nodes changed, what was the rate of change, did change clusters correlate with git commit topics or Discord conversation topics?

**Success criterion:** At least 3 distinct epistemic "episodes" are identifiable in the snapshot sequence — periods where the graph changed significantly in a coherent direction (e.g., "week 2 was a LumaWeave refactor, Cerebra's graph shows heavy new edges around graph/registry/physics topics"). The correlation is visible without post-hoc annotation.

**What failure looks like:** The snapshot sequence shows random noise with no coherent structure. Either the graph is too small to exhibit meaningful evolution, or Cerebra's ingestion is too coarse to capture fine-grained epistemic change.

---

### Milestone 12-B — Meta-Cognitive Profiler

**Hypothesis:** The agent trace streams (from Phase 6) contain enough information to characterize the developer's problem-solving patterns — which tools are reached for first, which reasoning strategies recur, how long reasoning steps are before a tool call, what distributions of tool call depths look like across different problem types.

**Experiment:** Build a profiler that reads the `agent-trace` ES stream and computes session-level statistics: average reasoning steps per tool call, tool call distribution per problem type (inferred from the first reasoning step), strategy entropy (how often does the agent repeat the same tool sequence vs. vary it?). Run over 2 weeks of actual Claude Code sessions.

**Success criterion:** The profiler produces a per-session summary that a human reviewer (the developer) agrees is an accurate characterization of that session's problem-solving approach — not in retrospect ("of course it used more file reads in the refactor"), but prospectively enough to be useful for calibrating future agent behavior.

**What failure looks like:** The per-session statistics are too noisy or too aggregated to distinguish between sessions with different problem types. The "characterization" reads as generic rather than specific.

---

### Milestone 12-C — Teacher-Student Distillation Pipeline

**Hypothesis:** A small local model (e.g., Qwen2.5-Coder:14b, Rhyzome's primary model) can be improved on reasoning tasks by training on corrections generated by a stronger model (Claude). The correction signal is generated by presenting the local model's reasoning traces to Claude and asking it to identify and correct reasoning errors — not to provide better answers, but to label the reasoning steps that led away from the correct answer.

**Experiment:** Select 50 Rhyzome repair attempts from the ES toolkit's `rhyzome:repair` stream where Rhyzome failed (outcome = failure). For each, send the full reasoning trace (strategy_selected, repair_attempted events) to Claude with the prompt "identify the reasoning step where this went wrong and explain what correct reasoning would look like." Collect corrections. Fine-tune Qwen2.5-Coder:14b on the corrected traces using Cerebra's LoRA pipeline (Phase 11). Measure Rhyzome's pass rate before and after.

**Success criterion:** Rhyzome's pass rate on the held-out failure set improves by ≥ 5 percentage points after fine-tuning. The improvement is measured by eval-core (Phase 10) on a held-out set, not the training set.

**What failure looks like:** No improvement in pass rate, or improvement on the training set without generalization to held-out cases. Could indicate: correction signal was too noisy, fine-tuning dataset was too small (50 examples may be insufficient), or the failure mode is not in the reasoning trace (e.g., failures are due to Rhyzome's code generation quality, not its strategy selection).

---

### Milestone 12-D — Reasoning Primitive Labeling

**Hypothesis:** The reasoning steps in agent traces can be classified into a small vocabulary of primitives (e.g., "hypothesis formation," "evidence gathering," "constraint propagation," "backtracking," "pattern matching"). Once labeled, the distribution of primitives across successful vs. failed repair attempts will reveal which primitive transitions predict failure.

**Experiment:** Build a classifier that labels each `reasoning_step` event in the `agent-trace` stream with one of 8–12 primitive labels. Start with a handcrafted taxonomy (built from 20 manually labeled examples). Train a small classifier (logistic regression or a fine-tuned embedding model) on the manual labels. Apply to 6 months of agent trace history. Compute primitive transition matrices for success vs. failure cases in Rhyzome's repair history.

**Success criterion:** Inter-rater reliability between the classifier and manual labeling ≥ 0.7 (Cohen's kappa). At least one primitive transition that significantly differs (p < 0.05) between successful and failed Rhyzome repairs is identified.

**What failure looks like:** Classifier cannot generalize beyond the training set (reasoning steps are too varied or too long to classify reliably). Or no significant differences between success/failure transition matrices (the taxonomy is not discriminative).

---

### Milestone 12-E — Calibration Training

**Hypothesis:** The local models (Qwen2.5-Coder, Granite) are poorly calibrated — their expressed confidence (via logprobs or verbalized uncertainty) does not correlate with their actual accuracy. This poor calibration can be measured using the eval-core framework and can be partially corrected by fine-tuning on examples where calibration was correct.

**Experiment:** Collect model outputs from bons.ai (generator/evaluator/mutator cycle) where the evaluator expressed a confidence level. Compare expressed confidence to actual correctness (measured by eval-core pass rate for the generated output). Compute calibration curves. Fine-tune on examples where confidence was well-calibrated, using a standard calibration loss (expected calibration error as a training objective).

**Success criterion:** Expected Calibration Error (ECE) decreases by ≥ 20% relative after fine-tuning, measured on a held-out set drawn from a different time period than the training data.

**What failure looks like:** ECE is already near 0 (the model is well-calibrated and there is nothing to fix), or ECE does not improve after fine-tuning (the calibration error is not a function of training — may be an architectural property of the model or a token probability distribution property that fine-tuning cannot address).

---

### Infrastructure required for Phase 12

All Phase 12 experiments depend on:
- Phase 6 ES toolkit (agent trace streams, time-travel viewer)
- Phase 10 eval-core across all modules (measurement infrastructure)
- Phase 11 LoRA pipeline (training experiments 12-C and 12-E)
- At minimum 2 weeks of platform operation to generate sufficient trace data for analysis

Phase 12 experiments can begin as soon as sufficient data is available, without waiting for Phase 11 to be complete in all details. Milestones 12-A and 12-B can run concurrently with Phase 11.

### Overall Confidence Rationale for Phase 12

SPECULATIVE. The individual experiments are well-formed (each has a testable hypothesis and a measurable success criterion). But whether any of them will succeed is genuinely unknown. The most likely outcome is that some succeed partially, some fail in ways that suggest better experiments, and a few produce surprising results that reframe the later milestones. That is the nature of a research agenda, not a deficiency in planning.

The platform phases (0–11) are justified independently of Phase 12. Phase 12 is what the platform makes possible, not what it requires.
