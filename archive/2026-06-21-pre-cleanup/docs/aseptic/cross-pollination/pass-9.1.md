# Cross-Pollination — Cerebra pass-9.1 → fossic

**Date:** 2026-06-13
**From:** Cerebra v0.3.6 (Phase 9 Step 1)
**To:** fossic / AGENT_TRACE_VOCABULARY.md
**Severity:** NEEDS-AWARENESS
**Author:** Cerebra bandit

---

## What changed in Cerebra

`ClutchDecisionMade` event payload extended with two new fields:

```
cascade_depth: int          # 0-indexed position of the matching rule in the clutch cascade
                            # equals len(rules) when no rule matched (escalation case)
escalate_to_catalyst: bool  # True when no rule matched; False otherwise
```

Both fields are now emitted by `CycleRuntime` via `emitter.emit_cycle_event("ClutchDecisionMade", {...})` on every step.

Previously emitted payload shape (Phase 8):
```json
{
  "session_id": "...",
  "cycle_id": "...",
  "step_id": "...",
  "decision_id": "...",
  "action": "accept",
  "rule_matched": "default_accept",
  "decided_at": 1718000000000,
  "evaluation_id": "..."
}
```

New payload shape (Phase 9 Step 1+):
```json
{
  "session_id": "...",
  "cycle_id": "...",
  "step_id": "...",
  "decision_id": "...",
  "action": "accept",
  "rule_matched": "default_accept",
  "cascade_depth": 3,
  "escalate_to_catalyst": false,
  "decided_at": 1718000000000,
  "evaluation_id": "..."
}
```

---

## Why this matters for fossic

`AGENT_TRACE_VOCABULARY.md` documents the canonical payload schema for `ClutchDecisionMade`. If the doc was copied verbatim into a consumer or used to generate a schema validator, the new fields will cause:

- **Schema validation rejections** if the consumer uses strict/deny-unknown mode
- **Stale documentation** if the vocab doc is used as the ground-truth reference

The extension is additive and non-breaking for permissive consumers.

---

## Recommended action for Lattica / fossic

Update `AGENT_TRACE_VOCABULARY.md` `ClutchDecisionMade` payload schema to document:
- `cascade_depth: int` — which rule index fired (0-indexed); `len(rules)` on no-match
- `escalate_to_catalyst: bool` — True only when no rule matched

No Rust/Python fossic code changes required — fossic core is payload-agnostic. This is documentation-only.

---

## Reference

- Cerebra `cerebra/cognition/cycle_runtime.py` — both ClutchDecisionMade emit sites updated
- Cerebra `cerebra/cognition/clutch.py` — ClutchDecision dataclass now includes these fields
- Cerebra deviation log: `docs/agent/deviations/v0.3.6.md` DEV-026
