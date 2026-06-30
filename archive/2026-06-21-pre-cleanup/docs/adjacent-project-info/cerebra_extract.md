# ES Consumer Profile — Cerebra

You are the Cerebra advocate. We're building **ES**, a local-first
event sourcing library: Rust core, PyO3/napi-rs bindings, SQLite-backed,
content-addressed events, branchable history, pure synchronous reducers.
It's being extracted from Lattica's Phase 6 plans because Cerebra and
several other projects also want event sourcing capabilities.

Before we lock the v1 API, we need Cerebra's actual shape. Answers below
are grounded in the current codebase at `/home/boop/Projects/cerebra`
(v0.2.8, Phase 5 complete).

---

## Cerebra

### Language and runtime

**1. Primary language(s)?**
Python 3.12+. Strict mypy, ruff + black lint, pytest. Packaged with `uv` /
setuptools. Entry point: `cerebra` CLI via Click.

**2. In-process embedded library, long-running daemon, CLI tool, or hybrid?**
Hybrid: CLI tool for user-facing commands plus an in-process library used
internally during cognitive cycle execution. Inspector events are emitted
in-process as side effects of cognition actions — not sent to a daemon.
No server process runs between CLI invocations.

**3. Multi-process? Multi-thread? Async runtime?**
Single-process, single-thread, fully synchronous. No asyncio, no threading.
The codebase has zero async patterns. This is a deliberate constraint —
cognitive cycle execution is intended to be sequential and inspectable.

---

### What you want from ES

**4. The three top things you want from event sourcing:**

1. **Immutable cognitive action audit trail.** Every cognitive cycle action
   must emit a structured event that survives the process lifetime. Today
   this is a custom NDJSON + SQLite dual layer (see Q22). ES would replace
   both. The core invariant: if SQLite is lost, events can be replayed to
   rebuild queryable state.

2. **Agent trace export to LumaWeave.** The inspector event stream is
   Cerebra's integration contract with LumaWeave. LumaWeave reads events
   and renders them as typed graph nodes and edges. Events need to be
   queryable by session, by subject (memory record ID), and by time range.
   ES's structured log is the ideal substrate for this.

3. **Cross-correlation queries by session and subject.** The inspector CLI
   needs "all events in session X" and "all events touching memory record Y."
   Today these are `query_by_session` and `query_by_subject` on the SQLite
   log (`cerebra/inspector/sqlite_log.py:67–94`). These are the read
   patterns we'd expect ES to satisfy natively.

**5. Anything you DON'T need or actively don't want?**

- **Branches:** Not needed for v0.1. Could be useful later for running
  two cycle configurations against the same memory state (cognitive
  counterfactuals), but not blocking and shouldn't complicate the minimal
  integration path.
- **OTel export:** Don't need it. Not in scope.
- **Cross-process pub/sub / real-time subscriptions:** Not needed. Cerebra
  is single-process; polling is fine. Real-time delivery latency is
  irrelevant.
- **Distributed / multi-tenant anything:** Local-first, single-user. Hard
  no on anything that requires a network or multi-tenant auth layer.

---

### Scale and shape of writes

**6. Estimated write rate at steady state?**
Very low. A developer runs a cognitive cycle manually via CLI. Estimate:
10–100 events per cycle invocation, a few invocations per hour at most.
Steady-state is effectively 0 events/sec between runs. Peak burst is
~20 events/sec during a busy cycle step. This is not a high-throughput
workload.

**7. Burst profile?**
Tight clusters during cycle execution, followed by long idle periods.
The pattern is: `cerebra run-cycle` fires a burst of events over a few
seconds, then nothing until the next invocation.

**8. Typical payload size? Maximum? Any payloads >64KB?**
Typical: 100–500 bytes (a few structured fields plus a short summary).
Maximum: ~2–4KB for payloads that include chunk content excerpts or
retrieval trace summaries. Almost certainly never >64KB — nothing in the
event schema carries raw embeddings or document blobs.

**9. Number of distinct streams? Events per stream per day?**
Probably one stream per vault session. A developer might have 2–5 active
sessions in a working day. Events per stream: 50–500 for a typical session.
May also want a vault-global stream for system lifecycle events
(`VaultCreated`, `MigrationRun`) that belong to no session.

**10. Single writer per stream, or concurrent writers?**
Single writer. Single-process, single-user CLI. No concurrency concern.

---

### Reads

**11. Read patterns?**
All of the following are used today:
- Linear replay from beginning (debug a full session)
- Tail-the-latest (live `cerebra memory status` rendering)
- Time-range queries (inspector CLI — "what happened in the last hour")
- Random access by event_id (specific event lookup)
- Filtered by `session_id` (all events in a session, ordered by timestamp)
- Filtered by `subject_id` (all events touching a specific memory record)

