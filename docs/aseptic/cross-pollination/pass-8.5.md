---
pass: 8.5
version: v0.8.1
date: "(retroactive estimate, not verified)"
impacts: [lumaweave]
---

# Cross-Pollination — Pass 8.5 (v0.8.1)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## lumaweave

**Severity:** NEEDS-AWARENESS

**What changed:** The Node binding subscription surface now correctly implements
`[Symbol.asyncIterator]` and `[Symbol.asyncDispose]` via a JS wrapper class in
`fossic-node/index.js`. Prior to v0.8.1, subscriptions were accessible but could
not be used with `for await...of` syntax or the `await using` disposer pattern.

**Action required:** Verify. If LumaWeave's planned Graph B (live state) integration
with fossic subscriptions uses `for await...of`, it can now proceed. No breaking
changes — this is additive.

**Advocate-agent message:**
> fossic v0.8.1 shipped. Node binding subscriptions now implement Symbol.asyncIterator
> and Symbol.asyncDispose correctly via a JS wrapper class.
>
> Impact for lumaweave: Graph B's live-state subscription pattern (`for await (const event of store.subscribe(streamId))`) is now available in the Node binding. No changes needed if you haven't integrated yet; this unblocks the Graph B subscription integration when you're ready.
>
> Severity: NEEDS-AWARENESS. No action needed now.

---

## cerebra, policy-scout, bo, ai-stack, rhyzome, bons.ai

No impact. Python binding unchanged. This pass touched only `fossic-node/index.js`.
