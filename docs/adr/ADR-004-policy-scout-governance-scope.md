# ADR-004: Policy Scout Governance Scope

> **Historical note:** This record references Rhyzome, bons.ai, and Bo. These do not represent active Lattica ecosystem members — their appearance here reflects early planning context that did not materialize. The decision rationale remains accurate.

**Status:** Accepted
**Date:** 2026-06-11
**Deciders:** Developer (bitmosh)
**Related ADRs:** [ADR-001](ADR-001-lattica-extends-lumaweave.md) (Lattica platform identity), [ADR-002](ADR-002-es-toolkit-over-nats.md) (ES toolkit as event bus), [ADR-003](ADR-003-eval-core-standalone.md) (eval-core extraction from Policy Scout)

---

## Context

Policy Scout currently governs shell commands: it sits between an actor's request and execution, classifies the command, evaluates policy, and emits a decision. That capability is stable and tested. As Lattica evolves toward a reflective twin architecture — with Rhyzome repairing files, bons.ai mutating agent strategies, and Cerebra writing to its own knowledge store — the question of which of those actions Policy Scout should govern becomes load-bearing.

The scope question has two dangerous failure modes:

1. **Too narrow:** Policy Scout only gates shell commands. A Rhyzome repair that writes a modified source file, or a bons.ai mutation that pushes a new generation into ChromaDB, both escape governance entirely. The harness has blind spots in precisely the areas that matter most as autonomous agents become more capable.

2. **Too broad:** Policy Scout gates every LLM call — every Cerebra classification, every Bo inference request, every bons.ai mutation-score computation. This adds latency to inference-intensive loops, produces a firehose of audit events for purely in-process decisions, and conflates "content policy" (Cerebra's domain) with "action policy" (Policy Scout's domain). It also creates a circular dependency: Policy Scout might use an LLM to explain a decision, which would then gate itself.

Phase 2 of the Lattica platform wires Policy Scout as a daemon module. At that point, the scope question must be answered — not left to convention — because the PreToolUse hook, the MCP `policy_scout_check` tool, and the HITL gate in Lattica's agent dispatch all need a clear definition of what they are and are not responsible for gating.

---

## Forces

- **Constraint design philosophy.** The governing principle across Lattica is: don't detect mistakes, make them structurally impossible. A scope boundary that must be re-argued for every new module is not a structural constraint. The right boundary is one that can be enforced mechanically: "does this action have a side effect on files, packages, or system state?"

- **Separation of concerns.** Cerebra is the cognitive runtime. Policy Scout is the action harness. These must not blur. If Policy Scout starts opining on whether Cerebra's classification of a memory was correct, it has crossed into Cerebra's domain. The two modules communicate through the event fabric, not through overlapping authority.

- **PreToolUse hook already exists.** The `--hook-mode` flag for Claude Code's PreToolUse hook is already implemented. It fires before every Bash tool call. The natural scope for Policy Scout is exactly the class of actions that PreToolUse can intercept: shell commands, file mutations, package installs. Extending scope beyond what PreToolUse sees creates an asymmetry between Claude Code integration and non-Claude-Code agent integration.

- **HITL gate needs a clear trigger.** Human-in-the-loop approval is expensive (it blocks agent execution waiting for a human). It should only fire for actions where a human can meaningfully assess the risk and where the cost of getting it wrong is meaningful. LLM inference calls don't meet this bar. File mutations, package installs, and agent-initiated system mutations do.

- **Lattica agent dispatch must compose Policy Scout as a pre-execution gate.** For this to work cleanly, the gate must be stateless with respect to what it's blocking: you hand it an action, it returns a decision. This works cleanly for shell commands and file mutations. It doesn't work for "is this classification reasonable?" — that question requires domain context Policy Scout doesn't have.

- **Audit volume.** In a running Lattica session, Cerebra may classify dozens of memory fragments per minute; bons.ai may run hundreds of mutation-score computations per hour. Gating these would produce an audit store that is too noisy to be useful for its primary purpose: auditing consequential actions.

---

## Considered Options

**Option (a): Shell/system commands only (current state)**

Policy Scout gates what it already gates: commands submitted through `policy-scout check` or the PreToolUse hook. Package installs are a subset of shell commands and already covered. File mutations triggered by agent tools (Rhyzome writes, bons.ai checkpoint writes) are not covered unless the agent routes through a shell command.

