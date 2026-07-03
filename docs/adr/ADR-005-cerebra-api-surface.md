# ADR-005: Cerebra API Surface — CLI Shell-Out Now, Unix Domain Socket Daemon in Phase 7

> **Historical note:** This record references Bo and discord-bot. These do not represent active Lattica ecosystem members — their appearance here reflects early planning context that did not materialize. The decision rationale remains accurate.

**Status:** Accepted
**Date:** 2026-06-11
**Deciders:** Developer (bitmosh)
**Supersedes:** —
**Related ADRs:** [ADR-001](ADR-001-lattica-extends-lumaweave.md) (Lattica is LumaWeave extended), [ADR-006](ADR-006-monorepo-structure.md) (monorepo structure, uv workspaces), [ADR-003](ADR-003-eval-core-standalone.md) (eval-core extraction — depends on Cerebra Phase 3 prerequisite)
**Related Plans:** Phase 3 (Memory Foundation / Cerebra), Phase 4 (Visualization Bridge), Phase 5 (Reflective Twin v1), Phase 7 (Cerebra Daemon), Phase 9 (Bo Memory Swap)

---

## Context

Cerebra is Lattica's memory and knowledge layer: Python 3.12+, SQLite WAL + FTS5 + vector embeddings (mxbai-embed-large-v1), LoRA pipeline via Unsloth on an RTX 4070 SUPER. As of Phase 3, Cerebra is CLI-only — `cerebra context`, `cerebra status`, `cerebra metrics` — with no persistent process.

Three consumers need programmatic Cerebra access in the near term:

1. **LumaWeave / Lattica (Phase 4–5):** The Cerebra SQLite → LumaWeave graph adapter needs to query retrieval results and working memory slot state to render them as graph nodes and subgraph overlays.
2. **discord-bot / Bo (Phase 9):** `gather_context()` is the existing seam for a Cerebra swap. Once Phase 5 (working memory + session daemon) and Phase 7 (UDS daemon) both land, Bo should send requests to the daemon rather than subprocess-invoking the CLI.
3. **Lattica status panel (Phase 3):** `cerebra metrics --format json` drives a Prometheus textfile poller and a Cerebra status panel in the Lattica UI.

The critical constraint is **first-query latency.** Model cold load (`mxbai-embed-large-v1`) takes approximately 10 seconds. Every subprocess invocation pays this cost again. That latency is acceptable for an early prototype; it is not acceptable for interactive use, for the Bo context pipeline, or for any future sub-second retrieval requirement.

The decision about API surface is therefore also a decision about **when to build the daemon.** Building a persistent daemon before Cerebra's own Phase 5 (working memory, truth tower, session concept) ships would mean building IPC infrastructure around a process model that does not yet exist. The phased approach is the correct one: prototype ugliness now, clean migration path later.

---

## Forces

- **Don't over-engineer before Phase 5.** Cerebra Phase 5 is the first point at which a session-aware daemon makes architectural sense. A daemon before Phase 5 has no session to maintain.
- **Cold-start is the real problem.** Only a persistent process eliminates the 10s model load penalty. SQLite WAL polling (already possible in Phase 4–5) avoids the penalty but only works for read-only graph rendering, not interactive retrieval.
- **Single-user, local-only.** No multi-tenancy, no remote clients, no authentication requirements. The API surface should reflect this.
- **Source adapter public interface must not change on transport swap.** LumaWeave's source adapter registry has a stable contract (LoaderFn, transport field). The Cerebra adapter must handle both shell-out and socket modes internally — callers never see which transport is active.
- **Migration path must be explicit.** The risk of a phased approach is accumulating technical debt that never gets resolved. The migration path (daemon detection, fallback, adapter internal branching) must be specified now so Phase 7 is a drop-in replacement, not a rearchitecture.

---

## Considered Options

### Option A — HTTP REST (FastAPI on TCP, localhost)

FastAPI with `uvicorn` bound to `127.0.0.1:PORT`. Standard REST interface, portable, familiar.

**Rejected because:** Requires port management (port selection, collision detection, firewall awareness). Adds a network surface that is semantically unnecessary for a single-user local daemon. Port-based access does not map cleanly to Lattica's single-user security model — any process on the machine can connect, not just Lattica modules with filesystem-level access to the socket path. Runtime cost is marginally higher than UDS.

### Option B — Unix Domain Socket (FastAPI-on-UDS, uvicorn --uds)

