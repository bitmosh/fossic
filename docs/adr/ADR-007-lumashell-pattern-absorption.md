# ADR-007: LumaShell UX Patterns Absorbed into Lattica Design

**Status:** Accepted
**Date:** 2026-06-11
**Deciders:** Developer (solo)
**Technical area:** Platform UX / Design reference

---

## Context and Problem Statement

LumaShell was a Go + Bubble Tea terminal workspace shell built before the Lattica platform concept existed. It implemented a cascading breadcrumb-style command dock, an atmosphere-driven theming system, YAML-persisted config with live hot-reload, and a 4-mode multi-pass panel layout algorithm. The project was benched when the developer got stuck on an unrelated constraint; the UX design work behind it is sound.

LumaShell's Go and Bubble Tea code does not port to Tauri/React. Attempting a direct port would produce worse results than a native re-implementation informed by the original concepts. However, losing the design reasoning entirely — the specific decisions about navigation hierarchy, config resilience, theming propagation, and panel degradation — would mean rederiving the same conclusions later, probably with less clarity.

The question is how to preserve the design value of a benched project without carrying dead code or creating a stale reference that misleads future implementers.

---

## Decision

Four UX patterns from LumaShell are explicitly absorbed as design references for Lattica. The original Go/Bubble Tea code is not reused or ported. Each pattern is documented with (a) what LumaShell implemented, (b) how the concept maps to Lattica's architecture, and (c) concrete implementation guidance sufficient for a future implementer to work from without the original codebase.

The Phase 0 deliverable `docs/design/lumaShell-absorbed-patterns.md` is the living implementation reference for these patterns. This ADR is the architectural rationale record.

---

## Considered Alternatives

### Option A: Port the Go code to a Tauri sidecar process
Rejected. The value in LumaShell is UX reasoning, not computation. A sidecar adds IPC overhead, a second runtime, and a maintenance surface for zero benefit — the patterns translate directly to React/Tauri primitives.

### Option B: Archive the LumaShell repo and reference it when needed
Rejected. "Reference it when needed" means context is lost when it's not needed, which is exactly the failure mode we're designing against. Structured absorption into docs with concrete Lattica-specific mapping is more durable than a pointer to archived code.

### Option C: Absorb patterns as documented design references (chosen)
The Go/Bubble Tea discipline (Elm-architecture Model/Update/View) is referenced as a mental model for Lattica's module lifecycle even though the implementation is React. The specific patterns are mapped to their Tauri/React/Zustand equivalents. No code is copied; the reasoning is.

---

## The Four Absorbed Patterns

### Pattern 1: Breadcrumb Module Navigation

**What LumaShell had.**
A cascading breadcrumb-style command dock that showed the user's navigation path through project contexts. Selecting a context pushed a crumb; navigating back popped it. The dock was always visible, always accurate, and never required the user to mentally track "where am I."

**How this maps to Lattica.**
Lattica hosts multiple modules (LumaWeave graph, Cerebra memory, Policy Scout governance, ai-stack GPU, evaluation dashboard). A user drilling into a retrieval trace inside Cerebra, then into a specific query node, then into the associated policy decision, traverses three module boundaries without an obvious "back" path. The breadcrumb pattern provides that path.

Concrete mapping: a persistent navigation chrome component showing the active module, the active sub-panel within that module, and the active context object (if any). Example trail: `Lattica > Cerebra > retrieval trace > query:42`. Each crumb is a navigable link. The chrome is module-registry-driven — each registered module declares its breadcrumb label and sub-panel hierarchy.

**Implementation reference.**
The `moduleRegistry.ts` stub (Phase 0 deliverable) is the hook point. Modules register with a `nav` descriptor that includes label, icon, and an ordered list of sub-panel identifiers. The breadcrumb component reads from the registry; it does not hardcode module names.

---

### Pattern 2: Config Hot-Reload

**What LumaShell had.**
YAML configuration files persisted to disk, watched with `fsnotify`, changes propagated through a message bus to the UI, with graceful fallback to the last valid config if the new file failed to parse. The UI updated without restart. The pattern was: watch → parse → validate → apply or fallback, never crash.

