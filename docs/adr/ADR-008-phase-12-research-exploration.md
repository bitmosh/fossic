# ADR-008: Phase 12 — Research Exploration, Not Engineering Deliverables

**Status:** Accepted
**Date:** 2026-06-11
**Deciders:** Developer (bitmosh)
**Related Phases:** Phase 12 (Cognitive OS / Reflective Twin v2) — see LUMAWEAVE_NOW.md
**Related ADRs:** [ADR-005](ADR-005-cerebra-api-shell-out-vs-daemon.md) (Cerebra is the memory substrate for Phase 12 experiments), [ADR-003](ADR-003-eval-core-standalone-package.md) (eval-core is the evaluation harness)

---

## Context

Phase 12 ("Cognitive OS / Reflective Twin v2") involves: epistemic evolution graphs, meta-cognitive profiling, teacher-student distillation pipelines for reasoning primitive correction, calibration training, and visualizing how a model's thinking evolves over time. This is genuinely novel territory that sits at the boundary of engineering and ML research.

The developer's background — music teacher, warehousing systems architect, AI platform developer — maps to a deep intuition for the problem. The warehousing principle ("don't detect mistakes, make them structurally impossible") is directly applicable to the meta-cognitive question: don't monitor whether a model is reasoning well, build infrastructure that makes bad reasoning patterns correctable. But there is no established recipe for what Phase 12 is attempting. The five research questions it raises (see below) do not have known answers.

The problem this ADR addresses is classification: what kind of project is Phase 12? That classification determines how milestones are defined, what counts as progress, how blocked vs. complete is distinguished, and whether a null result is a failure or a finding.

If Phase 12 is classified as an engineering project, the pressure is toward shipping features. A distillation pipeline is "done" when it runs end-to-end. An epistemic graph is "done" when it renders. This framing actively harms the work: it incentivizes declaring things done before the interesting question (does this actually improve reasoning?) is answered.

If Phase 12 is classified as research, each milestone is: a hypothesis, an experiment design, success and failure criteria, and a record of what was learned. Progress is not measured in features shipped but in questions answered — including answered "no."

---

## Forces

- **Novelty:** None of the five Phase 12 research questions have established answers in the literature. Treating them as engineering problems with known solutions would mean speculating in implementation rather than in experiment design.
- **Reversibility:** A distillation pipeline built before the research questions are answered may need to be discarded entirely if the answers are "this approach doesn't work." The infrastructure cost is lower if it's built to support experiments rather than to ship a product.
- **Developer time:** This is a single-developer project. Spending cycles on a Phase 12 "feature" that later turns out to rest on a false premise is a meaningful loss. Research framing forces hypothesis clarity before implementation.
- **Motivation:** Null results in research are findings. Null results in engineering are failures. The developer will encounter null results in Phase 12. The framing determines whether those results feel like progress or defeat.
- **Platform continuity:** Even if the research questions produce entirely negative answers, the infrastructure built along the way (distillation pipeline, epistemic graph visualization in LumaWeave) is real engineering value. Research framing does not mean the work disappears — it means the work is framed so that "we learned this doesn't work" is a complete and valid outcome.

---

## Decision

Phase 12 is documented and pursued as a **research agenda with experimental milestones**, not as an engineering project with feature deliverables. Each milestone is structured as: a hypothesis, an experiment design, explicit success and failure criteria, and a record of what was learned.

The following are valid Phase 12 outcomes:
- A research question answered affirmatively with a working artifact.
- A research question answered negatively with documented evidence.
- A research question answered partially, with a refined hypothesis and a next experiment.

The following are not valid Phase 12 outcomes:
- "The feature shipped" without evidence it answered the underlying research question.
- "The experiment was inconclusive" without documentation of what made it inconclusive and what a better-designed experiment would look like.

---

## The Research Questions Phase 12 Is Pursuing

### RQ1: Can meta-cognition be trained into a small model?

**Hypothesis:** A small language model (Granite 4.1 3B) can be made measurably more "correctable" through targeted fine-tuning — that is, trained to respond to critique by identifying failed assumptions and avoiding repetition of failure modes, rather than defending the original response.

**Experiment design:** Construct a dataset of (question, initial response, critique, improved response) tuples using Claude as the teacher. Fine-tune Granite 4.1 3B on this dataset. Measure "correctability" pre- and post-training using a held-out evaluation set: given a correction prompt, does the model's reasoning improve more per correction attempt than the baseline model?

**Why Granite 4.1 3B:** Small enough that behavioral changes are observable without confounding from emergent capabilities, large enough to have coherent reasoning traces worth working with.

**Success criterion:** Measurable improvement in correctability score (defined in RQ5) on the held-out evaluation set, statistically distinguishable from baseline.

