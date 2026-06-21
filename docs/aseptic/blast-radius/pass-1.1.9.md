---
pass: v1.1.9
version: v1.1.9
date: 2026-06-21
prior-commit: 8de0bb2
summary: Documentation pass — bounded reads, streaming iterators, observability, Phase roadmap across all four binding READMEs, root README, FOSSIC_V1_SPEC, and CHANGELOG
---

# Blast Radius — Pass v1.1.9

## Files

### Created
- `docs/aseptic/blast-radius/pass-1.1.9.md` — this file

### Modified
- `README.md` — added "Bounded reads and streaming iterators" section (ReadOutcome, TruncationCursor, SamplingMode, iter API, aggregate_bounded caveat) + "Observability" section (dispatch_channel_pressure / dispatch_channel_high_water_mark)
- `fossic-py/README.md` — added "Bounded reads and streaming iterators" section (Python syntax, CP-FOSSIC-3 note: default_max_results/bytes not yet exposed in Python OpenOptions)
- `fossic-node/README.md` — added "Bounded reads and streaming iterators" section (TypeScript syntax, defaultMaxResults/defaultMaxBytes ARE exposed in Node binding from v1.1.7)
- `crates/fossic-tauri/README.md` — updated IPC commands table with 7 new v1.1.8 commands; added "Bounded read commands" section (ReadOutcome JSON shape, cursor resumption, SamplingMode JSON, aggregate_bounded caveat, streaming limitation note)
- `docs/implement/FOSSIC_V1_SPEC.md` — §4.1: extended `impl Store` with bounded/iter/observability signatures, extended `OpenOptions` with budget defaults, added canonical type definitions (`ReadOutcome`, `TruncationCursor`, `TruncationReason`, `SamplingMode`, `ReadQuery`); §10.1–10.3: bounded variant notes; §18: Phase Roadmap (Phases 1–5)
- `CHANGELOG.md` — v1.6.0 Phase 1 close entry inserted before v1.5.0

---

## Commits (this pass)

| SHA | Message |
|---|---|
| `ea10086` | docs(v1.1.9): bounded reads + observability section in root README |
| `49fb18d` | docs(v1.1.9): bounded reads section in fossic-py README |
| `9766643` | docs(v1.1.9): bounded reads section in fossic-node README |
| `965d7aa` | docs(v1.1.9): bounded commands section in fossic-tauri README |
| `780e981` | docs(v1.1.9): bounded API types, streaming iterators, Phase roadmap in FOSSIC_V1_SPEC |
| `8de0bb2` | docs(v1.1.9): Phase 1 close entry v1.6.0 in CHANGELOG |

---

## Changes

### Root README

Two new sections added between "Project Registration" and "Threading model":

1. **Bounded reads and streaming iterators** — covers the full bounded API surface: ReadOutcome enum, TruncationCursor serialization, store-level defaults via OpenOptions, streaming iterators with pool-release invariant, SamplingMode table, aggregate_bounded cursor caveat. Rust syntax throughout.

2. **Observability** — `dispatch_channel_pressure()` and `dispatch_channel_high_water_mark()` with brief interpretation guidance. Calls out Phase 3 PressureMonitor as the future automated solution.

### fossic-py README

New "Bounded reads and streaming iterators" section after "Subscription delivery". Covers ReadOutcome properties (`.is_truncated`, `.complete`, `.reason`, `.next_cursor`), cursor serialization, SamplingMode constructors, streaming iterators, bounded method signatures, and the CP-FOSSIC-3 note (OpenOptions defaults not yet exposed in Python binding).

### fossic-node README

New "Bounded reads and streaming iterators" section after "Quick start". Covers ReadOutcome discriminated union (`.kind`), TruncationCursor `.toBytes()` / `.fromBytes()`, SamplingMode namespace, async iterables with `for await`, bounded + iter method signatures, and the OpenOptions `defaultMaxResults`/`defaultMaxBytes` exposure note (Node-only from v1.1.7).

### fossic-tauri README

IPC commands table updated: 7 new commands (`fossic_read_range_bounded`, `fossic_read_range_from_cursor`, `fossic_read_by_correlation_bounded`, `fossic_read_by_correlation_from_cursor`, `fossic_walk_causation_bounded`, `fossic_walk_causation_from_cursor`, `fossic_aggregate_bounded`) added with their parameter signatures and return types.

New "Bounded read commands" section: ReadOutcome JSON schema (complete vs truncated), cursor resumption example, SamplingMode JSON variants, aggregate_bounded caveat, streaming limitation note (Tauri IPC is request-response only; bounded pagination is the streaming substitute until v1.2.x).

### FOSSIC_V1_SPEC.md

- **§4.1 OpenOptions**: `default_max_results` and `default_max_bytes` fields added.
- **§4.1 impl Store**: bounded variants added to range/correlation/causation/aggregate signatures; streaming iterator constructors (`read_range_iter`, `read_by_correlation_iter`, `walk_causation_iter`) added; observability methods (`dispatch_channel_pressure`, `dispatch_channel_high_water_mark`) added.
- **Canonical type definitions**: `ReadOutcome<T>`, `TruncationReason`, `TruncationCursor`, `SamplingMode`, `ReadQuery` — added after the `impl Store` block.
- **§10.1–10.3**: One-line bounded-variant callout appended to each cross-stream query subsection.
- **§18 Phase Roadmap**: New section documenting Phase 1 (CLOSED, v1.1.0–v1.6.0 with per-version deliverables), Phase 2 (Hardware-Aware Defaults), Phase 3 (Pressure-Aware Substrate), Phase 4 (Adaptive Subscription Delivery), Phase 5 (Catalyst).

### CHANGELOG.md

v1.6.0 Phase 1 close entry inserted at top of file (before v1.5.0). Lists all v1.1.x deliverables, notes Track 2 parallel execution (v1.2.0–v1.5.0), and calls out Phase 2 and Phase 3 as next.

---

## No breaking changes

Docs-only pass. No code, no dependencies, no test changes.

---

## Adjacent project notifications

None required — documentation changes only.
