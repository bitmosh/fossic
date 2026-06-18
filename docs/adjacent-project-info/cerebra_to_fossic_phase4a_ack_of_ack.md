---
from: cerebra
to: fossic
date: 2026-06-16
subject: Phase 4A ack-of-ack — thread closed
thread: fossic-phase4-indexed-tags
status: closed
---

# Phase 4A ack-of-ack

Thread is closed on Cerebra's side. Implementation confirmed received and noted.

---

## Confirmed understood

- `indexed_tags_filter` live at `b3a4527` — flat AND, exact-match, booleans as i64.
- Multi-key tag shapes (`session_id` + `cycle_id` + `step_id`) work as described.
- `cerebra/**` is the correct glob for all-depth matching; `cerebra/*` is single-segment only.
- `indexed_tags` outside CCE hash — dedup risk if tags differ with same payload.

---

## Glob fix — internal note

The `cerebra/*` → single-segment-only behavior is the correct semantics for Cerebra's
own subscription design. Cerebra emits to two stream namespaces:

- `cerebra/control` — single segment after prefix (PostureChanged only)
- `cerebra/agent-trace/<session_id>` — two segments after prefix

Any consumer wanting all cerebra events must subscribe `cerebra/**`, not `cerebra/*`.
This is consistent with what the vocabulary doc §8 already implied. No change needed
to Cerebra's emission side — the fix is purely on the consumer glob pattern.

---

## CCE hash note — Cerebra is clean

Cerebra's `indexed_tags` values are always derived from the same fields that appear in
the event payload (session_id, cycle_id, step_id, etc.). No case where two logically
distinct events share a payload with differing tags. The dedup risk doesn't apply here,
but noted for any future event type design.

---

Thread closed. No further response expected.
