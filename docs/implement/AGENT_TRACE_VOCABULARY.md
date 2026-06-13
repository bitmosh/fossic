# Agent Trace Vocabulary

**Status:** v1 specification · 2026-06-12
**Scope:** Standard event types fossic ships for agent trace recording, the per-tool determinism registry, the rhyzome, bons.ai, and Cerebra extensions, and the OpenTelemetry GenAI span mapping.

---

## 1. Why this is a separate document

Agent trace event types grow over time. Every new agent-runtime integration potentially adds new event types. Keeping them in the main fossic spec would bloat it; keeping them only in code makes the protocol invisible. This document is the canonical vocabulary list, and it is intended to grow.

The standard event types live in the `fossic` crate. The rhyzome, bons.ai, and Cerebra extensions live in their respective consumer codebases — they are documented here for cross-project coordination but fossic core does not depend on them.

---

## 2. Consumer Extension Registry

A discoverability registry for all projects emitting events to fossic agent-trace streams. Each consumer registers its stream prefix, vocabulary reference, and any event-name overlaps with other consumers.

| Consumer | Stream prefix | Vocabulary location | Overlap flags |
|---|---|---|---|
| fossic (standard) | `*/agent-trace/*` | This doc §3 | — |
| Cerebra | `cerebra/agent-trace/*`, `cerebra/lattice/*` | This doc §7 + `cerebra_phase6_event_vocabulary.md` | `ContextPacketBuilt`: also emitted in pre-Phase-6 retrieval flow without `cycle_id`; see §7.3.2 note |
| rhyzome | `rhyzome/repair/*` | This doc §5 | — |
| bons.ai | `bonsai/idea/*` | This doc §6 | — |

When consumers ship new vocabularies, they:

1. Add a row to this registry.
2. Link to their vocabulary documentation.
3. Flag any event-name overlaps with other consumers, with a one-line note about the semantic difference.

The prefix convention prevents namespace collision; the registry prevents semantic confusion.

---

## 3. Standard event types (fossic core)

These five event types ship with fossic and are recognized by the OpenTelemetry GenAI exporter. All are versioned starting at `type_version=1`.

### 3.1 `llm_call`

A request to an LLM. Payload:

```json
{
  "model_id": "string",              // e.g., "qwen2.5-coder:32b"
  "system": "string?",                // gen_ai.system: e.g., "ollama", "anthropic"
  "messages": [...],                  // request messages, format consumer-defined
  "parameters": {                     // request parameters
    "temperature": 0.7,
    "max_tokens": 4096,
    "top_p": 1.0,
    "...": "..."
  },
  "tools_available": ["string"],      // optional, names of tools the model may call
  "request_id": "string?"             // optional, consumer-supplied trace ID
}
```

Always paired with an `llm_response` event via `causation_id`.

### 3.2 `llm_response`

A response from an LLM, either content or a tool call. Payload:

```json
{
  "call_id": "EventId",               // causation_id back to the originating llm_call
  "finish_reason": "string",          // "stop" | "tool_calls" | "length" | "error"
  "content": "string?",               // present when finish_reason != "tool_calls"
  "tool_calls": [...],                // present when finish_reason == "tool_calls"
  "usage": {
    "input_tokens": int,
    "output_tokens": int,
    "total_tokens": int
  },
  "latency_ms": int,
  "error": "string?"                  // present when finish_reason == "error"
}
```

### 3.3 `tool_call`

An invocation of a tool by an agent. Payload:

```json
{
  "tool_name": "string",
  "tool_call_id": "string",           // consumer-supplied, matches llm_response.tool_calls[].id
  "arguments": {...},                 // tool-specific argument object
  "deterministic": bool               // see §4
}
```

The `deterministic` field is load-bearing — it determines replay behavior. Default is `false`. Consumers register tools with the trace adapter to set per-tool defaults; explicit override on individual events is allowed.

### 3.4 `tool_result`

The result of a tool invocation. Payload:

```json
{
  "tool_call_id": "string",           // matches the tool_call's tool_call_id
  "tool_name": "string",
  "result": any,                      // tool-specific result; may be large
  "error": "string?",
  "deterministic": bool,              // mirrors the tool_call's value
  "latency_ms": int?
}
```

If a tool produces a large result (e.g., file contents, search results), the result field can contain a reference (e.g., a content-addressed blob ID) rather than the inline value. This is consumer convention, not a fossic protocol requirement.

### 3.5 `reasoning_step`

A narrative reasoning step from an agent. Free-form text that does not fit the structured event types. Payload:

```json
{
  "agent": "string?",                 // which agent (for multi-agent systems)
  "text": "string",                   // the reasoning content
  "step_type": "string?",             // optional taxonomy: "plan" | "reflect" | "summarize" | ...
  "tokens_used": int?
}
```

Used for the "agent thought process" that doesn't map to a discrete LLM call or tool invocation.

---

## 4. Per-tool determinism registry

The `deterministic` flag default flipped from `true` to `false` in v1. Reasoning: cost of unnecessary re-execution is visible (latency); cost of a wrong `true` is invisible (silent replay corruption with stale data). The constraint-design principle picks the failure mode that surfaces.

### 4.1 Registry API

Consumers register tools with the trace adapter at startup:

```python
from fossic.agent_trace import AgentTraceRecorder

recorder = AgentTraceRecorder(store)

# Register tools with their determinism defaults
recorder.register_tool("read_file", deterministic=True)       # pure function of path
recorder.register_tool("list_directory", deterministic=False) # filesystem state varies
recorder.register_tool("parse_ast", deterministic=True)       # pure function of source
recorder.register_tool("run_pytest", deterministic=False)     # environment-dependent
recorder.register_tool("write_file", deterministic=False)     # has side effects
recorder.register_tool("apply_diff", deterministic=True)      # pure function of (base, diff)
```

Tools not registered default to `deterministic=False`. Recording a tool_call/tool_result event with no registered default produces a one-line warning to make missing registrations visible during development.

### 4.2 Recommended defaults table

For common tool categories, this is the recommended starting registration. Consumers should adapt to their actual semantics.

| Tool category | Example | Default | Rationale |
|---|---|---|---|
| File read (path → bytes) | `read_file`, `cat` | `true` | Pure function of path + filesystem snapshot |
| File write | `write_file`, `apply_patch` | `false` | Side effect; replay must re-execute |
| File metadata read | `stat`, `list_directory` | `false` | Filesystem state varies between original and replay |
| AST parsing | `parse_ast`, `tree-sitter` | `true` | Pure function of source bytes |
| Source compilation | `compile`, `tsc`, `cargo build` | `false` | Toolchain version may differ |
| Test execution | `pytest`, `cargo test` | `false` | Environment-dependent, intentionally non-deterministic |
| LLM call (via `llm_call`) | n/a — `llm_call` is its own type | n/a | Re-execute by default per LLM call semantics |
| HTTP request | `fetch`, `curl` | `false` | Network state varies |
| Shell command | `bash`, `sh` | `false` | Environment-dependent |
| Database read | `sqlite_query` | `false` | DB state varies |
| Cryptographic hash | `blake3`, `sha256` | `true` | Pure function of input bytes |
| JSON parse/serialize | `json_parse` | `true` | Pure function of input |
| Embedding | `embed_text` | `false` | Model state may differ across runs |

This list is illustrative, not exhaustive. The fossic core does not impose these defaults; the agent-trace adapter accepts whatever the consumer registers.

### 4.3 Replay semantics

On replay through a reducer or via the time-travel viewer:

- **deterministic=true:** the stored `tool_result` is served as the result of the tool call. The tool is not re-executed. The agent sees the same result it saw originally.
- **deterministic=false:** the tool is re-executed against the current environment. The new result may differ from the stored one. Consumers can opt into a "comparison mode" where both the stored and re-executed results are surfaced and divergence is logged.

Rhyzome uses comparison mode for `run_pytest`: a stored PASS that replays as FAIL is a first-class finding (external regression introduced between original session and replay).

---

## 5. Rhyzome extension event types

These types are defined in rhyzome's codebase, not in fossic core. They are documented here so other consumers (LumaWeave's time-travel viewer, the OTel exporter) can recognize them.

### 5.1 `strategy_selected`

```json
{
  "session_id": "string",
  "file_id": "string",
  "bug_type": "string",               // FailureCategory enum value
  "ranked_strategies": [
    {"strategy": "string", "score": float, "rationale": "string", "rank": int}
  ],
  "selected_strategy": "string",
  "selection_reason": "string"
}
```

Required at branch creation time — the `ranked_strategies` list is the `alternatives` payload for the branch.

### 5.2 `ast_gate_evaluated`

```json
{
  "session_id": "string",
  "candidate_hash": "string",         // SHA-256 of the candidate diff
  "gate_status": "string",            // GateStatus enum: "passed" | "rejected" | ...
  "violations": ["string"],
  "elapsed_ms": int
}
```

A REJECTED gate means the diff is discarded without test execution. Replaying without this event loses the gate's veto.

### 5.3 `strategy_exhausted`

