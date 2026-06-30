# Blast Radius — v1.4.1

**pass:** v1.4.1 — Documentation: project registration for federated deployments
**prior-commit:** (see feat commit SHA once committed)
**date:** 2026-06-21

## Files changed

### Modified files
- `README.md` — new `## Project Registration (for federated deployments)` section:
  manual registration spec table (four fields), `RelayConfig` heartbeat example,
  indexed_tags discipline note, forward-link to §15 (fossic-coordinator) and §9.4
  (event schema)
- `docs/implement/FOSSIC_V1_SPEC.md` — §9.4 `_fossic/system` event type table:
  `ProjectRegistered` and `RelayHeartbeat` rows added with trigger, payload fields,
  and indexed_tags schema
- `CHANGELOG.md` — v1.4.1 entry
- `Cargo.toml` — version `1.4.0 → 1.4.1`

### New files
- `docs/aseptic/blast-radius/pass-1.4.1.md` — this file
- `docs/aseptic/pass-complete/pass-1.4.1.md` — pass-complete record

## Scope decision: no new federation section in spec

The brief called for a spec federation protocol section "if it exists" and "file as
CP-T2-N if not." The spec has no federation section. The `_fossic/system` table in
§9.4 is the natural and already-established home for system event type documentation.
Adding `ProjectRegistered` and `RelayHeartbeat` there is correct scope.

A full federation protocol section (hub coordinator discovery protocol, multi-project
relay topology, coordinator subscription lifecycle) is deferred as CP-T2-2. That work
belongs with the `fossic-coordinator` crate implementation, not in v1.4.x docs.

## Risk surface

Docs-only pass. No runtime behavior changes. No new test targets required.
322 tests continue to pass.

## Out of scope

fossic-coordinator crate (CP-T2-2). Python integration test for heartbeat thread.
Full federation protocol spec section.
