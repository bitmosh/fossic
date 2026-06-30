# ES Consumer Profile — discord-bot (Bo)

You are the discord-bot advocate. We're building **ES**, a local-first
event sourcing library: Rust core, PyO3/napi-rs bindings, SQLite-backed,
content-addressed events, branchable history, pure synchronous reducers.
It's being extracted from Lattica's Phase 6 plans because discord-bot and
several other projects also want event sourcing capabilities.

Before we lock the v1 API, we need your project's actual shape. Please
answer the questions below as concretely as possible — "I don't know yet"
and "we don't care about this" are both valid and useful answers.

If you need to look at the codebase to answer any of these, do so. Cite
files/lines where helpful.

## Language and runtime
1. **Primary language(s)?** Python 3.12 (venv-isolated). No Rust, no TypeScript.

2. **In-process embedded library, long-running daemon, CLI tool, or hybrid?**
   Long-running daemon. `bot.py` runs as a persistent async process connected to Discord's gateway.

3. **Multi-process? Multi-thread? Async runtime in use?**
   Single process, single thread, asyncio throughout. All Discord I/O and HTTP calls (`discord.py`, `AsyncOpenAI`, `httpx`) are async. No threads in use.

## What you want from ES
4. **The three top things you want from event sourcing:**
   - **Conversation persistence** — replace the current session-only context window (`gather_context()` fetches live Discord history on every message; `bot.py:252–290`). ES would let us persist and replay conversation streams across restarts without re-fetching from Discord.
   - **Agent trace logging** — the bot produces `(thinking, content, status_tag)` tuples on every inference (`bot.py:290–404`). Right now those thinking traces disappear after being posted to Discord threads. We want a durable, queryable log of reasoning chains for debugging and future replay.
   - **Lightweight audit trail** — who triggered what, which LLM backend was used, whether a retry or synthesis fallback fired. Currently only surfaced via status tags in Discord messages (`"_(retried)_"`, `"_(synthesized from reasoning)_"`); no persistent record.

5. **Anything from the ES spec you DON'T need?**
   - **Branching** — don't need it. This is a 1:1 private bot with linear conversation history. No experiments, no forks.
   - **OTel export** — don't need it at this stage. No metrics infrastructure, no tracing backend.
   - **Snapshots** — we don't care about them yet. Context windows are small (≤10 messages by default, `CONTEXT_HISTORY_SIZE`). Full replay is fine.
   - **Multi-agent / agent trace adapter** — could be interesting later (Bo mentions Bandit as a collaborator), but not a v1 need.

## Scale and shape of writes
6. **Estimated write rate at steady state:**
   Very low. Maybe 2–10 events/minute during an active conversation, effectively 0 when idle. This is a single developer's private bot.

7. **Burst profile:**
   Tight clusters during an active exchange (user sends message → bot replies → user follows up), then silence for hours. No sustained load.

8. **Typical payload size? Maximum payload size?**
   Typical: 200–1000 bytes per Discord message or LLM reply. Thinking traces can be larger — Qwen 3.5 reasoning blocks can run 2–8KB. Absolute max we'd ever expect: ~16KB (a long thinking trace). Nothing exceeds 64KB.

9. **Number of distinct streams? Events per stream per day?**
   A handful of Discord channels (≤5 active). Each channel is a stream, or each thread within a channel is a sub-stream. At most 20–50 events/stream/day on an active day, effectively 0 most days.

10. **Single writer per stream, or concurrent writers?**
    Single writer. One bot process, one Discord account.

## Reads
11. **Read patterns:**
    Primarily **tail-the-latest** — `gather_context()` wants the N most recent messages to build the LLM context window (`bot.py:252`). Occasionally linear replay from beginning of a thread when summarizing. No random access by event ID today.

12. **Live subscriptions: real-time delivery or polling?**
    Polling is fine. The bot's latency requirement is "respond before the user notices a delay" — a few hundred milliseconds. No sub-second streaming needed from the store itself; Discord's gateway push handles real-time delivery of new messages.

13. **Cross-stream queries needed?**
    Not today. Could be useful later ("what did Bo say about X across all channels"), but not a v1 requirement. We'd rather keep it simple.

