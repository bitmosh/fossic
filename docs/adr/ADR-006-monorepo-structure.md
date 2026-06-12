# ADR-006: Monorepo Structure from Phase 0

**Status:** Accepted
**Date:** 2026-06-11
**Deciders:** Developer (bitmosh)
**Related ADRs:** [ADR-001](ADR-001-mcp-transport-and-trust-model.md) (eval-core used by Policy Scout eval pipeline), [ADR-003](ADR-003-graph-export-contract.md) (eval-core is the shared eval infrastructure for all modules), [ADR-005](ADR-005-registry-coverage-expansion.md) (eval coverage contract depends on eval-core being importable across projects)

---

## Context

The Lattica platform consists of six active modules — LumaWeave (TypeScript/Rust), Cerebra (Python), Policy Scout (Python), the ES toolkit (Rust + Python + TypeScript bindings), the discord-bot Bo (Python), and Rhyzome (Python) — each currently living as an independent repository with no shared infrastructure.

Three forcing functions make this untenable beyond Phase 2:

**eval-core.** Policy Scout already has a working evaluation harness. ADR-003 extracted it into a standalone `eval-core/` package (stdlib-only, zero runtime deps) so Cerebra, Rhyzome, and bons.ai can all use the same evaluation primitives. Without a monorepo, each project consumes eval-core via a `pip install -e /path/to/eval-core` path-install, or worse, a copy-paste. Path installs break in CI and on a second machine. Copy-paste means fixes in eval-core never propagate.

**ES toolkit.** The event sourcing toolkit (ADR-002) has three language targets: a Rust core crate, PyO3 Python bindings, and napi-rs TypeScript bindings. All three must stay at the same version — a Python consumer importing `lattica_es` version 0.3 must be calling the same Rust core that the TypeScript consumer links against version 0.3. Co-versioning three packages across three separate repos requires either a synchronization bot or discipline that will eventually fail.

**Shared IPC types.** The Tauri application defines an IPC contract between its Rust backend and its TypeScript React frontend. If these live in separate files that are edited independently, type drift is silent — the Rust handler expects one payload shape, the TypeScript caller sends another, and no compiler catches it until runtime. A single `lattica-ipc-types/` package that both sides import is the structural fix.

The question is not whether to unify — it's when and how.

---

## Forces

- **eval-core is the immediate forcing function.** Three projects need it importable before Phase 3 ends. Every day the monorepo is deferred, one more path-install hack accumulates somewhere.
- **Migration tax compounds.** Moving a project into a monorepo after it has CI pipelines, lockfiles, and GitHub Actions referencing absolute paths costs more per project the later you do it. Phase 0 is the cheapest moment.
- **uv workspace support is still maturing.** uv workspaces (PEP 518 + pyproject.toml-based) are the correct Python tool for this, but uv's workspace feature set is newer than pip's. There is some risk of hitting edge cases with cross-member editable installs.
- **Python projects have heterogeneous constraints.** Policy Scout runs on Python 3.12+ and has no ML dependencies. Cerebra requires torch-compatible numpy, CUDA-aligned scipy, and constrained BLAS versions for the RTX 4070 SUPER. These constraint sets are incompatible in a single shared virtualenv. Workspace members must be able to have distinct lock files.
- **The monorepo must not force premature coupling.** Rhyzome, bons.ai, and Bo are still in active evolution. Adding them to the monorepo should not change their import paths, CI setup, or development workflow until they are ready. Workspace membership is progressive, not mandatory.
- **Rust workspace is already a known pattern.** Cargo workspaces are mature, well-documented, and the ES toolkit crate and the Tauri backend crate naturally belong in one.

---

## Decision

### D1 — Full monorepo from Phase 0 (pnpm workspaces for TypeScript/Rust, uv workspaces for Python)

Option 3: a single `lattica/` root containing pnpm workspaces for all TypeScript and Rust packages, and uv workspaces for all Python packages. The monorepo is established in Phase 0 with only the packages needed by Phase 0 and Phase 1; remaining modules join progressively.

**Rejected: Polyrepo (Option 1).** Leaves the eval-core distribution problem unsolved. Every module that wants eval-core gets a path-install or a copy. When eval-core is updated, each project is updated separately or falls behind. The IPC type drift problem has no structural solution.

**Rejected: Partial monorepo — TypeScript/Rust only, Python separate (Option 2).** Solves the IPC type problem and ES toolkit co-versioning, but leaves eval-core as a path-install across all Python modules. The split also creates two mental models for "where does this package live," which adds cognitive overhead with no commensurate benefit.

### D2 — Directory structure

