# Phase 3 Restructure Plan — fossic v1.8.1

**Phase:** 3 — INITIAL_CLEANUP mode  
**Status:** Complete. Staged, not committed.  
**Primary audience for all new docs:** employer reviewer.

---

## Files Written (new)

### `docs/architecture.md`
System overview grounded in source. Covers: module map (all 22 src/ files), workspace crates table, Store/StoreInner field table, SQLite schema (8 tables), CCE+BLAKE3 formula, append pipeline step-by-step, threading model (5 thread types), subscriptions overview, reducers/snapshots, BackgroundExecutor, cross-stream queries, system stream events, crypto-shredding stub, what's not in v1. All type and field names verified against `src/store.rs`, `src/types.rs`, `src/schema.rs`.

### `docs/operating.md`
API usage guide grounded in source. Covers: opening a store, declaring streams, append/append_batch/append_if, read_range/read_one/read_batch, bounded reads + ReadOutcome + streaming iterators, cross-stream queries (correlation/causation/aggregate), subscriptions (both modes), reducers + SnapshotPolicy, branches, consumer cursors, upcasters, deletion (purge/shred), observability (dispatch pressure, queue depth, custom background tasks), test suite commands, OpenOptions reference table.

### `docs/history.md`
Development narrative in 8 inflection points. Every claim cites a source artifact (ADR, archive file, or CHANGELOG entry). Derived from `cleanup/02-history-draft.md` with verification against git log and archive docs. Includes an "Open Gaps" section listing only gaps stated in source docs (not fabricated).

### `README.md` (rewritten)
Replaced the old dense Rust API reference with a routing document. Contains: v1.8.1 callout, workspace crates table, Rust quick start (5 lines), documentation table (5 docs), deep dives table (11 entries), implementation specs table (4 entries), ADR pointer, key concepts summary (6 concepts), test commands, license. Points out to fossic-py/README, fossic-node/README, crates/fossic-tauri/README for binding quick starts.

---

## Files Moved to `docs/deep-dives/` (SR-* promotion)

SR-01 through SR-10 were classified CURRENT in Phase 1 and promoted to `docs/deep-dives/` unchanged. SUBSTRATE_EXTENSION_PATTERNS.md was also promoted.

| Source | Destination | Rationale |
|---|---|---|
| `docs/state-reports/SR-01-identity-and-cce.md` | `docs/deep-dives/identity-and-cce.md` | CURRENT; primary reference for CCE spec |
| `docs/state-reports/SR-02-storage-schema-concurrency.md` | `docs/deep-dives/storage-schema-concurrency.md` | CURRENT |
| `docs/state-reports/SR-03-event-lifecycle.md` | `docs/deep-dives/event-lifecycle.md` | CURRENT |
| `docs/state-reports/SR-04-subscriptions-and-wal-watch.md` | `docs/deep-dives/subscriptions-wal-watch.md` | CURRENT |
| `docs/state-reports/SR-05-branches.md` | `docs/deep-dives/branches.md` | CURRENT |
| `docs/state-reports/SR-06-reducers-and-snapshots.md` | `docs/deep-dives/reducers-snapshots.md` | CURRENT |
| `docs/state-reports/SR-07-cross-stream-queries.md` | `docs/deep-dives/cross-stream-queries.md` | CURRENT |
| `docs/state-reports/SR-08-schema-evolution-deletion-errors.md` | `docs/deep-dives/schema-evolution-deletion-errors.md` | CURRENT |
| `docs/state-reports/SR-09-python-bindings.md` | `docs/deep-dives/python-bindings.md` | CURRENT |
| `docs/state-reports/SR-10-failure-modes.md` | `docs/deep-dives/failure-modes.md` | CURRENT; primary open-item register |
| `docs/SUBSTRATE_EXTENSION_PATTERNS.md` | `docs/deep-dives/extension-patterns.md` | CURRENT; sibling crate author guide |

---

## Files Moved to `docs/gotchas.md`

| Source | Destination | Rationale |
|---|---|---|
| `docs/SUBSTRATE_GOTCHAS.md` | `docs/gotchas.md` | CURRENT; canonical gotcha location per Phase 1 |

---

## Files Archived to `archive/2026-06-21-pre-cleanup/`

### Superseded design and planning docs