FastAPI with `uvicorn --uds ~/.cerebra/sockets/cerebra.sock`. Same FastAPI ergonomics, no TCP stack, kernel filesystem permissions.

**Selected for Phase 7.** See decision rationale below.

### Option C — SQLite WAL polling (no daemon, no IPC)

The Cerebra SQLite database is written with WAL mode. LumaWeave and other consumers poll the database file directly (read-only connection). No new process, no IPC protocol.

**Acceptable for Phase 4–5 graph rendering, rejected as the long-term API surface.** Read-only polling cannot support interactive retrieval, session queries, or Bo's context pipeline. It also creates a tight coupling between consumer code and the Cerebra database schema — the SQLite adapter becomes a hidden contract that is hard to evolve. Retained as the Phase 4–5 implementation specifically because it avoids building a daemon before Phase 5.

### Option D — CLI shell-out (current)

`subprocess` call to `cerebra context --format json`, `cerebra status --format json`, etc. Works today. No new code.

**Selected for now through Phase 6.** Slow (process spawn + model load per invocation) but correct. Acceptable for the Phase 3 Lattica status panel and the Phase 4 graph adapter in its initial form.

---

## Decision

**Phased approach:**

| Phase | Transport | Latency profile | Rationale |
|---|---|---|---|
| Now – Phase 6 | CLI shell-out (`cerebra context --format json`) | ~10s per query (model cold load) | Works today, no new process model required, acceptable for prototype graph rendering |
| Phase 7+ | Unix domain socket daemon (`cerebra serve`, FastAPI-on-UDS) | Sub-100ms after daemon startup (model stays loaded) | Phase 5 ships working memory + session concept; daemon now has state worth maintaining |

### Why UDS over TCP (Phase 7)

The Unix domain socket model matches Cerebra's invariants precisely:

- **No port collision.** The socket path (`~/.cerebra/sockets/cerebra.sock`) is chosen by Cerebra, not negotiated at runtime. There is no port registry, no `lsof` check, no `address already in use` race.
- **Filesystem-level permissions.** Access control is `chmod`/`chown` on the socket file — the same model as every other local Unix tool. No token, no header, no authentication handshake.
- **No network surface.** The socket does not appear in `netstat`, is not reachable from another machine, and cannot be accidentally exposed by a firewall misconfiguration.
- **Same FastAPI ergonomics.** `uvicorn --uds /path/to/socket` is a one-argument change. All route definitions, request/response models, and middleware are identical to an HTTP server. The Cerebra team writes FastAPI; they do not learn a new framework.
- **Performance advantage over TCP.** UDS bypasses the TCP stack entirely. For the retrieval-heavy workloads Cerebra handles, this is meaningful at high query rates, though the dominant latency is embedding computation, not transport.

### Daemon design (Phase 7)

**Socket path:** `~/.cerebra/sockets/cerebra.sock`

The directory `~/.cerebra/sockets/` is created on `cerebra serve` startup if it does not exist. Permissions: `700` on the directory, `600` on the socket file (owner-only access). If a stale socket file exists from a previous crash, `cerebra serve` detects the condition (connect attempt fails), removes the stale file, and binds fresh.

**Model loading:** The embedding model (`mxbai-embed-large-v1`) is loaded once at daemon startup and held in memory for the lifetime of the process. First query after `cerebra serve` starts still pays the ~10s model load cost — this is a known implication for Lattica startup sequencing (see Consequences). Subsequent queries are sub-100ms.

**Session state:** Working memory slots (Phase 5 output) are held in the daemon process. Requests from LumaWeave and Bo share the same in-process session state. This is correct for the single-user model; multi-session isolation is not in scope.

**CLI becomes thin client:** After Phase 7, `cerebra context`, `cerebra status`, etc. detect the socket and send the request to the daemon rather than executing locally. If the socket does not exist or the connection fails, the CLI falls back to direct execution (slower, with model cold load). This fallback is silent — the output is identical, the latency difference is the only observable change.

**Graceful shutdown:** `cerebra serve` handles `SIGTERM` and `SIGINT`. On shutdown: stop accepting new requests, complete in-flight requests (bounded wait: 5s), close the model, remove the socket file, exit 0.

**Health endpoint:** `GET /health` returns `{"status": "ok", "model_loaded": true, "uptime_seconds": N}`. Used by Lattica's Cerebra status panel and by the CLI fallback detection logic.