```
lattica/
  packages/                         # shared libraries (multiple consumers)
    eval-core/                      # Python — shared evaluation infrastructure (ADR-003)
      pyproject.toml
      src/
        lattica_eval_core/
    event-store/                    # ES toolkit — three-language package (ADR-002)
      core/                         # Rust crate (rusqlite, blake3)
        Cargo.toml
        src/
      python/                       # PyO3 Python bindings
        pyproject.toml
        src/
          lattica_es/
      typescript/                   # napi-rs TypeScript bindings
        package.json
        src/
    lattica-ipc-types/              # TypeScript + Rust — shared IPC type definitions
      package.json
      src/
        index.ts
      rust/
        Cargo.toml
        src/
          lib.rs
  apps/
    lattica/                        # main Tauri application (former LumaWeave)
      package.json
      src-tauri/
        Cargo.toml
        src/
      src/
  modules/                          # platform modules — added progressively
    policy-scout/                   # Python workspace member (joins in Phase 2)
    cerebra/                        # Python workspace member (joins in Phase 3)
    discord-bot/                    # Python workspace member (joins in Phase 3+)
    rhyzome/                        # Python workspace member (joins in Phase 8)
    ai-stack/                       # Docker Compose only — config files, no package
  docs/
    DESIGN.md
    PHASES.md
    EVENT_FABRIC.md
    adr/
      ADR-001.md
      ADR-002.md
      ...
  CLAUDE.md
  Cargo.toml                        # Rust workspace root
  pnpm-workspace.yaml
  pyproject.toml                    # uv workspace root
  package.json                      # root package.json (scripts only, not published)
```

### D3 — pnpm workspace configuration

`pnpm-workspace.yaml` at the repository root:

```yaml
packages:
  - "apps/*"
  - "packages/lattica-ipc-types"
  - "packages/event-store/typescript"
```

`packages/lattica-ipc-types/package.json`:
```json
{
  "name": "@lattica/ipc-types",
  "version": "0.1.0",
  "private": true,
  "main": "dist/index.js",
  "types": "dist/index.d.ts"
}
```

`apps/lattica/package.json` declares the workspace dependency:
```json
{
  "dependencies": {
    "@lattica/ipc-types": "workspace:*",
    "@lattica/event-store": "workspace:*"
  }
}
```

The root `package.json` holds only workspace-level scripts (`dev`, `build`, `typecheck`, `test`) and is marked `"private": true`. It is never published.

### D4 — Rust workspace configuration

`Cargo.toml` at the repository root:

```toml
[workspace]
resolver = "2"
members = [
  "apps/lattica/src-tauri",
  "packages/lattica-ipc-types/rust",
  "packages/event-store/core",
]

[workspace.dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
blake3    = "1.5"
serde     = { version = "1", features = ["derive"] }
serde_json = "1"
```

Workspace-level `[workspace.dependencies]` are declared once and pinned at the root. Individual crates inherit via `rusqlite.workspace = true`. This prevents version skew between the ES toolkit core and the Tauri backend.

`packages/lattica-ipc-types/rust/Cargo.toml` declares the types crate as a workspace member so both `apps/lattica/src-tauri` and `packages/event-store/core` can share its type definitions without duplication.

### D5 — uv workspace configuration

Root `pyproject.toml`:

```toml
[tool.uv.workspace]
members = [
  "packages/eval-core",
  "packages/event-store/python",
]
# modules/ members are added here as each module joins — see Migration Path

[tool.uv]
dev-dependencies = [
  "pytest>=8.0",
  "mypy>=1.10",
]
```

Each Python workspace member declares its own dependencies in its own `pyproject.toml`. Members do not share a single virtual environment — each gets its own, managed by uv. Cross-member dependencies use workspace-local references:

```toml
# modules/policy-scout/pyproject.toml (added in Phase 2)
[project]
name = "policy-scout"
dependencies = [
  "lattica-eval-core",
]

[tool.uv.sources]
lattica-eval-core = { workspace = true }
```

The `workspace = true` source directive tells uv to resolve `lattica-eval-core` from `packages/eval-core/` rather than PyPI. This is the canonical way to share packages within a uv workspace and requires no path-install hacks.

Each module that has CUDA-sensitive or ML-specific dependencies (Cerebra) manages its own lock file via `uv lock --package cerebra`. The root workspace lock covers only packages and non-ML modules.

### D6 — What stays outside the monorepo

**bons.ai / AI-lab.** Still experimental — three-agent loop, RL/bandit, ChromaDB. The architecture may change substantially before Phase 8. It joins the monorepo when its interfaces stabilize. Until then, it consumes eval-core via a path-install that is explicitly documented as temporary.

**LumaShell.** Benched. Four UX patterns have been absorbed as design references (ADR-007). The repository is archived; it is not added to the monorepo.

**blog.bumper.** Standalone npm package already published to a public registry under a different identity. It has its own release cycle and does not depend on any Lattica package. It stays separate.

**External tooling.** Grafana, Prometheus, Ollama, Open WebUI — these are infrastructure services configured in `modules/ai-stack/` as Docker Compose files. They have no package representation in the monorepo.

### D7 — Invariants this structure enforces

