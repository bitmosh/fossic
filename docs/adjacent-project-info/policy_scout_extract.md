# ES Consumer Profile — policy-scout

You are the policy-scout advocate. We're building **ES**, a local-first
event sourcing library: Rust core, PyO3/napi-rs bindings, SQLite-backed,
content-addressed events, branchable history, pure synchronous reducers.
It's being extracted from Lattica's Phase 6 plans because policy-scout and
several other projects also want event sourcing capabilities.

Before we lock the v1 API, we need your project's actual shape. Please
answer the questions below as concretely as possible — "I don't know yet"
and "we don't care about this" are both valid and useful answers.

---

## Language and runtime

**1. Primary language(s)?**
Python 3.12+ is the core. Rust (Tauri 2) for the desktop shell. TypeScript/React for the UI webview. The event-writing hot path is entirely Python.

**2. In-process embedded library, long-running daemon, CLI tool, or hybrid?**
Hybrid. Three distinct entry points:
- Short-lived CLI invocations (`policy-scout check`, `audit list`, etc.) — the common case.
- A long-running watch daemon (`policy-scout watch start`) that monitors filesystem changes in the background.
- A Tauri desktop app that drives the CLI as a subprocess and reads results over stdout JSON.

**3. Multi-process? Multi-thread? Async runtime?**
Multi-process — the Tauri shell spawns Python CLI subprocesses; the watch daemon runs as a separate OS process. Python side is synchronous throughout (no asyncio, no threading). No async runtime in use.

---

## What you want from ES

**4. Top three things wanted from event sourcing:**

1. **Durable audit trail with `request_id` correlation.** Every agent command flows through a pipeline: `CommandRequested → CommandClassified → PolicyMatched → DecisionIssued → (ApprovalRequested →) SandboxInstall → ExecutionCompleted`. Today these are separate SQLite rows. What's missing is the ability to replay or walk the full causal chain for a single `request_id` across all event types efficiently.

2. **Retention-aware storage with selective deletion.** The project already has a cleanup/deletion path (`policy-scout data cleanup --apply`). ES owning the lifecycle — TTL policies, PII scrub on individual events — would replace a bespoke maintenance job.

3. **Cross-process write ordering guarantee.** The CLI and the watch daemon can both emit events to the same SQLite file. Today this relies on SQLite's WAL mode and Python's `sqlite3` connection-level locks. A proper ES layer with a single-writer log would eliminate the occasional duplicate/lost write on rapid bursts.

**5. Things from the ES spec we DON'T need or actively don't want:**

- **Branching / branchable history**: Don't need. policy-scout has no experimental-scenario or what-if workflow.
- **Agent trace adapter**: Neutral — we have our own `request_id` correlation scheme. An adapter would only matter if we wanted to export traces to OTel, which isn't on the roadmap.
- **Snapshots**: Don't need. Event counts are low enough that full replay is never a bottleneck.
- **Multi-tenant / multi-user**: Actively don't want. This is a single-developer local tool and adding tenant isolation would be pure overhead.

---

## Scale and shape of writes

**6. Estimated write rate at steady state:**
Very low. Idle: 0 events/min. Active sweep: ~5–20 events/min (one sweep emits `SweepStarted`, per-finding events, `SweepCompleted`). A sandbox install burst: ~8–12 events over 2–5 seconds.

**7. Burst profile:**
Tight clusters. A `policy-scout check npm install lodash` emits 5–8 events in under 500ms, then silence. Sweeps are slightly longer bursts (seconds, not minutes). The watch daemon adds a low-level trickle of `FileChanged` events during active development.

**8. Typical payload size / maximum:**
Typical: 200–2 KB. The `data_json` column holds the serialized `data` dict from each `AuditEvent`. Largest payloads are `SweepCompleted` (full findings list) and `SandboxInstallCompleted` (transitive dep tree from pnpm/pip). Largest observed: ~40 KB for a large Python transitive tree. Nothing reliably over 64 KB.

**9. Number of distinct streams / events per stream per day:**
If streams map 1:1 to `request_id`: tens to low hundreds of streams per active day, each with 5–15 events. If streams map to `event_type`: ~25 named types (see `EventType` in `policy_scout/audit/events.py`), uneven distribution — `DecisionIssued` and `CommandRequested` are the highest-volume.

**10. Single writer per stream, or concurrent writers?**
Effectively single writer per `request_id` stream (the CLI process that owns that request). But the watch daemon writes to the same SQLite file concurrently with foreground CLI invocations, so at the store level there are concurrent writers.

---

## Reads

**11. Read patterns:**
- **Tail-the-latest**: most common — `audit list` returns recent N events sorted by timestamp DESC.
- **Type-filtered tail**: `audit type SweepCompleted` — same but filtered.
- **Correlation walk**: `audit list --request-id <id>` returns all events for a request, sorted ASC (causal order). This is the most valuable read pattern and the hardest with the current flat table.
- **Random access by event_id**: used by the detail view (`audit show <event_id>`).
- No full replay from version 0 needed today.

**12. Live subscriptions:**
Polling is fine. The Tauri UI refreshes on demand (button / filter change). The watch daemon writes and moves on — no consumers waiting on a subscription.

**13. Cross-stream queries:**
Yes — the `request_id` correlation query is the primary one. Also: "all events in the last hour", "all `DecisionIssued` events where decision = BLOCKED". These are currently SQL WHERE clauses on the flat table; they'd need to remain efficient in an ES layer.