**12. Live subscriptions, or polling acceptable?**
Polling is fine. No real-time delivery requirement in v0.1 or the foreseeable
future. The CLI inspector rerenders on demand, not on push.

**13. Cross-stream queries needed?**
Yes — by correlation field, not by scanning all streams. Specifically:
`session_id` as a cross-stream correlation key ("all events belonging to
session sess_xyz, regardless of which stream they landed in"). `subject_id`
as a secondary correlation key ("all events touching memory record M3_abc").
These are the two query patterns currently implemented in
`cerebra/inspector/sqlite_log.py`.

---

### Persistence and lifecycle

**14. How long do events need to live?**
Forever, ideally. The cognitive history IS the memory — being able to
replay why a memory record was created or promoted is a first-class
feature, not a nice-to-have. Configurable retention for large vaults
would be acceptable, but the default should be indefinite.

**15. Any need to delete individual events?**
No hard requirement for v0.1. Might want session-level tombstoning later
("forget everything from this session"), but not individual event deletion.
No PII concern by design — local-first, single-user, user's own content.

**16. Acceptable storage growth?**
"We'll cross that bridge later." Single-user local workstation; at the
write rates described above this won't be a problem for years. No bounded
retention policy needed now.

**17. Backup/restore expectations?**
Copy-the-SQLite-file is fine for v0.1. No streaming backup, no
point-in-time recovery, no replication. The NDJSON log already serves as
the human-readable backup; ES's append-only log would take that role.

---

### Security and deployment

**18. Sensitive data in event payloads?**
Potentially yes. Event payloads can contain document content excerpts,
memory record text, and user queries. This is the user's own local
content (their codebase, docs, notes), not cross-user data. No secrets
or credentials are expected in the event stream by design.

**19. Encryption at rest required?**
Not required for v0.1. Configurable encryption would be a welcome future
option (let the OS keyring manage the key), but blocking on it would delay
adoption unnecessarily.

**20. Single-user local-first, or multi-user/multi-tenant?**
Single-user, local-first. Hard constraint, not a roadmap item to revisit.

**21. Deployment target?**
Developer workstation only, in v0.1. Linux-first (current dev environment),
with Mac as a secondary target when Lattica desktop ships.

---

### Existing event/log infrastructure

**22. Does Cerebra already have an event store?**
Yes — a custom dual-layer setup built in Phase 0:

- **NDJSON append-only log** at `<vault>/events/events.ndjson` — the
  authoritative record. One JSON object per line. Written by
  `cerebra/inspector/ndjson_log.py`.
- **SQLite `inspector_events` table** — the queryable index. Written by
  `cerebra/inspector/sqlite_log.py`. Designed to be rebuildable from the
  NDJSON if lost.

Current event envelope schema:
```
event_id        TEXT  — "evt_<12-hex-uuid>"
event_type      TEXT  — controlled vocabulary (see event.py)
schema_version  INT   — currently always 1
timestamp       INT   — unix epoch seconds
session_id      TEXT  — nullable; null in Phase 0 events
cycle_id        TEXT  — nullable
step_id         TEXT  — nullable
subject_id      TEXT  — nullable; the resource this event touches
actor           TEXT  — e.g. "cycle_runtime"
summary         TEXT  — human-readable one-liner
data_json       TEXT  — JSON blob for event-specific payload
```

The intent was always to replace this with ES when it existed. Prefer
**full migration** (stop maintaining the custom layer) over bridging.

**23. If migrating: how many existing events would need translation?**
Very few — early development, probably a few hundred to a few thousand
events total across all test vaults. Migration cost is negligible.

---

### Integration shape

**24. How would you want to call ES?**
In-process Python API via PyO3 bindings. The cognition module calls
`emit_event()` (or equivalent) synchronously as a side effect of cognitive
actions — no IPC, no file-watching. The call must not block cycle execution;
a fire-and-forget synchronous append is the right model.

**25. Anything in the codebase that would make integration awkward?**
One significant friction point: **packaging a PyO3 extension alongside a
pure-Python `uv`-managed project.** Today `uv sync --extra dev` is the
entire setup. Adding a Rust extension means requiring a Rust toolchain at
build time (or maintaining pre-built wheels per platform). This is the
biggest adoption risk. Pre-built wheels for Linux x86_64 and macOS arm64
would close most of the gap.

No async patterns to worry about. No custom serializers that would conflict.
The existing `event_id` scheme (`evt_<uuid>`) would need to align with or
alias ES's content-addressed IDs — whether ES keeps a user-supplied ID
field alongside the blake3 hash matters for backward compatibility with
any stored events.

---

### Open questions and concerns

**26. What would make you NOT adopt ES?**
- If `uv sync` stops working for a developer who doesn't have Rust installed
  and there are no pre-built wheels available for their platform.
- If the API requires async (non-negotiable for Cerebra — the cognitive
  runtime is synchronous by design and that won't change).
- If schema evolution is rigid — Cerebra needs `schema_version` as a
  first-class field and must be able to add new event types without
  touching the ES library.
- If the minimal path (append + a few query patterns) pulls in the full
  branching/snapshot machinery as non-optional overhead.

**27. Anything in the ES design you'd push back on?**
- **Pure synchronous reducers** is a perfect fit. No pushback there.
- **Branchable history** is a nice-to-have for Cerebra (run two cycle
  configs against the same memory state, compare outcomes), but it should
  be opt-in — not something the simple append path has to reason about.
- **Content-addressed event IDs (blake3):** Cerebra currently uses
  `evt_<uuid>` IDs that are referenced in downstream code and test
  assertions. ES should either preserve a user-supplied ID field alongside
  the content hash, or provide a stable deterministic ID that doesn't
  change between writes.

---

## Additional Notes

The following context doesn't fit the ES consumer profile questions but
may be relevant for ES design and integration planning.

### Cerebra's role in the Lattica organism

Cerebra is the **hippocampus** — it remembers, attends, evaluates,
consolidates, and decides what cognitive process should happen next. It is
not the visualization layer (LumaWeave), not the governance daemon (Policy
Scout), not the inference layer (ai-stack). Its event stream is the primary
integration surface with LumaWeave: events become graph nodes and edges in
the reflective twin.

### Current implementation status (Phase 5 complete, v0.2.8)

Implemented modules: CLI, cognition (SKU addressing, truth tower, working
memory, Phase Lattice), governance (constitutional + leeway loaders),
inspector (event envelope, SQLite + NDJSON logs), memory records, retrieval
(ContextPacket, hybrid planner, salience scorer, lattice dedup), ingest
(markdown/text adapters, chunking, normalization pipeline), sources
(registry, hashing, discovery, detection), storage (SQLite store,
embeddings, graph store, lexical index, migration framework), vault init.

Phase 6 (cognitive extensions) is in design. ES adoption would ideally
land before or during Phase 6 so new phase events are emitted natively
through ES rather than the custom layer.

### Event vocabulary (controlled, versioned per phase)

Phase 0 types: `SystemInitialized`, `VaultCreated`, `MigrationRun`,
`ConfigLoaded`, `LeewayRuleLoaded`, `ConstitutionalBlock`.

Phase 5 types: `WorkingMemoryCreated`, `AttentionItemProposed/Promoted/
Evicted/Deferred`, `InterruptCandidateCreated`, `WorkingMemoryRendered/
Cleared`, `TowerInitialized`, `TowerItemPromoted/Evicted/CrossReferenceAdded/
Staled/Rendered`, `TowerTierRebuilt` (Phase 6+), `TowerCollapsed` (Phase 6+).

Lattice types: `LatticeCommit`, `LatticeSiblingResolved`.

New event types are added per phase in `cerebra/inspector/event.py`. ES
must not require recompiling or reconfiguring the library to register new
event type strings.

### LumaWeave rendering boundary

Cerebra ships: event emission infrastructure, structured event log, CLI
text rendering, query API. LumaWeave handles: visual graph rendering,
cross-session exploration, time-scrubber views. The rule is **Cerebra is
independently inspectable without LumaWeave** — the CLI inspector is the
floor. ES should support this: the CLI must be able to query and render
events without LumaWeave being present.

### Vault directory layout

```
<vault>/
  config.yaml
  data/cerebra.db         ← SQLite (all structured data incl. inspector_events)
  events/events.ndjson    ← append-only event log (authoritative)
  artifacts/
  indexes/
  exports/                ← graph export JSON for LumaWeave
  leeway/                 ← leeway rule YAML
  constitutional/         ← constitutional rule YAML
```

ES would take ownership of both `data/cerebra.db` (inspector_events table)
and `events/events.ndjson`, or consolidate them into a single ES-managed
store. The vault config (`config.yaml`) already handles vault path
resolution via a priority chain: `--vault` flag → `CEREBRA_VAULT` env →
`.cerebra/` ancestor search.

### Tech stack summary

Python 3.12+, SQLite, sentence-transformers (embeddings), Pydantic v2,
Click (CLI), uv (packaging). No cloud APIs. No async. Coverage ≥ 80%
enforced. Pre-commit hooks (ruff, black, mypy strict) gate every commit.
