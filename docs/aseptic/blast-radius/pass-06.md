---
pass: 6
version: v0.6.0
sha: c7030aa
date: 2026-06-12
summary: Tauri IPC companion crate and napi-rs build config
---

# Blast Radius — Pass 6 (v0.6.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content completely rewritten — original bootstrap estimate described the PyO3
> Python binding (which is v0.5.0). Actual commit c7030aa added crates/fossic-tauri
> and napi.config.json.

## Files

### Created

8 files in commit c7030aa:

- `crates/fossic-tauri/Cargo.toml` — tauri 2 dep, fossic path dep, `time = "=0.3.37"` exact pin (TD-003 origin)
- `crates/fossic-tauri/Cargo.lock` — dependency lockfile
- `crates/fossic-tauri/README.md` — Tauri plugin integration guide
- `crates/fossic-tauri/examples/basic.rs` — minimal Tauri app wiring example
- `crates/fossic-tauri/src/lib.rs` — Tauri plugin entry point; plugin registration
- `crates/fossic-tauri/src/commands.rs` — all `fossic_*` Tauri IPC command handlers
- `crates/fossic-tauri/src/serialization.rs` — JSON serialization helpers for IPC boundary
- `napi.config.json` — napi-rs build pipeline configuration (targets: linux-x64-gnu, darwin-x64, darwin-arm64, win32-x64-msvc)

---

## Public APIs

### Added

**Tauri IPC commands (registered via `tauri::Builder::plugin()`):**
- `fossic_open_store(path: String) -> Result<(), String>`
- `fossic_declare_stream(stream_id, declared_by, description) -> Result<(), String>`
- `fossic_append(stream_id, branch, event_type, type_version, payload, ...) -> Result<String, String>` — returns hex EventId
- `fossic_read_range(stream_id, branch, from_version, to_version, limit) -> Result<Vec<Value>, String>`
- `fossic_read_one(event_id: String) -> Result<Option<Value>, String>`
- `fossic_create_branch(...)`, `fossic_promote_branch(...)`, `fossic_mark_branch_dead_end(...)`, `fossic_list_branches(stream_id)`, `fossic_resolve_chain(stream_id, branch_id)` — branch commands
- `fossic_take_snapshot(stream_id, branch)`, `fossic_read_state(stream_id, branch)`, `fossic_read_state_at_version(stream_id, branch, version)`, `fossic_snapshot_info(...)`, `fossic_gc_orphaned_snapshots()` — snapshot commands
- `fossic_get_cursor(consumer_id, stream_id, branch)`, `fossic_set_cursor(...)` — cursor commands
- `fossic_aggregate(...)`, `fossic_read_by_external_id(...)`, `fossic_read_by_correlation(...)`, `fossic_walk_causation(...)` — cross-stream commands
- `fossic_purge_event(event_id, confirm, reason, purged_by)` — deletion command

All Tauri commands return `Result<T, String>` — no structured error type (TIDYUP A3). Error variant cannot be distinguished programmatically by the webview caller.

---

## Schema changes

None — fossic-tauri wraps the same SQLite database; no new tables.

---

## Configuration changes

None at fossic core level. Tauri app consumers must register the plugin:
```rust
tauri::Builder::default()
    .plugin(fossic_tauri::init())
    .run(...)
```

---

## Dependency changes

In `crates/fossic-tauri/Cargo.toml`:
- Added: `tauri = "2"` — Tauri 2 plugin support
- Added: `fossic` (path dependency)
- Added: `time = "=0.3.37"` — exact pin to avoid cookie crate coherence conflict in the Tauri workspace (TD-003); upgrading time without the pin causes build failures in tauri's cookie dependency chain

---

## Behavior changes

- Tauri commands serialize all fossic errors to `String` — no structured error propagation to the webview. TypeScript callers must string-parse error messages to distinguish types (TIDYUP A3).
- `fossic_read_state_at_version` accepts a `reducer_name` parameter but ignores it (TIDYUP Issue 4, retroactive estimate).

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)

*Note: TD-003 (`time = "=0.3.37"` exact pin) originates from this pass.*
