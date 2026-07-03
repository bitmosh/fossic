# ADR-003: eval-core as Standalone Package

> **Historical note:** This record references Rhyzome, bons.ai, Bo, and discord-bot. These do not represent active Lattica ecosystem members — their appearance here reflects early planning context that did not materialize. The decision rationale remains accurate.

**Status:** Accepted
**Date:** 2026-06-11

---

## Context and Problem Statement

Multiple Lattica modules need evaluation infrastructure, and they need it to share a result schema. Policy Scout has the most mature eval system today: 44 test cases, CI-gated regression baseline, 7 assertion types, and a run harness that emits a structured `EvalSummary`. Cerebra has the richest *requirements*: SKU classification accuracy, retrieval quality scoring, abstention calibration, and LoRA training quality — none of which exist yet. discord-bot (Bo) and ai-stack have eval needs (tier routing correctness, alias resolution smoke tests) but currently have nothing at all.

The Lattica dashboard — starting in Phase 10 — needs to aggregate pass rates, regression trends, and cross-module correlations. That aggregation is only possible if every module writes to the same `EvalResult`/`EvalSummary` schema. If each module grows its own eval framework independently, the dashboard must either translate between four incompatible schemas or the cross-module correlation layer cannot be built at all.

A second problem is dependency direction. Policy Scout and Cerebra are peer modules in the monorepo. Neither should own what the other imports for CI. If eval-core lives inside Policy Scout, Cerebra takes a dependency on a governance tool. If it lives inside Cerebra, Policy Scout takes a dependency on a memory tool. Both arrangements are semantically wrong and create fragile coupling.

---

## Decision Drivers

- **Unified schema requirement.** The Phase 10 Lattica evaluation dashboard requires a single `EvalSummary` schema across all modules. Cross-module correlation (e.g., detecting when Cerebra retrieval quality and bons.ai mutation pass rate drop together) is impossible without it.
- **Dependency direction.** Neither Policy Scout nor Cerebra should own infrastructure the other imports. Shared infrastructure belongs in a neutral package.
- **Minimal blast radius.** eval-core should be small enough that any module can adopt it without a significant investment. If the package requires pulling in a test framework, an HTTP client, or a data-science library, adoption friction rises and modules will avoid it.
- **CI gateability.** The package must be importable in a fresh virtual environment with no internet access. Zero runtime dependencies (beyond stdlib and PyYAML, which is already a universal dep in the Python monorepo) is the requirement.
- **Extractability.** Policy Scout's existing eval logic must be liftable into the package without modification. If the extraction requires rewriting the logic, it is evidence that Policy Scout's eval is entangled with Policy Scout internals and the entanglement should be resolved in Policy Scout first.

---

## Considered Options

### Option 1: Policy Scout as owner

eval-core lives at `lattica/policy_scout/eval_core/` or is re-exported from `lattica.policy_scout`. Other modules import it from the Policy Scout package.

**Pros:**
- No new package to maintain. The logic already exists.
- Policy Scout's eval is the most mature; letting it set the interface is reasonable.

**Cons:**
- Cerebra, Rhyzome, bons.ai, and Bo would all take a runtime dependency on `lattica-policy-scout` to import eval infrastructure. A governance daemon is in every module's dependency tree.
- Any breaking change to Policy Scout's package (a schema migration, a rename, a dependency bump) could break CI for all other modules simultaneously.
- When the Phase 10 dashboard aggregates eval results, it is importing from a governance tool — the abstraction is wrong at every call site.
- The dependency graph is inverted: modules being *governed* by Policy Scout also *depend* on it.

**Verdict:** Rejected. The dependency inversion is architecturally unsound and will create maintenance problems at scale.

---

### Option 2: Cerebra as owner

eval-core lives inside Cerebra (`lattica/cerebra/eval_core/`), because Cerebra has the richest eval requirements and the most sophisticated quality metrics.

**Pros:**
- Cerebra's LoRA quality metrics, retrieval scoring, and calibration tooling are natural inputs to eval-core's design.
- Cerebra is not a governance tool, so the semantic mismatch from Option 1 is avoided.

**Cons:**
- Policy Scout, Rhyzome, bons.ai, and Bo would take a dependency on `lattica-cerebra` — a memory/knowledge module — to import test infrastructure. This is equally semantically wrong, just in a different direction.
- Cerebra has significant runtime dependencies (SQLite, embedding model, vector search). eval-core's zero-dependency requirement would need to be enforced as a sub-package isolation rule, which is fragile.
- If Cerebra's API changes (as it will — multiple phases involve Cerebra schema evolution), every module's CI import path breaks.
- Cerebra is one of the modules that *uses* eval-core. The owner of a shared primitive should not be one of its primary consumers.

**Verdict:** Rejected. Same structural problem as Option 1, different direction.

---