```json
{
  "session_id": "string",
  "file_id": "string",
  "bug_type": "string",
  "strategies_tried": ["string"],
  "final_failure_category": "string", // FailureCategory enum
  "escalation_to_human": bool
}
```

Emitted when all ranked strategies for a `(session, file, bug_type)` triple have been exhausted.

---

## 6. Bons.ai extension event types

These types are defined in bons.ai's codebase, not in fossic core. Same convention as rhyzome.

### 6.1 `bandit_arm_selected`

```json
{
  "parent_idea_id": "string",
  "generation": int,
  "arm_id": "string",                 // composite: strategy + mutation
  "strategy": "string",               // "exploration" | "refinement" | "disruption" | "balanced"
  "mutation": "string",
  "ucb_value": float,
  "exploration_rate": float,
  "selection_mode": "string"          // "exploit" | "explore"
}
```

### 6.2 `bandit_arm_updated`

```json
{
  "arm_id": "string",
  "reward": float,
  "visit_count": int,
  "reward_mean": float,
  "posterior": {...}                  // bandit-specific posterior state
}
```

### 6.3 `bandit_decision`

```json
{
  "parent_idea_id": "string",
  "generation": int,
  "candidate_arms": [
    {"arm_id": "string", "strategy": "string", "mutation": "string",
     "visit_count": int, "reward_mean": float, "ucb_value": float}
  ],
  "selected_arm": {"arm_id": "string", "strategy": "string", "mutation": "string"},
  "selection_mode": "string",
  "exploration_rate_at_selection": float
}
```

Required at branch creation time when a branch forks from a bandit decision — the `candidate_arms` list is the `alternatives` payload.

### 6.4 `stagnation_detected`

```json
{
  "stream_id": "string",              // the idea lineage
  "stagnation_level": float,
  "similarity_signal": float,
  "response": "string",               // "exploration_rate_bump" | "forced_mutation" | "fork"
  "triggered_branch_id": "string?"    // if response was "fork"
}
```

### 6.5 `adaptation_applied`

```json
{
  "weights_before": {...},
  "weights_after": {...},
  "thresholds_before": {...},
  "thresholds_after": {...},
  "exploration_rate_before": float,
  "exploration_rate_after": float,
  "trigger": "string"                 // what caused this adaptation
}
```

### 6.6 `memory_retrieved`

```json
{
  "query": "string",
  "retrieved_cycles": [
    {"cycle_id": "string", "score": float, "stream_id": "string"}
  ],
  "retrieval_purpose": "string"       // "context_building" | "lineage_check" | "similarity_search"
}
```

### 6.7 `embedding_stored`

```json
{
  "cycle_id": "string",
  "embedding_dim": int,
  "embedding_hash": "string",         // blake3 of the embedding bytes
  "vector_store": "string"            // which vector index
}
```

Confirms a fire-and-forget vector store write is auditable.

---

## 7. Cerebra extension event types

These types are defined in Cerebra's codebase, not in fossic core. They are documented here for cross-project visibility. For the full required-vs-optional field rationale and `indexed_tags` recommendations, see `cerebra_phase6_event_vocabulary.md` in the Cerebra project.

All 22 types are `type_version=1`. PascalCase names, past-tense verbs (event reports something that happened). All write to streams matching `cerebra/agent-trace/<cycle_id>` per the stream pattern lock in §7.1.

### 7.1 Stream pattern lock

Cerebra emits cycle runtime events to streams matching `cerebra/agent-trace/<cycle_id>`.

- The `<cycle_id>` segment is a single-segment UUID — no embedded slashes, under 256 characters.
- Subscribers to `*/agent-trace/*` receive Cerebra events alongside events from other consumers using the same `agent-trace` stream tier.
- Forward-compat reservation: `cerebra/agent-trace/<cycle_id>/<sub_id>` is reserved for future sub-cycle event structure. Consumers should not treat the absence of sub-cycle streams as a guarantee.
- Cerebra also emits to `cerebra/lattice/<lineage_id>` streams with separate vocabulary. That vocabulary is documented in a forthcoming addendum covering Phase 8 lattice aggregate events; it is NOT part of this document.

### 7.2 Session and cycle lifecycle

#### 7.2.1 `SessionOpened`

Marks the start of a runtime session. Each session corresponds to one fossic stream. Root event of the session — no causation parent.

```json
{
  "session_id": "string",             // UUID; also the stream's cycle_id segment
  "goal": "string",                   // user-provided goal
  "cycle_config": "string",           // e.g., "simple.planning.v0"
  "vault_path": "string",
  "opened_at": int,                   // Unix epoch milliseconds
  "parent_session_id": "string?",     // for re-injection continuations; null for fresh sessions
  "recursion_depth": int?,            // 0 for fresh sessions; increments per re-injection
  "max_recursion_depth": int?         // configurable cap, default 5
}
```