### Why Phase 7, not earlier

Cerebra Phase 5 ships two things the daemon depends on:

1. **Working memory slots** — the in-process state the daemon is meant to maintain. Without Phase 5, a daemon is a shell-out proxy with no session to hold.
2. **Session concept** — the API surface for retrieval traces, memory updates, and context queries is stabilized in Phase 5. Building the daemon's route definitions before this stabilizes means rewriting them when Phase 5 reshapes the interface.

Building the daemon in Phase 3 or 4 would produce a daemon that gets partially rewritten in Phase 5 and again in Phase 7. The Phase 4–5 SQLite WAL polling approach is specifically chosen to avoid this: it gives LumaWeave read access to Cerebra's graph data without requiring any new Cerebra code or process model.

---

## Interface Definition

### CLI shell-out contract (now – Phase 6)

These are stable CLI interfaces. Changes to their output schema are a breaking change requiring a version bump.

```bash
# Retrieve context for a query string
cerebra context --query "..." --format json
# Returns: {"results": [...], "query": "...", "retrieved_at": "..."}

# Retrieve status and metrics
cerebra status --format json
# Returns: {"doc_count": N, "last_ingest": "...", "index_staleness_seconds": N, "model": "mxbai-embed-large-v1"}

# Prometheus-compatible metrics textfile (Phase 3 poller)
cerebra metrics --format prometheus
# Returns: text/plain in exposition format
```

The LumaWeave Cerebra source adapter calls these via `subprocess.run` or Tauri's `Command` API (shell sidecar). Output is parsed as JSON. Errors (non-zero exit, malformed JSON) are surfaced as adapter warnings in the graph source summary.

### UDS API contract (Phase 7)

FastAPI on `~/.cerebra/sockets/cerebra.sock`. All routes accept and return JSON. The HTTP framing is uvicorn's standard behavior over the socket.

```
GET  /health
     → {"status": "ok", "model_loaded": true, "uptime_seconds": N}

POST /context
     body: {"query": string, "top_k": int = 5, "session_id": string | null}
     → {"results": [...], "query": string, "retrieved_at": string, "latency_ms": N}

GET  /status
     → {"doc_count": N, "last_ingest": string, "index_staleness_seconds": N, "model": string}

GET  /memory/slots
     → {"slots": [...]}   # working memory slots (Phase 5 output)

POST /memory/slots/{slot_id}
     body: {"content": string}
     → {"slot_id": string, "updated_at": string}

GET  /metrics/prometheus
     → text/plain, exposition format
```

The `/context` and `/memory` routes are the primary consumers for Bo (Phase 9) and the LumaWeave reflective twin view (Phase 5). The `/status` and `/metrics` routes serve the Lattica status panel.

### Source adapter internal branching

The LumaWeave Cerebra source adapter (`src/adapters/cerebra.ts` or equivalent) encapsulates the transport selection. The public interface — `LoaderFn`, `transport` field in the registry, the data shape returned to the graph engine — does not change between Phase 4–5 (SQLite polling / shell-out) and Phase 7+ (UDS).

Internal logic:

```
load(params):
  if CEREBRA_DAEMON_SOCKET exists and is connectable:
    → HTTP request to UDS socket (fetch via node-fetch or Tauri http plugin)
  else if cerebra CLI is available:
    → subprocess shell-out, parse JSON stdout
  else:
    → warn, return empty graph
```

The socket path is read from an environment variable `CEREBRA_SOCKET` if set, defaulting to `~/.cerebra/sockets/cerebra.sock`. This allows test environments to point at a mock socket.

### `cerebra serve` CLI commands (Phase 7)

```bash
cerebra serve                          # start daemon, blocks (model loads, then accepts connections)
cerebra serve --socket /custom/path    # override socket path
cerebra serve stop                     # send SIGTERM to running daemon (via pidfile)
cerebra serve status                   # check if daemon is running, report socket path + uptime
```

Pidfile at `~/.cerebra/sockets/cerebra.pid`. The `stop` and `status` subcommands use the pidfile, not process name search.

---

## Migration Path

### Phase 4–5: SQLite WAL polling + shell-out

The Cerebra SQLite database (`~/.cerebra/db/cerebra.db`) is opened read-only by the LumaWeave graph adapter. WAL mode allows concurrent readers without blocking Cerebra writes. The adapter polls on a configurable interval (default: 30s for graph snapshots, not a real-time feed).

