# Phase 4 Verification Report — fossic v1.8.1

**Commit verified:** `36b9203`  
**Date:** 2026-06-29

---

## Summary

| Check | Result | Notes |
|---|---|---|
| 1. Link integrity | PARTIAL | 10 broken links in ADR files — all pre-existing, cross-repo references |
| 2. Code grounding | PASS | All 24 modules, all types, all methods, all just commands verified |
| 3. Source coverage | PASS | All HISTORICAL_NARRATIVE files accounted for |
| 4. ADR integrity | PASS | ADRs have single commit; not modified during cleanup |
| 5. Orphaned markers | PASS | No `[unverified]` or `[needs operator input]` found |
| 6. Coverage | PASS | All 10 spot-checked files at correct final location |

**Overall: PARTIAL — one qualified failure (Check 1). Failures are pre-existing cross-repo link rot in ADR files, not introduced by this cleanup pass. No failures in new docs.**

---

## Check 1 — Link integrity

**Result: PARTIAL**

All links in the four new docs (README.md, docs/architecture.md, docs/operating.md, docs/history.md, docs/gotchas.md) and in docs/deep-dives/*.md resolve correctly.

**Broken links — all in docs/adr/; all pre-existing (ADRs have one commit: `a10f7b3`):**

The broken links are cross-ADR "Related ADRs" references pointing to slugs that never existed in this repo. These ADRs appear to have been authored against a different ADR registry (possibly the LumaWeave or Lattica monorepo where different ADRs existed). This is pre-existing link rot, not introduced by the cleanup pass.

| File | Broken link target | Likely intended |
|---|---|---|
| `docs/adr/ADR-004-policy-scout-governance-scope.md` | `ADR-001-lattica-is-lumaweave-extended.md` | Slug mismatch; actual is `ADR-001-lattica-extends-lumaweave.md` |
| `docs/adr/ADR-004-policy-scout-governance-scope.md` | `ADR-002-event-sourcing-toolkit.md` | Slug mismatch; actual is `ADR-002-es-toolkit-over-nats.md` |
| `docs/adr/ADR-004-policy-scout-governance-scope.md` | `ADR-003-eval-core-package.md` | No `ADR-003-eval-core-package.md` exists |
| `docs/adr/ADR-005-cerebra-api-surface.md` | `ADR-001-lattica-is-lumaweave.md` | Slug mismatch |
| `docs/adr/ADR-005-cerebra-api-surface.md` | `ADR-006-monorepo-layout.md` | Slug mismatch; actual is `ADR-006-monorepo-structure.md` |
| `docs/adr/ADR-005-cerebra-api-surface.md` | `ADR-003-eval-core-standalone.md` | No match |
| `docs/adr/ADR-006-monorepo-structure.md` | `ADR-001-mcp-transport-and-trust-model.md` | No `ADR-001` with that slug exists |
| `docs/adr/ADR-006-monorepo-structure.md` | `ADR-003-graph-export-contract.md` | No match |
| `docs/adr/ADR-006-monorepo-structure.md` | `ADR-005-registry-coverage-expansion.md` | No match |
| `docs/adr/ADR-008-phase-12-research-exploration.md` | `ADR-005-cerebra-api-shell-out-vs-daemon.md` | No match |
| `docs/adr/ADR-008-phase-12-research-exploration.md` | `ADR-003-eval-core-standalone-package.md` | No match |

**Disposition:** These are verbatim ADRs (per operating principle 3 — never paraphrase ADRs). The link rot is pre-existing. Operator decision needed: either (a) update the "Related ADRs" slugs in each affected ADR to point to correct filenames, or (b) accept broken cross-references as inherent to verbatim preservation. Not fixed in this pass.

---

## Check 2 — Code grounding

**Result: PASS**

**Module map (architecture.md vs src/):** Exact match. All 24 modules listed in docs/architecture.md exist as files in `src/`. No file in `src/` is missing from the map.

**Struct and trait names:** All verified present in source.

| Name | Location |
|---|---|
| `Store` | `src/store.rs` |
| `StoreInner` | `src/store.rs:125` |
| `Append` | `src/types.rs:102` |
| `StoredEvent` | `src/types.rs:139` |
| `ReadQuery` | `src/types.rs:168` |
| `OpenOptions` | `src/types.rs:192` |
| `ReducerRegistry` | `src/reducers.rs:116` |
| `SubscriptionRegistry` | `src/subscriptions.rs:107` |
| `BackgroundExecutor` | `src/executor.rs:146` |
| `QuiescenceMonitor` | `src/executor.rs:107` |

**Method names (store.rs public API):** All 20 methods referenced in docs verified present in `src/store.rs`:
`append`, `read_range`, `declare_stream`, `subscribe`, `register_reducer`, `read_state`, `create_branch`, `set_cursor`, `get_cursor`, `register_upcaster`, `purge_event`, `shred_stream`, `dispatch_channel_pressure`, `dispatch_channel_high_water_mark`, `schedule_task`, `append_batch`, `append_if`, `read_one`, `read_by_external_id`, `read_batch`.

Additional methods referenced in operating.md also verified: `register_reducer_with_policy`, `read_state_at_version`, `take_snapshot`, `gc_orphaned_snapshots`, `stream_exists`, `streams`, `read_range_bounded`, `read_range_iter`, `walk_causation`, `walk_causation_bounded`, `read_by_correlation`, `aggregate`, `promote_branch`, `mark_branch_dead_end`.

**OpenOptions fields (operating.md reference table):** All 11 fields verified against `src/types.rs`:
`encryption`, `checkpoint_mode`, `on_first_open`, `read_pool_size`, `read_pool_timeout_ms`, `default_max_results`, `default_max_bytes`, `reducer_state_large_threshold_bytes`, `auto_gc_orphans`, `background_executor_grace_timeout_ms`, `executor_quiescence_window_ms`.

**Just commands:** `test`, `test-rust`, `test-py`, `test-node` all present in `Justfile`.

---

## Check 3 — Source coverage

**Result: PASS**

All 75 HISTORICAL_NARRATIVE files from the Phase 1 inventory are accounted for. Files cited directly in docs/history.md:

- `docs/aseptic/blast-radius/pass-01.md` through `pass-09.md` — cited as group
- `pass-10.md`, `pass-11.md`
- `pass-1.0.0n.md`, `pass-1.0.0w.md`, `pass-1.0.0z.md`
- `pass-1.1.0.md`, `pass-1.1.9.md`
- `pass-1.2.0.md`, `pass-1.3.1.md`
- `pass-1.7.0.md`, `pass-1.7.4.md`
- `docs/fossic-recon-and-arch-opinion-2026-06-20.md`
- `docs/FOSSIC_TIDYUP_SURVEY.md`

**Archived without derivation (acceptable — not failures):**

| File(s) | Reason not cited |
|---|---|
| `benchmarks/results/aggregate_volume_sweep.md`, `sqlite_wal_payload_sweep.md` | Benchmark data; no narrative content |
| `docs/adjacent-project-info/fossic-interview.md` | Cross-project relay; Lattica-scoped, not fossic history |
| `docs/aseptic/blast-radius/pass-02.md` through `pass-08.md` (7 files) | Covered by group citation in §2 of history.md |
| `docs/aseptic/blast-radius/pass-1.0.0o.md`, `pass-1.0.0p.md`, `pass-1.0.0y.md` | Sub-passes within cited clusters |
| `docs/aseptic/blast-radius/pass-10.0q.md` through `pass-10.x.md` (6 files) | v0.10.x sub-passes; covered by pass-10/pass-11 narrative |
| `docs/aseptic/blast-radius/pass-10.1.md`, `pass-10.v.md`, `pass-10.w.md` | Same cluster |
| `docs/aseptic/blast-radius/pass-1.1.1.md` through `pass-1.1.8.md` (8 files) | v1.1.x sub-passes covered by range citation pass-1.1.0..pass-1.1.9 |
| `docs/aseptic/blast-radius/pass-1.2.1.md`, `pass-1.2.2.md`, `pass-1.3.0.md` | v1.2/1.3 sub-passes; covered by cited endpoints |
| `docs/aseptic/blast-radius/pass-1.4.0.md`, `pass-1.4.1.md`, `pass-1.5.0.md` | v1.4/1.5 passes; not covered in history.md narrative |
| `docs/aseptic/blast-radius/pass-1.7.1.md` through `pass-1.7.3.md` | v1.7 sub-passes; covered by range citation 1.7.0..1.7.4 |
| `docs/aseptic/blast-radius/pass-8.5.md`, `pass-8.6.md` | Pre-sprint passes; covered by day-one narrative |
| `docs/aseptic/cross-pollination/` (9 files) | Inter-module relay docs; no fossic-specific history |
| `docs/aseptic/pass-complete/` (13 files) | Pass completion reports; not source material for narrative |
| `docs/aseptic/POLISH_DEBT.md`, `TECH_DEBT.md` | Point-in-time debt registers; no narrative content |

**Note:** v1.4.x and v1.5.0 blast-radius artifacts are archived without any derivation or citation. The history.md narrative jumps from v1.3.x to v1.7.x with no coverage of v1.4.x–v1.6.x. This is consistent with the "Open Gaps" section of history.md and the cleanup master principle of naming gaps rather than bridging them with invention.

---

## Check 4 — ADR integrity

**Result: PASS**

All ADRs in `docs/adr/` (8 files: ADR-001 through ADR-008) have a single commit in git history:

```
a10f7b3 feat: glob subscriptions, tilde path expansion, cursor alignment (v0.9.0)
```

No ADR file was modified during the Phase 3 cleanup pass (commit `36b9203`). ADRs were not touched — only SR-* files in `docs/state-reports/` were moved.

---

## Check 5 — Orphaned markers

**Result: PASS**

Searched README.md, docs/*.md, and docs/deep-dives/*.md for `[unverified]` and `[needs operator input]`.

No occurrences found in any file.

---

## Check 6 — Coverage

**Result: PASS**

Spot-checked 10 files from the Phase 1 inventory:

| Original path | Expected final location | Found |
|---|---|---|
| `docs/state-reports/SR-01-identity-and-cce.md` | `docs/deep-dives/identity-and-cce.md` | ✓ |
| `docs/state-reports/SR-05-branches.md` | `docs/deep-dives/branches.md` | ✓ |
| `docs/DESIGN.md` | `archive/2026-06-21-pre-cleanup/docs/DESIGN.md` | ✓ |
| `docs/FOSSIC_TIDYUP_SURVEY.md` | `archive/…/docs/FOSSIC_TIDYUP_SURVEY.md` | ✓ |
| `docs/aseptic/blast-radius/pass-01.md` | `archive/…/docs/aseptic/blast-radius/pass-01.md` | ✓ |
| `docs/aseptic/cross-pollination/pass-09.md` | `archive/…/docs/aseptic/cross-pollination/pass-09.md` | ✓ |
| `docs/aseptic/pass-complete/pass-1.2.0.md` | `archive/…/docs/aseptic/pass-complete/pass-1.2.0.md` | ✓ |
| `docs/adjacent-project-info/fossic-interview.md` | `archive/…/docs/adjacent-project-info/fossic-interview.md` | ✓ |
| `benchmarks/results/aggregate_volume_sweep.md` | `archive/…/benchmarks/results/aggregate_volume_sweep.md` | ✓ |
| `docs/SUBSTRATE_GOTCHAS.md` | `docs/gotchas.md` | ✓ |

In all 10 cases the original path no longer exists and the file is at the expected destination.

Additional coverage observations:
- `docs/state-reports/` directory is empty (all SR-* files promoted to deep-dives)
- `docs/adjacent-project-info/` directory has been removed
- `docs/aseptic/` retains 11 CURRENT methodology spec files (intentionally kept per Phase 1 inventory)

---

## Action item for operator

**Check 1 broken ADR links** require a decision:

> Option A — Fix slugs: Update the "Related ADRs" lines in ADR-004, ADR-005, ADR-006, ADR-008 to use correct filenames. This is a mechanical rename of cross-reference slugs, not a change to ADR content. Technically violates "never modify ADRs" principle but corrects factual errors.
>
> Option B — Accept: Leave the broken links as-is. ADRs are verbatim historical artifacts; their cross-reference rot is part of the historical record.

This decision is for the operator. Recommend noting the choice in a follow-up commit message if Option A is taken.