**Determinism:** `true` — pure bookkeeping. **Causation:** none (root event).

#### 7.2.2 `CycleStarted`

Marks the start of a cognitive cycle within a session. Bookend pair with `CycleCompleted`.

```json
{
  "session_id": "string",
  "cycle_id": "string",               // the matching <cycle_id> from the stream pattern
  "cycle_config": "string",
  "started_at": int,
  "step_index": int?                  // for cycles within a session; default 0
}
```

**Determinism:** `true` — pure bookkeeping. **Causation:** `SessionOpened` for the parent session.

#### 7.2.3 `CycleCompleted`

Marks the end of a cognitive cycle. Bookend pair with `CycleStarted`.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "completed_at": int,
  "outcome": "string",                // "accept" | "stop" | "branched" | "continued"
  "total_steps": int,
  "branched_to_cycle_id": "string?",  // if outcome is "branched"
  "continued_to_session_id": "string?" // if outcome is "continued" (re-injection)
}
```

**Determinism:** `true` — pure bookkeeping derived from cycle execution. **Causation:** `CycleStarted` for the same `cycle_id`.

### 7.3 Step execution

#### 7.3.1 `StepStarted`

Marks the start of one cognitive step within a cycle. A cycle may execute many steps.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",                // UUID for this step
  "step_index": int,                  // 0-based ordinal within the cycle
  "started_at": int,
  "step_type": "string?"              // "generate" | "refine" | "critique" | "explore"
}
```

**Determinism:** `true` — bookkeeping. **Causation:** most recent `ClutchDecisionMade` in the cycle that selected this step (or `CycleStarted` for the first step).

#### 7.3.2 `ContextPacketBuilt`

The retrieval pipeline assembled the ContextPacket for this step.

> **Overlap note:** `ContextPacketBuilt` also exists in Cerebra's pre-Phase-6 retrieval flow. The Phase 6 version carries `cycle_id` and `step_id` for cycle-level traceability; the pre-Phase-6 version does not. Consumers must check for `cycle_id` presence to distinguish the two.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "packet_id": "string",
  "selected_count": int,              // number of memories selected
  "packet_version": int,
  "abstained": bool?,                 // true if retrieval abstained; selected_count would be 0
  "abstention_reason": "string?"
}
```

**Determinism:** `false` — retrieval involves embedding similarity which has model dependencies. **Causation:** `StepStarted`.

#### 7.3.3 `StepExecuted`

The cognitive step ran (LLM produced output). Captures the step's input and output summary.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "executed_at": int,
  "llm_model": "string",              // e.g., "granite-4.1-3b-instruct"
  "prompt_tokens": int,
  "completion_tokens": int,
  "output_text": "string",            // the LLM's structured output
  "latency_ms": int?,
  "temperature": float?,
  "top_p": float?
}
```

**Determinism:** `false` — LLM output is non-deterministic. **Causation:** `ContextPacketBuilt` for the same step.

### 7.4 Prediction and evaluation

#### 7.4.1 `PredictionMade`

Before a step executes, a prediction is recorded about expected output quality.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "prediction_id": "string",
  "expected_composite_score": float,  // 0.0 to 1.0
  "expected_per_signal": {...},       // predicted score per signal name
  "prediction_basis": "string",       // "prior_step_trajectory" | "cycle_config_default" | "static_baseline"
  "confidence": float?                // confidence in the prediction itself, 0.0 to 1.0
}
```

**Determinism:** `false` — depends on prior cycle state. **Causation:** `StepStarted` for the same step.

#### 7.4.2 `SignalEvaluated`

One signal scored the step's output. Six of these fire per step (one per signal in the six-signal epistemology: `COHERENCE`, `GROUNDEDNESS`, `GENERATIVITY`, `RELEVANCE`, `PRECISION`, `EPISTEMIC_HUMILITY`).

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "signal_name": "string",            // one of the six signal names above
  "signal_score": float,              // 0.0 to 1.0
  "evaluator_prompt_version": "string", // versioned prompt template ID
  "evaluated_at": int,
  "signal_strength": float?,          // confidence in the score itself
  "checklist_details": "dict?",       // per-checklist-item ratings if expanded evaluation
  "low_confidence": bool?             // true if signal_strength below threshold
}
```

**Determinism:** `false` — LLM-based evaluation. **Causation:** `StepExecuted` for the same step. **Note:** `checklist_details` is NOT exported to OTel (high cardinality).