**How this maps to Lattica.**
Lattica manages configuration across multiple modules: `litellm-config.yaml` for the ai-stack, policy rule YAML for Policy Scout, Cerebra vault path and retrieval settings, and Lattica's own module settings. Each of these changes in normal operation (the developer edits a policy rule, adds a model to the LiteLLM config, changes the Cerebra vault path). Requiring a restart on each change is not acceptable for a platform that is supposed to be always-on infrastructure.

The `fsnotify` pattern translates directly: Tauri's built-in `fs::watch` capability on the Rust side fires an event when a watched file changes → Rust handler parses the new content → emits a typed IPC event to the frontend → Zustand settings slice updates → components re-render. If parse fails, the event is not emitted and the existing Zustand state (last valid config) is preserved. The fallback is structural, not a conditional.

**Implementation reference.**
LumaWeave's existing Zustand migration system (`useSettingsStore` with 90+ versioned migrations) already implements the "don't crash on bad state" invariant for persisted settings. The hot-reload extension follows the same discipline: the Zustand slice is the source of truth in memory; the file on disk is the input channel, not the authoritative store.

Phase 0 does not implement hot-reload for any module config. It documents the pattern so Phase 1 (ai-stack) and Phase 2 (Policy Scout) can implement it consistently rather than inventing separate approaches.

---

### Pattern 3: Atmosphere Layered Theming

**What LumaShell had.**
A single "atmosphere" setting (e.g., `dim`, `bright`, `warm`, `cold`) that propagated to background, text, accent, border, and shadow colors across all components through a token layer. Changing one setting changed the visual character of the entire shell. The key insight was that colors are not independent variables — they are coupled through perceptual relationships (contrast ratios, saturation relationships, warmth) that a single atmosphere parameter captures more naturally than individual color overrides.

**How this maps to Lattica.**
LumaWeave's existing theme token architecture (`themeTargetRegistry`, token-based CSS custom properties) provides the propagation layer. What's missing is the atmosphere concept above the token layer — a semantic category that selects a coordinated token set rather than requiring the user to configure individual values.

Lattica will be used in distinct operational modes: focused development work, passive monitoring of infrastructure, active agent runs where multiple agents are simultaneously modifying files, and exploratory research. These modes have different perceptual demands (monitoring wants high contrast at a glance; active agent runs want clear visual indication of activity without being distracting). An atmosphere layer lets the user switch the entire visual character with one setting.

Concrete mapping: extend `themeTargetRegistry` with an atmosphere dimension. Define four named atmospheres: `focus` (neutral, low distraction), `observe` (high contrast, optimized for dashboard readability), `active` (agents running; warm accent tones, animated indicators enabled), `research` (cooler palette, de-emphasizes operational panels in favor of graph and Cerebra panels). Each atmosphere is a preset of theme tokens, not a full override — the user's customizations are layered on top.

**Implementation reference.**
This is a design reference for Phase 4 onward when LumaWeave is fully embedded as a Lattica module and the platform needs a coherent visual identity across modules. Phase 0 does not implement atmospheres; it records the concept so the Phase 4 theming work starts with the right mental model instead of discovering the need for it mid-implementation.

---

### Pattern 4: 4-Mode Multi-Pass Panel Layout

**What LumaShell had.**
A layout algorithm that handled four display modes (compact, normal, expanded, fullscreen) through multiple passes: first pass allocates minimum viable space to each panel, second pass distributes remaining space by priority, third pass handles overflow by progressively hiding lower-priority elements. Content was never clipped without the user's knowledge — panels declared a minimum viable display (the smallest state where they still communicate their primary information) and the algorithm guaranteed at least that.

**How this maps to Lattica.**
Lattica will host many simultaneous panels: graph view, Cerebra status, Policy Scout audit stream, ai-stack GPU utilization, active agent trace, evaluation dashboard. On a single monitor, these panels cannot all be fully expanded simultaneously. Without a principled layout algorithm, the result is either clipping (content cut off silently) or a rigid grid that breaks when content doesn't fit the predetermined slots.

The multi-pass approach translates to Lattica's tile layout system as follows. Each panel (tile section in LumaWeave's current model) declares: minimum viable size (px), preferred size (px or fraction), priority (integer, lower = higher priority), and a minimum viable render mode (what gets hidden first when space is constrained). The layout algorithm runs two passes: first allocates minimums, then distributes surplus by priority. Panels that cannot fit their minimum in the current layout enter a collapsed state (header visible, content hidden) rather than being clipped.