## Persistence and lifecycle
14. **How long do events need to live?**
    "We'll cross that bridge later." Probably indefinitely for reasoning traces (they're rare and valuable for debugging), and 30–90 days for raw message history. No hard requirement yet.

15. **Any need to delete individual events?**
    Not currently. No PII obligations articulated. The server is private and single-user. If Discord message content is ever considered sensitive, we'd want tombstoning — but that's not a v1 driver.

16. **Acceptable storage growth?**
    Not a concern at this scale. The bot produces maybe 1–5MB/month of event data. Bounded by disk is fine; we'll never hit it.

17. **Backup/restore expectations?**
    Just-the-SQLite-file is perfectly fine. No streaming backup, no point-in-time recovery needed.

## Security and deployment
18. **Sensitive data in event payloads?**
    Potentially yes — Discord messages from a private server could contain personal conversation content. Not credentials or secrets, but private chat. Treat as "user content, private."

19. **Encryption at rest required?**
    No hard requirement. It's a local single-user setup. Would be nice-to-have but not a blocker.

20. **Single-user local-first, or multi-user / multi-tenant?**
    Single-user, local-first. One developer, one bot, one server.

21. **Deployment target:**
    Developer workstation today. Possibly a low-power server (Raspberry Pi class) in the future. Definitely not container orchestration or cloud.

## Existing event/log infrastructure
22. **Does this project already have an event store or audit log?**
    No. Context is entirely session-in-memory. `gather_context()` (`bot.py:252`) re-fetches Discord history on every trigger using the Discord API. There is no local store, no schema, nothing to migrate. Fresh start.

23. **If migrating: how many existing events?**
    N/A — no migration. We'd be bootstrapping from zero.

## Integration shape
24. **How would you want to call ES?**
    In-process embedded library, called from async Python. The natural integration point is `gather_context()` (`bot.py:252`) — replace the live Discord fetch with an ES-backed query for recent events, then append new events after each exchange. A secondary integration point is `ask_local_model()` (`bot.py:290`) — append thinking trace + final response as events for the trace log.

25. **Anything in the codebase that would make integration awkward?**
    One friction point: **ES uses pure synchronous reducers**, and the entire bot is asyncio. Calling synchronous Rust-backed code from an async context requires `loop.run_in_executor()` or a sync wrapper — not a blocker, but it's a seam that needs care. The `gather_context()` function is already documented as "the seam for future memory architecture" (`bot.py:252`), so the plug-point is intentional and clean.

## Open questions and concerns
26. **What would make you NOT adopt ES?**
    - If the PyO3/napi-rs bindings don't have a Python wheel and require a Rust toolchain to install — this bot runs on a workstation where `pip install` is the expected flow.
    - If the SQLite WAL or locking behavior causes the bot process to stall during high-frequency reads (e.g., every message triggers a context fetch). At this scale it's unlikely, but worth verifying.
    - If the API surface is large — this bot is intentionally 600 lines and growing slowly. We'd want a thin slice: append-event, fetch-recent-N, done.

27. **Anything in the ES design you'd push back on?**
    The **pure synchronous reducer** constraint is the only flag. If reducers must be sync, projections derived from conversation history (e.g., "summarize last 20 messages for context") can't be async — that's fine for simple reads, but if we ever want a projection that itself calls the LLM (to summarize before feeding context), it'll need an executor bridge. Worth noting in the API docs. Not a blocker for v1.

---

## Bo (discord-bot)

**Summary for ES_CONSUMER_PROFILES.md:**

Bo is a Python 3.12 asyncio Discord bot (~610 lines, single file) that routes @mentions to a local Qwen 3.5 LLM via LiteLLM/Ollama. It is single-user, local-first, low-volume (2–10 events/burst, effectively 0 at idle), and has no existing event store — context today is live-fetched from Discord's API on every message. The primary integration point is `gather_context()` (`bot.py:252`), already designed as a plug-in seam for persistent memory. ES would replace live Discord fetches with local event replay and add a durable reasoning-trace log. Requirements are minimal: append-event, fetch-recent-N, SQLite on a workstation. Does not need branching, OTel, or snapshots. Payloads max ~16KB. One friction point: synchronous ES reducers will need `run_in_executor()` wrappers in an asyncio context.