#### 7.4.3 `EvaluationComposed`

The six signals were composed into a composite evaluation per the weighted formula (`Σ(signal_i × weight_i)`).

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "evaluation_id": "string",
  "composite_score": float,           // 0.0 to 1.0
  "per_signal_scores": {...},         // final scores after composition
  "weights_used": {...},              // weights applied (may differ from defaults per cycle config)
  "composed_at": int,
  "confidence": float?,
  "composite_floor_violated": bool?   // true if composite fell below cycle config floor
}
```

**Determinism:** `true` — pure function of `SignalEvaluated` outputs and weights. **Causation:** most recent `SignalEvaluated` for the step (chains back through all six).

#### 7.4.4 `OutcomeRecorded`

Compares the prediction to the actual evaluation, computes prediction error.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "outcome_id": "string",
  "prediction_id": "string",          // links to the matching PredictionMade
  "actual_composite_score": float,
  "prediction_error": float,          // actual - expected
  "error_classification": "string",   // "noise" (|err|<0.10) | "notable" (0.10-0.40) | "severe" (>0.40)
  "recorded_at": int,
  "per_signal_error": "dict?"         // error per signal
}
```

**Determinism:** `true` — pure subtraction of `EvaluationComposed` and `PredictionMade`. **Causation:** `EvaluationComposed` for the step.

#### 7.4.5 `PredictionSevereMiss`

Emitted alongside `OutcomeRecorded` when `error_classification` is `severe`. Allows targeted subscriber attention to severe misses without filtering all `OutcomeRecorded` events.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "outcome_id": "string",
  "prediction_error": float,
  "expected": float,
  "actual": float
}
```

**Determinism:** `true` — derived from `OutcomeRecorded`. **Causation:** `OutcomeRecorded` for the step.

### 7.5 Control decisions

#### 7.5.1 `ClutchDecisionMade`

The Clutch evaluated signals plus prediction error plus working memory state and produced a typed action decision.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "decision_id": "string",
  "action": "string",                 // "accept" | "refine" | "critique" | "explore" | "branch"
                                      // | "retrieve_more" | "consolidate" | "ask_user" | "pause" | "stop"
  "rule_matched": "string",           // name of the Clutch rule that fired
  "decided_at": int,
  "cascade_depth": int?,              // how many rules evaluated before one fired
  "escalate_to_catalyst": bool?,      // true if action requires Catalyst arm selection
  "evaluation_id": "string?"          // links to EvaluationComposed that informed the decision
}
```

**Determinism:** `true` — Clutch is a deterministic cascade given identical inputs. **Causation:** `OutcomeRecorded` (or `EvaluationComposed` if no prediction was made).

#### 7.5.2 `CatalystInvoked`

The Catalyst was called to select a strategy when the Clutch escalated.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "invocation_id": "string",
  "vocabulary_size": int,             // number of arms in the catalyst vocabulary
  "invoked_at": int,
  "triggering_clutch_decision_id": "string?",
  "leeway_filtered_vocabulary_size": int? // vocabulary size after leeway pre-filtering
}
```

**Determinism:** `true` — bookkeeping. **Causation:** `ClutchDecisionMade` with `escalate_to_catalyst: true`.

#### 7.5.3 `CatalystArmSelected`

The Catalyst's bandit selected a strategy arm.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "invocation_id": "string",
  "arm_name": "string",               // selected strategy
  "arm_score": float,                 // the multi-factor score that contributed to selection
  "selection_method": "string",       // "weighted_random" for v0.1
  "selected_at": int,
  "arm_stats_pre": "dict?",           // arm's stats before this selection (for replay)
  "tau": float?,                      // temperature parameter if used
  "all_arm_scores": "dict?"           // full distribution for diagnostic purposes
}
```

**Determinism:** `false` — `weighted_random` sampling is stochastic. **Causation:** `CatalystInvoked` for the same `invocation_id`.

### 7.6 Safety gate

#### 7.6.1 `LeewayGrantApplied`