---

## Persistence and lifecycle

**14. How long do events need to live?**
Configurable retention, defaulting to 90 days. The project already has a `--retention-days` flag on the cleanup command.

**15. Any need to delete individual events?**
Yes. Two drivers:
- **PII/credential scrub**: command arguments can contain secrets or personal paths. The store already runs `redact_dict()` at write time, but post-hoc scrub of mis-recorded data is needed.
- **Cleanup by age**: the existing deletion path targets events older than a retention window.

**16. Acceptable storage growth:**
Bounded by retention policy. Not disk-bounded — a developer machine has ample space. The goal is to avoid unbounded growth, not to fit in a tight budget.

**17. Backup/restore:**
Just-the-SQLite-file is fine. No streaming backup or PITR needed.

---

## Security and deployment

**18. Sensitive data in event payloads?**
Yes. Command arguments can contain package names with embedded secrets, filesystem paths that reveal project structure, and occasionally credentials passed as CLI flags. The store already applies a `redact_dict()` pass before writing (`sqlite_store.py:96–98`), but the raw `data_json` column still holds structured command data.

**19. Encryption at rest required?**
Not required for v1. Nice-to-have. If added, OS keyring (secretservice/Keychain) is the preference — no env-var secrets.

**20. Single-user local-first, or multi-user / multi-tenant?**
Single-user, local-first. One developer, one machine.

**21. Deployment target:**
Developer workstation only (Linux primary, macOS secondary via Tauri app). No server, container, or edge.

---

## Existing event/log infrastructure

**22. Does this project already have an event store?**
Yes — a SQLite-backed audit store at `~/.policy_scout/audit.db`.

Schema (abridged from `sqlite_store.py:47–89`):
```sql
CREATE TABLE audit_events (
    event_id        TEXT PRIMARY KEY,       -- "evt_<ulid>"
    event_type      TEXT NOT NULL,
    timestamp       TEXT,                   -- ISO-8601
    request_id      TEXT,                   -- correlation key
    actor_type      TEXT,
    actor_name      TEXT,
    summary         TEXT,
    data_json       TEXT NOT NULL,          -- serialized AuditEvent.data dict
    schema_version  INTEGER NOT NULL,       -- currently 1
    created_at      TEXT,
    decision_id     TEXT,                   -- FK-style denorm
    approval_id     TEXT,
    sandbox_id      TEXT,
    sweep_id        TEXT,
    report_id       TEXT,
    execution_id    TEXT
);
-- Indexes: request_id, event_type
```

There is also a JSONL writer (`audit/jsonl_writer.py`) used for file-based export.

Migration preference: **bridge first, migrate later**. A read-only ES adapter over the existing table would let us validate the integration without a flag-day migration. Full migration only after the API is proven stable.

**23. If migrating: how many existing events?**
Unknown — depends on install age. A typical active installation: 500–5,000 events. An older heavy-use installation: up to ~50,000.

---

## Integration shape

**24. How would you want to call ES?**
In-process Python API is strongly preferred for the write path — the hot path is Python and adding a subprocess round-trip per event would be unacceptable. For the Tauri shell, the current pattern (invoke Python CLI as subprocess, parse stdout JSON) could call `audit` subcommands that internally use ES — no Rust-native ES integration needed in v1.

**25. Anything awkward about integration today?**

- The Tauri shell calls Python as a subprocess. If ES exposes a Rust-native API (via napi-rs), that doesn't reach the Python write path without an extra IPC hop. PyO3 bindings would be the right fit here.
- The existing `request_id` scheme (a `ulid`-prefixed string generated per CLI invocation) would need to map to ES's stream identity concept. If ES streams are content-addressed or use a different ID scheme, there's a translation layer to design.
- The `redact_dict()` pass happens at write time inside `SQLiteAuditStore.write_event()`. That logic would need to move to the ES integration point, not be bypassed.

---

## Open questions and concerns

**26. What would make you NOT adopt ES?**

- **Migration pain.** If there's no bridge/adapter path and migration requires a one-shot ETL of existing events, that's a blocking risk — the store is write-audited and a failed migration could silently drop the trail.
- **PyO3 dependency conflict.** policy-scout's Python env is intentionally lean (only `pyyaml` as a runtime dep). A heavy Rust extension with transitive C deps would complicate the install story for non-Rust developers.
- **Write latency regression.** Today a `write_event()` call completes in <5ms locally. If ES adds synchronous overhead (content-addressing, Merkle ops) that pushes this above ~20ms, it shows up in the CLI's interactive response time.

**27. Anything in the ES design to push back on?**

Haven't seen `EVENT_FABRIC.md` or `ADR-002` — can't review specific design decisions without those. General concern: if content-addressing means every event write involves a hash of the full payload, that needs benchmarking against the 40 KB sweep payloads mentioned above. Hashing is cheap but it's not free at the tail end of an interactive command.

---

## policy-scout

policy-scout is a local-first safety harness for agent command execution on developer workstations. It intercepts package installs, shell commands, and file changes; runs them through a policy engine; and emits a structured audit trail. The existing audit store is a flat SQLite table (`audit_events`) with a `request_id` correlation key. Write volume is low and bursty; read patterns skew heavily toward recent-N and correlation walks. The biggest wins from ES would be durable causal ordering across multi-step request pipelines, and a principled retention/deletion mechanism to replace the bespoke cleanup job. Integration preference is a PyO3 in-process Python API with a bridge adapter over the existing table as the first step.