### Option 3: Standalone package (selected)

eval-core is extracted to `lattica/eval-core/` as a first-class package in the Lattica monorepo, with its own `pyproject.toml`, declared in the uv workspace, importable as `from lattica.eval_core import ...`. No module owns it; it has no dependencies on any module.

**Pros:**
- Dependency direction is correct. Every module that uses eval-core takes a dependency on a shared primitive, not on a peer module.
- The zero-runtime-dependency constraint is enforced at the package boundary. `pyproject.toml` has no `dependencies` beyond stdlib. PyYAML is listed as an optional extra (`[yaml]`) since all existing Python packages already include it; importing the YAML DSL loader without the extra fails with a clear error.
- Extractability is verifiable: `pip install -e lattica/eval-core` in a fresh venv with no other Lattica packages installed should work and all tests should pass.
- The package is small by constraint: `<300 lines`, single-concern, no inheritance hierarchy, no plugin system. It should never need to grow beyond its stated scope.
- Phase 10's `eval_results.db` writer can live here, giving all modules a single, WAL-safe SQLite writer they can call without reimplementing it.

**Cons / Risks:**
- A new package adds monorepo overhead: another `pyproject.toml`, another uv workspace declaration, another test suite to keep green. For a `<300 line` package this is minimal but real.
- Versioning: any schema change to `EvalResult` or `EvalSummary` requires all adopting modules to be updated simultaneously, or a compatibility shim must be maintained. This is the correct tradeoff (a shared schema is the goal), but it means schema changes are coordinated changes, not local ones.
- If Policy Scout's eval logic *is* entangled with Policy Scout internals, the extraction in Phase 3 will surface that entanglement and require more refactoring than anticipated. This is a pre-condition to verify before Phase 3 begins, not a reason to choose a different option.

**Verdict:** Accepted.

---

## Decision Outcome

Option 3: `lattica/eval-core/` as a standalone package in the Lattica monorepo.

---

## What eval-core Contains

The package scope is intentionally narrow. Everything not listed here is out of scope.

**Core types (`lattica/eval_core/types.py`):**

```python
@dataclass
class EvalCase:
    id: str
    description: str
    input: Any
    expected: Any
    tags: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

@dataclass
class EvalResult:
    case_id: str
    passed: bool
    actual: Any
    score: float | None          # 0.0–1.0; None for binary pass/fail cases
    assertion_failures: list[str]
    duration_ms: float
    timestamp: str               # ISO 8601

@dataclass
class EvalSummary:
    suite_id: str
    module: str
    run_at: str                  # ISO 8601
    total: int
    passed: int
    failed: int
    pass_rate: float             # passed / total
    mean_score: float | None     # mean of non-None EvalResult.score values
    results: list[EvalResult]
    regression_vs_baseline: RegressionReport | None  # None if no baseline exists yet

@dataclass
class RegressionReport:
    baseline_pass_rate: float
    current_pass_rate: float
    delta: float                 # current - baseline; negative = regression
    regressed: bool              # True if delta < -threshold
    threshold: float             # the threshold used
    new_failures: list[str]      # case IDs that failed this run but passed in baseline
```

**Runner (`lattica/eval_core/runner.py`):**

```python
def run_eval_suite(
    cases: list[EvalCase],
    evaluator_fn: Callable[[EvalCase], EvalResult],
    suite_id: str,
    module: str,
    *,
    baseline: EvalSummary | None = None,
    regression_threshold: float = 0.05,
) -> EvalSummary:
    ...
```

`evaluator_fn` is a pure function: `(EvalCase) -> EvalResult`. The runner calls it for each case, collects results, computes the summary, and optionally compares against a baseline. The runner does not write to disk — callers decide what to do with the `EvalSummary`.

**YAML DSL loader (`lattica/eval_core/loader.py`, requires `[yaml]` extra):**

```python
def load_cases_from_yaml(path: str | Path) -> list[EvalCase]:
    ...
```

YAML schema:

```yaml
suite_id: policy-scout-shell-checks
module: policy_scout
cases:
  - id: allow-git-status
    description: "git status is always allowed"
    input:
      tool: shell
      command: "git status"
    expected:
      decision: allow
    tags: [shell, git]
  - id: deny-rm-rf
    description: "rm -rf / is always denied"
    input:
      tool: shell
      command: "rm -rf /"
    expected:
      decision: deny
    tags: [shell, destructive]
```

**Baseline persistence (`lattica/eval_core/baseline.py`):**

```python
def save_baseline(summary: EvalSummary, path: str | Path) -> None:
    ...

def load_baseline(path: str | Path) -> EvalSummary | None:
    # Returns None if the file does not exist (first run).
    ...

def compare_to_baseline(
    current: EvalSummary,
    baseline: EvalSummary,
    threshold: float = 0.05,
) -> RegressionReport:
    ...
```