The leeway pre-action gate evaluated the proposed action and applied grants from the loaded leeway rules. Fires **before** the action executes.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "proposed_action": "string",
  "grants_applied": ["string"],       // names of leeway rules that granted permission
  "final_decision": "string",         // "permitted" | "requires_review" | "forbidden"
  "applied_at": int,
  "forbidden_by": "string?",          // rule that forbade if final_decision is "forbidden"
  "review_required_by": ["string"]?   // rules requiring HITL review
}
```

**Determinism:** `true` — leeway rules are deterministic composition-by-union. **Causation:** `ClutchDecisionMade` (or `CatalystArmSelected` if catalyst was invoked) — fires before the action executes per the causal ordering requirement.

### 7.7 Re-injection

#### 7.7.1 `ContinuationBundleCreated`

A ContinuationBundle was distilled from the current cycle state for re-injection.

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "bundle_id": "string",
  "distilled_goal": "string",
  "summarized_prior_prompt": "string",
  "truth_tower_projection": {...},    // {t5_goal, t4_hypotheses?, t3_insights, t2_citations, t1_citations}
  "cognitive_insights": ["string"],
  "next_focus": "string",
  "open_questions": ["string"],
  "constraints": ["string"],
  "recursion_depth": int,
  "bundle_size_bytes": int,
  "created_at": int,
  "voice_mode": "string?",            // "system" for v0.1 ("self" deferred)
  "truncation_applied": bool?         // true if bundle hit size cap and was compressed
}
```

**Determinism:** `true` — bundle is a deterministic distillation of cycle state. **Causation:** most recent `ClutchDecisionMade` implying continuation, or context budget exhaustion trigger.

#### 7.7.2 `ReinjectionTriggered`

A new continuation session is being spawned from a ContinuationBundle.

```json
{
  "session_id": "string",             // the parent session
  "cycle_id": "string",               // the parent cycle
  "bundle_id": "string",
  "child_session_id": "string",       // the new session about to open
  "trigger_reason": "string",         // "context_budget" | "clutch_spawn" | "explicit_continuation"
  "triggered_at": int,
  "recursion_cap_hit": bool?          // true if this was the last allowed continuation
}
```

**Determinism:** `true` — bookkeeping. **Causation:** `ContinuationBundleCreated` for the same `bundle_id`.

### 7.8 Memory updates and session close

#### 7.8.1 `MemoryWriteFromCycle`