This is directly relevant to Lattica because the Reflective Twin vision (Graph A + Graph B + diff layer simultaneously visible) requires careful space management — the graph panel's minimum viable size is large (you can't usefully read a graph in 100px), while the GPU utilization panel's minimum is small (a single number with a sparkline).

**Implementation reference.**
LumaWeave's current tile/panel system (12 registered tile sections in `tileSectionRegistry`) is the hook point. The existing registry already has section metadata; the layout algorithm is the extension. Phase 0 documents the pattern; implementation belongs in a dedicated layout phase once the full set of Lattica panels is known.

---

## What Is NOT Absorbed

- **Go code.** No source is copied, referenced directly, or ported.
- **Bubble Tea components.** The TUI widget model does not translate to React DOM; attempting to map it would produce worse results than native React component design.
- **Terminal-first interaction model.** Lattica is GUI-first. LumaShell's command-dock interaction (keyboard-driven, no mouse) is not Lattica's primary interaction paradigm. The navigation hierarchy concept transfers; the terminal input model does not.
- **LumaShell's specific command hierarchy.** The module structure is different. LumaShell had `workspace > project > context > command`; Lattica has `platform > module > sub-panel > context object`. The breadcrumb pattern is the same; the levels are not.

---

## Implementation Notes

**Elm architecture as mental model.**
Bubble Tea implements the Elm Model/Update/View architecture. While Lattica's implementation is React + Zustand, the Elm discipline — state is a pure value, updates are pure functions, the view is a pure function of state — maps exactly onto Zustand slices with selector-driven components. Each Lattica module should be designed as if it were an Elm module: what is its state shape, what are its messages, what does it render given its state. This discipline prevents the "everything in AppShell" anti-pattern that LumaWeave is currently migrating away from.

**Ordering of implementation.**
None of the four patterns are implemented in Phase 0. Phase 0 produces the design reference document (`docs/design/lumaShell-absorbed-patterns.md`) with sufficient detail that each pattern can be implemented independently by a future implementer without access to the original LumaShell codebase or this ADR. The patterns become relevant at the following phases:

- Breadcrumb navigation: Phase 4 (when LumaWeave is embedded as a Lattica module and cross-module navigation exists)
- Config hot-reload: Phase 1 (ai-stack config) and Phase 2 (Policy Scout rules)
- Atmosphere theming: Phase 4 (Lattica visual identity work)
- Multi-pass panel layout: Phase 4 or later (once the full panel inventory is known)

**Relationship to LumaWeave's existing architecture.**
LumaWeave's registry spine (14+ registries with `register()` + `subscribe()`) is the structural foundation that makes all four patterns implementable without rewrites. The breadcrumb component reads from `moduleRegistry`. Hot-reload updates `useSettingsStore`. Atmosphere extends `themeTargetRegistry`. Multi-pass layout extends `tileSectionRegistry`. The patterns fit the existing architecture; they do not require it to change shape.

---

## Consequences

**Positive.**
- Design reasoning from a benched project is preserved in a durable, Lattica-specific form before context fades.
- Four UX patterns with well-understood implementations are available as references when the relevant phases begin, reducing the risk of re-deriving weaker versions.
- The patterns are anchored to specific hook points in the existing codebase, making them actionable rather than aspirational.

**Negative / Risks.**
- The design reference document (`docs/design/lumaShell-absorbed-patterns.md`) can become stale if the architecture evolves and the doc is not updated. Mitigation: the patterns are documented in terms of registry hook points (named registries, named slices) rather than file paths or component names, which are more stable.
- Atmosphere theming is the pattern most likely to require design iteration when actually implemented — the four named atmospheres are a starting point, not a final spec. Implementation should treat them as hypotheses to be tested against real Lattica panels, not commitments.

---

## References

- ADR-001: Lattica IS LumaWeave Extended
- ADR-006: Monorepo from Phase 0
- `docs/design/lumaShell-absorbed-patterns.md` (Phase 0 deliverable — concrete implementation notes)
- `docs/PHASES.md` Phase 0 deliverables
- LumaWeave `src/tileSectionRegistry.ts`, `src/themeTargetRegistry.ts`, `useSettingsStore` — hook points for Patterns 3 and 4
