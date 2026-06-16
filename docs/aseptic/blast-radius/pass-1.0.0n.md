---
pass: v1.0.0n
version: v1.0.0n
sha: pending
date: 2026-06-15
summary: AGENT_TRACE_VOCABULARY.md — Cerebra daemon-v1 additions (PostureChanged, CheckpointSaved, cerebra/control stream)
---

# Blast Radius — Pass v1.0.0n

## Files

### Modified
- `docs/implement/AGENT_TRACE_VOCABULARY.md` — five targeted edits (see below); status bumped to v1.0.0n

### Created
- `docs/aseptic/blast-radius/pass-1.0.0n.md` — this file

---

## Changes

### §2 Consumer Extension Registry

Added `cerebra/control` to Cerebra's stream prefix list.

**Why:** Daemon-v1 introduces a new global stream outside the `agent-trace` tier. Registry must reflect it so consumers know where to look.

### §7 preamble — type count + stream note

`All 22 types` → `All 24 types`. Stream note updated: agent-trace types go to `cerebra/agent-trace/<session_id>`; two daemon-control types (`PostureChanged`, `CheckpointSaved`) use distinct patterns per §7.1.

### §7.1 Stream pattern lock — cerebra/control added

Restructured into two labeled blocks: **Per-session streams** (existing content) and **Global daemon stream** (new `cerebra/control` block). Forward-compat and lattice notes preserved.

**Key constraint documented:** `cerebra/control` is NOT covered by the `*/agent-trace/*` glob — consumers subscribing to daemon state must name it explicitly.

### §7.11 Daemon controls (new section)

New section with two event type entries:

**§7.11.1 `PostureChanged`**
- Stream: `cerebra/control` (global)
- Trigger: `POST /posture` on `cerebra serve`
- Fields: `posture: "auto"|"hold"`, `changed_at: int (ms epoch)`
- No `indexed_tags` (global, not session-scoped)
- Source: Cerebra daemon-v1 cross-pollination (`daemon-v1-fossic.md`, 2026-06-15)

**§7.11.2 `CheckpointSaved`**
- Stream: `cerebra/agent-trace/<session_id>`
- Trigger: `POST /checkpoint` on `cerebra serve`
- Fields: `session_id`, `bundle_id`, `wm_item_count`, `t1_count`, `t2_count`, `checkpointed_at`
- `indexed_tags={"session_id": session_id}` — consistent with existing agent-trace events
- Source: Cerebra daemon-v1 cross-pollination (`daemon-v1-fossic.md`, 2026-06-15)

---

## Public APIs

No changes. Docs-only pass.

## Schema changes

Vocabulary additions only — fossic payloads are opaque, no fossic-core change required.

## Adjacent project notifications

- **Cerebra:** vocabulary doc updated; `PostureChanged` and `CheckpointSaved` now canonical in fossic's spec. `cerebra/control` stream added to §2 registry.
- **Lattica:** `cerebra/control` stream documented — subscriber glob for HOLD pill state (`cerebra/control` explicit, not `*/agent-trace/*`) noted in §7.1.

## Living report updates

No new entries this pass.
