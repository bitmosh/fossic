# Pass 9.3 — Cerebra → fossic Cross-Pollination

**Date:** 2026-06-13
**Severity:** NEEDS-AWARENESS (docs-only, non-blocking)
**Source pass:** Cerebra Phase 9 Step 3 (commit 432b834)
**Affected fossic surface:** `docs/implement/AGENT_TRACE_VOCABULARY.md` §7 (Cerebra extension events)
**Author:** Cerebra Claude

---

## Summary

Phase 9 Step 3 ships `CatalystEngine` (`cerebra/cognition/catalyst.py`) and wires it into
`CycleRuntime`. This introduces two new fossic events emitted when Clutch escalates to Catalyst:

- `CatalystInvoked` — emitted when `ClutchDecisionMade.escalate_to_catalyst` is True and
  `CatalystEngine` is configured on the cycle config
- `CatalystArmSelected` — emitted when `CatalystEngine.select()` returns (both success and
  cannot-select paths)

The payload shapes below are extracted from the **actual emission sites in
`cerebra/cognition/cycle_runtime.py`** as of commit 432b834, not from pre-implementation
planning docs. Where the code diverges from prior specs (D5 in
`catalyst_integration_decisions.md`), the code is canonical.

---

## Event: CatalystInvoked

**When emitted:** After `ClutchDecisionMade` fires with `escalate_to_catalyst: true`, immediately
before `CatalystEngine.select()` is called, when `self._catalyst_engine is not None`.

**Causation:** Auto-chained via `EventEmitter._last_event_id` — no explicit `causation_id`
argument. At the emission point, `_last_event_id` holds the `ClutchDecisionMade` event ID.
Effective causation chain: `ClutchDecisionMade → CatalystInvoked`.

**Indexed tags:** `session_id`, `cycle_id`, `step_id`

**Payload schema (actual):**

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "invoked_at": "int (Unix epoch milliseconds)"
}
```

**Divergence from D5 planning spec:** D5 anticipated `clutch_decision_id` and `available_arms`
fields. Neither appears in the actual emission. The causation link to `ClutchDecisionMade` is
carried implicitly via `causation_id` (auto-chain), making `clutch_decision_id` redundant.
`available_arms` was not emitted — vocabulary size information is not surfaced in v0.1.

---

## Event: CatalystArmSelected

**When emitted:** After `CatalystEngine.select()` returns. Two paths:

### Path A — arm selected (selection_reason: "forced_exploration" or "scored")

**Causation:** Auto-chained from `CatalystInvoked` (the immediately prior emit in this code path).

**Indexed tags:** `session_id`, `cycle_id`, `step_id`, `arm_id`

**Payload schema (actual):**

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "arm_id": "string",
  "arm_type": "string",
  "mapped_action": "string",
  "selection_reason": "string",
  "score": "float",
  "selected_at": "int (Unix epoch milliseconds)"
}
```

**Field semantics:**

| Field | Values / notes |
|---|---|
| `arm_id` | The arm's declared `arm_id` from cycle config (e.g., `"constraint_check"`) |
| `arm_type` | The arm's `type` field (e.g., `"verification"`, `"structuring"`, `"estimation"`) |
| `mapped_action` | The CLUTCH_ACTION this arm maps to (e.g., `"refine"`) |
| `selection_reason` | `"forced_exploration"` (arm had zero prior selections) or `"scored"` (bandit scored) |
| `score` | Composite bandit score: `base_reward × type_penalty × confidence_ramp`; `0.0` on forced exploration |

**Note on `score_components`:** The `CatalystSelection` dataclass carries
`score_components: dict[str, float]` with `base_reward`, `type_penalty`, `confidence_ramp`
individually. This field is **not emitted** to fossic in v0.1. If Lattica or fossic
needs the decomposed score for diagnostic rendering, this is a gap to address in v0.2.

### Path B — cannot select (no arms in vocabulary)

Fires when `CatalystEngine.select()` returns `None` (empty arm vocabulary, i.e., the engine
was initialized with an empty `catalyst_arms` list that bypassed the `not self._arms` guard —
see code). In practice, `CatalystEngine` is only constructed when `config.catalyst_arms` is
non-empty, so this path fires via the inner `catalyst_selection is None` branch.

**Payload schema (actual):**

```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "arm_id": null,
  "selection_reason": "no_arms",
  "selected_at": "int (Unix epoch milliseconds)"
}
```

**Divergence from D5/D6 planning spec:** D5 proposed `selected_arm_id` (now `arm_id`),
`selected_arm_type` (now `arm_type`), `arm_score` (now `score`), `sampled_at` (now
`selected_at`), `catalyst_invocation_id` (removed — handled by causation chain).
D6 proposed `selected_arm_id: null` and `reason: "cannot_select"` — actual uses `arm_id: null`
and `selection_reason: "no_arms"`.

---

## Causation chain (actual)

```
ClutchDecisionMade (escalate_to_catalyst=True)
  ↓ [auto-chain: _last_event_id at time of CatalystInvoked emit]
CatalystInvoked
  ↓ [auto-chain: _last_event_id at time of CatalystArmSelected emit]
CatalystArmSelected
```

**StepStarted (next iteration) is NOT causally chained to CatalystArmSelected.** The next
step's `StepStarted` emits with `causation_id=last_clutch_decision_id`, which still points to
the `ClutchDecisionMade` that triggered escalation. The catalyst sub-chain
(CatalystInvoked → CatalystArmSelected) is a sibling branch off ClutchDecisionMade, not a
linear extension to the next StepStarted.

**Strategy prompt injection:** The arm's `strategy_prompt` is held in `pending_strategy_hint`
and injected into the next step's prompt template as the `strategy_hint` Jinja2 variable.
This does NOT appear in any fossic event payload — it is invisible to fossic subscribers.
The connection from `CatalystArmSelected.arm_id` to the specific strategy text requires
consulting the cycle config's `catalyst_arms` definition.

---

## Requested action on fossic side

Update `docs/implement/AGENT_TRACE_VOCABULARY.md` §7.5 (Control decisions) to document both
events with the schemas above. Replace the speculative schemas in §7.5.2 (`CatalystInvoked`)
and §7.5.3 (`CatalystArmSelected`) if pre-implementation entries exist, or append new
entries if the section contains only the Phase 8 `ClutchDecisionMade` entry.

Key corrections vs. any pre-implementation entries:
- `CatalystInvoked`: remove `invocation_id`, `vocabulary_size`, `leeway_filtered_vocabulary_size`,
  `triggering_clutch_decision_id` if present — none are emitted
- `CatalystArmSelected`: field names are `arm_id` (not `arm_name` or `selected_arm_id`),
  `score` (not `arm_score`), `selected_at` (not `sampled_at`), `selection_reason`
  (not `selection_method`); `score_components` is NOT emitted

No fossic core changes required. Cerebra emits via the standard fossic event API;
events are payload-agnostic from fossic's perspective.

---

## Cerebra-side references

- Commit `432b834` (Phase 9 Step 3 — CatalystEngine)
- `cerebra/cognition/catalyst.py` — `CatalystEngine`, `CatalystSelection` dataclass
- `cerebra/cognition/cycle_runtime.py` — `CatalystInvoked` and `CatalystArmSelected`
  emission sites (~line 447 and ~line 463)
