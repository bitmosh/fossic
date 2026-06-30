# Pass 9.4 — Cerebra → fossic Cross-Pollination

**Date:** 2026-06-14
**Severity:** NEEDS-AWARENESS (docs-only, non-blocking)
**Source pass:** Cerebra Phase 9 Step 4 (v0.3.6)
**Affected fossic surface:** `docs/implement/AGENT_TRACE_VOCABULARY.md` §7 (Cerebra extension events)
**Author:** Cerebra Claude

---

## Summary

Phase 9 Step 4 wires `ReinjectionTriggerEvaluator` into `CycleRuntime`, making the
re-injection path active for cycle configs that declare `reinjection_triggers`. When a
cycle terminates with `outcome="cap_reached"` and no accepted step, `ReinjectionTriggered`
is emitted on the parent session's fossic stream before the child session spawns.

One new event is introduced:

- `ReinjectionTriggered` — emitted at cycle close on the PARENT stream when re-injection fires

The payload schema below is extracted from the **actual emission site in
`cerebra/cognition/cycle_runtime.py`** (method `_try_reinject`), not from the
pre-implementation kickoff schema in the Step 4 brief. Code is canonical.

---

## Event: ReinjectionTriggered

**When emitted:** After `SessionFlushed`, on the parent session's stream, when:
1. `CycleResult.outcome == "cap_reached"`
2. No step in the cycle had `clutch_action == "accept"`
3. `session.recursion_depth < config.max_recursion_depth`
4. A matching `reinjection_triggers` predicate fires

If `recursion_depth >= max_recursion_depth` (depth limit reached), NO event is emitted.
The cycle terminates normally with `outcome="cap_reached"` and `child_result=None`.

**Causation:** Auto-chained from the immediately prior emit (`SessionFlushed`) via
`EventEmitter._last_event_id`. Effective chain: `SessionFlushed → ReinjectionTriggered`.

**Stream:** Parent session's stream (`cerebra/agent-trace/<parent_session_id>`). The child
session's events appear on a separate stream starting with `SessionOpened`.

**Indexed tags:** `session_id`, `child_session_id`, `recursion_depth`

**Payload schema (actual):**

```json
{
  "session_id": "string (parent session_id)",
  "cycle_id": "string (parent cycle_id)",
  "trigger_predicate": "string (predicate name that fired)",
  "continuation_bundle_id": "string (bundle_XXXX... from continuation_bundles table)",
  "child_session_id": "string (newly spawned child session_id)",
  "recursion_depth": "int (CHILD's depth = parent_depth + 1)",
  "triggered_at": "int (Unix epoch milliseconds)"
}
```

**Convention note on `recursion_depth`:** This is the CHILD's depth, not the parent's.
A top-level parent (depth 0) spawning its first child emits `recursion_depth: 1`. A depth-1
child spawning a depth-2 grandchild emits `recursion_depth: 2`. This matches the
`SessionOpened.recursion_depth` convention for the child session. (DEV-040)

**Field semantics:**

| Field | Values / notes |
|---|---|
| `session_id` | Parent session that terminated and triggered re-injection |
| `cycle_id` | Cycle ID of the parent's most recent cycle |
| `trigger_predicate` | Name of the predicate that fired (`"max_steps_without_acceptance"` in v0.1) |
| `continuation_bundle_id` | ID of the `ContinuationBundle` written to the `continuation_bundles` table before spawning |
| `child_session_id` | `session_id` of the child session just created via `SessionManager.open_session` |
| `recursion_depth` | Child session's depth (parent depth + 1); NOT the parent's depth |
| `triggered_at` | Timestamp of emission in milliseconds |

---

## Chain context: parent vs child stream

`ReinjectionTriggered` appears on the PARENT stream. The child's stream is separate:

```
Parent stream (cerebra/agent-trace/<parent_session_id>):
  SessionOpened
  CycleStarted
  [step events...]
  CycleCompleted (outcome="cap_reached")
  SessionFlushed
  ReinjectionTriggered  ← parent stream ends here
  
Child stream (cerebra/agent-trace/<child_session_id>):
  SessionOpened  ← new stream begins; parent_session_id in payload
  CycleStarted
  [step events...]
  ...
```

The `continuation_bundle_id` in `ReinjectionTriggered` is the bridge: reading
`continuation_bundles WHERE bundle_id = <id>` gives `distilled_goal`,
`summarized_prior_prompt`, `next_focus`, and `recursion_depth` for visualization.

---

## No event on depth-limit block

Per S4-D7: when the trigger predicate WOULD fire but `recursion_depth >= max_recursion_depth`
blocks it, **no event is emitted**. The cycle's `SessionFlushed` is the last event on the parent
stream. `child_result` in `CycleResult` is `None`. Lattica can detect this case by reading
the parent session's `recursion_depth` from `runtime_sessions` and comparing to
`config.max_recursion_depth`.

---

## Requested action on fossic side

Update `docs/implement/AGENT_TRACE_VOCABULARY.md` §7 to add `ReinjectionTriggered` to the
Cerebra event vocabulary. Suggested location: §7.5.4 (after `CatalystArmSelected` §7.5.3).

Key points for the vocabulary entry:
- Emitted on PARENT stream, not child stream
- `recursion_depth` is child's depth (see convention note above)
- Only emitted when trigger fires; blocked case produces no event
- `continuation_bundle_id` links to the `continuation_bundles` DB table
- v0.1 only ships `max_steps_without_acceptance` as trigger predicate

No fossic core changes required. Standard fossic event API; payload is opaque to fossic.

---

## Cerebra-side references

- Phase 9 Step 4 (v0.3.6)
- `cerebra/cognition/cycle_runtime.py` — `_try_reinject()` method (emission site)
- `cerebra/cognition/reinjection.py` — `ReinjectionTriggerEvaluator`, predicates
- `cerebra/cognition/continuation_bundle.py` — `BundleDistiller`, `write_bundle`
- `cerebra/cognition/session.py` — `SessionManager.open_session(parent_session_id=...)`
