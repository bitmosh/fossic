---
pass: 10.0r
version: v0.10.0r
date: 2026-06-13
summary: Test surface unification — Justfile, cargo test --workspace fix, CI alignment.
---

# Blast Radius — Pass 10.0r (v0.10.0r)

## Files

### Modified
- `crates/fossic-tauri/tests/read_range.rs` — added `#![cfg(feature = "test-helpers")]`
  crate-level attribute; tests now silently skip under default features and activate
  under `--all-features` or `--features test-helpers`
- `crates/fossic-tauri/examples/basic.rs` — replaced `tauri::Wry` (gated behind
  `feature = "wry"`) with `tauri::test::MockRuntime` (always available via the existing
  `tauri/test` dev-dependency); removes the compile failure under default features
- `crates/fossic-tauri/src/lib.rs` — changed two doc-test fences from `rust,no_run`
  to `rust,ignore`; `tauri::generate_context!()` is a proc-macro that reads
  `tauri.conf.json` at compile time — the file doesn't exist in this library crate,
  so even `no_run` failed to compile. `ignore` is the correct marker for illustrative
  examples that cannot compile in the library crate's test context
- `.github/workflows/ci.yml` — updated `test` job from `cargo test --all-features`
  (root crate only) to `cargo test --workspace --all-features` (matches `just test`)
- `README.md` — §Tests section replaced: `just test` is now the canonical command;
  first-run timing noted; per-binding variants documented; no-just fallback added
- `docs/aseptic/TECH_DEBT.md` — TD-006 filed and resolved; `last_reviewed` → v0.10.0r
- `docs/aseptic/POLISH_DEBT.md` — PD-008 filed and resolved; `last_reviewed` → v0.10.0r
- `docs/aseptic/DEVIATION.md` — `last_reviewed` → v0.10.0r
- `docs/aseptic/README.md` — version → v0.10.0r

### Created
- `Justfile` — canonical test runner with `just test`, `just test-rust`, `just test-py`,
  `just test-node` targets
- `docs/aseptic/blast-radius/pass-10.0r.md` — this file

### Deleted
- (none)

---

## Public APIs

### Added
- (none — Justfile is a developer tool, not a public API)

### Modified (breaking)
- (none)

### Modified (non-breaking)
- `crates/fossic-tauri/tests/read_range.rs`: tests now gated behind `test-helpers`
  feature (silently skipped under `cargo test --workspace`; active under
  `--all-features` or `--features test-helpers`). Behavior unchanged; test coverage
  unchanged under the canonical `just test` invocation.
- `crates/fossic-tauri/examples/basic.rs`: `tauri::Wry` → `tauri::test::MockRuntime`.
  Example behavior identical; `let _ = plugin(store)` still constructs the plugin.

### Removed
- (none)

---

## Schema changes

None.

---

## Configuration changes

- CI `test` job: `cargo test --all-features` → `cargo test --workspace --all-features`.
  Now tests the full workspace (including fossic-tauri, fossic-py, fossic-node Rust
  internals) rather than the root crate only.

---

## Dependency changes

None.

---

## Behavior changes

None for production code. Test infrastructure changes:

- `cargo test --workspace` (no feature flags) now compiles and runs cleanly. Previously
  failed to compile due to the `plugin_with_test_helpers` feature gate and `tauri::Wry`
  type being unavailable.
- `cargo test --workspace --all-features` is now the single canonical Rust test
  invocation: includes all workspace members, enables `test-helpers` for fossic-tauri,
  runs the tauri integration tests.

---

## Test counts (verified post-pass)

| Suite | Count | Notes |
|---|---|---|
| Rust (`--workspace --all-features`) | 202 passed | 200 core + 2 fossic-tauri integration |
| Python | 92 passed, 2 skipped | skips pre-existing (`expected_hex: null` vector) |
| Node | 47 passed | prompt anticipated 43; actual is 47 (count drift from earlier snapshot) |

Node count discrepancy (47 vs. 43 in pass prompt): all 47 tests pass; the prior count
in the prompt was a stale snapshot. No test failures; no action required.

---

## Living report updates

Entries filed and resolved this pass:
- TECH_DEBT: TD-006 (test surface fragmented) — filed and resolved in same pass
- POLISH_DEBT: PD-008 (no canonical test invocation) — filed and resolved in same pass

`last_reviewed` bumped to v0.10.0r on TECH_DEBT, POLISH_DEBT, and DEVIATION.

---

## Editorial decisions

- `rust,ignore` vs. `rust,no_run` for fossic-tauri doc tests: `no_run` only prevents
  execution; `tauri::generate_context!()` expands at compile time and requires
  `tauri.conf.json` to be present in the package directory. Since `fossic-tauri` is
  a library crate with no `tauri.conf.json`, even `no_run` fails to compile. `ignore`
  is the correct marker — these are reference examples for app developers, not
  compile-testable library tests.

- `cargo test --workspace --all-features` vs. two commands: using `--all-features`
  enables `test-helpers` on fossic-tauri automatically, so the single command covers
  everything the previous two-command sequence covered. CI updated to match.

- `MockRuntime` in `examples/basic.rs`: the example already notes it has "no webview"
  and runs headlessly. Using `MockRuntime` instead of `Wry` is semantically correct
  for this example. Production Tauri apps would use a concrete runtime via their own
  Tauri app setup, not the example.
