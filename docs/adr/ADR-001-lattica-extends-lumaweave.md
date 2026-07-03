# ADR-001: Lattica IS LumaWeave Extended

> **Historical note:** This record references Rhyzome, bons.ai, Bo, and discord-bot. These do not represent active Lattica ecosystem members — their appearance here reflects early planning context that did not materialize. The decision rationale remains accurate.

**Status:** Accepted
**Date:** 2026-06-11
**Deciders:** Developer + Claude Code alignment analysis

## Context and Problem Statement

LumaWeave exists as a mature Tauri 2 desktop application with a registry-driven architecture, custom physics engine (gwells), a Sigma.js/Graphology graph layer, 14+ typed registries with zero-core-change extensibility, a Playwright E2E suite, and 90+ versioned Zustand migrations. It is a working, tested, non-trivial codebase.

Lattica is the planned platform OS/suite that unifies all portfolio projects — LumaWeave, Cerebra, Policy Scout, ai-stack, discord-bot (Bo), Rhyzome, bons.ai, gwells, eval-core, and the ES toolkit — under one Tauri desktop application with a shared runtime, common memory model, event fabric, and visual language.

The architectural question: how does Lattica relate to LumaWeave? Three structural options were evaluated. The choice determines the development trajectory for all twelve phases of the roadmap and sets the physical boundary of the codebase for the life of the project.

## Decision Drivers

- **No parallel maintenance.** Two separate Tauri applications means two Rust backends, two Zustand trees, two build pipelines, two sets of Playwright fixtures, and two upgrade paths for every dependency. The developer is working alone; that surface area is not viable.
- **LumaWeave's registry pattern IS the module system Lattica needs.** The `sourceAdapterRegistry`, `tileSectionRegistry`, `physicsDialectRegistry`, `inspectorSpokeRegistry`, and the rest of the 14+ Map/array registries already implement zero-core-change extensibility with `register()` and `subscribe()`. Lattica's module integration model is not a new thing to build — it is the pattern LumaWeave already runs on, extended to cross-module sources and inter-module events.
- **gwells is the physics engine for the platform graph, not just for LumaWeave.** Policy Scout audit graphs, Cerebra retrieval trace overlays, Rhyzome repair strategy DAGs, and the Reflective Twin's dual-graph view all require the same gwells n-body simulation. Keeping gwells inside LumaWeave and treating LumaWeave as a downstream consumer of Lattica creates an inversion; treating LumaWeave as the graph module of Lattica keeps physics colocated with its primary consumer.
- **The transport:"live" slot and database-schema adapter slot in LumaWeave's source adapter registry are precisely the extension points Lattica phases 2–5 need.** Phase 2 (Policy Scout audit JSONL as live source adapter), Phase 4 (Cerebra SQLite as database-schema adapter), and Phase 5 (polling-based twin view) all plug directly into existing declared slots. No new protocol is needed; the slots are already named and waiting.
- **Portfolio coherence.** The developer's projects are "organs of a larger organism." That framing only holds if there is one organism with a shared identity — one codebase, one runtime, one graph that renders all of them. A subprocess wrapper or shared library does not produce the emergent behavior the Reflective Twin Architecture requires: agents visually active in the graph, semantic diff events flowing through a common event fabric, cross-module memory and governance in a single visual workspace.
- **LumaWeave as a standalone identity still matters.** LumaWeave is a demonstrable portfolio artifact. Its graph visualization work, gwells physics engine, registry architecture, and Playwright discipline are worth showing independently. The decision must preserve LumaWeave's named identity and its ability to be described as a distinct module, not dissolve it into an anonymous layer.

## Considered Options

1. **Option (a): Lattica as a subprocess wrapper** — Lattica launches LumaWeave as a child process and embeds its window via xembed or a secondary webview.
2. **Option (b): Shared React library** — Extract LumaWeave core into an npm package; both a standalone LumaWeave app and Lattica import from it.
3. **Option (c): Lattica IS LumaWeave extended** — The LumaWeave codebase becomes Lattica. LumaWeave's graph module identity is preserved as a named module within the platform. No separate app, no extracted library.

## Decision Outcome

**Chosen option: (c)**

LumaWeave's codebase becomes Lattica. The Tauri project is extended in place — new modules register into the existing registries, new tile sections and source adapters plug into the existing slots, the Rust backend grows new Tauri commands alongside the existing ones, and the monorepo (pnpm workspaces for TypeScript/Rust, uv workspaces for Python) is initialized at the repository root rather than alongside a separate Lattica repo.

