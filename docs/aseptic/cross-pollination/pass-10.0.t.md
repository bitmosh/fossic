---
pass: 10.0.t
version: v0.10.0t
date: 2026-06-12
summary: Cerebra's Phase 6/7/8 cycle runtime vocabulary contribution landed in AGENT_TRACE_VOCABULARY.md.
---

# Cross-Pollination — Pass 10.0.t (v0.10.0t)

> Cross-pollination records what adjacent projects need to know about changes made
> in this pass. Severity: **FYI** = awareness only, **NEEDS-AWARENESS** = read before
> implementing the named feature, **ACTION-REQUIRED** = breaking change; act before
> consuming the updated API.

---

## Cerebra

**Severity: NEEDS-AWARENESS**

Your Phase 6/7/8 cycle runtime vocabulary contribution is now canonical at `AGENT_TRACE_VOCABULARY.md §7`. The Consumer Extension Registry table includes Cerebra at §2. The OTel mapping table includes all 22 Cerebra event types at §8.2.

**What changed:** Your contribution (from `cerebra_phase6_event_vocabulary.md`) has been integrated into the fossic canonical vocabulary reference. Payload content was preserved exactly; formatting was adapted to match the doc's JSON code block style (from bullet lists). Stream pattern lock language was preserved verbatim per the "language is the contract" instruction.

**One discrepancy to resolve:** The pass prompt listed `StepExecutionFailed` as one of 23 events in the step execution group. Your contribution document defines 22 events — `StepExecutionFailed` has no schema there. It was not added (no fabricated schema). If this event type is planned for Phase 7/8, please provide its schema in a follow-up contribution.

**Suggested action:** When Phase 7/8 implementation references the canonical vocabulary, link to `AGENT_TRACE_VOCABULARY.md §7` rather than the contribution document. Your `cerebra_phase6_event_vocabulary.md` remains the implementation-side reference with full `indexed_tags` recommendations and required-vs-optional field rationale. For `StepExecutionFailed`, either add it to your contribution doc and file a follow-up integration pass, or confirm it was an editorial error in the pass prompt.

**Pre-drafted advocate-agent message:**
"Cerebra Phase 6/7/8 cycle runtime vocabulary is now canonical in fossic's AGENT_TRACE_VOCABULARY.md at §7. Consumer Extension Registry includes Cerebra at §2. OTel mapping includes all 22 defined events at §8.2. Your contribution landed without modification beyond editorial formatting to match the receiving doc's style. Phase 7/8 implementation can reference the canonical doc. One open item: `StepExecutionFailed` was listed as event 23 in the pass prompt but has no schema in your contribution — please confirm whether it's planned for Phase 7/8."

---

## LumaWeave

**Severity: NEEDS-AWARENESS**

`AGENT_TRACE_VOCABULARY.md` now documents Cerebra's 22 cycle runtime event types and the complete OTel GenAI span mapping for those events. When LumaWeave's time-travel viewer renders Cerebra cycle traces, use this as the canonical reference.

**What changed:** §7 documents all 22 Cerebra event types with full JSON payload schemas. §8.2 documents the OTel span structure: span hierarchy is session > cycle > step (INTERNAL spans throughout). Re-injection sessions are linked via OTel span links on `ReinjectionTriggered`.

**Suggested action:** Reference `AGENT_TRACE_VOCABULARY.md §7` and §8.2 when designing the time-travel viewer's cycle rendering. The span hierarchy (session > cycle > step) maps directly to LumaWeave's expected visualization shape. The `gen_ai.cerebra.*` namespace is the attribute reference for Cerebra-specific span attributes.

---

## Policy Scout / Bo / ai-stack

**Severity: FYI**

The Consumer Extension Registry pattern now exists at §2. If these projects develop their own agent-trace event vocabularies, §2 is where a new row gets added, and a new section (§10+) is where the events get documented.

No action required. Awareness only.

---

## Rhyzome / bons.ai

**Severity: FYI**

AGENT_TRACE_VOCABULARY.md section numbers shifted — rhyzome vocabulary is now §5 (was §4); bons.ai vocabulary is now §6 (was §5). Update any internal docs that reference specific section numbers.

No behavioral changes. Awareness only for cross-references.
