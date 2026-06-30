# Build and Distribution

**Status:** v1 plan · 2026-06-12
**Scope:** How fossic is packaged, distributed, and consumed across Rust, Python, and Node.js. This document captures the operational side of v1 — the things that determine whether a consumer can actually run `cargo add` / `pip install` / `npm install` and have it work.

---

## 1. The friction principle

Every Python consumer profile flagged the same risk: "if `uv sync` stops working for a dev without Rust installed, we don't adopt." Cerebra called it a hard requirement. Policy Scout, Bo, and ai-stack each independently raised the same concern. This means **pre-built wheels are mandatory day one**, not a follow-up.

A consumer should be able to `pip install fossic` (or `npm install fossic` or `cargo add fossic`) on any of the supported platforms without:

- Installing a Rust toolchain
- Installing C/C++ build tools
- Configuring `LIBSQLITE` paths
- Setting up `maturin` or `napi-rs` CLI
- Reading more than one line of documentation

If any of these become necessary, we've failed the adoption test for at least one consumer.

---

## 2. Supported platforms

v1 ships pre-built artifacts for:

| Platform | Tier |
|---|---|
| Linux x86_64 (glibc 2.28+) | tier 1 |
| Linux aarch64 (glibc 2.28+) | tier 2 |
| macOS arm64 (Apple Silicon, macOS 12+) | tier 1 |
| macOS x86_64 (Intel Mac, macOS 12+) | tier 1 |
| Windows x86_64 (Windows 10+) | tier 1 |

**Tier 1** means: pre-built binary + integration tests run in CI + officially supported.
**Tier 2** means: pre-built binary published + smoke tests in CI + supported best-effort.

Out of scope for v1:
- Linux musl (Alpine) — would require static linking strategy decisions that we can defer
- Linux armv7 (32-bit ARM) — not requested by any current consumer
- Windows arm64 — not requested
- FreeBSD / OpenBSD / Solaris — not requested
- WASM — explicit non-goal (Section 1 of main spec)

The tier 2 designation for Linux aarch64 reflects that no current consumer has an aarch64 dev machine, but Apple Silicon Linux VMs and ARM cloud instances exist, and dropping aarch64 would surprise people. Wheels are built; CI coverage is smoke-only.

---

## 3. Rust crate (`fossic`)

Published to crates.io. Standard Cargo crate layout. Public re-exports of consumer-facing types from the crate root; internal modules organized by concern (`store`, `events`, `branches`, `snapshots`, `cce`, `agent_trace`).

```toml
[dependencies]
fossic = "1.0"
```

Optional features:

- `agent-trace` (default-on): the agent trace adapter and OTel exporter
- `tauri` (default-off): the `fossic-tauri` companion crate's content moved here as a feature for consumers that want a single crate
- `encryption` (default-off): SQLCipher support via `rusqlite/sqlcipher` feature
- `similarity` (default-off): the `SimilaritySearchProvider` trait declaration (without it, the trait isn't compiled)

Internal dependencies pinned with care:

- `rusqlite` — the SQLite binding. Pinned to a known-good major version; SQLite version pinned via `bundled` feature so consumers don't depend on system SQLite.
- `blake3` — for content addressing.
- `serde` + `rmp-serde` — for msgpack.
- `tokio` (optional via `tauri` feature) — for the subscription dispatcher and OTel exporter.

Documentation: rustdoc on every public item. Examples in `examples/`. The crate's README is the same as the main spec's "API surface" section but reformatted for crates.io discovery.

---

## 4. Python package (`fossic`)

Built with maturin. Published to PyPI as `fossic`.

### 4.1 Wheel matrix

| Tag | Platform | Python versions |
|---|---|---|
| `cp311-cp311-manylinux_2_28_x86_64` | Linux x86_64 | 3.11 |
| `cp312-cp312-manylinux_2_28_x86_64` | Linux x86_64 | 3.12 |
| `cp313-cp313-manylinux_2_28_x86_64` | Linux x86_64 | 3.13 |
| `cp311-cp311-manylinux_2_28_aarch64` | Linux aarch64 | 3.11 |
| `cp312-cp312-manylinux_2_28_aarch64` | Linux aarch64 | 3.12 |
| `cp313-cp313-manylinux_2_28_aarch64` | Linux aarch64 | 3.13 |
| `cp311-cp311-macosx_12_0_arm64` | macOS arm64 | 3.11 |
| `cp312-cp312-macosx_12_0_arm64` | macOS arm64 | 3.12 |
| `cp313-cp313-macosx_12_0_arm64` | macOS arm64 | 3.13 |
| `cp311-cp311-macosx_12_0_x86_64` | macOS x86_64 | 3.11 |
| `cp312-cp312-macosx_12_0_x86_64` | macOS x86_64 | 3.12 |
| `cp313-cp313-macosx_12_0_x86_64` | macOS x86_64 | 3.13 |
| `cp311-cp311-win_amd64` | Windows x86_64 | 3.11 |
| `cp312-cp312-win_amd64` | Windows x86_64 | 3.12 |
| `cp313-cp313-win_amd64` | Windows x86_64 | 3.13 |

15 wheels per release. Built via cibuildwheel in CI on every tag push.

Python 3.10 and earlier are not supported in v1. Every current Python consumer profile uses 3.11+ (Cerebra is 3.12+, Policy Scout is 3.12+, Bo is 3.12, ai-stack uses 3.12). Building for 3.10 adds a wheel-row dimension we don't need.

PyPy: not supported in v1. No consumer requested it; building for PyPy adds substantial CI complexity. If a consumer asks, we add it.

### 4.2 Source distribution

A source distribution (`fossic-1.0.0.tar.gz`) is published alongside the wheels. The sdist requires a Rust toolchain (`rustc 1.75+`) and `maturin` to build. The sdist is the fallback only — wheels cover every supported platform, so reaching the sdist path means the user is on an unsupported platform.

The sdist's `pyproject.toml` declares the build-system requirement explicitly so `pip install fossic` on an unsupported platform produces a clear error message ("install a Rust toolchain to build from source") rather than a cryptic compile failure.

### 4.3 Companion packages

Published to PyPI:

- `fossic-aio` — asyncio wrapper around the synchronous core for async consumers (Bo, LiteLLM). Thin layer using `asyncio.to_thread`. Same wheel matrix.
- `fossic-cli` — command-line tool for inspecting fossic stores. Pure Python depending on `fossic`. Not platform-specific.

---

## 5. Node.js package (`fossic`)

Built with napi-rs CLI. Published to npm as `fossic`.

### 5.1 Prebuilt binary matrix

napi-rs's standard prebuilt distribution: the main `fossic` package depends on platform-specific subpackages, only one of which is installed per consumer based on their platform.

Subpackages published:

- `fossic-linux-x64-gnu`
- `fossic-linux-arm64-gnu`
- `fossic-darwin-arm64`
- `fossic-darwin-x64`
- `fossic-win32-x64-msvc`

Node.js versions: 20 LTS, 22 LTS, 24 (current). N-API version stability means one binary per platform covers all three. Bun and Deno: best-effort — the napi-rs binary works with Bun via its N-API support; Deno requires the `--unstable-ffi` flag. Documented but not officially supported.

### 5.2 Installation

```bash
npm install fossic
# Resolves to fossic + the matching prebuilt subpackage for the user's platform.
```

If no matching prebuilt is available (unsupported platform), `npm install` fails with a clear error rather than attempting to build. The fallback build path requires Rust toolchain + napi-rs CLI; consumers on unsupported platforms can take that path manually.

### 5.3 Bundling

The napi-rs binary is a `.node` file (Node.js native addon). Consumers that bundle their applications (Webpack, esbuild, Vite for Node targets) must include the `.node` file in their output. This is documented in the README with example configurations for each bundler.

For Tauri consumers: this is **not relevant** to the Tauri webview path (the webview doesn't use napi-rs — see main spec Section 4.4). The `fossic-tauri` companion crate is the Tauri integration; Node.js bundling concerns only matter for consumers that run fossic in a Node.js process.

---

## 6. CI pipeline

A single CI workflow drives every release. Triggered on tag push (`v1.0.0`, `v1.0.1`, etc.).

### 6.1 Workflow stages

1. **Verify** — `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, run the test vector suite for CCE conformance.
2. **Build Rust crate** — `cargo build --release --all-features` on every tier-1 platform; publish dry-run to crates.io.
3. **Build Python wheels** — cibuildwheel for every wheel-matrix row; run the Python integration test suite against each wheel.
4. **Build Node prebuilts** — napi-rs build for every prebuilt-matrix row; run the Node integration test suite against each prebuilt.
5. **Publish** — on green, publish to crates.io, PyPI (via twine), and npm (via npm CLI). Each publish step is gated on the previous succeeding to avoid partial releases.

A failure at any stage aborts the entire release. There is no "publish Python but not Node" mode — fossic is one library across three languages and the artifacts ship as one release.

### 6.2 Reproducible builds

`cargo build --release` is reproducible by default for Rust ≥ 1.75. The wheels are not strictly reproducible (cibuildwheel does not currently produce identical wheels across runs), but the embedded binary is reproducible — verified by extracting the `.so`/`.dylib`/`.dll` from each wheel and comparing.

A SLSA-3 attestation is generated for each release artifact and published as a sigstore signature. Consumers that pin fossic in security-sensitive contexts (Policy Scout's audit substrate qualifies) can verify the attestation chain.

### 6.3 Pre-release validation

Before tagging, a pre-release validation runs:

- The 40 KB payload bench (`benchmarks/sqlite_wal_payload_sweep.py`) against the candidate build. Regression > 10% on p99 total_us blocks the release.
- The full CCE test vector suite against all three language implementations.
- The "consumer profile compatibility" suite: a synthetic consumer for each of the five v1 consumer profiles, running a representative event volume against the candidate build.

If any of these fail, the release does not tag.

---

## 7. Versioning

Semantic versioning. Major.Minor.Patch.

- **Patch** (1.0.x): bug fixes only, no API changes, no schema changes.
- **Minor** (1.x.0): additive API surface, additive schema columns (with auto-migration), additive event types in the agent trace vocabulary, new optional features. Backwards-compatible at the read-events level.
- **Major** (x.0.0): breaking API changes, breaking schema changes, removal of features, changes to CCE encoding.

Schema migrations live in the `migrations/` directory of the crate and run automatically on `Store::open` if the stored `fossic_schema_version` in the `meta` table is older than the crate's version. Migrations are forward-only — there is no rollback path. A failed migration leaves the store with the prior schema version and surfaces an error to the consumer.

Backporting fixes: only the latest minor version receives patch releases. v1.5.x lives, v1.4.x does not. If a consumer needs to stay on v1.4 for compatibility reasons, they pin and accept that no further fixes will land there.

---

## 8. Distribution timeline (v1 release plan)

Approximate, not a commitment. Drives the build infrastructure work prior to v1.0 tag.

| Milestone | Deliverable |
|---|---|
| T-8 weeks | Rust crate API frozen at v1.0-rc.1. Internal API may still change. |
| T-6 weeks | PyO3 binding API frozen at v1.0-rc.1. Wheel matrix builds successfully in CI. |
| T-4 weeks | napi-rs binding API frozen at v1.0-rc.1. Prebuilt matrix builds successfully in CI. |
| T-2 weeks | Full CI pipeline green on three consecutive RC builds. Consumer profile compatibility suite green for all five v1 profiles. |
| T-0 | Tag v1.0.0, run release pipeline, announce. |
| T+2 weeks | Post-release patch window. Bug fixes only. |

Cerebra, Policy Scout, Bo, LumaWeave, and ai-stack/claw-code integrations begin after v1.0.0 ships. The v1.0 → v1.1 window incorporates feedback from those integrations.

---

## 9. Consumer integration paths

For each v1 consumer, the expected first-integration path:

**Cerebra** — `pip install fossic`, register lattice node reducers against `cerebra/lattice/*`, register the read adapter for `inspector_events`. Existing Python sync code stays sync.

**Policy Scout** — `pip install fossic`, register the redact_dict payload transform against `policy-scout/audit`, write audit events via fossic instead of directly to `audit.db`. The existing `audit.db` becomes a fossic store with the v1 schema migration.

**Bo** — `pip install fossic fossic-aio`, write conversation events through the async wrapper. No reducers in v1; just durable conversation log.

**LumaWeave** — add `fossic-tauri` to `src-tauri/Cargo.toml`, call `register_commands(app)` in the setup hook, use `invoke("fossic_*", ...)` from the frontend. Add `listen("fossic:event")` for the live diff layer.

**ai-stack / claw-code** — `cargo add fossic` for the Rust core. The Python companion uses `pip install fossic`. LiteLLM stays on its current event log until the `fossic-http-gateway` v1.1 companion ships.

Each integration is documented in a per-consumer integration guide that ships in `docs/integrations/` of the fossic repository.

---

*End of build and distribution.*