Baseline files are plain JSON. The format is a serialized `EvalSummary` (excluding the `regression_vs_baseline` field, which is always `None` in a stored baseline). Each module stores its baseline at a path it controls; eval-core does not dictate paths.

**`eval_results.db` writer (`lattica/eval_core/db.py`):**

```python
def write_summary_to_db(summary: EvalSummary, db_path: str | Path) -> None:
    ...
```

Creates the database and schema if they do not exist. Uses WAL mode. Safe for concurrent writers from multiple modules. The schema:

```sql
CREATE TABLE eval_runs (
    id          TEXT PRIMARY KEY,   -- suite_id + run_at, concatenated
    module      TEXT NOT NULL,
    suite_id    TEXT NOT NULL,
    run_at      TEXT NOT NULL,      -- ISO 8601
    total       INTEGER NOT NULL,
    passed      INTEGER NOT NULL,
    pass_rate   REAL NOT NULL,
    mean_score  REAL,               -- NULL if no scored cases
    regressed   INTEGER,            -- NULL if no baseline; 0 or 1 otherwise
    delta       REAL                -- NULL if no baseline
);

CREATE TABLE eval_case_results (
    run_id          TEXT NOT NULL REFERENCES eval_runs(id),
    case_id         TEXT NOT NULL,
    passed          INTEGER NOT NULL,
    score           REAL,
    duration_ms     REAL NOT NULL,
    assertion_json  TEXT            -- JSON array of assertion_failures
);
```

The Lattica Phase 10 dashboard reads this database. eval-core owns the schema; the dashboard is a reader, not a writer.

---

## Positive Consequences

- **Correct dependency direction.** No module's CI depends on a peer module. Policy Scout, Cerebra, Rhyzome, bons.ai, and Bo all take a dependency on a shared primitive that has no opinion about any of them.
- **Phase 10 dashboard is possible.** A single `eval_results.db` schema means the dashboard can aggregate, trend, and correlate without translation layers.
- **Cross-module regression gating is possible.** A monorepo root CI step can read `eval_results.db` and fail if any module regresses. This would not be possible if each module used its own schema.
- **Policy Scout extraction is a phase gate check.** If Policy Scout's eval logic is not extractable without modification, that is a signal to clean up Policy Scout first. The extraction pressure improves the source codebase.
- **Zero-dep constraint forces discipline.** A package that cannot import anything except stdlib (and optionally PyYAML) cannot accumulate logic it shouldn't own. If someone proposes adding an HTTP client to eval-core to fetch baseline results from a remote store, the constraint makes the right answer obvious: put that in the module's eval runner, not in eval-core.

---

## Negative Consequences and Risks

- **Coordinated schema changes.** A change to `EvalResult` or `EvalSummary` that is not backward-compatible requires updating all adopting modules simultaneously. This is intentional (shared schema is the goal) but means schema evolution is a platform-level decision, not a module-local one. Mitigation: keep the schema conservative and stable. The types listed above are the full schema; additions should require a documented decision. Removals are breaking changes.
- **Extraction may surface entanglement.** If Policy Scout's eval logic imports Policy Scout internals (config objects, the YAML registry, the policy-check client), those imports must be removed or abstracted before extraction. The Phase 3 entry criterion includes verifying that the logic is extractable; if it is not, Phase 3 starts with a Policy Scout cleanup sub-task.
- **Monorepo overhead.** A new package means a new `pyproject.toml`, a new uv workspace entry, a new test suite. For a `<300 line` package, this overhead is small. If the package ever grows beyond its stated scope, that is a signal that something has gone wrong, not a justification for accepting the growth.
- **PyYAML as optional extra.** Modules that do not use the YAML DSL loader and do not already have PyYAML installed will get a clear `ImportError` if they accidentally import `lattica.eval_core.loader`. The optional extra pattern handles this correctly, but it requires that the loader module not be imported at the package's `__init__.py` top level.

---

## Per-Project Integration Plan

### Policy Scout

Policy Scout currently has the eval logic that eval-core is extracted from. The integration steps are:

1. Identify all eval-related code in Policy Scout: the test case data classes, the run harness, the YAML case files, and the baseline comparison logic.
2. Verify that none of these import Policy Scout-specific internals (config, database clients, policy check logic). If they do, refactor to pass those values in as parameters rather than importing them at module level.
3. Copy (verbatim, no refactoring) to `lattica/eval-core/`.
4. In Policy Scout's eval tests, replace `from policy_scout.eval import ...` with `from lattica.eval_core import ...`.
5. Add `lattica-eval-core` to Policy Scout's `pyproject.toml` dev dependencies.
6. The `regression_vs_baseline` field in `EvalSummary` is the one field that Policy Scout does not yet produce. The extraction adds it. Policy Scout's eval suite establishes its first baseline on the first run after extraction; subsequent runs report regression deltas.