**Failure criterion:** No measurable improvement after two training approaches. Document which approaches were tried and the null result.

---

### RQ2: What is a "reasoning primitive" and can it be labeled consistently?

**Hypothesis:** Reasoning steps can be categorized into a small set of primitives (assumption formation, evidence gathering, analogy, elimination, synthesis, etc.) and two independent labelers can agree on these categorizations at a level above chance.

**Experiment design:** Construct a 100-sample test set of reasoning trace excerpts. Two independent labelers (developer + Claude as second labeler) assign primitive labels to each excerpt using a defined taxonomy. Compute Cohen's kappa.

**Success criterion:** Cohen's kappa > 0.6 on the 100-sample test set. This is the conventional threshold for "substantial agreement" — below this, the taxonomy is too ambiguous to be a training signal.

**Why this matters:** If reasoning primitives can't be consistently labeled, the training signal for primitive-level correction is noise. RQ1 cannot proceed on solid footing until RQ2 is resolved. RQ2 is therefore the first experiment to run.

**Failure criterion:** Kappa < 0.6 after two attempts to refine the taxonomy. Document which distinctions caused disagreement — this is itself a finding about the structure of reasoning.

---

### RQ3: Does teacher-student distillation transfer reasoning patterns, not just answers?

**The trap:** Standard distillation trains on (question → answer) pairs. The model learns to produce the right answer, not to reason the right way. If the training signal is (question → reasoning → critique → improved-reasoning), does the trained model produce better reasoning traces even on questions where the answer would have been correct without correction?

**Hypothesis:** Training on the full correction chain (Q → initial reasoning → critique → corrected reasoning) produces a model that exhibits improved reasoning process on novel questions, not just improved accuracy on the training distribution.

**Experiment design:** Construct two training sets from the same source material: (a) Q → final answer only, (b) Q → initial reasoning → critique → corrected reasoning. Train two model variants. Evaluate both on a held-out set. Judge the reasoning traces (not just answer correctness) using the teacher model as judge. The judge prompt asks: "Which reasoning trace demonstrates better process?" — blind to which variant produced it.

**Success criterion:** Variant (b) produces reasoning traces rated better by the teacher judge at a rate distinguishable from 50/50, on questions where both variants produce the correct final answer. This tests whether the process transferred, not just the output.

**Failure criterion:** No distinguishable difference in reasoning quality between variants. Document the null result.

---

### RQ4: Does epistemic evolution visualization reveal something actionable?

**Hypothesis:** Visualizing confidence changes, branch exploration, and abandoned hypotheses in LumaWeave reveals patterns that predict failure modes — not post-hoc, but before the model produces the wrong answer.

**Experiment design:** Instrument a reasoning session to capture: confidence at each step, branching points where alternatives were considered and rejected, steps where the model backed up. Render this as a graph in LumaWeave (confidence mapped to node size or color, abandoned branches as faded edges, backtracking as edge weight). Identify candidate visual signatures. Then: given a new set of reasoning sessions with known outcomes (correct / incorrect), can the visual signatures predict the outcome before the final answer is reached?

**Success criterion:** At least one failure mode is predicted by a visual pattern in the epistemic graph before the model produces the wrong answer, validated on a test set not used to identify the signature. The prediction cannot be post-hoc rationalized — it must be specified as a rule before the test set is evaluated.

**Failure criterion:** No visual signature generalizes to the test set. The null result is itself informative about whether the epistemic graph contains signal at this granularity.

**Note:** This research question directly motivates the "Reflective Twin v2" visualization work. Even if the predictive result is null, the visualization infrastructure is real platform value.

---

### RQ5: Is "easier to correct" a measurable property?

**Operationalizing the key outcome.** All of RQ1-RQ4 depend on having a metric for "correctability." This research question designs and validates that metric.

**Proposed metric:** Given a fixed set of correction prompts, compute: (reasoning quality after correction) - (reasoning quality before correction), normalized by correction count. A model with high correctability improves substantially per correction attempt; a low-correctability model restates its original reasoning even under explicit critique.

**Experiment design:** Develop a rubric for "reasoning quality" (drawing on the primitive taxonomy from RQ2). Apply the rubric to pre- and post-correction traces for a set of (question, correction) pairs. Compute delta. Establish a pre-training baseline using the unmodified Granite 4.1 3B. Use the baseline to determine whether Phase 12 training interventions move the metric.

**Success criterion:** The metric is stable (repeated measurement of the same trace produces the same score within a defined tolerance) and sensitive (the metric distinguishes clearly between traces humans rate as "well-reasoned" vs. "poorly-reasoned").

**Failure criterion:** The metric is unstable or insensitive. Document which aspects of reasoning quality resisted operationalization.

