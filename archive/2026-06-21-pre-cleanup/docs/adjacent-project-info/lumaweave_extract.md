# ES Consumer Profile — LumaWeave

You are the LumaWeave advocate. We're building **ES**, a local-first
event sourcing library: Rust core, PyO3/napi-rs bindings, SQLite-backed,
content-addressed events, branchable history, pure synchronous reducers.
It's being extracted from Lattica's Phase 6 plans because LumaWeave and
several other projects also want event sourcing capabilities.

Before we lock the v1 API, we need LumaWeave's actual shape. Answers
below reflect the codebase as of v0.19.0 (arc v113 open, 2026-06-11).
Files cited are under `/home/boop/Projects/lumaweave/`.

---

## Language and runtime

**1. Primary language(s)?**
TypeScript (React 19, runs in Tauri's Chromium webview) + Rust (Tauri 2
backend). Node 22 for tooling. No Python anywhere in LumaWeave.

**2. In-process embedded library, long-running daemon, CLI tool, or hybrid?**
Hybrid. The TypeScript frontend is in-process via napi-rs bindings. The
Rust Tauri backend would call the crate directly (no IPC hop). The
frontend talks to the backend via Tauri's IPC channel; ES calls that
originate in TypeScript would go through a thin `tauri-invoke.ts` wrapper
(`src/lib/tauri-invoke.ts`) unless napi-rs puts the binding in the webview
process directly — clarification needed on which model ES targets for
Tauri webviews.

**3. Multi-process? Multi-thread? Async runtime in use?**
Tauri is multi-process: one Rust process (Tokio async runtime) + one
Chromium webview process. The webview JS context is single-threaded.
The Rust backend uses `tokio` (already in `Cargo.toml` with `rt` and
`time` features, added in v108). No multi-user, no multi-tenant.

---

## What you want from ES

**4. The three top things you want from ES:**

- **Semantic diff events for the Reflective Twin diff layer.** LumaWeave
  is the rendering surface for Graph A vs Graph B. The diff layer between
  them is a stream of semantic events (`Agent investigating`, `Repair
  pending`, `Consensus reached`). LumaWeave needs to subscribe to that
  stream and translate events into visual graph transitions in real time.
  This is the primary use case — not producing events, but consuming and
  rendering them.

- **Agent trace replay for the time-travel viewer.** When a Claude Code
  session or Rhyzome repair run completes, LumaWeave should be able to
  scrub backward through the event log and re-render the graph state at
  any point. This requires ordered replay from a checkpoint, not just
  tail-the-latest.

- **Cross-stream correlation traversal.** Lighting up a subgraph
  associated with a causal chain (one user action triggering events
  across Cerebra, Policy Scout, and the graph layer). LumaWeave needs
  `correlation_id` lookup across streams to know which nodes to highlight.

**5. Anything you DON'T need or actively don't want?**

- **Branching for LumaWeave's own settings state.** The Zustand settings
  store (`src/control-plane/settings/settings.store.ts`, schema v95, 90+
  migrations) will NOT be migrated to ES. It stores current state only,
  and its migration chain is load-bearing for the existing app. ES is
  additive.
- **PII compliance / event deletion.** LumaWeave's graph data is file
  paths, node types, and edge relationships — no PII, no credentials.
  Append-only is fine.
- **Encryption at rest for v1.** Single developer workstation, no
  sensitive payload content.
- **Multi-user / multi-tenant** — completely out of scope.

---

## Scale and shape of writes

**6. Estimated write rate at steady state?**
Very low — LumaWeave is primarily a visualization consumer, not a heavy
producer. At idle: near zero. During an active agent run: estimate
10–50 events/sec burst, then silence.

**7. Burst profile?**
Tight clusters. When Claude Code fires a tool call sequence, many events
arrive quickly. Between agent runs, nothing for minutes or hours.

**8. Typical payload size? Maximum?**
Small. Agent trace steps: a few KB. Graph diff events: a few hundred
bytes to ~2 KB. No LumaWeave-originated payload expected to exceed 64 KB.

**9. Number of distinct streams? Events per stream per day?**
A handful active at any time: one per agent run, one per source adapter
load session, one for graph state transitions, one for UI-level
interactions worth recording. Estimate 5–10 concurrent active streams;
most streams quiescent. Events per stream per day: highly variable —
zero on idle days, hundreds during an active coding session.

**10. Single writer per stream, or concurrent writers?**
Single writer. Tauri's architecture means one Rust process owns the
backend. No concurrent writers from LumaWeave's side.

---

## Reads

**11. Read patterns?**
Three patterns all needed:
- **Tail-the-latest** — Graph B (live state) subscribes to new events as
  they arrive; this is the primary real-time display path.
- **Replay from a checkpoint** — time-travel viewer scrubs back to a
  specific version and re-derives graph state forward.
