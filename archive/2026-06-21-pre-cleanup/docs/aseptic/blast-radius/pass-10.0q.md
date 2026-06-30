---
pass: 10.0q
version: v0.10.0q
date: 2026-06-13
summary: Post-rc.1 corrections to §8.2 OTel mapping table — span shape, attribute separators, StepExecutionFailed, signal.basis.
---

# Blast Radius — Pass 10.0q (v0.10.0q)

## Files

### Modified
- `docs/implement/AGENT_TRACE_VOCABULARY.md` — §8.2 Cerebra OTel mapping corrected (see below)
- `docs/aseptic/TECH_DEBT.md` — `last_reviewed` → v0.10.0q
- `docs/aseptic/POLISH_DEBT.md` — `last_reviewed` → v0.10.0q
- `docs/aseptic/DEVIATION.md` — `last_reviewed` → v0.10.0q
- `docs/aseptic/README.md` — version → v0.10.0q

### Created
- `docs/aseptic/blast-radius/pass-10.0q.md` — this file

### Deleted
- (none)

---

## Public APIs

None. Docs-only pass.

---

## Correction summary

### Correction 1 — Attribute separator: dots not underscores

`SignalEvaluated` row updated. Old attributes used underscores inside a hierarchical
sub-namespace (`signal_name`, `signal_score`, `low_confidence`). Corrected to use OTel
dot hierarchy convention:

- `gen_ai.cerebra.signal_name` → `gen_ai.cerebra.signal.name`
- `gen_ai.cerebra.signal_score` → `gen_ai.cerebra.signal.score`
- `gen_ai.cerebra.low_confidence` → `gen_ai.cerebra.signal.low_confidence?`

No other §8.2 attributes required separator correction — remaining underscores are within
single compound concepts (e.g., `cycle_config`, `recursion_depth`), not sub-namespace
hierarchies.

### Correction 2 — Add `gen_ai.cerebra.signal.basis`

Added to `SignalEvaluated` row: `gen_ai.cerebra.signal.basis` (from `evaluator_prompt_version`).
Also added `gen_ai.cerebra.signal.strength?` and `gen_ai.cerebra.event_id` per the locked
coordination attributes.

Added attribute mapping note below the table explaining that `evaluator_prompt_version`
in the fossic payload maps to `gen_ai.cerebra.signal.basis` at OTel emit time.

### Correction 3 — Span shape: Option A (events on step span)

Updated opening paragraph of §8.2 to describe Option A explicitly: sub-events within a
step are span events on the step span, not separate child spans.

These rows changed from `INTERNAL (single span)` to `(span event on step span)`:
- `ContextPacketBuilt` — was `INTERNAL (sub-span of step)`
- `PredictionMade`
- `SignalEvaluated` — annotated `6 per step`
- `EvaluationComposed`
- `OutcomeRecorded`
- `PredictionSevereMiss` — annotated `sets step span status to warn`
- `ClutchDecisionMade`
- `LeewayGrantApplied`
- `ContinuationBundleCreated` — was `INTERNAL (sub-span)`
- `MemoryWriteFromCycle`

These rows remain as their own spans (correct, unchanged):
- `SessionOpened`/`SessionFlushed` — session span begin/end
- `CycleStarted`/`CycleCompleted` — cycle span begin/end
- `StepStarted`/`StepExecuted` — step span begin/end
- `CatalystInvoked`/`CatalystArmSelected` — sub-span begin/end (children of step span)
- `ReinjectionTriggered` — own span with link to child session span
- `ConsolidationStarted`/`ConsolidationCompleted` — consolidation span begin/end
- `GraphExported` — single INTERNAL span (CLI invocation)

### Correction 4 — StepExecutionFailed added to §8.2

Row added after `StepExecuted`:

| `StepExecutionFailed` | (span event on step span, sets step span status to error) | `gen_ai.cerebra.error_type`, `gen_ai.cerebra.retry_count`, `gen_ai.cerebra.error_message?` |

This event type is documented in §7 of the Cerebra contribution but was absent from the
v0.10.0t integration (it had no schema at contribution time — see cross-pollination
pass-10.0.t.md). The current pass adds it to the OTel mapping table based on the locked
Cerebra coordination specification. The §7 entry remains absent pending Cerebra providing
the full payload schema.

### Correction 5 — §2 cross-reference verified

The Consumer Extension Registry in §2 references "§7.3.2 note" for `ContextPacketBuilt`.
Confirmed: §7.3.2 is indeed `ContextPacketBuilt` and it has the overlap note. Cross-reference
is correct. No change made.

---

## Schema changes

None.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

None. Docs-only. The OTel exporter does not yet exist; these corrections align the spec
to the locked Cerebra coordination decisions before implementation begins.

---

## Living report updates

`last_reviewed` bumped to v0.10.0q on TECH_DEBT, POLISH_DEBT, and DEVIATION.
No new entries filed or resolved this pass.

---

## Adjacent project impact

**Cerebra** — §8.2 is now aligned to the locked coordination decisions. When building
the OTel exporter, use the updated §8.2 as the attribute reference. The `StepExecutionFailed`
§8.2 row is based on the locked coordination specification; the §7 payload schema is still
pending from Cerebra.

**LumaWeave** — The time-travel viewer reference in cross-pollination pass-10.0.t.md
described span shape as "INTERNAL spans throughout." This was the pre-correction reading.
The corrected shape is: step sub-events are span events on the step span, not separate
child spans. Update any time-travel viewer design that assumed per-sub-event INTERNAL spans.