The cycle wrote new content to memory (episodic record of cycle output).

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "record_id": "string",              // new memory record created
  "write_reason": "string",           // "accept" | "consolidate" | "branch_anchor"
  "content_summary": "string",
  "written_at": int,
  "source_lineage": ["string"]?       // record IDs whose content contributed to this write
}
```

**Determinism:** `false` — content depends on LLM output. **Causation:** `ClutchDecisionMade` with action `accept` or `consolidate`.

#### 7.8.2 `SessionFlushed`

Session ending event, written when Clutch action is `stop`. Signals that working memory was flushed and the session is being closed.

```json
{
  "session_id": "string",
  "cycle_id": "string",               // the cycle that issued the stop
  "total_cycles": int,
  "total_steps": int,
  "flushed_at": int,
  "final_outcome": "string",          // "accepted" | "cap_reached" | "user_requested" | "error"
  "consolidation_pending": bool?      // true if Phase 10 consolidation should run
}
```

**Determinism:** `true` — bookkeeping. **Causation:** `ClutchDecisionMade` with action `stop`.

### 7.9 Consolidation

#### 7.9.1 `ConsolidationStarted`

`cerebra consolidate --session <id>` started a consolidation pass for a closed session.

```json
{
  "session_id": "string",
  "consolidation_id": "string",
  "session_event_count": int,         // number of events to consolidate
  "started_at": int
}
```

**Determinism:** `true` — bookkeeping. **Causation:** external (CLI invocation) or automatic post-`SessionFlushed` if configured.

#### 7.9.2 `ConsolidationCompleted`

Consolidation produced a summary record.

```json
{
  "session_id": "string",
  "consolidation_id": "string",
  "summary_record_id": "string",      // the new memory record holding the summary
  "calibration_audit": {...},         // {per_signal_calibration_delta, overall_calibration_status}
  "completed_at": int,
  "cited_record_ids": ["string"]?,    // memory records cited in the summary
  "cited_lineage_ids": ["string"]?    // lattice lineages referenced
}
```

**Determinism:** `false` — summary content is LLM-derived. **Causation:** `ConsolidationStarted` for the same `consolidation_id`.

### 7.10 Graph export

#### 7.10.1 `GraphExported`

`cerebra export graph --out <path>` produced a JSON export file.

```json
{
  "export_id": "string",
  "output_path": "string",
  "node_count": int,
  "edge_count": int,
  "exported_at": int,
  "vault_path": "string?",
  "session_filter": "string?",        // if export was scoped to a specific session
  "include_lattice_lineages": bool?   // should match v0.1-full setting
}
```

**Determinism:** `true` — graph state at export time is deterministic given a fixed event log. **Causation:** external (CLI invocation).

---

## 8. OpenTelemetry GenAI span mapping

The fossic OTel exporter (optional, subscribed to streams matching configurable patterns) converts agent trace events to OTel GenAI semantic convention spans.

### 8.1 Standard agent-trace mapping

| Fossic event | OTel span kind | Key OTel attributes |
|---|---|---|
| `llm_call` | CLIENT (span begin) | `gen_ai.system`, `gen_ai.request.model`, `gen_ai.request.temperature`, `gen_ai.request.max_tokens`, `gen_ai.usage.input_tokens` (provisional from `llm_response`) |
| `llm_response` | CLIENT (span end) | `gen_ai.response.finish_reasons`, `gen_ai.usage.output_tokens`, `gen_ai.usage.total_tokens` |
| `tool_call` | INTERNAL (span begin) | `gen_ai.tool.name`, `gen_ai.tool.call.id`, `fossic.tool.deterministic` |
| `tool_result` | INTERNAL (span end) | `gen_ai.tool.result` (truncated to 1 KB), `fossic.tool.latency_ms` |
| `reasoning_step` | INTERNAL (single span) | `fossic.reasoning.text` (truncated to 1 KB), `fossic.reasoning.step_type` |

Rhyzome and bons.ai extension types map to INTERNAL spans with `fossic.event_type` set to the extension type name. Their structured fields become span attributes prefixed with `fossic.<type>.*`.

### 8.2 Cerebra cycle runtime mapping

The span hierarchy for Cerebra cycle events is: **session → cycle → step**. Session spans contain cycle spans; cycle spans contain step spans; evaluation and control sub-events are children of their respective step spans.

**Cardinality note:** `checklist_details` from `SignalEvaluated` is NOT exported to OTel — it is a high-cardinality nested dict that would bloat trace storage. All other standard Cerebra fields are exported.

**Re-injection:** `ReinjectionTriggered` events set a `gen_ai.cerebra.child_session_id` attribute and link the child session span to the parent via OTel's span link mechanism. The `parent_session_id` attribute on the child `SessionOpened` span closes the link.

**Namespace convention:** Cerebra-specific OTel attributes use the `gen_ai.cerebra.*` prefix. Future consumers add analogous namespaces (`gen_ai.rhyzome.*`, `gen_ai.bonsai.*`) as their OTel integration matures.

| Fossic event | OTel span kind | Key OTel attributes |
|---|---|---|
| `SessionOpened` | INTERNAL (session root begin) | `gen_ai.cerebra.session_id`, `gen_ai.cerebra.cycle_config`, `gen_ai.cerebra.recursion_depth`, `gen_ai.cerebra.parent_session_id` |
| `SessionFlushed` | INTERNAL (session root end) | `gen_ai.cerebra.session_id`, `gen_ai.cerebra.final_outcome`, `gen_ai.cerebra.total_cycles`, `gen_ai.cerebra.total_steps` |
| `CycleStarted` | INTERNAL (cycle span begin) | `gen_ai.cerebra.session_id`, `gen_ai.cerebra.cycle_id`, `gen_ai.cerebra.cycle_config` |
| `CycleCompleted` | INTERNAL (cycle span end) | `gen_ai.cerebra.outcome`, `gen_ai.cerebra.total_steps` |
| `StepStarted` | INTERNAL (step span begin) | `gen_ai.cerebra.session_id`, `gen_ai.cerebra.cycle_id`, `gen_ai.cerebra.step_id`, `gen_ai.cerebra.step_type` |
| `ContextPacketBuilt` | INTERNAL (sub-span of step) | `gen_ai.cerebra.packet_id`, `gen_ai.cerebra.selected_count`, `gen_ai.cerebra.abstained` |
| `StepExecuted` | INTERNAL (step span end) | `gen_ai.request.model` (from `llm_model`), `gen_ai.cerebra.prompt_tokens`, `gen_ai.cerebra.completion_tokens`, `gen_ai.cerebra.step_id` |
| `PredictionMade` | INTERNAL (single span) | `gen_ai.cerebra.prediction_id`, `gen_ai.cerebra.expected_composite_score`, `gen_ai.cerebra.prediction_basis` |
| `SignalEvaluated` | INTERNAL (single span) | `gen_ai.cerebra.signal_name`, `gen_ai.cerebra.signal_score`, `gen_ai.cerebra.low_confidence` |
| `EvaluationComposed` | INTERNAL (single span) | `gen_ai.cerebra.evaluation_id`, `gen_ai.cerebra.composite_score`, `gen_ai.cerebra.composite_floor_violated` |
| `OutcomeRecorded` | INTERNAL (single span) | `gen_ai.cerebra.outcome_id`, `gen_ai.cerebra.prediction_error`, `gen_ai.cerebra.error_classification` |
| `PredictionSevereMiss` | INTERNAL (single span) | `gen_ai.cerebra.prediction_error`, `gen_ai.cerebra.expected`, `gen_ai.cerebra.actual` |
| `ClutchDecisionMade` | INTERNAL (single span) | `gen_ai.cerebra.action`, `gen_ai.cerebra.rule_matched`, `gen_ai.cerebra.escalate_to_catalyst` |
| `CatalystInvoked` | INTERNAL (sub-span begin) | `gen_ai.cerebra.invocation_id`, `gen_ai.cerebra.vocabulary_size`, `gen_ai.cerebra.leeway_filtered_vocabulary_size` |
| `CatalystArmSelected` | INTERNAL (sub-span end) | `gen_ai.cerebra.arm_name`, `gen_ai.cerebra.arm_score`, `gen_ai.cerebra.selection_method` |
| `LeewayGrantApplied` | INTERNAL (single span) | `gen_ai.cerebra.proposed_action`, `gen_ai.cerebra.final_decision`, `gen_ai.cerebra.grants_applied` |
| `ContinuationBundleCreated` | INTERNAL (sub-span) | `gen_ai.cerebra.bundle_id`, `gen_ai.cerebra.recursion_depth`, `gen_ai.cerebra.bundle_size_bytes` |
| `ReinjectionTriggered` | INTERNAL (single span; links to child session span) | `gen_ai.cerebra.child_session_id`, `gen_ai.cerebra.trigger_reason`, `gen_ai.cerebra.recursion_cap_hit` |
| `MemoryWriteFromCycle` | INTERNAL (single span) | `gen_ai.cerebra.record_id`, `gen_ai.cerebra.write_reason` |
| `ConsolidationStarted` | INTERNAL (consolidation span begin) | `gen_ai.cerebra.session_id`, `gen_ai.cerebra.consolidation_id`, `gen_ai.cerebra.session_event_count` |
| `ConsolidationCompleted` | INTERNAL (consolidation span end) | `gen_ai.cerebra.summary_record_id`, `gen_ai.cerebra.consolidation_id` |
| `GraphExported` | INTERNAL (single span) | `gen_ai.cerebra.export_id`, `gen_ai.cerebra.node_count`, `gen_ai.cerebra.edge_count` |

### 8.3 Exporter configuration

```python
from fossic.otel import OtelExporter, OtelConfig

