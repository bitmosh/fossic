── PASS COMPLETE · v1.4.0 · 2026-06-21 ──────────────────────

Title: Phase 8 — ProjectRegistered + RelayHeartbeat event-sourced project discovery primitives

Summary: Two new system event types (ProjectRegistered, RelayHeartbeat) emitted to _fossic/system via a new registry.rs module. Substrate-side emit only — consumer coordination (fossic-coordinator) is future work. Python relay.py updated with startup emit and daemon heartbeat thread.

Project: fossic

Highlights:
· registry.rs: substrate-side emit-only helpers; receives &mut SystemStreamWriter, no Store/StoreInner dependency
· Both events carry indexed_tags={"source_store":"<name>"} (required for future coordinator filtering — enforced by test)
· project_registry_writer: lazy Mutex<Option<SystemStreamWriter>> — same pattern as reducer_system_writer; dedicated connection, relay threads never contend with dispatcher or reducer writers
· fossic-py: PyO3 bindings + typed Store wrappers; RelayAgent emits ProjectRegistered on startup, spawns daemon heartbeat thread (default 5s interval)
· Heartbeat CCE uniqueness confirmed by test: distinct uptime_us values → distinct CCE hashes → distinct events

Learnings:
· The "lazy Mutex<Option<SystemStreamWriter>>" pattern is now the established substrate pattern for optional, thread-safe, dedicated system event writers — used for dispatcher, reducer, and now registry writers

Skipped test note:
· 1 ignored test (doc-test src/store.rs::append_if, marked # ```ignore) is stable and intentional — a code example requiring external state, not a regression

Commit: eedf26b
Tests: 322 passed · 0 failed · 1 ignored
Branch: clean
