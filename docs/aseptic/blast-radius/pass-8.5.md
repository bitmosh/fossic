---
pass: 8.5
version: v0.8.1
date: "(retroactive estimate, not verified)"
summary: Node subscriptions — Symbol.asyncIterator via JS wrapper class; AsyncDispose support
---

# Blast Radius — Pass 8.5 (v0.8.1)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic-node/index.js` — JS wrapper class exposing `[Symbol.asyncIterator]()` and
  `[Symbol.asyncDispose]()` on subscription handles

### Modified
- `fossic-node/src/store.rs` — subscribe() returns raw handle wrapped by the JS layer
- `docs/implement/FOSSIC_V1_SPEC.md` — §4.3 updated to document JS wrapper pattern
  (retroactive estimate — may have been a separate doc pass)

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