*Rejected:* Leaves the largest gap exactly where it matters most. Rhyzome repairing a source file is a consequential action. bons.ai writing a new generation to ChromaDB is a mutation with audit value. Both escape governance under option (a).

**Option (b): Shell + package installs + file mutations**

Extends option (a) to include explicit file mutation gates, not only shell-command-expressed mutations. An agent that calls a write API directly (rather than via `echo > file` or similar) still hits the gate. Package installs are already in scope; this makes the coverage explicit.

*Partially accepted:* This is the right technical scope for what Policy Scout governs. But it doesn't address the HITL gate question — who calls it, when, and what happens while waiting for approval.

**Option (c): All of above + LLM call interception**

Every Cerebra classification request, every Bo inference call, every bons.ai strategy evaluation passes through Policy Scout. Policy Scout can inspect prompt content and optionally block or flag before inference.

*Rejected:* This is a content policy function, not an action policy function. It conflates two distinct governance problems. It creates tight coupling between Policy Scout and every module's inference path. It introduces latency into inference loops that are already latency-sensitive. And it duplicates the function of a separate, better-suited mechanism (prompt injection detection, which is a distinct planned feature for Policy Scout that applies only to agent-submitted content, not to all LLM calls in the system).

**Option (d): All of (b) + HITL agent gates (pre-execution gate for any action with side effects on files, packages, or system state)**

Extends option (b) with an explicit HITL gate interface: any agent in Lattica that is about to take an action with file, package, or system state side effects routes through Policy Scout before executing. The gate is synchronous for high-risk actions (REQUIRE_APPROVAL blocks and waits), async-logged for medium-risk actions (ALLOW_LOGGED proceeds immediately with an audit event), and transparent for low-risk actions (ALLOW). LLM inference calls are explicitly out of scope regardless of which agent initiates them.

*Accepted.* This is the right scope.

---

## Decision

**Option (d):** Policy Scout's governance scope is shell commands + package installs + file mutations + HITL agent gates for any action with side effects on files, packages, or system state.

### The explicit scope boundary

**IN scope — Policy Scout gates these:**

- Shell commands (any actor: human, agent, CI, MCP client)
- Package manager operations (`npm install`, `pip install`, `cargo add`, `apt install`, etc.)
- Explicit file mutations by agents (source file writes, config writes, lockfile changes)
- Destructive file operations (delete, move, rename, chmod, chown)
- Subprocess spawns by agents that result in any of the above
- Agent-initiated system state mutations (port binding, daemon start/stop, credential file access)
- Any action that Rhyzome, bons.ai, Lattica agent dispatch, or another module routes through `policy_scout_check` or the HITL gate API

**OUT of scope (explicitly, not by omission):**

- LLM inference calls: Cerebra classifications, Bo inference requests, bons.ai mutation scoring, any call to the ai-stack inference layer
- Cerebra's internal knowledge graph mutations (Cerebra is the cognitive runtime; it owns its own consistency guarantees)
- bons.ai's in-memory mutation strategy decisions (evaluation and selection of strategies is cognition, not action)
- Read-only operations: file reads, registry lookups, audit store queries, graph exports
- Network requests to Policy Scout's own configured intel sources
- The content of what an LLM says, as opposed to what an agent does as a result

### The governing principle

The line is: **does this action have a side effect on files, packages, or system state?** If yes, Policy Scout gates it. If the action is purely in-process computation or inference — even computation that will eventually inform a future action — it is not Policy Scout's domain. The action that results from a decision is Policy Scout's domain; the decision itself is not.

This principle is mechanically testable: can you observe the side effect from outside the process? If yes, Policy Scout governs it. If the only observable outcome is a return value consumed by the same process, Policy Scout does not govern it.

### Why LLM content is explicitly excluded

Cerebra is responsible for the quality and safety of its own classifications. Policy Scout is responsible for what agents do with those classifications. These are different accountability surfaces. If Cerebra misclassifies something, the right remedy is to improve Cerebra's pipeline or its evaluation suite (eval-core). If an agent takes a dangerous action based on a classification, the right remedy is Policy Scout catching that action at the gate. Merging these concerns into Policy Scout would make Cerebra's quality a Policy Scout problem, and would require Policy Scout to understand enough domain context to evaluate classification quality — which it structurally cannot without becoming Cerebra.

