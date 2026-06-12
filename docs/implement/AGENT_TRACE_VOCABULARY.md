# Agent Trace Vocabulary

**Status:** v1 specification · 2026-06-12
**Scope:** Standard event types fossic ships for agent trace recording, the per-tool determinism registry, the rhyzome and bons.ai extensions, and the OpenTelemetry GenAI span mapping.

---

## 1. Why this is a separate document

Agent trace event types grow over time. Every new agent-runtime integration potentially adds new event types. Keeping them in the main fossic spec would bloat it; keeping them only in code makes the protocol invisible. This document is the canonical vocabulary list, and it is intended to grow.

The standard event types live in the `fossic` crate. The rhyzome and bons.ai extensions live in their respective consumer codebases — they are documented here for cross-project coordination but fossic core does not depend on them.

---

## 2. Standard event types (fossic core)

These five event types ship with fossic and are recognized by the OpenTelemetry GenAI exporter. All are versioned starting at `type_version=1`.

### 2.1 `llm_call`

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

### 2.2 `llm_response`

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

### 2.3 `tool_call`

An invocation of a tool by an agent. Payload:

```json
{
  "tool_name": "string",
  "tool_call_id": "string",           // consumer-supplied, matches llm_response.tool_calls[].id
  "arguments": {...},                 // tool-specific argument object
  "deterministic": bool               // see §3
}
```

The `deterministic` field is load-bearing — it determines replay behavior. Default is `false`. Consumers register tools with the trace adapter to set per-tool defaults; explicit override on individual events is allowed.

### 2.4 `tool_result`

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

### 2.5 `reasoning_step`

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

## 3. Per-tool determinism registry

The `deterministic` flag default flipped from `true` to `false` in v1. Reasoning: cost of unnecessary re-execution is visible (latency); cost of a wrong `true` is invisible (silent replay corruption with stale data). The constraint-design principle picks the failure mode that surfaces.

### 3.1 Registry API

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

### 3.2 Recommended defaults table

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

### 3.3 Replay semantics

On replay through a reducer or via the time-travel viewer:

- **deterministic=true:** the stored `tool_result` is served as the result of the tool call. The tool is not re-executed. The agent sees the same result it saw originally.
- **deterministic=false:** the tool is re-executed against the current environment. The new result may differ from the stored one. Consumers can opt into a "comparison mode" where both the stored and re-executed results are surfaced and divergence is logged.

Rhyzome uses comparison mode for `run_pytest`: a stored PASS that replays as FAIL is a first-class finding (external regression introduced between original session and replay).

---

## 4. Rhyzome extension event types

These types are defined in rhyzome's codebase, not in fossic core. They are documented here so other consumers (LumaWeave's time-travel viewer, the OTel exporter) can recognize them.

### 4.1 `strategy_selected`

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

### 4.2 `ast_gate_evaluated`

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

### 4.3 `strategy_exhausted`

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

## 5. Bons.ai extension event types

These types are defined in bons.ai's codebase, not in fossic core. Same convention as rhyzome.

### 5.1 `bandit_arm_selected`

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

### 5.2 `bandit_arm_updated`

```json
{
  "arm_id": "string",
  "reward": float,
  "visit_count": int,
  "reward_mean": float,
  "posterior": {...}                  // bandit-specific posterior state
}
```

### 5.3 `bandit_decision`

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

### 5.4 `stagnation_detected`

```json
{
  "stream_id": "string",              // the idea lineage
  "stagnation_level": float,
  "similarity_signal": float,
  "response": "string",               // "exploration_rate_bump" | "forced_mutation" | "fork"
  "triggered_branch_id": "string?"    // if response was "fork"
}
```

### 5.5 `adaptation_applied`

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

### 5.6 `memory_retrieved`

```json
{
  "query": "string",
  "retrieved_cycles": [
    {"cycle_id": "string", "score": float, "stream_id": "string"}
  ],
  "retrieval_purpose": "string"       // "context_building" | "lineage_check" | "similarity_search"
}
```

### 5.7 `embedding_stored`

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

## 6. OpenTelemetry GenAI span mapping

The fossic OTel exporter (optional, subscribed to streams matching configurable patterns) converts agent trace events to OTel GenAI semantic convention spans.

### 6.1 Mapping

| Fossic event | OTel span kind | Key OTel attributes |
|---|---|---|
| `llm_call` | CLIENT (span begin) | `gen_ai.system`, `gen_ai.request.model`, `gen_ai.request.temperature`, `gen_ai.request.max_tokens`, `gen_ai.usage.input_tokens` (provisional from `llm_response`) |
| `llm_response` | CLIENT (span end) | `gen_ai.response.finish_reasons`, `gen_ai.usage.output_tokens`, `gen_ai.usage.total_tokens` |
| `tool_call` | INTERNAL (span begin) | `gen_ai.tool.name`, `gen_ai.tool.call.id`, `fossic.tool.deterministic` |
| `tool_result` | INTERNAL (span end) | `gen_ai.tool.result` (truncated to 1 KB), `fossic.tool.latency_ms` |
| `reasoning_step` | INTERNAL (single span) | `fossic.reasoning.text` (truncated to 1 KB), `fossic.reasoning.step_type` |

Rhyzome and bons.ai extension types map to INTERNAL spans with `fossic.event_type` set to the extension type name. Their structured fields become span attributes prefixed with `fossic.<type>.*`. The OTel exporter does not need to know the extension types specifically — it inspects the payload structure.

### 6.2 Exporter configuration

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

### 6.3 Span context propagation

`correlation_id` on fossic events maps to OTel trace_id (with appropriate truncation/expansion to OTel's 16-byte format). `causation_id` chains form OTel parent-child span relationships. This means consumers that pre-existing OTel infrastructure can see fossic-recorded agent traces in their normal trace explorer (Tempo, Grafana, Jaeger, etc.) with proper causal structure.

---

## 7. Adding new event types

To add a new standard event type to fossic core (not an extension):

1. Open a proposal documenting the use case, the payload shape, and the determinism implications.
2. The payload must be representable as JSON-compatible types (objects, arrays, strings, numbers, booleans, null). No binary blobs except as base64-encoded strings or as separate event payloads referenced by id.
3. The type name uses `snake_case`. Standard types have no namespace prefix; extension types use `<consumer>_<type>` (e.g., `rhyzome_strategy_selected`).
4. The OTel mapping is specified.
5. Test vectors are added to the `agent-trace-test-vectors.json` file.

To add a new extension type (rhyzome, bons.ai, or another consumer):

1. Document it in the consumer's own codebase under `docs/agent-trace-extensions.md` (or equivalent).
2. Append a section to this document via PR.
3. No fossic core change is required — the OTel exporter handles unknown types via the `fossic.event_type` + attribute prefix convention.

---

*End of agent trace vocabulary.*