Policy Scout registers its evaluator as a plain function: `evaluator_fn: (EvalCase) -> EvalResult`. The evaluator calls `policy_scout_check(case.input)` and compares the response to `case.expected`. No eval-core types are needed inside the evaluator — only at the call site of `run_eval_suite`.

### Cerebra

Cerebra's eval requirements are the richest and are not yet implemented. The integration plan defines the target shape.

`CerebraEvalCase` is a subclass of `EvalCase` (or a parallel dataclass with the same fields plus Cerebra-specific metadata). Cases have additional fields in `metadata`:

```python
metadata = {
    "eval_type": "retrieval" | "sku_classification" | "abstention" | "lora_quality",
    "query": str,                   # for retrieval cases
    "expected_sku": str,            # for SKU classification cases
    "min_score": float,             # for retrieval quality cases
}
```

Retrieval quality cases populate `EvalResult.score` with the retrieval score from `retrieval_traces`. The baseline for retrieval quality is established after Phase 3's `top_score`/`mean_score` fields are added to `retrieval_traces`. CI fails if `mean_score` drops more than the configurable threshold below the baseline.

LoRA training quality cases are run after each training epoch: `EvalCase.input` contains the dataset fingerprint and hyperparameters; `EvalCase.expected` contains the minimum acceptable validation loss. The evaluator reads the training run's output metrics.

### discord-bot (Bo)

Bo currently has no eval infrastructure. The integration establishes a baseline for tier routing correctness.

`BotEvalCase` uses `EvalCase` directly (no subclass needed). Cases are loaded from YAML:

```yaml
suite_id: bo-tier-routing
module: discord_bot
cases:
  - id: local-qwen-default
    description: "Default messages route to local Qwen via LiteLLM"
    input:
      message: "what is the capital of France"
      context: {}
    expected:
      tier: local
      model_prefix: "qwen"
  - id: fallback-on-local-failure
    description: "Local failure triggers Claude fallback"
    input:
      message: "complex reasoning task"
      context: {simulate_local_failure: true}
    expected:
      tier: fallback
```

The evaluator calls Bo's tier routing logic directly (not through Discord). Bo's three-tier resilience pipeline is the system under test; the eval cases cover the routing decision, not the LLM output.

### ai-stack

`GatewayEvalCase` covers LiteLLM alias routing smoke tests — verifying that model aliases resolve to the correct backend.

```yaml
suite_id: ai-stack-alias-routing
module: ai_stack
cases:
  - id: qwen-coder-alias
    description: "qwen/coder alias resolves to Qwen2.5-Coder:14b"
    input:
      alias: "qwen/coder"
    expected:
      resolved_model: "qwen2.5-coder:14b"
      backend: ollama
```

The evaluator calls LiteLLM's routing resolution logic. These tests run in CI against a mock LiteLLM config (not a live Ollama instance) by passing the config path as a fixture. The goal is to detect alias misconfigurations before they cause silent fallback to the wrong model.

---

## Notes

**On size constraint.** The `<300 lines` constraint is a design invariant, not a suggestion. If a proposed addition to eval-core would push it past 300 lines, that is a signal to ask whether the addition belongs in eval-core or in the module's own eval runner. The types, runner, YAML loader, baseline persistence, and `eval_results.db` writer are the complete scope. Visualization, HTTP transport, statistical analysis, and model comparison logic belong in the Lattica dashboard or in the individual module's eval runner.

**On PyYAML.** PyYAML is the only non-stdlib dependency considered (as an optional extra). `pyyaml` is already present in every Python package in the Lattica monorepo, so in practice it will always be available. The `[yaml]` optional extra pattern is there to make the dependency explicit and to ensure the package passes its zero-dep verification (`pip install lattica-eval-core` with no extras should import `lattica.eval_core.types`, `lattica.eval_core.runner`, `lattica.eval_core.baseline`, and `lattica.eval_core.db` without error).

**On the extraction timing.** eval-core extraction is a Phase 3 deliverable. Phases 1 and 2 do not depend on it. The extraction should not begin until Phase 3 formally starts, to avoid premature stabilization of an interface that may need to flex during Phase 3's Cerebra retrieval quality work. The interface described here is the target; what comes out of Policy Scout on day one of Phase 3 may differ slightly and should be reconciled before the package is published to the workspace.

**On `regression_vs_baseline` as the key addition.** Policy Scout's existing eval already has everything except regression tracking against a stored baseline. The `RegressionReport` dataclass and the `compare_to_baseline` function are the substantive additions that justify the extraction — not just a rename and re-package. Every module gains regression gating on its first run after adopting eval-core, because `load_baseline` returns `None` on the first run (no baseline file exists yet), the summary is saved as the baseline, and all subsequent runs compare against it.