The core rationale: LumaWeave already solved the module integration problem. Its registry spine — `register()`, `subscribe()`, typed slot contracts, zero-core-change extensibility — is architecturally identical to what a platform module system requires. Choosing option (c) means Lattica inherits that solution on day zero of Phase 0 rather than re-deriving it. Every architectural decision in LumaWeave that was made the hard way (the gwells NaN-guard, the stale-closure discipline in Zustand callbacks, the controlSurfaceContractRegistry, the QA registry/Playwright discipline) transfers intact.

The Reflective Twin Architecture — Graph A (canonical snapshot) and Graph B (live state) with a semantic diff layer — requires a single rendering surface that can hold both graphs simultaneously and animate the diff events as they arrive. That surface is LumaWeave's Sigma.js/Graphology layer. Building it inside LumaWeave and calling the result Lattica is not a rename; it is recognizing that LumaWeave was always building toward this and the platform grows from the graph outward, not the other way around.

### Positive Consequences

- **Zero new infrastructure on day one of Phase 0.** The Tauri project, the React webview, the registry system, the tile/panel layout, the Playwright suite, the Zustand store with versioned migrations — all present and working before the first Lattica-specific line is written.
- **The transport:"live" and database-schema adapter slots activate without protocol invention.** Phase 2 and Phase 4 slot directly into declared extension points. The adapter contract is already defined; the integration work is implementing the loader, not designing the interface.
- **gwells remains colocated with its primary consumer.** The physics engine that drives Policy Scout DAGs, Cerebra retrieval overlays, and the Reflective Twin's dual-graph view lives in the same module that renders those graphs. No cross-package import, no version drift between the engine and the renderer.
- **Single Zustand tree means single source of truth across all modules.** Cerebra status, Policy Scout audit state, ai-stack GPU metrics, and the LumaWeave graph state share one versioned, migration-safe store. Cross-module UI (the System Overview tile, the agent dispatch panel, the live audit panel) reads from one store with one subscription model.
- **One Playwright suite covers the entire platform.** As new modules add UI, their E2E tests join the existing suite with existing parallelism, QA registry discipline, and controlSurfaceContractRegistry contracts. No second test infrastructure to maintain.
- **Portfolio story is additive, not dilutive.** LumaWeave is described as the graph visualization module of Lattica. Its standalone architectural work (gwells, registry spine, Playwright discipline) is demonstrable as a module. Lattica is described as the platform those modules compose into. Both stories are true simultaneously.
- **Constraint design principle enforced at the structural level.** By extending rather than wrapping or abstracting, the module boundaries are enforced by the registry contracts rather than by inter-process communication discipline. A module that violates its adapter contract fails at registration, not at runtime IPC negotiation.

### Negative Consequences / Risks

- **The LumaWeave repository name and Tauri bundle identifier must change.** The Tauri `tauri.conf.json` identifier, the window title, the package name in `package.json`, and any CI/CD references to "LumaWeave" as a project handle need to be updated as part of Phase 0. This is a one-time mechanical cost but it touches several files simultaneously and must be done cleanly in a single commit.
- **Scope creep risk is structurally higher.** Because all modules share one codebase, a developer impulse to "quickly wire in" a Cerebra feature during a LumaWeave graph pass is always technically possible. The phase discipline and STOP gates in `docs/agent/survival-manual/06_STOP_CONDITIONS.md` must be applied more strictly, not less, precisely because the boundaries are soft.
- **The monorepo grows in complexity over twelve phases.** pnpm workspaces + uv workspaces + Rust workspace (Cargo) in one repository is manageable but not trivial. Workspace dependency resolution, per-package typecheck boundaries, and the Playwright suite's file-level parallelism all need to remain coherent as the tree grows. This is ongoing maintenance, not a one-time cost.
- **LumaWeave's standalone demonstrability requires deliberate preservation.** As Lattica-specific UI and modules accumulate, the graph module must remain identifiable and runnable independently (or near-independently) for portfolio purposes. Without explicit effort to maintain that boundary — module registry isolation, clear documentation of what "the graph module" is — the LumaWeave identity risks dissolving into ambient platform infrastructure.

## Pros and Cons of Discarded Options

### Option (a) — Subprocess/xembed wrapper

**Why rejected:**

