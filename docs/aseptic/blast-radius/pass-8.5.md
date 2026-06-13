---
pass: 8.5
version: v0.8.1
sha: 78760c8
date: 2026-06-12
summary: Node subscriptions — Symbol.asyncIterator via JS wrapper class; AsyncDispose support
---

# Blast Radius — Pass 8.5 (v0.8.1)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Header and Files section updated; content otherwise correct.

## Files

### Created (actual commit 78760c8)
- `docs/aseptic/blast-radius/pass-8.5.md` — this file (retroactive blast-radius artifact)
- `docs/aseptic/cross-pollination/pass-8.5.md` — LumaWeave notification for Symbol.asyncIterator fix

> **Note (retroactive commit walkthrough):** Source code changes for this pass
> (`fossic-node/index.js` JS wrapper, `fossic-node/src/store.rs` subscribe() update,
> `docs/implement/FOSSIC_V1_SPEC.md` §4.3 correction) are not represented in the git
> history. The retroactive commit walkthrough committed all source files at their
> post-v0.10.x final state in the v0.8.0 commit (26001a1). The behavioral and API
> changes described below are accurate to what this pass delivered.

---

## Public APIs

### Modified (non-breaking)
- Node binding subscription surface: subscriptions now implement the full AsyncIterator
  and AsyncDispose protocols via a JS wrapper class
- The raw napi-rs handle is not exposed directly; consumers use the wrapper

---

## Schema changes

None.

---

## Configuration changes

None.

---

## Dependency changes

None (JS wrapper is vanilla JS, no additional npm packages).

---

## Behavior changes

- `for await (const event of store.subscribe(streamId))` now works correctly in
  TypeScript/JavaScript consumers. Prior to this pass, subscriptions were accessible but
  did not implement the well-known Symbol protocols.

---

## Living report updates

DV-001 opened and resolved in this same pass:
- DEVIATION: ~~DV-001~~ — Symbol.asyncIterator via JS wrapper class (resolved — spec updated)
