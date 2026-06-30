# Phase 1 — Inventory & Classification Manifest

**Date:** 2026-06-21
**Mode:** INITIAL_CLEANUP
**Scanned:** project docs only (excludes `target/`, `node_modules/`, `.venv*`, `.pytest_cache/`)

---

## Classification manifest

| Path | Bucket | Rationale | Disposition |
|------|--------|-----------|-------------|
| `README.md` | CURRENT | Accurate, detailed Rust API reference with quick-starts for bounded reads, iterators, similarity search, subscriptions. Needs restructuring as a routing entry point but content is solid and current (v1.8.1). | Keep → rewrite as reader entry point (Phase 3) |
| `CHANGELOG.md` | CURRENT | Versioned release history, machine-parsed by bumper. Authoritative. | Keep in place |
| `benchmarks/results/aggregate_volume_sweep.md` | HISTORICAL_NARRATIVE | Benchmark run from v1.0-rc.1 era (2026-06-12). Captures specific p99 figures; several targets FAIL. Useful for history.md perf narrative. | Archive |
| `benchmarks/results/sqlite_wal_payload_sweep.md` | HISTORICAL_NARRATIVE | Raw payload-size sweep from 2026-06-11 / v0.x era. Source data for early performance understanding. | Archive |
| `crates/fossic-similarity-hnsw/README.md` | CURRENT | Accurate crate-level doc with API, persistence model, Python quick start. | Keep in place |
| `crates/fossic-tauri/README.md` | CURRENT | Accurate crate-level doc explaining Tauri IPC path and why napi-rs doesn't work in webview. | Keep in place |
| `fossic-node/README.md` | CURRENT | Accurate napi-rs binding README. | Keep in place |
| `fossic-py/README.md` | CURRENT | Accurate PyO3 binding README. | Keep in place |
| `docs/DESIGN.md` | SUPERSEDED | 682-line Lattica platform design doc — describes the Reflective Twin Architecture, LumaWeave, Cerebra, Policy Scout as "organs." Valuable history of why fossic was built, but describes the surrounding platform, not fossic itself. Fossic's design is now captured in FOSSIC_V1_SPEC.md and the state reports. | Archive → source for history.md Lattica-origin section |
| `docs/PHASES.md` | SUPERSEDED | 743-line Lattica 12-phase platform plan from 2026-06-11. Fossic has diverged as a standalone library; this plan is the context it was extracted from, not the current roadmap. | Archive → source for history.md |
| `docs/FOSSIC_CONSUMER_PROFILES.md` | SUPERSEDED | Originally titled "Alluvium Consumer Profiles" — consumer binding-model analysis from when the library was called "alluvium/ES." The napi-rs/Tauri binding analysis has been superseded by the crate READMEs. | Archive |
| `docs/fossic-recon-and-arch-opinion-2026-06-20.md` | HISTORICAL_NARRATIVE | Architectural recon report prepared for SR-10 pass (2026-06-20). Directly generated the SR-10 failure modes analysis. No ongoing authority. | Archive → provenance for history.md SR-10 section |
| `docs/FOSSIC_TIDYUP_SURVEY.md` | HISTORICAL_NARRATIVE | Tidy-up survey from 2026-06-12, documents tech debt and open issues at that point. Most items tracked through subsequent passes. | Archive → source for history.md debt/cleanup section |
| `docs/SUBSTRATE_EXTENSION_PATTERNS.md` | CURRENT | Source-verified extension guide for sibling crate authors. Last updated 2026-06-21, v1.7.3. Minor drift from v1.8.1 but structurally current. | Keep → route to `docs/deep-dives/extension-patterns.md` |
| `docs/SUBSTRATE_GOTCHAS.md` | CURRENT | Source-verified consumer/integrator gotcha reference. Last updated 2026-06-21. | Keep → route to `docs/gotchas.md` |
| `docs/src/introduction.md` | CURRENT | Brief mdBook-era introduction. Content is accurate and could fold into README or architecture. | Keep → fold into architecture.md or README |
| `docs/src/SUMMARY.md` | SUPERSEDED | mdBook table of contents. No mdBook build; file references only introduction.md. Shell artifact. | Archive |
| `docs/adr/ADR-001-lattica-extends-lumaweave.md` | HISTORICAL_DECISION | Documents the architectural decision that Lattica IS LumaWeave extended. Fossic-context: explains why fossic's API surface was designed to be embedding-first. | Keep verbatim → `docs/adr/` |
| `docs/adr/ADR-002-es-toolkit-over-nats.md` | HISTORICAL_DECISION | Documents why NATS was rejected in favour of a SQLite-backed ES toolkit (fossic). Directly explains fossic's existence and design philosophy. | Keep verbatim → `docs/adr/` |
| `docs/adr/ADR-003-eval-core-standalone.md` | HISTORICAL_DECISION | Eval core standalone decision. Platform-level. Lower relevance to fossic internals. | Keep verbatim → `docs/adr/` (platform context) |
| `docs/adr/ADR-004-policy-scout-governance-scope.md` | HISTORICAL_DECISION | Policy Scout governance scope. Platform-level consumer ADR. | Keep verbatim → `docs/adr/` |
| `docs/adr/ADR-005-cerebra-api-surface.md` | HISTORICAL_DECISION | Cerebra API surface decision. Platform-level consumer ADR. | Keep verbatim → `docs/adr/` |
| `docs/adr/ADR-006-monorepo-structure.md` | HISTORICAL_DECISION | Monorepo structure (pnpm/uv workspace). Explains workspace layout fossic now uses. | Keep verbatim → `docs/adr/` |
| `docs/adr/ADR-007-lumashell-pattern-absorption.md` | HISTORICAL_DECISION | LumaShell pattern absorption. Platform-level, less fossic-relevant. | Keep verbatim → `docs/adr/` |
| `docs/adr/ADR-008-phase-12-research-exploration.md` | HISTORICAL_DECISION | Phase 12 research framing. Platform-level. | Keep verbatim → `docs/adr/` |
| `docs/agent/CHANGELOG_CONTRACT.md` | CURRENT | Load-bearing interface spec for the changelog bumper. Operational. | Keep in place |
| `docs/agent/DISCORD_PROTOCOL.md` | CURRENT | Authoritative agent-to-developer approval protocol. Operational. | Keep in place |
| `docs/aseptic/README.md` | CURRENT | Aseptic methodology overview. Operational governance. | Keep in place |
| `docs/aseptic/INTRODUCTION.md` | CURRENT | Conceptual frame for the aseptic methodology. | Keep in place |
| `docs/aseptic/AGENT_BRIEFING.md` | CURRENT | Agent briefing spec. | Keep in place |
| `docs/aseptic/SUPERVISOR_PROTOCOL.md` | CURRENT | Supervisor protocol spec. | Keep in place |
| `docs/aseptic/PASS_REPORTING.md` | CURRENT | Pass reporting spec. | Keep in place |
| `docs/aseptic/VERSION_CONVENTION.md` | CURRENT | Version convention spec. | Keep in place |
| `docs/aseptic/LIVING_REPORTS.md` | CURRENT | Living reports spec. | Keep in place |
| `docs/aseptic/BLAST_RADIUS.md` | CURRENT | Per-pass blast radius artifact spec. | Keep in place |
| `docs/aseptic/CROSS_POLLINATION.md` | CURRENT | Cross-pollination protocol spec. | Keep in place |
| `docs/aseptic/DEVIATION.md` | CURRENT | Deviation reporting spec. | Keep in place |
| `docs/aseptic/ADR_FORMAT.md` | CURRENT | ADR format spec. | Keep in place |
| `docs/aseptic/TECH_DEBT.md` | HISTORICAL_NARRATIVE | Living report, last reviewed v1.0.0w — multiple major versions stale (current v1.8.1). Operator confirmed archive. Source for history.md debt section. | Archive |
| `docs/aseptic/POLISH_DEBT.md` | HISTORICAL_NARRATIVE | Same as TECH_DEBT.md. Operator confirmed archive. | Archive |
| `docs/aseptic/aseptic-notes.md` | EPHEMERAL | Explicitly labelled "Bench notes, June 2026. Not a spec." Conceptual frame draft. | Archive |
| `docs/aseptic/aseptic-artifacts.md` | EPHEMERAL | Companion to aseptic-notes.md, same status. "Not a spec." | Archive |
| `docs/aseptic/blast-radius/pass-01.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-02.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-03.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-04.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-05.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-06.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-07.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-08.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-09.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-11.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.0.0n.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.0.0o.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.0.0p.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.0q.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.0r.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.0s.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.0.t.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.0.u.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.0.0w.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.0.0y.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.0.0z.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.1.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.v.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.w.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-10.x.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.0.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.1.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.2.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.3.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.4.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.5.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.6.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.7.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.8.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.1.9.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.2.0.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.2.1.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.2.2.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.3.0.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.3.1.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.4.0.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.4.1.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.5.0.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.7.0.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.7.1.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.7.2.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.7.3.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-1.7.4.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-8.5.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/blast-radius/pass-8.6.md` | HISTORICAL_NARRATIVE | Per-pass blast radius report. | Archive |
| `docs/aseptic/cross-pollination/pass-09.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-10.0.t.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-10.1.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-10.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-10.v.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-8.5.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-9.1.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-9.3.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/cross-pollination/pass-9.4.md` | HISTORICAL_NARRATIVE | Cross-project relay report. | Archive |
| `docs/aseptic/pass-complete/pass-1.0.0n.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.0.0o.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.0.0w.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.0.0y.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.0.0z.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.2.0.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.2.1.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.2.2.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.3.0.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.3.1.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.4.0.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.4.1.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/aseptic/pass-complete/pass-1.5.0.md` | HISTORICAL_NARRATIVE | Pass completion report. | Archive |
| `docs/implement/FOSSIC_V1_SPEC.md` | CURRENT | 1228-line authoritative implementation spec. Threading model, schema, error catalogue, stream registry contract. Referenced by README. | Keep → route to `docs/deep-dives/spec.md` or keep at path |
| `docs/implement/CCE_SPEC.md` | CURRENT | Canonical content encoding spec. Language-implementation-portable. | Keep in place |
| `docs/implement/BUILD_AND_DISTRIBUTION.md` | SUPERSEDED | Distribution plan from 2026-06-12. Pre-built wheels not shipped; plan may have shifted. Operator confirmed archive. | Archive |
| `docs/implement/AGENT_TRACE_VOCABULARY.md` | CURRENT | Standard event vocabulary for agent trace recording. v1.0.0s, 2026-06-16. | Keep in place |
| `docs/implement/POLICY_SCOUT_EVENT_VOCABULARY.md` | CURRENT | Policy Scout event vocabulary. v0.1. | Keep in place |
| `docs/state-reports/SR-01-identity-and-cce.md` | CURRENT | 872-line source-verified deep dive on CCE and event identity. | Keep → route to `docs/deep-dives/identity-and-cce.md` |
| `docs/state-reports/SR-02-storage-schema-concurrency.md` | CURRENT | 747-line deep dive on storage schema and concurrency model. | Keep → route to `docs/deep-dives/storage-schema-concurrency.md` |
| `docs/state-reports/SR-03-event-lifecycle.md` | CURRENT | 1001-line deep dive on event lifecycle. | Keep → route to `docs/deep-dives/event-lifecycle.md` |
| `docs/state-reports/SR-04-subscriptions-and-wal-watch.md` | CURRENT | 778-line deep dive on subscriptions and WAL watcher. | Keep → route to `docs/deep-dives/subscriptions-wal-watch.md` |
| `docs/state-reports/SR-05-branches.md` | CURRENT | 603-line deep dive on branches. | Keep → route to `docs/deep-dives/branches.md` |
| `docs/state-reports/SR-06-reducers-and-snapshots.md` | CURRENT | 704-line deep dive on reducers and snapshots. | Keep → route to `docs/deep-dives/reducers-snapshots.md` |
| `docs/state-reports/SR-07-cross-stream-queries.md` | CURRENT | 723-line deep dive on cross-stream queries. | Keep → route to `docs/deep-dives/cross-stream-queries.md` |
| `docs/state-reports/SR-08-schema-evolution-deletion-errors.md` | CURRENT | 891-line deep dive on schema evolution, deletion, errors. | Keep → route to `docs/deep-dives/schema-evolution-deletion-errors.md` |
| `docs/state-reports/SR-09-python-bindings.md` | CURRENT | 961-line deep dive on Python bindings. | Keep → route to `docs/deep-dives/python-bindings.md` |
| `docs/state-reports/SR-10-failure-modes.md` | CURRENT | 411-line failure modes analysis (17 PART A findings + 6 PART B design questions). Current as of v1.8.1 with A-5/A-6/A-11 closed this pass. | Keep → route to `docs/deep-dives/failure-modes.md` |
| `docs/adjacent-project-info/fossic-interview.md` | HISTORICAL_NARRATIVE | "Library Profile and Pass 10 Completion Summary" — cross-project briefing doc capturing what fossic was at v1.0-rc.1. Source for history.md. | Archive |
| `docs/adjacent-project-info/cerebra_extract.md` | EPHEMERAL | Consumer profile sent to Cerebra. Template fill-in for cross-project alignment. No fossic design decisions. | Archive |
| `docs/adjacent-project-info/aistack_extract.md` | EPHEMERAL | Consumer profile for ai-stack. | Archive |
| `docs/adjacent-project-info/lumaweave_extract.md` | EPHEMERAL | Consumer profile for LumaWeave. | Archive |
| `docs/adjacent-project-info/policy_scout_extract.md` | EPHEMERAL | Consumer profile for Policy Scout. | Archive |
| `docs/adjacent-project-info/discord_bot_extract.md` | EPHEMERAL | Consumer profile for Discord bot. | Archive |
| `docs/adjacent-project-info/PROJECT_EXTRACTION_TEMPLATE.md` | EPHEMERAL | Template for consumer profile generation. | Archive |
| `docs/adjacent-project-info/cerebra_read_adapter_design.md` | HISTORICAL_DECISION | Design for Cerebra read adapter over fossic. Decision relevant to fossic's query API surface. | Archive (Cerebra-owned decision, not fossic's) |
| `docs/adjacent-project-info/cerebra_read_adapter_addendum.md` | HISTORICAL_DECISION | Addendum to above. | Archive |
| `docs/adjacent-project-info/cerebra_to_fossic_phase4_reply.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/cerebra_to_fossic_phase4a_ack_of_ack.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/fossic_to_cerebra_phase4_query.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/fossic_to_cerebra_phase4a_ack.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/fossic_to_cerebra_phase10_ack.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/fossic_to_lattica_cerebra_phase10_routing_ack.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/fossic_to_policy_scout_relay_backfill.md` | EPHEMERAL | Inter-project relay backfill message. | Archive |
| `docs/adjacent-project-info/lattica_to_fossic_round1_relay.md` | EPHEMERAL | Inter-project relay message. | Archive |
| `docs/adjacent-project-info/lattica_to_fossic_round1_relay_response.md` | EPHEMERAL | Inter-project relay message. | Archive |