Two Rust backends running simultaneously means two separate Tauri command surfaces, two event loops, two sets of IPC contracts, and two upgrade paths every time Tauri or its Rust dependencies change. Embedding a Tauri webview inside another Tauri webview via xembed is not a supported pattern on all target platforms (particularly Wayland) and introduces window management fragility that has no upstream fix path.

Cross-process communication between the Lattica shell and the embedded LumaWeave would require serializing and deserializing Zustand state across process boundaries — either via IPC messages or shared memory — which recreates the synchronization problem that a single Zustand tree solves structurally. The Reflective Twin's dual-graph view, which requires LumaWeave to render both Graph A and Graph B simultaneously with the semantic diff layer, becomes a message-passing choreography problem rather than a state derivation problem.

The subprocess model also means the gwells physics engine runs in the child process and cannot directly respond to events arriving in the Lattica parent process without a serialization round-trip. For live source adapters (Phase 2 onwards), this latency and complexity is unacceptable.

**Single sentence rejection:** Two runtimes for one visual surface is complexity without benefit.

### Option (b) — Shared React library

**Why rejected:**

Extracting LumaWeave core into an npm package requires deciding, up front, which parts of LumaWeave are "core" (library) and which are "app" (consumer). The registry spine, gwells, the Sigma.js layer, the Zustand store shape, the tile/panel system, and the Tauri IPC bindings are all deeply intertwined. Separating them into a publishable package without breaking their interdependencies is a significant refactor — one that would need to happen before any Lattica work begins, at the highest possible risk to the existing working codebase.

The shared library model also means Tauri commands — which are defined in Rust and invoked from TypeScript — cannot live in the library; they must be re-exposed through Lattica's own Rust backend. This creates a permanent synchronization requirement: every new capability that touches the Rust backend must be implemented twice, once in the library's TypeScript interface and once in the consuming app's Rust handler.

Maintaining two publish targets (a standalone LumaWeave app that consumes the library, and Lattica which also consumes the library) means every breaking change to the library requires updating two consumers, running two test suites, and managing two version lines. For a solo developer this is not a sustainable maintenance model.

The versioned Zustand migrations — 90+ entries — would need to be split or duplicated between the library and each consumer. Migration ordering across package boundaries is a known source of store corruption bugs.

**Single sentence rejection:** The refactor cost is front-loaded, the maintenance cost is permanent, and the architectural benefit is a shared package that one developer maintains for an audience of one.

## Implications for LumaWeave Identity

LumaWeave is not retired, dissolved, or renamed out of existence. It becomes the **graph visualization module** of Lattica — the module responsible for the Sigma.js/Graphology render layer, the gwells physics engine, the tile/panel system, and the source adapter registry. Its name appears in module documentation, in the registry, and in the monorepo directory structure. The gwells engine, when extracted to a standalone npm package (a planned but non-binding future step), publishes under a name that credits its origin.

For portfolio and interview purposes: LumaWeave is described as the foundational module of Lattica — the graph layer that the platform was built on and around. The architectural work (registry-driven extensibility, gwells n-body physics, Playwright discipline, Reflective Twin visualization) is attributable to LumaWeave specifically. Lattica is described as what LumaWeave became when the other organs of the system were connected to it. Both descriptions are accurate; neither erases the other.

The `tauri.conf.json` identifier and the visible application name change to reflect Lattica. The internal module identity — the registry key, the documentation, the directory name for the graph module — preserves the LumaWeave name.

## Notes

- The Tauri bundle identifier change (Phase 0) must be treated as a breaking change for any locally installed development builds. Any stored Tauri app data keyed to the old identifier will not migrate automatically; this is acceptable for a development-phase project with no end users.
- The monorepo workspace boundaries (pnpm / uv / Cargo) should be defined in Phase 0 and treated as stable contracts thereafter. Workspace restructuring mid-roadmap is disruptive; front-load the decision.
- The `transport:"live"` slot and `database-schema` adapter slot in `sourceAdapterRegistry` are the first integration seams Lattica will use. Their contracts should be formally documented (not just declared in code) before Phase 2 begins, so that Policy Scout and Cerebra adapter work has a written interface to implement against.
- Option (c) places a higher burden on phase discipline than either discarded option. The STOP gates, change-list scoping, and "one commit, one concern" rule in `CLAUDE.md` are load-bearing for this architecture. Relaxing them is how option (c) accrues the failure modes of option (a) and (b) without their theoretical benefits.
- If gwells is eventually extracted to a standalone npm package, the extraction should follow the "copy verbatim first, refactor in a separate commit" discipline — no semantic changes during the move.