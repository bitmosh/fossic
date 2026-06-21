# Blast Radius — v1.5.0

**pass:** v1.5.0 — Track 2 close: fossic core substrate-complete
**prior-commit:** (see chore commit SHA once committed)
**date:** 2026-06-21

## Files changed

### Modified files
- `CHANGELOG.md` — v1.5.0 entry with full Track 2 arc summary
- `Cargo.toml` — version `1.4.1 → 1.5.0`

### New files
- `docs/aseptic/blast-radius/pass-1.5.0.md` — this file
- `docs/aseptic/pass-complete/pass-1.5.0.md` — pass-complete record

## What closes

Version bump only. No API changes, no new behavior. Marks the substrate-complete
milestone for Track 2.

## Track 2 arc (v1.2.0 → v1.5.0)

| Version | Work |
|---|---|
| v1.2.0 | `EveryNEvents` snapshot policy (Phase 6 open) |
| v1.2.1 | `ReducerStateLarge` diagnostics + `StateAdaptive` policy |
| v1.2.2 | `auto_gc_orphans`, Phase 6 close |
| v1.3.0 | `BackgroundExecutor` + `QuiescenceMonitor` scaffold (Phase 7 open) |
| v1.3.1 | `EveryNSeconds` enforcement + recurring background GC, Phase 7 close |
| v1.4.0 | `ProjectRegistered` + `RelayHeartbeat` emit primitives (Phase 8 open) |
| v1.4.1 | Project registration docs pass |
| v1.5.0 | Track 2 close (this version) |

## Risk surface

Version bump only. 322 tests unchanged.

## Open items carried forward

- CP-T2-2: full federation protocol spec section (fossic-coordinator crate work)
- `fossic-coordinator` consumer crate (subscribes to `ProjectRegistered`)
- `TaskPriority::High` variant — reserved, unused
- `EveryNSeconds` quiescence window integration test (wall-clock, deferred)