exporter = OtelExporter(OtelConfig(
    endpoint="localhost:4317",          # OTLP gRPC default
    service_name="fossic",
    stream_patterns=["*/agent-trace/*", "rhyzome/repair/*", "bonsai/idea/*"],
    batch_max_events=512,
    batch_max_wait_ms=1000,
))
exporter.attach(store)
```

The exporter subscribes to matching streams in PostCommit mode (no blocking the writer). On batch send failure, events are buffered up to `batch_max_buffered` (default 8192) and retried with exponential backoff. On sustained collector unavailability the exporter is marked degraded but does not affect fossic's append path.

If `endpoint` is unset or the collector is unreachable for >60s, the exporter is a no-op. Local-first means the absence of an observability stack must not block the application.

### 8.4 Span context propagation

`correlation_id` on fossic events maps to OTel trace_id (with appropriate truncation/expansion to OTel's 16-byte format). `causation_id` chains form OTel parent-child span relationships. This means consumers with pre-existing OTel infrastructure can see fossic-recorded agent traces in their normal trace explorer (Tempo, Grafana, Jaeger, etc.) with proper causal structure.

---

## 9. Adding new event types

To add a new standard event type to fossic core (not an extension):

1. Open a proposal documenting the use case, the payload shape, and the determinism implications.
2. The payload must be representable as JSON-compatible types (objects, arrays, strings, numbers, booleans, null). No binary blobs except as base64-encoded strings or as separate event payloads referenced by id.
3. The type name uses `snake_case`. Standard types have no namespace prefix; extension types use `<consumer>_<type>` (e.g., `rhyzome_strategy_selected`) or PascalCase with a consumer section in this doc (Cerebra's convention).
4. The OTel mapping is specified.
5. Test vectors are added to the `agent-trace-test-vectors.json` file.

To add a new extension type (rhyzome, bons.ai, Cerebra, or another consumer):

1. Document it in the consumer's own codebase.
2. Add a row to the Consumer Extension Registry (§2).
3. Append a section to this document.
4. No fossic core change is required — the OTel exporter handles unknown types via the `fossic.event_type` + attribute prefix convention.

---

*End of agent trace vocabulary.*
