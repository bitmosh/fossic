# Pass Report — v1.2.1

**Date:** 2026-06-21
**Version:** v1.2.1
**Type:** feat — Phase 6 (ReducerStateLarge emission + StateAdaptive policy enablement)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| `OpenOptions::reducer_state_large_threshold_bytes` (default 1 MiB) | DONE | `src/types.rs` |
| `StateMonitor` struct (rolling 32-entry state-size + apply-cost buffers) | DONE | `src/store.rs` |
| `StoreInner::reducer_system_writer` (lazy Mutex, separate connection) | DONE | `src/store.rs` |
| `StoreInner::state_monitors` (per-stream/branch rolling monitor map) | DONE | `src/store.rs` |
| `update_state_monitor` — per-event call inside apply loop | DONE | `src/store.rs` |
| `maybe_emit_state_large` — threshold + 60-second throttle + lazy writer | DONE | `src/store.rs` |
| `ReducerStateLarge` emitted to `_fossic/system` when mean > threshold | DONE | `src/store.rs` |
| `StateAdaptive` enabled in `validate_snapshot_policy` | DONE | `src/reducers.rs` |
| `StateAdaptive` branch in `maybe_auto_snapshot` | DONE | `src/store.rs` |
| `fossic-py` drift fix — `reducer_state_large_threshold_bytes` field | DONE | `fossic-py/src/types.rs` |
| `tests/snapshot_policy.rs` — 5 new tests (11 total) | DONE | all pass |

---

## 2. Test results

283 tests, 0 failures. 4 net new snapshot_policy tests (5 added, 1 renamed/inverted).

---

## 3. Files touched

Reference: `docs/aseptic/blast-radius/pass-1.2.1.md`

- Modified in e765866: `src/store.rs`, `src/types.rs`, `src/reducers.rs`,
  `fossic-py/src/types.rs`, `tests/snapshot_policy.rs`, `CHANGELOG.md`
- This commit: `Cargo.toml` (version bump), blast-radius, pass-complete

---

## 4. API changes

**New `OpenOptions` field:**
- `reducer_state_large_threshold_bytes: usize` — default `1_048_576` (1 MiB); `usize::MAX` disables

**Behaviour change — `SnapshotPolicy::StateAdaptive`:**
- Previously returned `Error::NotImplemented` at registration; now accepted and operational.
  No existing call sites could have depended on the error (they were rejected before the store
  opened).

**New system event:**
- `ReducerStateLarge` in `_fossic/system` stream — payload: `stream_id`, `branch`,
  `mean_state_bytes`, `threshold_bytes`. Throttled to once per 60 s per `(stream_id, branch)`.

**Breaking changes:** None. `OpenOptions::Default` provides the new field; callers using
`..Default::default()` are unaffected. fossic-py manual literal updated in this pass.

---

## 5. Architecture note

Two `SystemStreamWriter` connections in steady state — dispatcher's (dispatch thread) and
reducer's (lazy Mutex on StoreInner). The separation is load-bearing: reducer apply runs on
arbitrary threads (write thread or read pool); routing through the dispatcher's writer would
require cross-thread synchronisation and would couple the read path to the dispatch loop.

---

## 6. CCE deduplication discovery

Test reducers with `Null` payloads produce identical CCE hashes across appends → only one
event stored (`INSERT OR IGNORE`). Discovered while writing `state_large` tests; fixed by
using `{"seq": N}` payloads. Documented in blast-radius for future test authors.

---

## 7. Tech debt / polish debt

No new entries. Dead-code warnings on `CursorInner` / `TruncationCursor` carry over.

---

## 8. PASS COMPLETE message

```
── PASS COMPLETE · v1.2.1 · 2026-06-21 ──────────────────────

Title: ReducerStateLarge Emission + StateAdaptive Policy
Summary: ReducerStateLarge events are now emitted to _fossic/system when the rolling-mean state size exceeds OpenOptions::reducer_state_large_threshold_bytes; StateAdaptive snapshot policy is live, firing when estimated replay cost exceeds the target threshold.
Project: fossic

Highlights:
· reducer_system_writer: lazy Mutex<Option<SystemStreamWriter>> on StoreInner — separate SQLite connection from the dispatcher's writer; WAL handles concurrent writes without contention
· read_state and read_state_bytes apply loops now time each apply_bytes call (now_us delta) and feed size + cost into rolling StateMonitor buffers per (stream_id, branch)
· maybe_emit_state_large throttles to once per 60 s; lazy-inits writer on first emission; fires ReducerStateLarge to _fossic/system with mean_state_bytes and threshold_bytes in payload
· StateAdaptive: replay_cost_estimate = accumulated_events × avg_apply_cost_us; fires when estimate > target_replay_cost_us AND accumulated >= min_events_between; counter resets same as EveryNEvents

Learnings:
· CCE deduplication bites test reducers with identical payloads — INSERT OR IGNORE silently drops all but the first event; use {"seq": N} payloads in test helpers to ensure distinct CCE ids

Commit: [PENDING_SHA]
Tests: 283 passed · 0 failed · 0 skipped
Branch: clean
```
