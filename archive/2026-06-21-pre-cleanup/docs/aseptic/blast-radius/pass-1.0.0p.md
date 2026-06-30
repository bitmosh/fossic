---
pass: 1.0.0p
version: v1.0.0p
date: 2026-06-13
summary: Docs-only. AGENT_TRACE_VOCABULARY.md §7.5 and §8.2 corrected to match Cerebra Phase 9 Step 1 and Step 3 actual emissions.
---

# Blast Radius — Pass 1.0.0p (v1.0.0p)

## Files

### Modified
- `docs/implement/AGENT_TRACE_VOCABULARY.md` — §7.5 and §8.2 corrected (see below)
- `docs/aseptic/TECH_DEBT.md` — `last_reviewed` → v1.0.0p
- `docs/aseptic/POLISH_DEBT.md` — `last_reviewed` → v1.0.0p
- `docs/aseptic/DEVIATION.md` — `last_reviewed` → v1.0.0p
- `docs/aseptic/README.md` — version → v1.0.0p

### Created
- `docs/aseptic/blast-radius/pass-1.0.0p.md` — this file

### Deleted
- (none)

---

## Correction summary

### Source cross-pollinations

This pass responds to two Cerebra cross-pollination files:
- `cerebra/docs/aseptic/cross-pollination/pass-9.1.md` — ClutchDecisionMade payload extension (cascade_depth, escalate_to_catalyst promoted from optional to required)
- `cerebra/docs/aseptic/cross-pollination/pass-9.3.md` — CatalystInvoked and CatalystArmSelected canonical schemas extracted from commit 432b834

### §7.5 — Topology preamble added

Added a blockquote preamble to `### 7.5 Control decisions` clarifying the causation topology:
- The catalyst sub-flow (`CatalystInvoked → CatalystArmSelected`) is a **sibling branch** off `ClutchDecisionMade`, not a continuation of the step's main causal chain.
- `ClutchDecisionMade` remains the causation parent of whatever action event follows.
- Relevant to LumaWeave's `walk_causation` rendering for R-F-003.

### §7.5.1 — ClutchDecisionMade field changes

Promoted two fields from optional to required, reordered, updated comments:

| Field | Before | After |
|---|---|---|
| `cascade_depth` | `int?` — "how many rules evaluated before one fired" | `int` — "0-indexed position of the matching rule; equals len(rules) when no rule matched" |
| `escalate_to_catalyst` | `bool?` — "true if action requires Catalyst arm selection" | `bool` — "true only when no rule matched; false otherwise" |
| `evaluation_id` | `"string?"` | `"string"` |

Field order updated to match actual Cerebra emission order:
`session_id, cycle_id, step_id, decision_id, action, rule_matched, cascade_depth, escalate_to_catalyst, decided_at, evaluation_id`

Determinism and causation lines unchanged.

### §7.5.2 — CatalystInvoked canonical schema

Complete replacement. Prior entry was pre-implementation speculative. Removed fields that are NOT emitted in v0.1:
- `invocation_id` — removed (causation carried by auto-chain, not an explicit ID field)
- `vocabulary_size` — removed (not emitted)
- `triggering_clutch_decision_id` — removed (not emitted; causation_id does this job)
- `leeway_filtered_vocabulary_size` — removed (not emitted)

Canonical payload (4 fields):
```json
{
  "session_id": "string",
  "cycle_id": "string",
  "step_id": "string",
  "invoked_at": int
}
```

Causation note updated: auto-chained from `ClutchDecisionMade` via `EventEmitter._last_event_id`; no explicit `causation_id` argument at the emission site.

### §7.5.3 — CatalystArmSelected canonical schema

Complete replacement. Prior entry was pre-implementation speculative. Three field name corrections:

| Old | New |
|---|---|
| `arm_name` | `arm_id` |
| `arm_score` | `score` |
| `selection_method` | `selection_reason` |

Two fields added (not in prior speculative schema):
- `arm_type` — arm's `type` field from cycle config
- `mapped_action` — the CLUTCH_ACTION the arm maps to

Five fields removed (not emitted in v0.1):
- `invocation_id` — removed (handled by causation chain)
- `arm_stats_pre` — not emitted
- `tau` — not emitted
- `all_arm_scores` — not emitted
- `score_components` — not emitted (noted as v0.2 gap in a blockquote)

Path B (cannot-select) emission shape added. Prior doc only documented the success path.

Causation note updated: auto-chained from `CatalystInvoked` for the same step. Reference to `invocation_id` removed.

### §8.2 OTel mapping table

Three rows updated:

| Event | Before | After |
|---|---|---|
| `ClutchDecisionMade` | `…action, rule_matched, escalate_to_catalyst` | `…action, rule_matched, clutch.cascade_depth, clutch.escalate_to_catalyst` |
| `CatalystInvoked` | `…invocation_id, vocabulary_size, leeway_filtered_vocabulary_size` | `…session_id, cycle_id, step_id` |
| `CatalystArmSelected` | `…arm_name, arm_score, selection_method` | `…arm_id, arm_type, score, selection_reason` |

`escalate_to_catalyst` moved to `gen_ai.cerebra.clutch.escalate_to_catalyst` sub-namespace (consistent with `signal.` sub-namespace pattern from v0.10.0q; `clutch.` groups clutch-specific diagnostic attributes). `cascade_depth` follows the same convention as `gen_ai.cerebra.clutch.cascade_depth`.

---

## Public APIs

None. Docs-only pass.

---

## Schema changes

None. fossic core is payload-agnostic; these are vocabulary documentation changes only.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

None. Docs-only. The OTel exporter does not yet exist; the §8.2 corrections align the spec to actual Cerebra emission before implementation begins.

---

## Living report updates

`last_reviewed` bumped to v1.0.0p on TECH_DEBT, POLISH_DEBT, DEVIATION, and README.
No new entries filed or resolved this pass.

---

## Adjacent project impact

**Cerebra** — This pass closes the fossil-side gap created by Cerebra passes 9.1 and 9.3. The vocabulary doc now reflects actual Cerebra v0.3.6 emission. Strict schema validators built from §7.5 entries can now pass `ClutchDecisionMade` events without rejecting `cascade_depth` / `escalate_to_catalyst`. When building the OTel exporter, use the corrected §8.2 attribute names for all catalyst events.

**LumaWeave** — The topology preamble added to §7.5 documents that the catalyst sub-chain is a sibling branch from `ClutchDecisionMade`. R-F-003 (cross-project causation visualization) and the time-travel viewer should model the catalyst sub-flow accordingly — `walk_causation(Forward)` from `ClutchDecisionMade` reaches the catalyst events as one branch and the next-step events as another.
