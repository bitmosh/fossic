# Policy Scout Event Vocabulary

**Status:** v0.1 — Phase 2 anchor types defined · 2026-06-14
**Scope:** Event types emitted by policy-scout to `policy-scout/audit/*` and
`policy-scout/approval/*` streams. Governance and audit events — not agent-trace
events. See `AGENT_TRACE_VOCABULARY.md` for the agent-trace vocabulary and the
cross-project causation note in §2 of that document.

---

## 1. Why this is a separate document

Policy-scout events are governance and infrastructure events, not LLM agent trace
events. They record command classification decisions, policy enforcement outcomes,
human-in-the-loop approvals, sandbox operations, and audit integrity checks — none
of which are cognitive execution traces.

The audience for this vocabulary differs from `AGENT_TRACE_VOCABULARY.md`:
- **Agent-trace consumers** (Cerebra, rhyzome, bons.ai) care about reasoning loops,
  LLM calls, tool calls, and cycle metadata.
- **Policy-scout consumers** (Lattica governance tiles, audit dashboards, incident
  response tooling) care about command decisions, approval state, and sandbox results.

Keeping them separate prevents vocabulary pollution and makes each doc navigable
for its intended audience.

**Cross-project causation boundary:** When Cerebra proposes an action to Policy Scout,
the causation chain crosses from `cerebra/agent-trace/<session_id>` into
`policy-scout/audit/<request_id>`. The Cerebra-side anchor is `ActionProposed`;
`CommandRequested.upstream_causation_id` (in the event payload) holds the fossic
event ID of `ActionProposed`. fossic's `walk_causation` follows this link across
the stream boundary. See `AGENT_TRACE_VOCABULARY.md` §2 scope note for the
cross-stream chain diagram.

---

## 2. Consumer Registry

Stream prefixes owned by policy-scout:

| Stream prefix | Purpose | Vocabulary location |
|---|---|---|
| `policy-scout/audit/<request_id>` | Per-request governance pipeline events | This doc §3 |
| `policy-scout/approval/<approval_id>` | HITL approval lifecycle events | This doc §4 |

---

## 3. Governance pipeline events

Events on `policy-scout/audit/<request_id>`. One stream per policy-check request.
All payloads are redacted before emission (credentials, tokens, and secrets are
removed by `redact_dict()` before the fossic append call).

All events carry `schema_version: 1` unless noted.

### 3.1 `CommandRequested`

Emitted when a governed command arrives at the policy gate. First event in every
audit stream. The `causation_id` field on the fossic `Append` is set to the EventId
of the upstream `ActionProposed` event (from `cerebra/agent-trace/<session_id>`)
when Cerebra is the caller; `None` for human or direct CLI invocations.

**Payload:**

```json
{
  "command": "string",
  "cwd": "string",
  "request_id": "string (ulid)",
  "actor_type": "string (agent | human | system)",
  "actor_name": "string",
  "upstream_causation_id": "string | null"
}
```

`upstream_causation_id` is the hex EventId of the upstream `ActionProposed` event.
It is carried in the payload (in addition to the fossic-level `causation_id`) so
that consumers reading the event without fossic infrastructure can reconstruct the
cross-stream chain manually.

### 3.2 `CommandParsed`

Emitted after the command string is parsed into structured form.

**Payload:**

```json
{
  "command": "string",
  "args": ["string"],
  "flags": {"string": "string | bool"},
  "request_id": "string (ulid)"
}
```

### 3.3 `CommandClassified`

Emitted after risk classification runs.

**Payload:**

```json
{
  "command": "string",
  "category": "string",
  "risk_score": "int (0–10)",
  "risk_band": "low | medium | high | critical",
  "request_id": "string (ulid)"
}
```

### 3.4 `PolicyMatched`

Emitted when a policy rule matches the command.

**Payload:**

```json
{
  "command": "string",
  "matched_rule": "string | null",
  "policy_hits": [{"rule": "string", "action": "string"}],
  "request_id": "string (ulid)"
}
```

### 3.5 `DecisionIssued`

Emitted when the final enforcement decision is determined. Key event for
cross-project observability — Lattica tiles use this as the summary event
for governance outcome display.

**Payload:**

```json
{
  "command": "string",
  "decision": "ALLOW | ALLOW_LOGGED | REQUIRE_APPROVAL | SANDBOX_FIRST | DENY | DENY_AND_ALERT",
  "risk_score": "int (0–10)",
  "risk_band": "low | medium | high | critical",
  "category": "string",
  "matched_rule": "string | null",
  "reasons": ["string"],
  "request_id": "string (ulid)"
}
```

### 3.6 `PolicyError`

Emitted when the policy engine encounters an internal error.

**Payload:**

```json
{
  "error_message": "string",
  "request_id": "string (ulid)"
}
```

