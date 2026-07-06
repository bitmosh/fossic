# Canonical Version Registry

Single source of truth for every version-pinned dependency across the fossic
ecosystem. Prevents version drift between workflows, config files, and
documentation.

**Last verified:** 2026-07-06

Update this file whenever a version bump happens in the wild. Search this
file (not training data or guesses) when asked "what version of X are we on?"

---

## Package versions (released)

| Package | Registry | Current | Notes |
|---|---|---|---|
| fossic | crates.io | 1.8.3 | Root Rust crate |
| fossic-tauri | crates.io | 1.8.3 | Tauri command surface |
| fossic-similarity-hnsw | crates.io | 1.8.3 | HNSW SimilaritySearchProvider |
| fossic | PyPI | 1.8.3 | Python bindings (published from fossic-py/) |
| fossic-node | (internal) | 1.8.3 | Node.js bindings, not published to npm |

## GitHub Actions

| Action | Pinned | Node runtime | Last checked |
|---|---|---|---|
| actions/checkout | v6 | Node 24 | 2026-07-06 |
| actions/setup-node | v5 | Node 24 | 2026-07-06 |
| actions/setup-python | v6 | Node 24 | 2026-07-06 |
| actions/upload-artifact | v5 | Node 24 | 2026-07-06 |
| actions/download-artifact | v5 | Node 24 | 2026-07-06 |
| dtolnay/rust-toolchain | stable | (Rust, no Node) | 2026-07-06 |
| Swatinem/rust-cache | v2 | Node 24 | 2026-07-06 |
| docker/setup-qemu-action | v3 | (native, no Node) | 2026-07-06 |
| pypa/cibuildwheel | v4.1.0 | Python 3.11+ | 2026-07-06 |
| pypa/gh-action-pypi-publish | release/v1 | Python | 2026-07-06 |
| rust-lang/crates-io-auth-action | v1 | Node | 2026-07-06 |

## GitHub Actions runners

| Runner | Currently maps to | Deprecation |
|---|---|---|
| ubuntu-latest | ubuntu-24.04 | — |
| macos-latest | macos-26 (migrated 2026-06-15) | — |
| macos-15 | macos-15 | supported |
| macos-14 | macos-14 | deprecating 2026-07-06 → unsupported 2026-11-02 |
| macos-13 | (removed 2025-12-04) | — |
| windows-latest | windows-2022 | — |

## Language runtimes

| Runtime | Constraint | Where |
|---|---|---|
| Rust | stable (unpinned) | ci.yml, release-crates.yml |
| Node.js | >= 22 | fossic-node/package.json engines (bump to reflect Node 18/20 EOL) |
| Python | >=3.12,<3.14 | fossic-py/pyproject.toml |

## Rust crate dependencies (fossic-node internal)

| Crate | Version | Notes |
|---|---|---|
| napi | 3 | Migrated from v2 in 1.8.4 cycle |
| napi-derive | 3 | Same as above |
| @napi-rs/cli (npm) | ^3 | Consistent v3 stack, native protocol |

## Python tooling

| Tool | Pin | Notes |
|---|---|---|
| maturin | build-system requires >=1.5,<2.0 | fossic-py/pyproject.toml |
| pyo3 | 0.29 | (verify against fossic-py/Cargo.toml before changes) |
| cibuildwheel Python build | 3.12, 3.13 | Drop cp311; matches requires-python |

## Deprecation watchlist

Items to bump before they become blockers:

| Item | Deadline | Action |
|---|---|---|
| macos-14 runner | 2026-11-02 (unsupported) | If pinned, migrate to macos-latest |
| Node 20 on legacy actions | Fall 2026 (removed from runners) | All bumps landed 2026-07-06 |


## How to update this file

- After every workflow file change → update Actions table
- After every published release → update Package versions table
- After every Rust/Python/Node dep bump → update Language runtimes or Rust dep tables
- Once per quarter → re-verify GitHub runner mappings (they migrate)
- Always web-search current versions before writing to this file — do not
  rely on training data or memory
