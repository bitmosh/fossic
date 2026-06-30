---
pass: 7
version: v0.7.0
sha: c65eab9
date: 2026-06-12
summary: CI pipeline, wheel builder, release workflow, mdBook documentation scaffold
---

# Blast Radius — Pass 7 (v0.7.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content completely rewritten — original bootstrap estimate described upcasters,
> transforms, cursors, and cross-stream queries (which are v0.4.0). Actual commit
> c65eab9 added CI workflows, build scripts, release automation, and docs scaffold.

## Files

### Created

12 files in commit c65eab9:

**GitHub Actions:**
- `.github/workflows/ci.yml` — Rust test matrix + fossic-py pytest on every push and PR; lint and format checks
- `.github/workflows/release.yml` — automated wheel and .node artifact publishing on version tags; cibuildwheel for manylinux and musllinux; napi-rs cross-compilation
- `.github/workflows/bench-validation.yml` — scheduled benchmark regression guard (runs sqlite_wal_contention.py, compares to baseline.json)

**Build scripts:**
- `scripts/build-napi.sh` — local napi-rs build helper for fossic-node
- `scripts/build-wheels.sh` — local cibuildwheel invocation for fossic-py
- `scripts/consumer-profile-test.sh` — integration test runner exercising all consumer binding surface areas
- `scripts/verify-attestation.sh` — SLSA provenance check for released artifacts

**Release config:**
- `.cibuildwheel.toml` — cibuildwheel configuration: manylinux2014 + musllinux, Python 3.12 only, test command

**Documentation:**
- `docs/DESIGN.md` — platform architecture and module interaction model; fossic's role in the Lattica monorepo
- `docs/book.toml` — mdBook configuration
- `docs/src/SUMMARY.md` — mdBook table of contents
- `docs/src/introduction.md` — mdBook introduction chapter

---

## Public APIs

### Added

None — infrastructure-only pass. No new fossic Rust or Python APIs.

---

## Schema changes

None.

---

## Configuration changes

None at fossic core level.

---

## Dependency changes

None in fossic Rust or Python packages. CI configuration adds build tooling as system deps (napi-rs CLI, cibuildwheel) but these are not runtime dependencies.

---

## Behavior changes

None — no runtime code changes. Infrastructure only.

CI now runs:
- `cargo test` for fossic core (Rust)
- `pytest fossic-py/tests/` for Python binding (requires pre-built .so)
- `bench-validation.yml` enforces latency regression via baseline.json comparison (weekly schedule)

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
