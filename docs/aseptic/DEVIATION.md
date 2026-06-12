---
title: Deviation — Living Report
last_reviewed: v0.10.x
---

# Deviation — Living Report

Where implementation diverged from spec or ADR. Information log, not failure log.
See `LIVING_REPORTS.md` for entry format and resolution conventions.

---

---
id: DV-001
type: deviation
status: resolved
pass_opened: v0.8.1
pass_resolved: v0.8.1
---

### ~~DV-001 — Symbol.asyncIterator via JS wrapper class, not native napi-rs~~

> **Resolved in v0.8.1** — Pass 8.5. Spec updated in the same pass to document the
> JS wrapper pattern as the standard mitigation. See blast-radius/pass-8.5.md.

<details>
<summary>Original entry (preserved for history)</summary>

**Spec said:** FOSSIC_V1_SPEC.md §4.3 — "subscriptions surface as AsyncIterable" with
implicit assumption that napi-rs would expose `[Symbol.asyncIterator]` natively.

**Implementation did:** `fossic-node/index.js` wraps the napi-rs subscription handle in
a JS class that manually exposes `[Symbol.asyncIterator]()` and `[Symbol.asyncDispose]()`.

**Why:** napi-rs 2.x treats `js_name` as a literal string. Well-known symbols
(`Symbol.asyncIterator`, `Symbol.asyncDispose`) cannot be expressed via the `#[napi(js_name)]`
attribute — using `js_name = "Symbol.asyncIterator"` would create a property literally named
`"Symbol.asyncIterator"`, not the well-known symbol. The JS wrapper class is the standard
pattern for exposing well-known symbols from napi-rs.

**Status:** RESOLVED — spec updated in Pass 8.5 to document the JS wrapper pattern.
The pattern is correct and expected; napi-rs consumers universally use this approach.

**Adjacent impact:** None — internal implementation detail of the Node binding.

</details>

---

---
id: DV-002
type: deviation
status: open
pass_opened: v0.8.6
severity: LOW
---

### DV-002 — `purge_event` removes events from read path entirely (RB-2)

**Spec said:** `FOSSIC_V1_SPEC.md` §9.1 (Deletion modes) and §9.3 use "tombstone"
language: "The act of purging is itself permanently recorded" (spec invariant §16.8).
Spec §16.8 says "Purge events always precede the row deletion" — implying the row
is deleted but the purge event (tombstone) is the permanent record.

**Implementation did:** After `purge_event`, `read_one(event_id)` returns `None` and
`read_range` does not include the event. The event is removed from the read path
entirely. The purge is recorded in the `_fossic/system` stream as a `fossic.Purged`
(or `Purged`) audit event, but the original event is inaccessible through normal read
APIs.

**Why:** The "removes from read path" model is the more honest deletion semantics —
there is no ambiguity about whether "tombstoned" means "still readable but marked"
or "not readable." Choosing the fully-removed model makes the consumer's guarantee
clear: after purge, the payload is gone from the read path. The audit trail lives in
the system stream.

**Status:** OPEN — spec should be updated. Recommended change: replace "tombstone"
language in §9.1 and §9.3 with "removes from read path" language. Add a note that
the purge is recorded in `_fossic/system` for audit purposes but that the event
payload is not accessible via normal `read_one`/`read_range` after purge.

**Adjacent impact:** Any consumer that assumed `read_one` would return a
"redacted-payload" event after purge (rather than None) would be silently broken.
Current adjacent consumers (cerebra, policy-scout) have not implemented purge
workflows yet — low urgency but should be corrected before they do.

---

---
id: DV-003
type: deviation
status: resolved
pass_opened: v0.9.0
pass_resolved: v0.11.0
---

### ~~DV-003 — Spec §14 described Tokio threading model that was never implemented~~

> **Resolved in v0.11.0** — Pass 11. Spec §14 rewritten to describe the actual
> threading model (std::thread + crossbeam-channel). `OpenOptions::tokio_handle`
> removed from spec §4.1. §17 reference row removed. See blast-radius/pass-11.md.

<details>
<summary>Original entry (preserved for history)</summary>

**Spec said:** `FOSSIC_V1_SPEC.md` §14 described fossic as using Tokio internally for
the subscription dispatcher, file-watcher, and OTel exporter. §4.1 included
`OpenOptions::tokio_handle: Option<Handle>` for Tauri consumers.

**Implementation did:** fossic core uses `std::thread::spawn` and `crossbeam-channel`
throughout. There is no Tokio dependency in `fossic/Cargo.toml`. The napi-rs binding
(`fossic-node`) has its own Tokio runtime via napi-rs, but this is not coordinated
with any host handle.

**Why:** The spec was written aspirationally before the implementation was complete.
The subscription dispatcher was implemented using `crossbeam-channel` because it was
simpler and matched the "local-first, no async runtime" design goal. The Tokio section
of the spec was never updated to reflect this.

**Status:** RESOLVED in Pass 11 — spec §14 rewritten; tokio_handle removed from §4.1.

**Adjacent impact:** HIGH before resolution — any agent implementing a Tauri consumer by
following the spec would write code that fails to compile (`OpenOptions::tokio_handle`
does not exist). TIDYUP survey Issue 2. No known affected consumers at time of resolution.

</details>
