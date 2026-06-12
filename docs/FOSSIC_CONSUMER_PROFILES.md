# Alluvium Consumer Profiles

Answers to binding-model and integration-shape questions for each Lattica module,
compiled before the alluvium TypeScript API is locked.

---

## LumaWeave Tauri Binding Model

**Context:** The senior dev review flagged that EVENT_FABRIC.md created an ambiguity
about napi-rs in the Tauri webview. This section resolves that ambiguity with reference
to the actual LumaWeave codebase (v0.19.0, `src-tauri/Cargo.toml` and Vite config).

---

### 1. Can any napi-rs native addon load inside LumaWeave's Tauri webview today?

**No. Definitively.**

The Tauri webview is a Chromium browser context, not a Node.js runtime. Evidence from
the codebase:

- `tauri.conf.json` `build.devUrl` is `http://localhost:1420` — Vite serves a browser
  bundle to the webview over HTTP, exactly as it would to Chrome or Firefox.
- `vite.config.ts` is a standard browser Vite config: `@vitejs/plugin-react`,
  `@tailwindcss/vite`, no Node.js compat shims, no `@rollup/plugin-native`. No
  mechanism exists to load `.node` files into the browser JS context.
- Zero napi-rs packages anywhere in `package.json` (checked dependencies and
  devDependencies). No `@napi-rs/*`, no `better-sqlite3`, no other N-API packages.
- Zero `import` statements referencing napi-rs bindings anywhere under `src/`.
  The only occurrences of `"native"` in the codebase are hotkey status labels
  (`HotkeyStatus = "active" | "native" | "banned"`) — unrelated.
- The `@tauri-apps/api/core` import used in `tauri-invoke.ts` is a browser-compatible
  shim that calls into the Tauri runtime via `window.__TAURI_INTERNALS__`, not through
  Node.js APIs.

Attempting `import('@napi-rs/canvas')` from frontend code would fail at Vite's module
resolution step (non-browser-compatible package) or at runtime when the N-API
`process.binding()` call finds no Node.js runtime underneath it. There is no workaround
path within the Tauri webview model.

**Implication for alluvium:** The napi-rs binding (`@lattica/es`) cannot be used from
LumaWeave's frontend JS context. The correct description from EVENT_FABRIC.md's footnote
needs to be the *leading* description: the frontend uses Tauri IPC for all alluvium
access. napi-rs is relevant only for the Node.js paths (tests, the optional standalone
time-travel demo — see item 5).

---

### 2. IPC command shape for the time-travel viewer

Since napi-rs cannot load in the webview, all alluvium access from the frontend must go
through Tauri commands registered in `src-tauri/src/lib.rs` and called via
`tauri-invoke.ts`.

**Request-response commands** (frontend calls `invoke("es_*", args)`, gets a response):

| Command | Args | Return | Purpose |
|---|---|---|---|
| `es_list_streams` | — | `StreamInfo[]` | Populate stream selector in the viewer |
| `es_list_branches` | `stream_id` | `BranchInfo[]` | Populate branch selector |
| `es_read_range` | `stream_id, branch, from_version, to_version` | `SerializedEvent[]` | Load a segment for scrubbing |
| `es_read_state_at_version` | `stream_id, branch, version, reducer_name` | serialized state | Derive graph state at a scrub point |
| `es_subscribe` | `stream_id, branch` | `subscription_id: string` | Register a live subscription (Rust sets up the stream listener) |
| `es_unsubscribe` | `subscription_id` | — | Cancel a live subscription |

**Push path for live subscriptions** (what `es_subscribe` enables):