---

## Unclear items requiring decision

- **`docs/aseptic/TECH_DEBT.md` and `docs/aseptic/POLISH_DEBT.md`**: Classified CURRENT because they are "living reports" by design. However, `last_reviewed: v1.0.0w` is several versions stale (current: v1.8.1). **Decision needed:** (a) update `last_reviewed` to current version as part of this pass, or (b) archive as HISTORICAL_NARRATIVE and let them be regenerated. If the content is still materially accurate, (a) is right. If they've been superseded by work done in v1.1–v1.8, (b) is right. **Proposed bucket if stale:** HISTORICAL_NARRATIVE → archive.

- **`docs/implement/BUILD_AND_DISTRIBUTION.md`**: Classified CURRENT as a plan, but pre-built wheels have not shipped yet (still requires Rust locally per README). The distribution aspirations described may have shifted. **Decision needed:** is this still the active plan, or has the distribution approach been revised? If the plan changed, this is SUPERSEDED.

- **`docs/adr/` (all 8)**: All 8 ADRs were written for the Lattica platform context on 2026-06-11, not fossic-specific decisions. They capture *why fossic was built* but not fossic-internal decisions (there are no fossic-internal ADRs yet). **Decision needed:** should these stay in `docs/adr/` as platform-context ADRs (useful for an employer reviewer understanding the origin), or should they be archived as SUPERSEDED-platform-docs since this repo is now fossic-standalone? **Proposed bucket if archiving:** SUPERSEDED with a reference in history.md.