The planned prompt injection detection feature is not a counterexample: it scans agent-submitted content for injection attempts before that content reaches an LLM, not all LLM calls. It is a specific, bounded interception at the boundary between untrusted-agent input and the inference layer. It does not extend to gating all LLM calls.

---

## Integration Points

### PreToolUse hook (Claude Code agents)

Policy Scout is registered in `.claude/settings.json` as a `PreToolUse` hook via `policy-scout check --hook-mode`. This fires before every Bash tool call. Exit 0 allows; exit 1 with JSON on stderr blocks. The hook schema is stable and versioned (`"policy_scout_version"` field in every response).

This is the primary integration path for Claude Code. It requires no agent cooperation — the hook operates at the harness level regardless of what the agent attempts.

### MCP `policy_scout_check` tool (non-Claude-Code agents)

The MCP server exposes `policy_scout_check`, `policy_scout_sandbox`, `policy_scout_sweep`, and `policy_scout_get_report` as MCP tools. Agents that voluntarily participate (Rhyzome, bons.ai, future Lattica agent dispatch) call `policy_scout_check` before executing an action with side effects.

The trust model (ADR-001 in Policy Scout's own ADR series) assigns trust server-side; agents cannot self-escalate. The check-before-run session enforcement logs `MCPUncheckedExecution` if an agent skips the check step.

### Lattica agent dispatch (HITL gate)

Lattica's agent dispatch coordinates active agents across the platform. For Phase 2, agent dispatch wires Policy Scout as a mandatory pre-execution gate for all agent actions with side effects. The integration is:

1. Agent dispatch receives an agent action request (e.g., Rhyzome proposes repairing a file).
2. Dispatch calls `policy_scout_check` via the MCP tool (or directly via the Python API if in-process).
3. If decision is `ALLOW` or `ALLOW_LOGGED`: dispatch proceeds immediately, audit event written.
4. If decision is `REQUIRE_APPROVAL`: dispatch posts the pending gate to the Lattica dashboard and blocks.
5. If decision is `SANDBOX_FIRST`: dispatch routes the action to the sandbox executor.
6. If decision is `DENY` or `DENY_AND_ALERT`: dispatch cancels the action and notifies the agent.

Agent dispatch is the chokepoint. No module in Lattica dispatches consequential agent actions without routing through this chokepoint. This is a structural constraint, not a convention.

---

## HITL Gate Design

### Decision routing

HITL (human-in-the-loop) is triggered by `REQUIRE_APPROVAL` decisions. The gate has two modes based on risk band:

| Risk band | Behavior | Wait |
|---|---|---|
| `critical` (score 9–10) | Synchronous block — agent waits, Lattica surfaces notification immediately | Unbounded |
| `high` (score 7–8) | Synchronous block — agent waits, 15-minute auto-escalation to dashboard if no response | Unbounded |
| `medium` (score 4–6) | Async gate — action proceeds with `ALLOW_LOGGED`, approval request queued for review | None |
| `low` (score 1–3) | Transparent — `ALLOW` or `ALLOW_LOGGED`, no gate |  None |

The split between synchronous and async gates encodes the constraint design principle: for high-risk actions, humans are structurally in the loop. For medium-risk actions, humans can review after the fact. The threshold is deliberately conservative — if uncertain, treat as `high`.

### One-use expiring token

For `REQUIRE_APPROVAL` decisions, the approval store issues a one-use expiring token tied to the specific `request_id`. The token encodes:

```
{request_id, command_hash, actor_id, expiry_timestamp, approval_scope}
```

Approval scope is always `once` in v1 — approving an action does not create a permanent allow rule for that command. The token expires after 10 minutes (configurable). An expired token requires re-approval; it does not carry over. This prevents approval laundering: an agent cannot accumulate approvals in advance and spend them later.

The token is opaque to the agent. The agent receives only a boolean: approved or not. The token internals are only visible to the audit store and to the Lattica dashboard.

### Lattica dashboard surfacing

Pending approvals are surfaced in the Lattica dashboard as a distinct panel in Phase 2. Each pending approval shows:

- The command or action being requested
- The actor (agent name, trust level)
- The risk score and risk band
- The policy reasons (why `REQUIRE_APPROVAL` fired)
- The time the request arrived
- Approve / Deny / Escalate controls

The dashboard polls the approval store (or subscribes via the ES toolkit event fabric in Phase 6). Approvals granted from the dashboard emit an `ApprovalApprovedOnce` audit event with `actor_type: "developer"`.

### Async approval flows

For async gates (medium-risk actions that proceed as `ALLOW_LOGGED`), the audit record is the governance artifact. The Lattica dashboard shows these as a review queue: actions that proceeded without explicit approval, annotated with their policy decision and risk score. The developer can mark each as reviewed or flag it for follow-up. A flagged action triggers a `REQUIRE_APPROVAL` gate for future identical commands from the same actor.

This pattern — proceed now, review later — is the correct posture for medium-risk agent actions where blocking would crater productivity for acceptable risk. The audit trail is not optional; it is the accountability mechanism.

### Difference from synchronous gates

Synchronous gates (high/critical risk):
- Agent execution is suspended pending approval
- Lattica dispatch holds the action request in a pending queue
- Agent receives no result until a human approves or denies
- Denial terminates the action plan; the agent must re-plan

Async gates (medium risk):
- Action proceeds immediately with `ALLOW_LOGGED`
- Approval request is queued in the review panel for later acknowledgment
- Agent receives an `ALLOW_LOGGED` decision and continues
- Post-hoc review does not block or undo the action

---

## Positive Consequences

- Every consequential agent action in Lattica — regardless of which module initiates it — passes through a single, inspectable gate. The audit trail is complete.
- The scope boundary is mechanically enforceable: "side effect on files, packages, or system state" is a test you can run on any action description.
- LLM inference paths remain unimpeded. Cerebra, bons.ai, and ai-stack are not bottlenecked by the governance layer.
- The PreToolUse hook already covers Claude Code integration. Extending to the MCP tool and HITL gate covers the remaining integration surface without requiring Policy Scout to understand every agent's internal architecture.
- eval-core (ADR-003) can be used to regression-test governance decisions — a decision suite that covers all decision categories with known outcomes, CI-gated.
- The tighten-only YAML override system (Policy Scout ADR-002) means any module can declare stricter governance for its own domain without weakening the global harness.

---

## Negative Consequences / Risks

- **Chokepoint risk.** Making Lattica agent dispatch the mandatory routing point for all consequential agent actions means a bug or downtime in Policy Scout can block all agent activity. Mitigation: fail-open policy for `ALLOW_LOGGED`-band actions (if Policy Scout is unreachable, proceed with an audit event noting the governance gap). Fail-closed for `REQUIRE_APPROVAL` and `DENY` band actions (if Policy Scout is unreachable, block and surface an error).

- **Scope creep pressure.** As Lattica grows, there will be pressure to add "just this one" LLM call to Policy Scout's scope — for example, to gate Bo's inference requests before sending a message to a user. Resist this. The governing principle is the firewall. If a feature genuinely needs content-level governance, it belongs in a dedicated content safety module, not in Policy Scout.

- **Latency in synchronous gates.** High-risk synchronous gates block agent execution indefinitely waiting for human approval. In an automated workflow (e.g., a Rhyzome repair job running unattended), this will stall. Mitigation: agents should declare their intended action scope in their session manifest, and Policy Scout should be configured with appropriate trust levels per agent. A Rhyzome session with `trusted_agent` trust may qualify for `ALLOW_LOGGED` on file mutations below a certain risk score, removing most synchronous blocking in practice.

- **Agent cooperation for non-hook paths.** The PreToolUse hook is mandatory for Claude Code agents. For other agents (Rhyzome, bons.ai), cooperation requires routing through the MCP tool or the dispatch chokepoint. If an agent bypasses dispatch and writes files directly, it evades the gate. Mitigation: Lattica agent dispatch is the structural enforcement point — agents that do not route through dispatch are not recognized as Lattica-coordinated agents and do not receive dispatch scheduling, memory access, or cross-module communication.

---

## Notes

This ADR governs the Lattica-level scope definition for Policy Scout. Policy Scout's own internal ADR series (ADR-001 through ADR-008 in `policy-scout/docs/adr/`) governs its implementation details: MCP transport, config precedence, graph export contract, sandbox backend, registry expansion, report lifecycle, sweep delta, and desktop UI. The Lattica-level scope decision is upstream of all of those; nothing in Policy Scout's implementation ADRs contradicts this scope.

The four scope items — shell, packages, file mutations, HITL agent gates — are cumulative. Option (a) through option (d) are a progression; option (d) includes everything in (a), (b), and (c) except LLM interception, which is explicitly excluded. Future scope expansion (if any) requires a new ADR, not a scope annotation.
