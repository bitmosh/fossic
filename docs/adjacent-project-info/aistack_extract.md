# ES Consumer Profile — ai-stack / claw-code

You are the ai-stack advocate. We're building **ES**, a local-first
event sourcing library: Rust core, PyO3/napi-rs bindings, SQLite-backed,
content-addressed events, branchable history, pure synchronous reducers.
It's being extracted from Lattica's Phase 6 plans because ai-stack and
several other projects also want event sourcing capabilities.

Before we lock the v1 API, we need your project's actual shape.

---

## Language and runtime

**1. Primary language(s)?**

The ai-stack is a Docker Compose stack, not a single-language project:

- **Infrastructure layer**: Docker / YAML. Four services:
  - `ollama` — GPU-accelerated local LLM inference (pre-built image, no code)
  - `litellm` — OpenAI-compatible API proxy, Python (async), custom Dockerfile based on `ghcr.io/berriai/litellm:main` with `async_generator` added
  - `tts` — text-to-speech via `openedai-speech`, GPU-enabled, Python
  - `open-webui` — React chat frontend served at `:3000`, talks to LiteLLM at `:4000`

- **Agent harness sub-project (`claw-code`)**: Rust (primary runtime, `cargo` workspace in `rust/`) + Python companion workspace in `src/` (Python 3.x, `venv` present). The Python layer is the reference implementation; Rust is the production surface.

**2. In-process embedded library, long-running daemon, CLI tool, or hybrid?**

Hybrid across two surfaces:
- The four Docker services are **long-running daemons** (container restart policy `unless-stopped`).
- `claw-code` is a **CLI tool** (`claw prompt "..."`, `claw doctor`, etc.) that spawns sessions and exits. The Python companion is also CLI-oriented.

**3. Multi-process? Multi-thread? Async runtime?**

- Docker services run in separate processes (separate containers, separate network namespaces). They communicate over HTTP (OpenAI-compatible APIs on `:11434`, `:4000`).
- LiteLLM is **async Python** (likely asyncio under the hood — it wraps aiohttp/httpx).
- The TTS service is Python, likely sync or gevent.
- `claw-code` Rust: unclear from source whether Tokio is in use; the Python companion is **synchronous** (no `asyncio` in the Python files read).
- Open WebUI is a containerized web server (its internals are not in this repo).

---

## What you want from ES

**4. The three top things you want from event sourcing:**

1. **Agent trace logging and replay** — `claw-code` currently tracks session events in `TranscriptStore` (in-memory list), `HistoryLog` (in-memory title/detail pairs), and `stream_events` (a tuple of raw dicts on `RuntimeSession`). These are ephemeral or saved as flat JSON files in `.port_sessions/`. There is no queryable history, no causal ordering, and no replay from a durable store. ES would replace this with content-addressed, replayable session traces — critical for debugging autonomous agent failures after the fact.

2. **LLM call audit trail** — `CostTracker` is in-memory only (`total_units` + list of `label:units` strings). LiteLLM logs requests internally but doesn't expose them durably to other services. An append-only event store per session/stream would make token spend, routing decisions (which model alias resolved to which backend), and latency fully auditable without scraping logs.

3. **Cross-service event stream for clawhip integration** — The broader claw-code philosophy (see `PHILOSOPHY.md`, `ROADMAP.md`) is building toward typed lifecycle events: `lane.started`, `lane.ready`, `prompt.accepted`, `lane.commit.created`, etc. Today these are emitted as Discord channel messages or scraped from terminal output. ES would provide the durable, ordered, machine-readable backbone that clawhip currently has to reconstruct heuristically from prose.

**5. ES features you DON'T need or actively don't want:**

- **Branching for experiments** — the roadmap mentions `branch.stale_against_main` but that's a git branch concept, not ES branch. A single developer running serial sessions has no need for ES-level branching.
- **OTel export** — not in the stack today; would be a nice-to-have but not a v1 requirement.
- **Cross-process pub/sub at ES layer** — the services communicate via HTTP already. ES as a shared in-process event log is more useful than ES as a message bus; pub/sub can stay at the LiteLLM / clawhip network layer.
- **Snapshots** — session event streams are short enough (hundreds of events per session, not millions) that replaying from zero is fine.