- **Random access by event ID / correlation_id** — clicking a node in
  the graph queries which events are causally associated with it.

**12. Live subscriptions — real-time or polling acceptable?**
Real-time required for Graph B. The visual layer needs to feel live;
lagged delivery would break the "watching the system think" experience.
Acceptable latency for a graph node to light up after the event is
recorded: <100 ms. Tauri's IPC event channel is already used for similar
real-time updates (`refreshToken` increment pattern in
`useGraphSourceSummary` — `src/source-adapter/sourceAdapterRegistry.ts`).
ES can follow the same model: Tauri command writes → emits a typed IPC
event to the frontend → Zustand slice updates → Sigma re-renders.

**13. Cross-stream queries needed?**
Yes. `correlation_id` traversal across streams is the mechanism for
subgraph highlighting. Example: a user triggers a source adapter reload
(stream A), which causes a Cerebra retrieval (stream B), which triggers a
Policy Scout check (stream C). LumaWeave needs to find all three from the
single originating event's `correlation_id` to draw the causal subgraph
overlay.

---

## Persistence and lifecycle

**14. How long do events need to live?**
Forever, or at least until the developer explicitly archives. The
Reflective Twin invariant requires Graph A to be reproducible from the
event log at any time. Lossy retention breaks that invariant structurally.
For non-canonical streams (e.g. debug traces), configurable retention
would be a nice-to-have but not required for v1.

**15. Any need to delete individual events?**
No. No PII, no compliance driver. Append-only is the right model for
LumaWeave.

**16. Acceptable storage growth?**
Developer workstation; the developer manages disk. No programmatic bound
required for v1. If retention policy is added later, it should be
opt-in per stream.

**17. Backup/restore expectations?**
Just-the-SQLite-file is fine. Same approach LumaWeave already uses for
Tauri's app data directory — developer copies the directory to back up.

---

## Security and deployment

**18. Sensitive data in event payloads?**
No. Graph data is file paths, node types, edge relationships, agent
reasoning steps. No credentials. Source adapter configs contain file
paths but those don't flow into event payloads.

**19. Encryption at rest required?**
Not required for v1. Single-user developer workstation.

**20. Single-user local-first, or multi-user / multi-tenant?**
Completely single-user local-first. No remote sync in scope.

**21. Deployment target?**
Developer workstation only. No server, no container, no edge device.

---

## Existing event/log infrastructure

**22. Existing event store, audit log, or similar?**
None. The `agents.inference` chat history (`AgentChatTile` introduced in
v112) is held in React component state — transient, not persisted. The
Zustand settings store uses versioned migrations (v95, 90+ entries in
`src/control-plane/settings/settings.migrations.ts`) but stores only
current state, not event history. ES is additive; no bridge or migration
required.

**23. If migrating — how many existing events to translate?**
Zero. No prior event store exists.

---

## Integration shape

**24. How would you want to call ES?**
Primary path: in-process Rust (Tauri backend calls the `lattica-es` crate
directly when handling a Tauri command). Secondary path: napi-rs binding
in the TypeScript frontend for subscriptions and queries that don't need
to go through the Rust backend. The Tauri IPC channel then carries typed
events to the React layer (`useEffect` keyed to an IPC listener →
Zustand update → re-render). File-watching bridge not needed — LumaWeave
is not config-file-driven the way Policy Scout is.

**25. Anything in the codebase that would make integration awkward?**

- **`__lwTauriMock` shim** (`src/lib/tauri-invoke.ts`) — Playwright tests
  intercept Tauri invocations via this shim (gated on `DEV || PLAYWRIGHT`).
  Any ES-backed Tauri command needs a mock entry here or it throws in the
  test suite.
- **Zustand settings store is NOT a candidate for migration.** Its 90+
  migration chain is load-bearing for existing installed builds; touching
  it for ES is out of scope.
- **`transport: "live"` slot in `sourceAdapterRegistry`**
  (`src/source-adapter/sourceAdapterRegistry.ts`) — this declared slot is
  how live data sources (Policy Scout JSONL, etc.) will push into the
  graph. ES event delivery and this slot should compose, not compete; the
  integration design should clarify which layer owns delivery to the
  `useGraphSourceSummary` effect.
- **Playwright test discipline** — new Tauri commands need corresponding
  `__lwTauriMock` entries and `controlSurfaceContractRegistry` entries
  (`src/control-plane/contracts/controlSurfaceContract.registry.ts`) if
  they add user-facing UI. This is not optional; it's enforced by the QA
  registry contract tests.

---

## Open questions and concerns

**26. What would make you NOT adopt ES?**

