# Technical Debt

Running inventory of known issues, workarounds, and deferred improvements
in the fossic repository. Each entry documents what the debt is, why it
exists, the cost of leaving it, and the path to resolution.

Entries are ordered by area, not priority. Read the "Cost of leaving"
field to gauge urgency.

## Build & tooling

### fossic-node/index.d.ts historical tracking

**Location:** `fossic-node/index.d.ts`

**What it is:** `index.d.ts` was tracked in git despite being a build artifact
regenerated on every `npm run build`. Removed from tracking in the 1.8.3
release (Commit 2), added to `.gitignore`.

**Why it exists:** Pre-1.8.3 workflow assumed the file was durable. Under
napi v2 + the incorrect build script (`napi build --platform --release`
without `--js`/`--dts` flags), the file was generated once and preserved
across commits, giving the appearance of durable content.

**Cost of leaving:** Fully addressed in 1.8.3. Only listed here for
historical context.

**Path to fix:** Complete (Commit 2 of 1.8.3 release).

## CI infrastructure

### GitHub Actions Node.js runtime deprecation

**Location:** `.github/workflows/*.yml`

**What it is:** Multiple actions target Node.js 20 (`actions/checkout@v4`,
`actions/setup-python@v5`, `actions/setup-node@v4`, `actions/upload-artifact@v4`,
`actions/download-artifact@v4`). GitHub is forcing them to run on Node.js 24
because of Node 20's deprecation. Currently every workflow run emits:
`Node.js 20 is being deprecated. This workflow is running with Node 24 by default.`

**Why it exists:** Action versions were pinned before Node 24 support was
widely available. Bumping now requires verifying each action has a
Node 24-compatible release and testing behavior changes.

**Cost of leaving:** Currently just warnings. When GitHub fully deprecates
Node 20, workflows may fail entirely. Timeline uncertain.

**Path to fix:** Audit action versions against latest releases with
Node 24 support. Bump versions individually with verification between
each bump. Estimate: 1-2 hours.

### Rust toolchain unpinned

**Location:** repo root — no `rust-toolchain.toml`

**What it is:** CI uses `dtolnay/rust-toolchain@stable`, which picks whichever
Rust stable is current at CI-run time. Local development uses whatever version
the contributor has installed. Any Rust version bump could introduce new
clippy warnings that fail CI (`-D warnings`) without warning.

**Why it exists:** No explicit decision to pin was made. Default cargo behavior.

**Cost of leaving:** Occasional CI failures on Rust toolchain releases when
new clippy lints activate. Reproducing locally may require matching CI's
Rust version.

**Path to fix:** Add `rust-toolchain.toml` pinning to a specific stable
version (e.g., `1.96.0`). Update in a controlled way when new stable
releases pass local testing. Estimate: 15 minutes to add, ongoing discipline.

### Bench validation aspirational scaffolding

**Location:** `.github/workflows/bench-validation.yml`,
`benchmarks/baseline.json`

**What it is:** The workflow references `benchmarks/sqlite_wal_payload_sweep.py`
which has never been committed to git. `baseline.json` exists with expected
p99 numbers, but the script that would generate matching numbers was never
implemented. The workflow's schedule is disabled (as of 1.8.3 release);
manual `workflow_dispatch` triggers exit neutral (78) via a preflight check.

**Why it exists:** Bench workflow was scaffolded before the underlying
bench script was written. Script implementation was deferred and never
returned to.

**Cost of leaving:** No cost while schedule is disabled. If someone
manually dispatches, they get a neutral exit. Real cost is the missing
regression protection — no automated detection of p99 regressions.

**Path to fix:** Implement `benchmarks/sqlite_wal_payload_sweep.py` with the
5-scenario output schema `baseline.json` expects. Verify baseline numbers
reproduce on GitHub Actions runners (which may be too noisy for reliable
p99 measurement — may require alternate methodology like tmpfs or
statistical framing). Re-enable schedule. Estimate: multi-day project
with methodology validation.

## Version management

### fossic-similarity-hnsw path resolution for sdist

**Location:** `fossic-py/Cargo.toml`, `crates/fossic-similarity-hnsw/Cargo.toml`

