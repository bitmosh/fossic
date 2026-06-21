# Blast Radius — v1.4.0

**pass:** v1.4.0 — Phase 8: Hub Coordinator Preparation — project discovery primitives
**prior-commit:** (see feat commit SHA once committed)
**date:** 2026-06-21

## Files changed

### New files
- `src/registry.rs` — `emit_project_registered` and `emit_relay_heartbeat` helpers;
  call `SystemStreamWriter::emit` with correct payload + `{"source_store": ...}` indexed
  tags
- `tests/registry.rs` — 4 tests: system event presence, payload fields,
  indexed_tags discipline, multiple-heartbeat distinctness
- `docs/aseptic/blast-radius/pass-1.4.0.md` — this file
- `docs/aseptic/pass-complete/pass-1.4.0.md` — pass-complete record

### Modified files
- `src/lib.rs` — `mod registry;` added
- `src/store.rs` — `StoreInner`: `project_registry_writer` field (Phase 8 section);
  `Store::emit_project_registered` + `Store::emit_relay_heartbeat` public methods;
  `project_registry_writer: parking_lot::Mutex::new(None)` in `Store::open`
- `fossic-py/src/store.rs` — `emit_project_registered` + `emit_relay_heartbeat`
  PyO3 bindings on `PyStore`
- `fossic-py/python/fossic/__init__.py` — typed wrappers for both emit methods on
  `Store` Python class
- `fossic-py/python/fossic/relay.py` — `RelayConfig`: `heartbeat_interval_s: float = 5.0`
  and `project_description: str = ""`; `RelayAgent`: `_last_event_version`,
  `_start_us`, `_heartbeat_loop`, daemon heartbeat thread in `run()`,
  `emit_project_registered` on startup; `relay_event` updates `_last_event_version`
- `Cargo.toml` — `[[test]] registry`; version `1.3.1 → 1.4.0`
- `CHANGELOG.md` — v1.4.0 entry (placeholder, filled in chore commit)

## Architecture

`registry.rs` is the substrate-side emit-only module. It has no knowledge of
`Store` or `StoreInner` — it receives a `&mut SystemStreamWriter` from the caller.
This keeps the module testable and dependency-free.

`project_registry_writer` uses the same lazy `Mutex<Option<SystemStreamWriter>>`
pattern as `reducer_system_writer` — connection opened on first use, dedicated so
relay threads never contend with the dispatcher or reducer writers.

Both `emit_project_registered` and `emit_relay_heartbeat` always return `Ok(())`
(best-effort delivery, errors silently dropped inside `SystemStreamWriter::emit`).

## Risk surface

**Consumer side not shipped.** `fossic-coordinator` crate (subscribes to
`ProjectRegistered` to dynamically open project stores) is FUTURE work. Nothing in
v1.4.0 consumes these events.

**CCE collision on identical payloads.** `emit_project_registered` with the same
four arguments produces the same CCE hash — the `INSERT OR IGNORE` in
`SystemStreamWriter` silently deduplicates. This is correct: re-announcing the same
project is idempotent.

**Heartbeat CCE uniqueness.** Each `emit_relay_heartbeat` call varies `uptime_us`,
so hashes differ — every heartbeat produces a distinct event (confirmed by test
`multiple_heartbeats_are_distinct_appends`).

**relay.py thread safety.** `_last_event_version` is a Python `int` field written
by the relay loop and read by the heartbeat thread. Under the GIL, int attribute
writes are atomic at the bytecode level; no additional locking is needed for this
liveness signal.

**heartbeat thread is daemon.** The thread exits when the interpreter exits, so no
explicit join is needed. `stop_hb.set()` in the `finally` block shuts it down
cleanly when `run()` exits normally.

## Test coverage

- `emit_project_registered_writes_system_event` — event present in `_fossic/system`
  with correct payload fields
- `emit_relay_heartbeat_writes_system_event` — event present with correct payload fields
- `emit_project_registered_indexed_tag_source_store` — `indexed_tags["source_store"]`
  is set correctly (required for future coordinator filtering)
- `multiple_heartbeats_are_distinct_appends` — distinct uptime_us values produce
  two separate events

## Out of scope

`fossic-coordinator` crate — consumer-side subscription to `ProjectRegistered`.
Python integration test for heartbeat thread (would require maturin build +
wall-clock sleep). Python typing for new relay.py fields (covered by Pyright
via `__init__.py` wrappers).

## Skipped test note

The 1 skipped test from v1.3.0 onward (`src/store.rs - store::Store::append_if`)
is a doc-test marked `# ```ignore` — a code example requiring external state.
Intentional; not a regression.