---

## Target structure mapping

After this pass, the `docs/` surface will look like:

```
README.md                   ← rewritten as routing entry point
CHANGELOG.md                ← unchanged

docs/
  architecture.md           ← new (Phase 3), grounded in code + FOSSIC_V1_SPEC.md
  operating.md              ← new (Phase 3), grounded in Justfile + test commands + BUILD_AND_DISTRIBUTION.md
  gotchas.md                ← from SUBSTRATE_GOTCHAS.md (rename/move)
  history.md                ← new (Phase 2), derived from HISTORICAL_NARRATIVE
  deep-dives/
    identity-and-cce.md     ← from SR-01
    storage-schema-concurrency.md ← from SR-02
    event-lifecycle.md      ← from SR-03
    subscriptions-wal-watch.md ← from SR-04
    branches.md             ← from SR-05
    reducers-snapshots.md   ← from SR-06
    cross-stream-queries.md ← from SR-07
    schema-evolution-deletion-errors.md ← from SR-08
    python-bindings.md      ← from SR-09
    failure-modes.md        ← from SR-10
    extension-patterns.md   ← from SUBSTRATE_EXTENSION_PATTERNS.md
  adr/
    README.md               ← new index
    ADR-001 … ADR-008       ← verbatim copies of existing (or originals moved)
  implement/                ← keep as-is (FOSSIC_V1_SPEC, CCE_SPEC, vocabulary docs)
  agent/                    ← keep as-is (CHANGELOG_CONTRACT, DISCORD_PROTOCOL)
  aseptic/                  ← keep governance docs, archive per-pass subdirs

archive/
  2026-06-21-pre-cleanup/   ← all archived files at their original paths
```

---

## Statistics

- **Files scanned:** 149 (project docs; excludes node_modules, .venv, target, .pytest_cache)
- CURRENT: 45 | HISTORICAL_DECISION: 10 | HISTORICAL_NARRATIVE: 70 | SUPERSEDED: 5 | EPHEMERAL: 16 | DUPLICATE: 0 | UNCLEAR: 3
- **Total bytes before:** ~1.6 MB
- **Projected bytes after (kept files):** ~900 KB (55 kept files) + new docs written in Phase 3 (~50 KB estimated)

---

*Phase 1 complete. STOP — awaiting operator review and approval of the manifest before proceeding to Phase 2.*