---

## The Relationship to Cerebra

Cerebra is the memory substrate for Phase 12 experiments, not the agent running them.

Specifically:
- Training datasets are built from material in Cerebra's vault.
- Evaluation results are stored in Cerebra with full provenance (experiment ID, hypothesis, result, date).
- The retrieval planner surfaces analogous prior experiments when designing new ones ("have we tested this variant before?").
- Cerebra's LoRA pipeline (RTX 4070 SUPER, mxbai-embed-large-v1) is the local training infrastructure.

Cerebra does not make experimental design decisions. It remembers the experiments and makes the vault queryable. This distinction matters: Cerebra is infrastructure, not a research collaborator. The research questions are answered by the developer running experiments, not by Cerebra retrieving answers.

---

## The Relationship to LumaWeave

LumaWeave's role in Phase 12 is twofold:

1. **Epistemic graph rendering (RQ4):** The epistemic evolution graph is a new graph source adapter in LumaWeave. Confidence, branching, and backtracking data from a reasoning session are mapped to nodes and edges. This follows the existing source adapter pattern (transport:"live", database-schema adapter slot). The gwells physics engine is used to cluster by reasoning phase; confidence drives repulsion force.

2. **Reflective Twin v2:** Graph B (live state) now includes model epistemic state as a first-class source. The diff layer (Graph A vs. Graph B) includes reasoning divergence events alongside code change events. This is the "Reflective Twin" completing its original design intent: the system observes itself reasoning, not just itself executing.

These are real engineering deliverables that emerge from the research work. They ship when the research work produces enough clarity about what to build. They do not drive the research timeline.

---

## What Success Looks Like for Phase 12

Not "we shipped a feature."

Phase 12 is complete when:
- At least 2 of the 5 research questions have clear answers (positive or negative).
- The answers are documented with the experimental evidence that produced them.
- At least one finding is novel enough to write about (a post, a paper, a documented case study).
- The platform infrastructure built along the way (distillation pipeline, epistemic graph visualization) is a real engineering artifact deployed in Lattica, regardless of whether the research conclusions were positive.

A phase that produces three clear null results and one positive result is a successful Phase 12. A phase that ships a distillation pipeline without answering whether it improves reasoning is not.

---

## Why the Developer's Background Is Directly Relevant

The music teaching background is not incidental. A good music teacher does not tell a student "that note was wrong." They ask: "Why did you think that was the correct note?" The answer to that question reveals whether the student has a faulty model of the scale, a misread of the notation, a motor habit that overrides their intention, or something else entirely. The correction then targets the actual source of the error, not the symptom.

Phase 12 is building the infrastructure to ask that question of a language model. The epistemic graph is the equivalent of a teacher watching the student's decision process, not just the output. The distillation pipeline is the equivalent of teaching the student to recognize and verbalize their own reasoning errors, not just to produce correct answers.

This framing should inform every experiment design in Phase 12. The question is never "did the model get the answer right?" It is: "can we see where the model's reasoning went wrong, and can we build an intervention that addresses that specific failure, not just the downstream output?"

---

## Consequences

**Enabled:**
- Phase 12 milestones can be defined clearly (hypothesis + success criterion + failure criterion) without requiring premature commitment to a specific implementation path.
- Null results are treated as findings and documented, rather than as failures that require spinning up new implementation attempts to obscure.
- The research timeline is decoupled from the platform engineering timeline: LumaWeave and Cerebra continue to develop while Phase 12 experiments run at whatever pace the research questions require.
- The developer can stop a Phase 12 experiment when the evidence is clear, rather than continuing because the "feature" isn't done yet.

**Given up:**
- Traditional milestone tracking ("Phase 12 is 60% complete") does not apply. Progress is measured in questions answered, not percentage of implementation done.
- External legibility: a research agenda is harder to explain as a roadmap item than a feature list. This is an acceptable tradeoff for a single-developer project where the audience for the roadmap is primarily the developer.

**Risks to watch:**
- **Scope creep into Phase 11 (GPU Infrastructure):** Phase 12 experiments require GPU access and training infrastructure. That infrastructure is Phase 11's domain. If Phase 12 research questions start pulling Phase 11 work forward prematurely, flag it and reorder explicitly rather than absorbing it silently into Phase 12.
- **Cerebra eval-core coupling:** Phase 12 evaluation results need to be tracked with eval-core (ADR-003). If the eval-core schema is too rigid to capture research-style results (hypothesis, null result, confidence interval), extend the schema rather than working around it.
- **Conflating "working artifact" with "answered question":** The distillation pipeline running end-to-end is not evidence that teacher-student transfer works. The experiment design must include the measurement, not just the machinery.