The biggest risk is startup overhead. Tauri webview cold start is already
a known CI pain point (E2E in CI deferred precisely because Vite + Tauri
startup is too slow for GitHub Actions' runner tier). If the napi-rs
binding has a non-trivial initialization cost — spinning up a Tokio
runtime, opening and checking the SQLite WAL, running schema migrations —
it compounds an existing problem. The question is: does `@lattica/es`
expose a `connect()` that blocks until ready, and can it share the Tokio
runtime already owned by the Tauri process rather than creating a second
one?

A long-running separate daemon (not in-process) would also be a
non-starter — one Rust process per Tauri app is the supported model.

**27. Anything in the ES design to push back on?**

Nothing fundamental. The append-only, content-addressed, SQLite-backed
design aligns cleanly with the Reflective Twin invariant.

One API question: `EVENT_FABRIC.md` says WAL mode is mandatory and opens
with `PRAGMA journal_mode = WAL`. LumaWeave's Tauri backend already opens
SQLite connections for the settings store via Tauri's app data directory
(implicitly, via the `tauri-plugin-store` path). Are the ES SQLite file
and the Tauri settings store in separate files? If they share a
connection pool or accidentally share a file, WAL locking behavior
becomes a concern. Assume separate files but worth confirming.

---

*Compile the response as a single markdown section under
"## LumaWeave" suitable for inclusion in ES_CONSUMER_PROFILES.md.*

---

## LumaWeave

**Language/runtime:** TypeScript (React 19 in Tauri 2 webview) + Rust
(Tokio async backend). Single-user, developer workstation only. No Python.

**Integration shape:** In-process Rust crate calls from the Tauri backend;
napi-rs binding in the webview for subscriptions and queries. Tauri IPC
channel carries typed events to React. Existing `__lwTauriMock` shim in
`tauri-invoke.ts` must cover any ES-backed Tauri commands for Playwright
compatibility.

**What we want:** (1) Real-time semantic diff event subscription for
rendering Graph B live in Sigma.js — this is the primary use case.
(2) Ordered replay from a checkpoint for the time-travel viewer.
(3) `correlation_id` cross-stream traversal for causal subgraph
highlighting.

**What we don't need:** Branching for our own settings state (Zustand
handles it, migration chain is load-bearing). PII deletion. Encryption
at rest. Multi-user.

**Writes:** Low-volume, bursty. 10–50 events/sec during agent runs, near
zero at idle. Small payloads (< 64 KB). Single writer per stream.

**Reads:** Tail-the-latest (live Graph B), replay-from-checkpoint
(time-travel), random-access-by-correlation-id (subgraph highlight). Live
subscription latency target: < 100 ms for visual update.

**Persistence:** Forever — Reflective Twin invariant requires Graph A
reproducibility from the full event log. Append-only is correct. WAL SQLite
file only; no streaming backup needed.

**Existing infra:** None. Zero events to migrate. Zustand settings store
(v95, 90+ migrations) stays separate and is not a candidate for ES.

**Key risk:** Startup overhead. Tauri cold-start is already a CI
bottleneck. ES init must share the existing Tokio runtime or initialize
lazily. A second daemon process is not acceptable.

**Open question:** Does `@lattica/es`'s napi-rs binding initialize
synchronously, and does it share or conflict with the Tauri process's
existing Tokio runtime?

---

## Additional notes — LumaWeave architecture reference for Lattica integration

*This section captures LumaWeave's current built state as a reference for
Lattica phase work. It is not part of the ES consumer profile above.*

### Version and status

- **Version:** 0.19.0 · arc v113 open (source adapter UX maturity)
- **Active branch:** `feat/gwells-c10a-structural-resolver` (gwells hardening)
- **Mid-migration:** "everything in AppShell" → "everything through a registry." Both patterns coexist; migrate selectively.

### Registry spine

20+ typed registries. Two tiers:

- **T1** (const-array + pure helpers) — static list known at build time; no mutation.
- **T2** (register + subscribe) — entries added at runtime; UI reacts via `subscribe()`. Use for new Lattica module registration.

Key T2 registries for Lattica integration:

| Registry | File | Lattica hook |
|---|---|---|
| `sourceAdapterRegistry` | `src/source-adapter/sourceAdapterRegistry.ts` | Phase 2 (`transport:"live"`), Phase 4 (`database-schema` slot) |
| `tileSectionRegistry` | `src/control-plane/panels/tileSectionRegistry.ts` | New module panels register here |
| `inspectorSpokeRegistry` | `src/themes/inspectorSpokeRegistry.ts` | New inspector spokes (e.g. Cerebra spoke) |
| `themeTargetRegistry` | `src/themes/themeTargetRegistry.ts` | Atmosphere theming layer (Phase 4) |
| `physicsDialectRegistry` | `src/graph/physics/physicsDialectRegistry.ts` | New gwells layouts for Platform-specific graphs |
| `commandRegistry` | `src/control-plane/commands/command-registry.ts` | New platform commands |

