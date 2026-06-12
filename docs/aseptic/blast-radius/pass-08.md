---
pass: 8
version: v0.8.0
date: "(retroactive estimate, not verified)"
summary: fossic-node napi-rs binding and crates/fossic-tauri initial implementation
---

# Blast Radius — Pass 8 (v0.8.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic-node/` — entire napi-rs binding directory
- `fossic-node/Cargo.toml` — napi, napi-derive, napi-build (pending napi dep approval)
- `fossic-node/src/lib.rs` — napi module entry point
- `fossic-node/src/store.rs` — Store napi class, async methods via spawn_blocking
- `fossic-node/src/types.rs` — JS-compatible type structs
- `fossic-node/build.rs` — napi_build::setup()
- `fossic-node/package.json` — npm package manifest
- `fossic-node/__test__/` — vitest tests (retroactive estimate)
- `crates/fossic-tauri/` — entire Tauri companion crate directory
- `crates/fossic-tauri/Cargo.toml` — tauri, fossic dep, time=0.3.37 pin
- `crates/fossic-tauri/src/lib.rs` — Tauri plugin entry point
- `crates/fossic-tauri/src/commands.rs` — all fossic_* Tauri IPC commands
- `crates/fossic-tauri/src/serialization.rs` — JSON serialization helpers

---

## Public APIs

### Added (Node binding — napi-rs)
- `Store.open(path: string)` — sync factory (retroactive estimate)
- `Store.declareStream(streamId, declaredBy)` — async
- `Store.append(a: AppendInput)` — async, returns EventId as hex string
- `Store.readRange(query: ReadQueryInput)` — async
- `Store.readOne(eventId: string)` — async
- `Store.subscribe(streamId, branch)` — returns subscription handle
- `Store.createBranch`, `Store.promoteBranch`, `Store.markBranchDeadEnd`,
  `Store.listBranches`, `Store.resolveChain` — async
- `Store.getCursor(name: string)` — NOTE: single-arg API, differs from Rust (B2)
- `Store.setCursor(name: string, value: number)` — NOTE: simplified API

### Added (Tauri IPC commands)
- `fossic_open_store`, `fossic_declare_stream`, `fossic_append`, `fossic_read_range`,
  `fossic_read_one`, `fossic_create_branch`, `fossic_promote_branch`,
  `fossic_mark_branch_dead_end`, `fossic_list_branches`, `fossic_resolve_chain`,
  `fossic_take_snapshot`, `fossic_read_state`, `fossic_read_state_at_version`,
  `fossic_get_cursor`, `fossic_set_cursor`

---

## Schema changes

None — bindings share the same SQLite database format.

---

## Configuration changes

None at fossic core level. Tauri app consumers must register the fossic-tauri plugin.

---

## Dependency changes

In `fossic-node/Cargo.toml`:
- `napi`, `napi-derive`, `napi-build` — [DEPENDENCY REQUEST — REQUIRES MANUAL APPROVAL
  at the time of landing; napi crates from the napi-rs organization]

In `crates/fossic-tauri/Cargo.toml`:
- `tauri` — Tauri 2 plugin support
- `time = "=0.3.37"` — exact pin to avoid cookie coherence conflict (TD-003)

---

## Behavior changes

- Node binding errors are all `GenericFailure` strings — no typed error hierarchy.
  TypeScript callers cannot programmatically distinguish error types (TIDYUP A1).
- Node cursor API uses single-arg `getCursor(name: string)` rather than three-arg
  `getCursor(consumerId, streamId, branch)` (TIDYUP B2).
- Tauri commands return `Result<T, String>` — no structured error type (TIDYUP A3).
- `fossic_read_state_at_version` accepts `reducer_name` parameter but ignores it (TIDYUP Issue 4).

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: Multiple TIDYUP findings (A1, A3, B1, B2, B3) originate from this pass.*