**What it is:** After the 1.8.3 release, all workspace path deps in
fossic-py declare version fallbacks
(`fossic = { workspace = true, version = "1.8.3" }`) so that when maturin
builds an sdist, cargo can resolve the deps from crates.io if the workspace
isn't present. This works, but hardcodes the version, requiring a manual
bump every release.

**Why it exists:** Necessary for `pip install fossic --no-binary :all:` to
work. Without the fallback, sdist consumers see cargo resolution errors.

**Cost of leaving:** Every release requires bumping the version in every
`{ workspace = true, version = "..." }` declaration. If forgotten, sdist
builds break at the next release.

**Path to fix:** Investigate whether cargo has a way to declare
"workspace dep, use whatever the workspace version is" that's compatible
with sdist consumption. If yes, migrate. If no, accept the manual bump
as part of release process and document in CONTRIBUTING.md.

### v1.8.2 tag on wrong commit

**Location:** local git only (not on remote)

**What it is:** Prior to 1.8.3 release, a `v1.8.2` tag existed locally
pointing at commit `95b504c` (post-1.8.2-shipping cleanup work). The
actual commit that shipped as 1.8.2 is `9c5409e`. The misleading tag
was deleted during 1.8.3 release preparation.

**Why it exists:** Tag was created after 1.8.2 shipped, on a later commit
by mistake. Never pushed to remote (crates.io is the authoritative record).

**Cost of leaving:** Currently deleted, so no cost. If someone wants to
reconstruct v1.8.2 source, they need to use crates.io's archived .crate
file or commit `9c5409e`.

**Path to fix:** Optional — create a proper `v1.8.2` annotated tag on
commit `9c5409e` and push to remote for clean history. Estimate:
5 minutes.

## Documentation

### fossic-node index.d.ts type coverage

**Location:** `fossic-node/index.d.ts` (regenerated on build)

**What it is:** With the build.rs bridge in place (see version-skew entry),
`index.d.ts` is now generated with real content. However, it hasn't been
audited for completeness against the 66 `#[napi]` annotation sites. There
may be type gaps where certain attributes don't emit expected TypeScript.

**Why it exists:** Discovered as part of 1.8.3 release work. Full audit
of generated TypeScript surface wasn't in scope.

**Cost of leaving:** No known consumer surface today. If fossic-node is
ever brought into publish scope or consumed by TypeScript projects,
missing type declarations become a real user-facing issue.

**Path to fix:** After the napi v3 migration (see version-skew entry),
audit generated `index.d.ts` against Rust source. Any gaps indicate
attribute issues that need fixing on the Rust side. Estimate: 2-3 hours.

## Testing

### fossic-tauri Cargo.toml packaging scope

**Location:** `crates/fossic-tauri/Cargo.toml`

**What it is:** No `include` block. `cargo package -p fossic-tauri` bundles
everything in the crate directory, potentially including tests, dev
artifacts, or files not needed by consumers.

**Why it exists:** Not addressed in initial fossic-tauri crates.io publish.

**Cost of leaving:** Larger `.crate` file downloads. Potential surprise
for consumers wondering why unused files are in the package.

**Path to fix:** Add an `include` block matching root fossic's pattern:
`["src/**/*.rs", "Cargo.toml", "README.md", "LICENSE"]`. Verify with
`cargo package --list -p fossic-tauri`. Estimate: 15 minutes.

## Ecosystem coordination

### fossic-similarity-hnsw sdist limitation resolution

**Location:** Repo-wide, blocked by external consumer needs

**What it is:** fossic-similarity-hnsw is a shipping public API used by
fossic-py's Python `similarity` module (18 pytest tests, first-class
exports). It's now published on crates.io as of 1.8.3. Previously it
was `publish = false`, causing sdist consumption to fail.

**Why it exists:** Fully addressed in 1.8.3 release. Only listed for
historical context.

**Path to fix:** Complete (Commit 3-4 of 1.8.3 release).

## Adding new entries

When you find debt worth tracking:

1. Add an entry under the most-fitting section
2. Include: What, Why, Cost of leaving, Path to fix
3. Estimate effort where possible
4. Commit as its own hygiene commit with a message like `docs(tech-debt): add <topic>`

Debt worth logging: anything that works but isn't done right, anything
future-you would want to know, anything that's currently deferred without
a scheduled follow-up.

Not worth logging: standard bug backlog (use issues), style preferences,
"could be nicer" without a concrete rationale.