For the Lattica status panel (Phase 3), `cerebra metrics --format prometheus` is invoked via shell-out on a 15s Prometheus scrape interval. Latency is acceptable at this frequency.

### Phase 7: Transport swap

1. `cerebra serve` is added as a new CLI subcommand in the Cerebra codebase.
2. The daemon binds to `~/.cerebra/sockets/cerebra.sock`.
3. The LumaWeave source adapter's internal branching (see above) detects the socket and switches to UDS transport automatically on the next load cycle — no config change, no restart required.
4. The CLI (`cerebra context`, etc.) is updated to detect the socket and proxy to the daemon. CLI output is byte-for-byte identical to the current direct-execution output.
5. The Phase 4–5 SQLite polling path remains as the final fallback (daemon down, CLI unavailable) — it is never removed, only deprioritized.

### Phase 9: Bo memory swap

`gather_context()` in the discord-bot is replaced with a direct HTTP request to the UDS daemon's `/context` endpoint. The request is made via the daemon socket, not via subprocess. This path is only wired after both Phase 5 (session concept stable) and Phase 7 (daemon exists) are confirmed green.

---

## Consequences

### Positive

- **Zero new code before Phase 7.** The CLI shell-out path uses Cerebra as-is. Phase 3–6 work requires no changes to Cerebra's process model or internal architecture.
- **Sub-100ms retrieval in Phase 7.** Eliminating the per-query model cold load is the primary goal. The daemon achieves this.
- **Clean public interface.** Consumers (LumaWeave, Bo, Lattica panels) never see the transport layer. The source adapter's public contract is stable across the Phase 4 → Phase 7 migration.
- **Filesystem-level access control.** No token management, no authentication code, no session headers. The socket file's Unix permissions are the access control policy.
- **Fallback chain is explicit.** UDS → CLI shell-out → SQLite polling → empty graph. Each step degrades gracefully with a logged warning.

### Negative / Risks

- **Phase 7 startup latency is still paid once.** Even with the daemon, Lattica's first Cerebra-dependent view after a cold boot waits ~10s for `mxbai-embed-large-v1` to load. Lattica startup sequencing must account for this: the Cerebra status panel should show a "model loading" state rather than an error during the first 10–15s after daemon start.
- **Shell-out latency is a user-visible problem through Phase 6.** Any interactive feature that calls `cerebra context` during Phase 4–6 will feel slow. The design constraint is: don't build interactive features that require sub-second Cerebra responses before Phase 7 lands.
- **Socket file lifecycle needs care.** A crashed daemon leaves a stale socket file. The startup stale-socket detection and removal logic (see daemon design) must be correct, or subsequent `cerebra serve` invocations will fail silently. Integration tests should cover the stale-socket case.
- **CLI thin-client fallback adds a code path.** After Phase 7, the Cerebra CLI has two execution modes (proxy-to-daemon and direct). This doubles the test surface for CLI commands. The fallback path must be integration-tested, not just unit-tested.
- **RTX 4070 SUPER VRAM budget.** `mxbai-embed-large-v1` held in memory by the daemon competes for VRAM with inference workloads (Ollama, LoRA training). The Lattica GPU allocation panel (Phase 11) should track this reservation. Until Phase 11, the developer must be aware of this contention and may need to stop the daemon during training runs.

---

## Notes

- The socket path `~/.cerebra/sockets/cerebra.sock` uses a `sockets/` subdirectory rather than placing the socket directly in `~/.cerebra/` to allow future addition of other local sockets (e.g., a Cerebra IPC socket for inter-process signaling) without cluttering the top-level config directory.
- `uvicorn --uds` is available in uvicorn 0.12.0+. The Cerebra Python environment (managed via uv workspaces per ADR-006) must pin uvicorn at this version or above. No new runtime dependency — FastAPI and uvicorn are already Cerebra's web layer.
- The decision to use FastAPI on UDS rather than a custom asyncio socket protocol is deliberate: FastAPI gives Cerebra's team standard request validation, OpenAPI schema generation (useful for debugging), and middleware hooks (logging, error handling) at no additional complexity cost. The transport is UDS; the application protocol is HTTP/1.1 over that socket, not a bespoke framing.
- This ADR does not govern Cerebra's internal architecture (embedding pipeline, LoRA training, truth tower, retrieval algorithm). Those are Cerebra-domain decisions. This ADR governs only the external API surface that other Lattica modules consume.