---

## Scale and shape of writes

**6. Estimated write rate at steady state:**

Low. A human-paced interactive session generates roughly 10–50 events/minute during active use. An autonomous claw loop running unattended might hit 100–300 events/minute during a busy execution phase.

**7. Burst profile:**

Tight clusters. A single LLM turn generates a batch of events atomically: `stream_events` (multiple), `turn_result`, `command_execution_messages`, `tool_execution_messages` — then silence until the next prompt. Bursts last seconds; quiet periods last minutes.

**8. Typical payload size? Maximum?**

- Small for lifecycle/history events: `HistoryEvent` is a title string + detail string, typically < 500 bytes.
- Medium for stream events: raw JSON dicts from the LLM streaming API, typically 1–10 KB per event.
- Large for turn results: full LLM response text can be 10–100 KB for long coding outputs.
- Maximum: LiteLLM response payloads from a large model could theoretically exceed 64 KB for very long completions. This is not routine but is possible.

**9. Number of distinct streams? Events per stream per day?**

A single developer might run 5–20 sessions per day. One stream per session is the natural model. Each session stream might accumulate 50–500 events. Total: a few thousand events/day across all streams.

**10. Single writer per stream, or concurrent writers?**

Single writer per stream. One session = one agent process = one writer. No concurrent writers to the same session stream.

---

## Reads

**11. Read patterns:**

- **Linear replay from version 0** — the primary use case for session debugging (reconstruct what happened).
- **Tail-the-latest** — for live clawhip monitoring of an active session.
- Random access by event ID is not needed.
- Time-range queries would be useful for "show me all events from the last run" but not critical at v1.

**12. Live subscriptions needed?**

Polling is acceptable for v1. Real-time delivery (clawhip watching an active lane) would be a nice upgrade but is not blocking. If real-time: latency of 1–2 seconds is acceptable; sub-100ms is not required.

**13. Cross-stream queries needed?**

Nice-to-have, not required at v1. Example: "all `prompt.accepted` events across all sessions today." The current design has no cross-session queries at all. A simple linear scan across stream metadata would satisfy most needs.

---

## Persistence and lifecycle

**14. How long do events need to live?**

Effectively forever for a single developer. Session volumes are small enough that there is no practical pressure to expire anything. Configurable retention would be welcome but is not a v1 requirement.

**15. Need to delete individual events?**

No. There is no PII in the payload (the system handles code and LLM responses, not user personal data). No compliance driver for deletion.

**16. Acceptable storage growth?**

Bounded by disk in practice. At the write rates above, even a year of sessions would be tens of megabytes — well within any modern disk. No retention policy needed; "we'll cross that bridge later" applies.

**17. Backup/restore expectations?**

Just-the-SQLite-file is fine. The developer already has `webui.db` and `chroma.sqlite3` on disk with no special backup infrastructure.

---

## Security and deployment

**18. Sensitive data in event payloads?**

- LLM response content may contain proprietary code or internal reasoning.
- API keys (`ANTHROPIC_API_KEY`, `sk-fake` LiteLLM master key) are passed via environment variables and should never land in event payloads — this is a caller-side discipline concern, not an ES-level one.
- No PII expected.

**19. Encryption at rest required?**

No. Single-developer local machine. OS-level disk encryption (if any) is sufficient.

**20. Single-user local-first, or multi-user?**

Single-user, local-first. The entire stack runs on one developer workstation.

**21. Deployment target:**

Developer workstation running Docker. All services are containers on the local machine. The Rust `claw` binary runs natively on the host. No cloud, no server, no edge devices.

---

## Existing event/log infrastructure

**22. Does this project already have an event store or audit log?**

Yes, several partial ones that don't talk to each other:

| Surface | Location | Format | Queryable? |
|---|---|---|---|
| Chat history | `webui-data/webui.db` | SQLite (Open WebUI schema) | Via WebUI only |
| Vector store | `webui-data/vector_db/chroma.sqlite3` | ChromaDB SQLite | Via ChromaDB API |
| Session store | `.port_sessions/<id>.json` | JSON files | No — filename lookup only |
| Transcript | `TranscriptStore` in `src/transcript.py` | In-memory list | No — only `replay()` dumps all |
| History log | `HistoryLog` in `src/history.py` | In-memory list of `HistoryEvent` | No |
| Cost tracker | `CostTracker` in `src/cost_tracker.py` | In-memory list of strings | No |
| LiteLLM logs | Container stdout + LiteLLM internals | Text/structured | Via LiteLLM UI only |

The session JSON files (`StoredSession`: session_id, messages tuple, input_tokens, output_tokens) are the closest thing to a durable event store. They are written once per session and never appended to.

Migration posture: **bridge first, then selectively migrate**. The WebUI SQLite and ChromaDB are owned by upstream projects and should be left alone. The claw-code Python/Rust surfaces (session store, transcript, history, cost tracker) are owned locally and would benefit most from ES migration.

**23. If migrating: how many existing events would need translation?**

Minimal — this is a relatively new local setup. At most a few dozen JSON session files. Translation would be straightforward: one `StoredSession` → one stream with a handful of events (session_started, each message, session_ended with token counts).

---

## Integration shape

**24. How would you want to call ES?**

- **In-process API** for the `claw-code` Rust binary (via the Rust core directly) and for the Python companion (via PyO3 bindings).
- A **file-watching bridge or HTTP adapter** for LiteLLM: LiteLLM is a containerized Python service that can't easily embed a Rust library; a sidecar or callback hook writing to a shared SQLite file is more practical.
- Open WebUI: read-only adapter at most — we'd read events from ES to display session history, not write from WebUI to ES.

**25. Anything in the codebase that would make integration awkward?**

- **LiteLLM is async Python** (`asyncio`) — the PyO3 bindings would need to be safe to call from async contexts, or we'd need a sync wrapper / background thread pattern.
- **Container isolation** — LiteLLM, Ollama, and TTS run in separate containers. A shared SQLite file would require a volume mount across containers; a network-accessible ES daemon would require a new service. Neither is terrible, but both add complexity.
- **claw-code Rust**: the existing Python session store uses flat JSON files; replacing it with ES would be a clean win but requires touching the `session_store.py` / Rust equivalent and the `RuntimeSession` struct.
- The Python companion in `src/` appears to be a reference/porting workspace rather than production code — so Python integration is lower priority than the Rust integration.

---

## Open questions and concerns

**26. What would make you NOT adopt ES?**

- If the Rust core requires a separate daemon or background thread just to do simple appends — the current session store is a one-liner `path.write_text(...)`. If ES adds significant startup latency or process complexity for what is today a CLI tool, that friction is real.
- If the SQLite file must be accessed by multiple containers simultaneously without a locking story. LiteLLM and claw may both want to write events; SQLite WAL mode handles this, but it needs to be explicitly designed for.
- If PyO3 bindings don't support being called from async Python cleanly — LiteLLM is the most interesting integration point and it's fully async.

**27. Anything in the ES design you'd push back on?**

- **Pure synchronous reducers**: fine for the CLI harness, but LiteLLM's middleware context is async. If "pure synchronous reducer" means "cannot be called from an async event loop without `run_in_executor`," that's a friction point worth solving at the binding layer.
- **Content-addressed events**: welcome for dedup and replay integrity, but the address scheme needs to handle large payloads (LLM response text up to 100 KB) without making append noticeably slower than a file write.
- **Branchable history**: not needed here, but the API surface for it shouldn't add conceptual weight to the common case (linear append + replay). If branching is opt-in and the default API is just `append` / `replay`, no objection.

---

Compile the response as a single markdown section under
"## ai-stack / claw-code" suitable for inclusion in
ES_CONSUMER_PROFILES.md.
