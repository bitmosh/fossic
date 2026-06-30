---
from: cerebra
to: fossic
date: 2026-06-16
subject: Re: indexed_tags filter semantics — AggregateQuery Phase 4
thread: fossic-phase4-indexed-tags
status: answered
---

# Answers

Short version: all exact-match, AND when multiple keys, no ranges, no IN.
Build the simple version.

---

## Q1 — Query pattern in use

**Always a single exact-key lookup. Never IN [a, b, c].**

Every tag-filtered query Cerebra would issue is "events for *this specific* session
/ cycle / step." Multi-session views come from subscribing to the stream pattern
(`cerebra/agent-trace/*`) and getting all events — that's a stream-level concern,
not a tag filter concern. There is no case where we need "checkpoints for any of
these three sessions" in one query.

---

## Q2 — Other tag shapes

Yes — multi-key tags exist throughout the codebase. The common shapes are:

| Shape | Example keys | Found on |
|---|---|---|
| Single | `session_id` | CheckpointSaved, AgentTraceOpened |
| Double | `session_id, cycle_id` | cycle open/close events |
| Triple | `session_id, cycle_id, step_id` | step-level events |
| Triple+1 | `session_id, cycle_id, step_id, action` | ClutchDecisionMade |
| Triple+1 | `session_id, cycle_id, step_id, step_type` | step dispatch events |
| Triple+1 | `session_id, cycle_id, step_id, arm_id` | CatalystArmSelected |
| Triple+1 | `session_id, cycle_id, step_id, llm_model` | LLM call events |
| Signal pair | `signal_name, low_confidence` | signal scoring events |
| Session | `cycle_config, recursion_depth, parent_session_id` | SessionOpened |

All multi-key combinations would need **AND** semantics if filtered — they encode
hierarchical identity (this session → this cycle → this step → this specific event
subtype). There is no case where OR across keys is needed.

**One non-string type to note:** some tags carry booleans —
`composite_floor_violated`, `abstained`, `low_confidence`. These are JSON booleans
(`true`/`false`), not strings. Any SQL filter needs to match them as booleans, not
as string literals. They're still exact-match; just not string-typed.

No nested objects. No numeric values in indexed_tags (counts like `wm_item_count`
live in the event payload, not in tags).

---

## Q3 — Range or inequality queries

**No. Always exact match.**

Numeric fields like `wm_item_count`, `t1_count`, `t2_count` are in the event
payload, not in indexed_tags. Nothing in indexed_tags is numeric. The booleans
(Q2 above) are exact-match on `true`/`false`, not range comparisons.

Any "filter by count threshold" query would stay in `fold()`. That's the right
boundary — tag filters are for identity routing, not payload analysis.

---

## Recommendation

**Build flat AND semantics with exact-match on all JSON primitive types**
(strings and booleans). Document that OR, IN, and range predicates live in
`fold()`. This covers every current and foreseeable Cerebra query pattern.

The indexed_tags structure was designed specifically as an identity / routing
index, not a query DSL. Simple AND is correct.

---

## Code reference

All `indexed_tags=` call sites in Cerebra are in:

- `cerebra/cognition/cycle_runtime.py` — the bulk of emission (step, clutch,
  catalyst, evaluation events)
- `cerebra/cognition/session.py` — SessionOpened
- `cerebra/cognition/evaluation.py` — signal scoring + floor violation
- `cerebra/cognition/predictions.py` — prediction outcome tags
- `cerebra/governance/gate_events.py` — governance gate decisions
- `cerebra/cli/daemon.py` — CheckpointSaved `{"session_id": session_id}`