### 3.7 Execution events

`CommandExecutionStarted`, `CommandExecutionCompleted`, `CommandExecutionBlocked`,
`CommandExecutionFailed` — emitted when a command is executed through the run gate.

Payload shape for `CommandExecutionCompleted`:

```json
{
  "execution_id": "string (ulid)",
  "command": "string",
  "exit_code": "int",
  "duration_ms": "int",
  "request_id": "string (ulid)"
}
```

---

## 4. HITL approval events

Events on `policy-scout/approval/<approval_id>`. One stream per approval request.

Cerebra (and other calling agents) may subscribe to `policy-scout/approval/<approval_id>`
and resume on `ApprovalApprovedOnce` rather than polling. This is the planned Phase 2
HITL subscription pattern.

### 4.1 `ApprovalRequested`

**Payload:**

```json
{
  "approval_id": "string (ulid)",
  "command": "string",
  "risk_score": "int",
  "risk_band": "string",
  "request_id": "string (ulid)",
  "expires_at": "string (ISO-8601)"  // configurable via `approvals set-timeout`; default 24h, range 1h–8760h
}
```

### 4.2 `ApprovalApprovedOnce`

**Payload:**

```json
{
  "approval_id": "string (ulid)",
  "approved_by": "string",
  "approved_at": "string (ISO-8601)",
  "request_id": "string (ulid)"
}
```

### 4.3 `ApprovalDeniedOnce`

**Payload:**

```json
{
  "approval_id": "string (ulid)",
  "denied_by": "string",
  "denied_at": "string (ISO-8601)",
  "request_id": "string (ulid)"
}
```

### 4.4 `ApprovalExpired`

**Payload:**

```json
{
  "approval_id": "string (ulid)",
  "expired_at": "string (ISO-8601)",
  "request_id": "string (ulid)"
}
```

---

## 5. Remaining event domains (Phase 3+)

Full vocabulary — ~70 event types across all domains. Documented incrementally
as each domain is wired for fossic emission.

| Domain | Event types | Count |
|---|---|---|
| Sandbox | `SandboxRequested`, `SandboxWorkspaceCreated`, `SandboxInstallStarted`, `SandboxInstallCompleted`, `SandboxResultWritten`, `SandboxError`, `SandboxMigration*`, `GeneralSandbox*`, `SandboxBehaviorFinding` | 16 |
| Sweep + supply chain | `SweepStarted`, `SweepFindingCreated`, `SweepCompleted`, `SweepError`, `SecretScanCompleted`, `SecretFindingCreated` | 6 |
| Policy management | `PolicySimulated`, `PolicyValidated`, `PolicyHistoryTested`, `ProjectOverrideLoaded`, `ProjectOverrideViolated` | 5 |
| Audit integrity | `ChainVerificationCompleted`, `IntegrityCheckFailed`, `IntegrityCheckPassed`, `ScoutReportGenerated` | 4 |
| Incident response | `LockdownActivated`, `LockdownDeactivated`, `EvidencePreserved`, `ClearanceCheckRun` | 4 |
| Watch daemon | `WatchTriggerDetected`, `WatchDaemonStarted`, `WatchDaemonStopped`, `WatchDaemonHeartbeat` | 4 |
| Threat intel | `IntelLookupCompleted`, `IntelLookupFailed`, `IntelCacheHit` | 3 |
| MCP server | `McpServerStarted`, `McpToolCallReceived`, `McpToolCallCompleted`, `McpSessionEnded` | 4 |
| Injection detection | `InjectionPatternFound` | 1 |

---

## 6. OTel mapping

When a GenAI OTel exporter is available, governance pipeline events map to spans
with the `gen_ai.policy_scout.*` namespace:

| Event type | OTel span name | Key attributes |
|---|---|---|
| `CommandRequested` | `gen_ai.policy_scout.request` | `gen_ai.policy_scout.command`, `gen_ai.policy_scout.actor` |
| `DecisionIssued` | `gen_ai.policy_scout.decision` | `gen_ai.policy_scout.decision`, `gen_ai.policy_scout.risk_score` |
| `ApprovalRequested` | `gen_ai.policy_scout.approval` | `gen_ai.policy_scout.approval_id` |

Not yet implemented. Placeholder for when the exporter supports governance spans.

---

## 7. Adding new event types

To add a new policy-scout event type to fossic:

1. Define it in `policy_scout/audit/events.py` (EventType enum + factory function).
2. Add an emit call in the relevant pipeline stage (follows the `write_event()` path
   automatically once the event is created and passed to `SQLiteAuditStore`).
3. Document the payload schema in this file under the appropriate domain section.
4. Update the event count in the §5 domain table.

No fossic-core changes are required — policy-scout events are purely consumer-defined.