- A TypeScript file cannot import from `@lattica/ipc-types` at a different version than the Rust backend exports — both are members of the same pnpm workspace and both reference the same `packages/lattica-ipc-types/` directory.
- The eval-core package cannot have a runtime dependency added without it being visible in the root `pyproject.toml` and failing the zero-runtime-deps contract (ADR-003).
- The ES toolkit's Rust core, Python bindings, and TypeScript bindings share a single version number surfaced in the root `Cargo.toml` workspace; bumping the Rust crate version without updating the bindings is caught by the workspace version check in CI.
- A module that references `eval-core` via `workspace = true` will fail to resolve if eval-core is not in the workspace members list — the dependency is structurally declared, not a documentation convention.

---

## Migration Path

Existing projects are standalone repositories. Phase 0 creates the `lattica/` monorepo skeleton and adds only the packages needed immediately. Modules join progressively by phase:

| Phase | What joins | What the join entails |
|---|---|---|
| **Phase 0** | `apps/lattica` (LumaWeave rename), `packages/eval-core`, `packages/lattica-ipc-types` | Tauri project init; eval-core extracted from Policy Scout ad-hoc copy; IPC types skeleton from existing `invoke()` calls |
| **Phase 0** | `packages/event-store/` skeleton | Core, python/, typescript/ directories created with placeholder `lib.rs` and `__init__.py`; built out across Phases 6 and 8 |
| **Phase 1** | `modules/ai-stack/` | Docker Compose files only — no package, no pyproject.toml; just config |
| **Phase 2** | `modules/policy-scout/` | `pyproject.toml` updated to `workspace = true` for eval-core; existing test suite runs unmodified |
| **Phase 3** | `modules/cerebra/` | Same pattern; separate lock file for ML deps |
| **Phase 3+** | `modules/discord-bot/` | Same pattern |
| **Phase 8** | `modules/rhyzome/` | Same pattern; bons.ai deferred until interface stabilizes |

Each module join is a standalone commit. No module is required to change its internal import paths on join — the workspace source directive handles resolution transparently.

---

## Consequences

### Positive

- eval-core is importable by any Python workspace member with `from lattica_eval_core import ...` — no path hacks, no copies, no version drift. One change in `packages/eval-core/` propagates to all consumers on next `uv sync`.
- IPC type drift between the Tauri Rust backend and the React TypeScript frontend becomes a compile error, not a runtime surprise.
- ES toolkit co-versioning is enforced by the Rust workspace and pnpm workspace simultaneously. You cannot ship Python bindings that call a different Rust ABI version than the TypeScript bindings.
- CI can run `pnpm typecheck` and `cargo check` at the root and get coverage across all packages and apps in one invocation.
- The `docs/adr/` directory lives at the monorepo root and documents decisions that span modules — there is one canonical ADR log, not one per module.

### Negative / Risks

- **uv workspace maturity.** uv workspaces are newer than Poetry workspaces or pip editable installs. Edge cases exist, particularly around cross-workspace editable installs and lock file behavior when a member has CUDA-pinned transitive dependencies. Mitigation: test the workspace setup end-to-end in Phase 0 before any module depends on it.
- **Python projects with incompatible constraint sets.** Cerebra's ML dependencies (torch, numpy, CUDA-pinned scipy) conflict with the clean stdlib-only constraint of eval-core. uv's per-member virtual environments handle this, but a developer who runs `uv sync` from the root without understanding member isolation may get a confused environment. Mitigation: document the per-member sync pattern in CLAUDE.md; add a root `uv sync --package eval-core` script for the common case.
- **Monorepo CI complexity.** A CI pipeline that runs `pnpm test` at the root runs tests for every TypeScript package. Touched-path filtering (only run tests for packages affected by a given commit) requires a change detection layer. For the current scale (one developer, six modules), full-suite CI is acceptable. Filtering is deferred until build times justify it.
- **Merge conflicts when multiple modules are active simultaneously.** If Policy Scout and Cerebra are both under active development, a `pnpm-workspace.yaml` or root `pyproject.toml` change from one can conflict with the other. Mitigation: root config files are low-churn by design; most work happens in module subdirectories. This is acceptable friction.
- **Module joins require a brief migration commit.** Adding a module to the workspace requires updating the root `pyproject.toml` members list and replacing any path-installs in the module's `pyproject.toml` with `workspace = true` sources. This is a small but real coordination cost at each phase boundary.

---

## Notes

The `modules/ai-stack/` directory is a special case: it contains only Docker Compose files and configuration, no Python or TypeScript package. It is included in the monorepo because its `docker-compose.yml` references environment variables and network names that are also referenced by Lattica's `apps/lattica/` Prometheus polling adapter. Co-locating them makes the shared configuration surface explicit.

The `gwells` physics engine is embedded in `apps/lattica/src/` for now. The long-term intent (noted in the architecture context) is to extract it to a standalone npm package. When that extraction happens, it becomes `packages/gwells/` in the pnpm workspace — no structural change to the monorepo is needed, only a new workspace member declaration.