LumaWeave currently has no Rust-to-frontend push events — all backend communication is
request-response via `invoke()`. The `refreshToken` increment in `GraphSourcesTileContent`
is polling, not push (`setSetting("sources.refreshToken", refreshToken + 1)` increments a
Zustand value, which re-triggers `useGraphSourceSummary`'s effect). For Graph B's live
diff layer, polling is not acceptable (target latency < 100 ms; polling at that rate would
be busy-looping). The correct model is Tauri's built-in event emission:

```rust
// Rust side (inside alluvium's stream listener, spawned on Tauri's runtime)
app_handle.emit("es:event", &serialized_event)?;
```

```ts
// TypeScript side (in a React hook, e.g. useEsDiffLayer)
import { listen } from "@tauri-apps/api/event";
useEffect(() => {
  const unlisten = listen<SerializedEvent>("es:event", (event) => {
    dispatchGraphDiffUpdate(event.payload);
  });
  return () => { unlisten.then(f => f()); };
}, []);
```

This pattern is not used anywhere in LumaWeave today but is fully supported by
`@tauri-apps/api/event` (already a transitive dependency via `@tauri-apps/api`). It
does NOT go through `tauri-invoke.ts` or `__lwTauriMock` — the `listen()` path is
separate from the `invoke()` path. **New design constraint:** the `__lwTauriMock` shim
only covers `invoke()` calls. Playwright tests that need to simulate push events will
need a separate mock mechanism (e.g., directly dispatching a custom DOM event that
the `listen()` wrapper re-emits, or a test-only Tauri plugin that injects events).

**Payload format: JSON, not msgpack over IPC.**

Tauri's `#[tauri::command]` macro serializes return values and emitted payloads via
`serde_json`. The `invoke()` call in `tauri-invoke.ts` receives plain JS objects.
The `__lwTauriMock` shim works with plain JS objects. Msgpack blobs in alluvium's
SQLite store should be deserialized to structured JSON on the Rust side before crossing
the IPC boundary. The IPC layer is JSON; the persistence layer is msgpack internally.
This is the correct layering: alluvium owns its encoding; consumers see plain objects.

---

### 3. Tokio runtime sharing

LumaWeave's `Cargo.toml`:

```toml
tokio = { version = "1", features = ["rt", "time"] }
```

Features present: `rt` (the core runtime trait + single-threaded executor),
`time` (timers, `sleep`, `interval`). Feature **absent**: `rt-multi-thread`.

This matters because `rt` alone gives the current-thread (single-threaded) executor;
`rt-multi-thread` adds the work-stealing thread pool. LumaWeave's own `tokio` dep
is minimal — it provides `tokio::time::sleep` and `tokio::time::timeout` used in
`fs.rs` (`run_script` 60 s timeout).

However: **Tauri 2 itself creates a multi-threaded Tokio runtime.** When LumaWeave
declares `#[tauri::command] pub async fn`, those futures execute on Tauri's runtime,
not on a runtime LumaWeave creates itself. The `features = ["rt", "time"]` declaration
just pulls in the crate's API surface; it does not create a second runtime.

**Recommendation for alluvium:** use `tauri::async_runtime::spawn()` (Tauri 2's
recommended pattern for background task spawning) rather than `tokio::spawn()` directly.
This guarantees alluvium's tasks land on the existing Tauri-managed runtime, not a
new one. For background tasks that need to start at app launch (file-watch notification,
OTel flush loop), register them in `tauri::Builder::setup()` in `lib.rs` via
`tauri::async_runtime::spawn()`.

Alluvium should **not** call `tokio::runtime::Runtime::new()` or
`tokio::runtime::Builder::new_*().build()` — that creates a second runtime sitting
alongside Tauri's, which wastes threads and can deadlock if alluvium's tasks try to
`.await` on handles from Tauri's runtime (different executors). The pattern to avoid:

```rust
// BAD — creates a second runtime
let rt = tokio::runtime::Runtime::new().unwrap();
rt.spawn(alluvium_background_task());
```

The pattern to use:

```rust
// GOOD — runs on Tauri's existing runtime
tauri::async_runtime::spawn(alluvium_background_task());
```

If alluvium needs `rt-multi-thread` features (e.g., Rayon-style parallelism for
snapshot compaction), it should declare that in its own `Cargo.toml` feature flags
rather than requiring LumaWeave to add it. Tauri 2 already enables multi-thread
transitively, so the feature is available in the compiled binary even if LumaWeave
doesn't declare it explicitly.

---

### 4. All alluvium consumers in LumaWeave

Listed in order of integration priority:

**a. Graph B diff layer subscriber** *(Phase 5 — primary use case)*

The main visual payoff of alluvium in LumaWeave. A hook (likely `useEsDiffLayer()` in
`src/graph/`) that calls `es_subscribe` at mount and listens for `es:event` push
events. Translates incoming semantic events (`AgentInvestigating`, `RepairPending`,
`ConsensusReached`, etc.) into Sigma graph animations — node highlight, edge pulse,
cluster boundary shift. This is the Reflective Twin diff layer rendered live.

IPC surface: `es_subscribe`, `es_unsubscribe` + `listen("es:event")`.

**b. Source adapter `transport: "live"` path** *(Phase 2 — Policy Scout integration)*

When the active source adapter has `transport: "live"` (declared but not yet
implemented in `sourceAdapterRegistry.ts`), the adapter's loader needs to tail an
ES stream rather than do a one-shot file read. The `useGraphSourceSummary` effect
currently re-runs on `refreshToken` increment (polling). The live adapter path needs
to re-run on `es:event` arrival instead — specifically when Policy Scout appends a
new audit JSONL event to its stream.

IPC surface: `es_subscribe`, `es_unsubscribe` + `listen("es:event")`. The loader
function for a live adapter becomes a subscription setup call rather than a one-shot
invoke call.

**c. Time-travel viewer** *(Phase 5 — Reflective Twin visualization)*

A UI component (likely a dedicated tile section registered in `tileSectionRegistry`)
that shows a scrub bar over the event log. Selecting a version calls
`es_read_state_at_version` to derive graph state, then re-renders the Sigma graph
from that state. `es_read_range` populates the scrub bar timeline with event markers.

IPC surface: `es_list_streams`, `es_list_branches`, `es_read_range`,
`es_read_state_at_version`.

**d. Agent chat tile** *(Phase 3 — not a v1 consumer)*

Currently (`AgentChatTile`, v112) calls the inference backend directly via the `chat`
Tauri command. Does not use alluvium today. In Phase 3, logging `llm_call` /
`tool_call` / `tool_result` events to an agent-run stream becomes relevant. Not
needed for alluvium's v1 API design — can be added as a write-side integration once
the IPC layer is established.

**Playwright testability constraint for all four:**

Every new `es_*` Tauri command needs:
1. An entry in `__lwTauriMock` in `tauri-invoke.ts` (for `invoke()`-based commands).
2. For the `listen("es:event")` push path: a separate test mechanism TBD (the mock
   shim only covers `invoke()`). Design this before implementing the diff layer
   subscriber so Playwright tests can inject synthetic events.
3. `controlSurfaceContractRegistry` entries for any user-facing UI added by the
   time-travel viewer or live subscription indicator.

---

### 5. Standalone web page mode (optional scope question)

A standalone Node.js time-travel viewer (for portfolio demo without the full Tauri
app) **would** run under Node.js, and in that context napi-rs bindings **do** work.
The `@lattica/es` napi-rs package could be imported directly, bypassing IPC entirely.

This scope is **out of scope for alluvium v1** and should not drive the API design.
The reason to note it: the napi-rs binding should be designed to be usable in both
paths (Node.js direct + Tauri IPC wrapper), but the IPC command shape in item 2 is
the primary API surface for LumaWeave. If the Node.js standalone path is built later,
it calls the napi-rs binding directly and does not go through `tauri-invoke.ts`.

Do not let the possibility of this path complicate the v1 binding API. The distinction
is already encoded in EVENT_FABRIC.md's footnote; it just needs to be the leading
description, not a footnote.
