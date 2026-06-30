---
pass: 10.1
version: v0.10.1
date: 2026-06-12
summary: Add event_type_filter to ReadQuery across all four layers
---

# Cross-Pollination — Pass 10.1 (v0.10.1)

> Cross-pollination records what adjacent projects need to know about changes made
> in this pass. Severity: **FYI** = awareness only, **NEEDS-AWARENESS** = read before
> implementing the named feature, **ACTION-REQUIRED** = breaking change; act before
> consuming the updated API.

---

## Cerebra

**Severity: FYI**

- `ReadQuery` now accepts `event_type_filter`. Cerebra's `LatticeNodeReducer` replays
  all events on a stream (no filter needed) — no action required. If Cerebra later
  needs to read only specific event types from a mixed stream, `event_type_filter` is
  now available.

---

## Policy Scout

**Severity: NEEDS-AWARENESS**

- The primary motivation for this pass was Policy Scout's use case: reading only
  `PolicyViolation` events from a mixed audit stream without client-side filtering.
  When Policy Scout implements its audit stream reader, use:
  ```python
  store.read_range(ReadQuery(stream_id="policy-scout/audit", event_type_filter="PolicyViolation"))
  ```
  Server-side filtering; no bandwidth waste on non-violation events.

---

## LumaWeave (src/)

**Severity: FYI**

- `fossic_read_range` Tauri IPC command now accepts `event_type_filter` as an optional
  parameter. Existing calls without the parameter continue to work (Tauri deserializes
  absent JSON keys as `None`). If LumaWeave uses `read_range` IPC calls and wants to
  filter by event type, it can now pass `eventTypeFilter` in the IPC payload.

- `fossic-node` `index.d.ts` was empty before this pass — now fully populated. If
  LumaWeave imports TypeScript types from `fossic`, it can now get proper type checking.

---

## Bo / discord-bot / ai-stack

**Severity: FYI**

- No impact. Bo reads events but does not use `ReadQuery` with type filtering yet.

---

## Rhyzome

**Severity: FYI**

- Rhyzome uses `read_range` for stream introspection. `event_type_filter` is available
  if Rhyzome needs to read only repair-related event types from a mixed stream. No
  action required now.

---

## bons.ai

**Severity: FYI**

- The `event_type_filter` field makes ReadQuery parity with AggregateQuery. When bons.ai
  reads from idea or session streams that carry mixed event types (e.g. `IdeaCreated`,
  `IdeaScored`, `IdeaDiscarded`), filtering can now be done server-side.
