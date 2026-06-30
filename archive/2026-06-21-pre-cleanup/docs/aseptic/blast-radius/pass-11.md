---
pass: 11
version: v0.11.0
sha: 7323f09
date: 2026-06-12
summary: Threading model spec correction — §14 rewritten; tokio_handle removed; Tokio myth dispelled
---

> **Note:** No standalone v0.11.0 git tag exists. Pass 11 was incorporated into the v0.10.x
> retroactive commit (7323f09) alongside the Aseptic bootstrap. SHA and date reflect
> that commit. (retroactive — Aseptic not yet active)

# Blast Radius — Pass 11 (v0.11.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Modified
- `docs/implement/FOSSIC_V1_SPEC.md` — §14 Threading Model completely rewritten:
  removed Tokio references, documented actual model (std::thread + crossbeam-channel
  for subscription dispatcher, notify crate for WAL watcher). `OpenOptions::tokio_handle`
  removed from §4.1 OpenOptions struct definition. §17 reference row for
  `tokio::runtime::Handle option | LumaWeave` removed.
- `fossic/src/types.rs` — `OpenOptions::tokio_handle` field removed if it existed
  (retroactive estimate — may have only been in the spec, not implemented)

---

## Public APIs

### Removed
- `OpenOptions::tokio_handle` — removed from spec and, if present, from code.
  No consumers used this field (it was aspirational spec content).

---

## Schema changes

None.

---

## Configuration changes

- `OpenOptions::tokio_handle` removed — no consumer action needed (field never functioned).

---

## Dependency changes

None.

---

## Behavior changes

No behavioral changes. The threading model was always std::thread + crossbeam-channel;
this pass corrected the spec to match implementation, not the other way around.

---

## Living report updates

Entries resolved:
- DEVIATION: ~~DV-003~~ — Tokio threading model spec vs implementation — RESOLVED.
  Spec §14 now correctly describes std::thread + crossbeam-channel.

No new entries.
