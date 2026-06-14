---
pass: v1.0.0o
version: v1.0.0o
sha: 8abbb8e
date: 2026-06-14
summary: AGENT_TRACE_VOCABULARY.md corrections — stream key, CatalystArmSelected score_components, ReinjectionTriggered schema, OTel fix
---

# Blast Radius — Pass v1.0.0o

## Files

### Modified
- `docs/implement/AGENT_TRACE_VOCABULARY.md` — four corrections applied (see below); status bumped to v1.0.0o

### Created
- `docs/aseptic/blast-radius/pass-1.0.0o.md` — this file

---

## Changes

### Stream key correction (§7.1, §7.2.1, §7.2.2, preamble)

**Scope:** Docs only. No fossic core behavior change — fossic stream routing is name-agnostic at runtime.

- §7.1 preamble + stream pattern lock: `cerebra/agent-trace/<cycle_id>` → `cerebra/agent-trace/<session_id>` throughout
- Forward-compat reservation updated to use `<session_id>/<sub_id>`
- §7.2.1 `SessionOpened` JSON comment: `cycle_id segment` → `session_id segment`
- §7.2.2 `CycleStarted` JSON comment: clarified that `cycle_id` is a payload field, not the stream segment

**Why:** Cerebra's streams are session-scoped, not cycle-scoped. A single session spans multiple cycles (including re-injection children). Stream path was wrong in the spec; actual Cerebra implementation uses `session_id`. Source: Cerebra Claude (3-way session, 2026-06-14), routed via Lattica.

**Adjacent impact:**
- LumaWeave: `stream_glob: "cerebra/agent-trace/*"` in payloadRendererRegistry is unaffected — glob still matches
- Consumers who hardcoded `cerebra/agent-trace/<cycle_id>` as a literal stream path should update to `<session_id>`

### Correction A — §7.5.3 `CatalystArmSelected`

Added `score_components: null` to Path A JSON schema with a v0.2 gap note. The field exists in the `CatalystSelection` dataclass but is absent from the emitted payload in v0.1. Consumers must not assume its presence. Updated inline note for precision.

### Correction B — §7.7.2 `ReinjectionTriggered` (schema replacement)

**Old schema (stale — fields never existed in actual emission):**
- `trigger_reason` (wrong — does not exist)
- `bundle_id` (wrong — should be `continuation_bundle_id`)
- `recursion_cap_hit` (wrong — does not exist)
- Causation: `ContinuationBundleCreated` (wrong)

**New schema (from actual emission site `cerebra/cognition/cycle_runtime.py` `_try_reinject()`):**
- `trigger_predicate` — predicate name that fired
- `continuation_bundle_id` — references `continuation_bundles` table in Cerebra's DB
- `child_session_id` — newly spawned child session
- `recursion_depth` — child's depth (parent + 1)
- `triggered_at` — Unix epoch ms

Causation corrected to: auto-chained from `SessionFlushed` via `EventEmitter._last_event_id`.

Added: no-emit note when depth limit blocks re-injection. Added: causal chain diagram showing the two separate chains (within-cycle Catalyst vs. post-cycle re-injection).

Source: Cerebra pass-9.4 cross-pollination (b175874).

### §8.2 OTel mapping fix (`ReinjectionTriggered` row)

- `gen_ai.cerebra.trigger_reason` → `gen_ai.cerebra.trigger_predicate`
- Removed `gen_ai.cerebra.recursion_cap_hit`
- Added `gen_ai.cerebra.recursion_depth`

### §2 Sibling vocabulary note

Added scope note after Consumer Extension Registry table clarifying that governance/audit event vocabularies (streams not in the `agent-trace` tier) live in sibling vocabulary files. Named `POLICY_SCOUT_EVENT_VOCABULARY.md` as the first planned sibling.

---

## Public APIs

No changes. Docs-only pass.

## Schema changes

`AGENT_TRACE_VOCABULARY.md` is a vocabulary specification, not a fossic schema. The `ReinjectionTriggered` payload shape correction reflects the actual emission — no fossic core change required (fossic payloads are opaque).

## Adjacent project notifications

- **Cerebra:** FYI — `ReinjectionTriggered` schema in vocabulary doc now matches actual emission. OTel attributes corrected.
- **LumaWeave:** FYI — stream path is `<session_id>`, not `<cycle_id>`. Glob subscriptions unaffected.
- **Policy Scout:** Sibling vocab doc guidance sent (round 2 response).
- **Lattica:** Stream key correction applied; `ActionProposed` banked for v0.2.

## Living report updates

No new entries this pass.