### Source adapter integration seams

`src/source-adapter/sourceAdapterRegistry.ts`

- **`database-schema` slot** — `adapterType: "database-schema"` declared, status `candidate`. Hook for Phase 4 (Cerebra SQLite adapter).
- **`transport: "live"` slot** — declared as a forward-compat field on file-envelope adapters. Hook for Phase 2 (Policy Scout JSONL live source).
- **`coupling: "sibling-module"`** — declared on `SourceAdapterEntry` type, unused today. Reserved for Cerebra vault and other in-process Lattica adapters (vs `"external"` for user-supplied paths).
- **`registerSourceAdapter(entry)`** — T2 pattern; no core change needed to add a new adapter.

### Settings store

`src/control-plane/settings/settings.store.ts` — schema v95, 90+ sequential additive migrations.

- **Read pattern in callbacks:** always use `useSettingsStore.getState().field` — never capture in a closure (stale-closure trap documented in `docs/agent/KNOWN_SHARP_EDGES.md`).
- **Adding a slice:** add field to schema, write migration `CURRENT_SCHEMA_VERSION → CURRENT_SCHEMA_VERSION + 1`, add default. Skipping the migration silently drops state on load.
- **Agent inference config** (v95): `settings.agents.inference.{endpoint, model, byokKey}`. Default endpoint is Ollama at `http://localhost:11434/v1`. This is the hook point for Phase 3 (route through Cerebra's retrieval layer).
- **Dev mode gate** (v94): `settings.devMode` — controls `requiresDevMode` tile visibility. 3 tiles currently dev-gated (QaPanel, system-index, graph-visual-inventory).

### Rust Tauri commands

`src-tauri/src/`

| Command | Purpose |
|---|---|
| `read_file` | Project-relative file read; `canonicalize → starts_with` path validation |
| `run_script` | Allowlisted scripts; 60 s timeout, 10 MB output cap |
| `list_files` | Recursive directory listing; depth cap 20/50 |
| `read_user_file` | User-supplied absolute path; symlink-rejected, regular-file only |
| `read_vault_file` | Relative path under caller-supplied root; `canonicalize → starts_with` |
| `get_project_root` | Returns project root (pops `src-tauri/` suffix if present) |
| `open_in_ide` | Opens file:line URL in configured editor |
| `chat` | Proxies chat message to inference backend endpoint |
| `test_inference_connection` | Tests connectivity to configured inference endpoint |

TypeScript wrappers: `src/lib/tauri-invoke.ts`. New commands go here. The `__lwTauriMock` shim in the same file intercepts calls in `DEV || PLAYWRIGHT` — required for Playwright testability.

### gwells physics engine

`src/physics/` — N-body simulation. Extensible via `physicsDialectRegistry` (T2) and `seedFunctionRegistry` (T1). Recent hardening: structural resolver for well assignment (current branch). This is the shared physics engine for Platform-level graphs (Policy Scout DAGs, Cerebra retrieval overlays, Rhyzome repair strategy DAGs).

### Playwright suite

- **Run:** `npm run qa:e2e` · `npm run typecheck` (requires `npm run generate:graph` first)
- **CI state:** lint-css + typecheck only; E2E in CI deferred (Vite cold-start too slow)
- **No `test.only`** — disables the rest of the file; use `test.fixme` for pending
- New module UI needs `controlSurfaceContractRegistry` entries and `__lwTauriMock` entries

### Phase hook table

| Lattica phase | LumaWeave hook | Status |
|---|---|---|
| Phase 0 (monorepo + identity) | `tauri.conf.json` rename; pnpm workspace init; `moduleRegistry.ts` stub | Planned |
| Phase 1 (ai-stack) | `agents.inference` endpoint → LiteLLM; config hot-reload (`fs::watch` → IPC → Zustand) | Schema present; hot-reload not yet built |
| Phase 2 (Policy Scout) | `transport: "live"` slot in `sourceAdapterRegistry` | Slot declared; loader not yet written |
| Phase 3 (Cerebra) | Extend `InferenceBackend` trait; `coupling: "sibling-module"` on Cerebra vault adapter | Trait present; sibling-module coupling declared but unused |
| Phase 4 (Cerebra SQLite) | `database-schema` adapter in `sourceAdapterRegistry` | Slot declared as `candidate`; loader not yet written |
| Phase 4 (visual identity) | `themeTargetRegistry` atmosphere extension; `tileSectionRegistry` multi-pass layout | ADR-007 patterns documented; not yet implemented |
| Phase 4+ (breadcrumb nav) | `moduleRegistry.ts` (Phase 0 deliverable) + breadcrumb chrome | Not yet built |
| All phases | New tile sections → `tileSectionRegistry`; new commands → `commandRegistry` | Registries present and working |