| Source | Archive path | Rationale |
|---|---|---|
| `docs/DESIGN.md` | `docs/DESIGN.md` | SUPERSEDED; early design intent, diverged from implementation |
| `docs/PHASES.md` | `docs/PHASES.md` | SUPERSEDED; build-phase plan, all phases completed |
| `docs/FOSSIC_CONSUMER_PROFILES.md` | `docs/FOSSIC_CONSUMER_PROFILES.md` | SUPERSEDED; planning doc, superseded by actual bindings |
| `docs/FOSSIC_TIDYUP_SURVEY.md` | `docs/FOSSIC_TIDYUP_SURVEY.md` | SUPERSEDED; triggered the v0.10.x fix cluster, no longer current |
| `docs/fossic-recon-and-arch-opinion-2026-06-20.md` | `docs/fossic-recon-and-arch-opinion-2026-06-20.md` | SUPERSEDED; SR-10 source, content now in docs/deep-dives/failure-modes.md |
| `docs/implement/BUILD_AND_DISTRIBUTION.md` | `docs/implement/BUILD_AND_DISTRIBUTION.md` | SUPERSEDED; pre-CI notes, distribution not yet shipped |
| `docs/src/SUMMARY.md` | `docs/src/SUMMARY.md` | SUPERSEDED; mdbook nav artifact, no longer the doc structure |
| `docs/src/introduction.md` | `docs/src/introduction.md` | SUPERSEDED; content now in README and docs/architecture.md |

### Aseptic process artifacts

| Source | Archive path | Rationale |
|---|---|---|
| `docs/aseptic/TECH_DEBT.md` | `docs/aseptic/TECH_DEBT.md` | Process artifact; point-in-time debt register |
| `docs/aseptic/POLISH_DEBT.md` | `docs/aseptic/POLISH_DEBT.md` | Process artifact; point-in-time polish register |
| `docs/aseptic/aseptic-notes.md` | `docs/aseptic/aseptic-notes.md` | Process artifact; working notes |
| `docs/aseptic/aseptic-artifacts.md` | `docs/aseptic/aseptic-artifacts.md` | Process artifact; artifact registry |
| `docs/aseptic/blast-radius/` (41 files) | `docs/aseptic/blast-radius/` | Per-pass scope manifests; historical record, not operational |
| `docs/aseptic/cross-pollination/` (9 files) | `docs/aseptic/cross-pollination/` | Inter-module relay documents; historical |
| `docs/aseptic/pass-complete/` (13 files) | `docs/aseptic/pass-complete/` | Per-pass completion reports; historical |

### Adjacent project info

| Source | Archive path | Rationale |
|---|---|---|
| `docs/adjacent-project-info/` (15 files) | `docs/adjacent-project-info/` | Cross-project extraction and relay docs for Lattica coordination; out of scope for fossic repo after archival |

### Benchmark results (untracked by git, moved with mv)

| Source | Archive path | Rationale |
|---|---|---|
| `benchmarks/results/aggregate_volume_sweep.md` | `benchmarks/results/aggregate_volume_sweep.md` | Benchmark snapshot; point-in-time, not operational docs |
| `benchmarks/results/sqlite_wal_payload_sweep.md` | `benchmarks/results/sqlite_wal_payload_sweep.md` | Benchmark snapshot; point-in-time, not operational docs |

---

## Files Not Touched (out of scope for Phase 3)

The following were classified CURRENT in Phase 1 and remain in place:

- `docs/adr/` — all ADRs; first-class artifacts kept verbatim, not modified
- `docs/implement/CCE_SPEC.md` — CCE spec; canonical, active
- `docs/implement/FOSSIC_V1_SPEC.md` — implementation spec; active
- `docs/implement/AGENT_TRACE_VOCABULARY.md` — vocabulary doc; active
- `docs/implement/POLICY_SCOUT_EVENT_VOCABULARY.md` — vocabulary doc; active
- `CHANGELOG.md` — versioned release history; untouched
- `cleanup/01-inventory.md`, `cleanup/02-history-draft.md` — Phase 1/2 outputs; retained per master prompt
- All source files, tests, binding crates — code was not modified

---

## Staging summary

All changes are staged (`git add`) but not committed. The index shows:
- 3 new files (`A`): architecture.md, history.md, operating.md + 2 benchmark archives
- 1 modified file (`M`): README.md
- 67+ renames (`R`): SR-* promotions, gotchas move, all archive moves

Ready for operator review before commit.
