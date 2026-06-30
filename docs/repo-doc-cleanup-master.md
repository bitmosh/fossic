Repo Documentation Cleanup — Master Prompt
You are operating on a git repository whose documentation has accumulated over time and needs to be brought to a presentable, navigable, current-state-first shape. Your job is to compress historical sprawl into a thin narrative spine while making the current working state the primary surface for any reader landing in the repo.
This prompt has two modes. Pick one based on the operator's instruction.
    • INITIAL_CLEANUP — full pass on a repo that has never been cleaned. Heavy archival, full restructure, large diff. 
    • LANDMARK_PASS — incremental pass at a milestone. Fold new working docs into the existing structure, archive what's now superseded, refresh current-state docs. Small diff. 

Operator inputs (fill in before running)
REPO_ROOT: ~/Projects/fossic
MODE: INITIAL_CLEANUP
PROJECT_NAME: Fossicv1
ONE_LINE_DESCRIPTION: Rust-core Event-sourced substrate modeled around SQLite
PRIMARY_AUDIENCE: employer reviewer
ARCHIVE_STRATEGY: in_repo (merge/concatenate old docs & move to /archive)

Operating principles — non-negotiable
    1. Archive, do not delete. No matter how confident you are a doc is dead, it moves to the archive location specified by ARCHIVE_STRATEGY. Git history alone is insufficient when the goal is human-readable history. 
    2. Never invent development history. Every claim in any derived narrative document must trace to a specific source doc, ADR, commit, or code artifact. If you cannot cite the source, you cannot make the claim. Mark uncertain reconstructions with [unverified] and surface them in the verification report for human review. 
    3. Never paraphrase ADRs or decision records. They are first-class artifacts. Move them, link to them, index them — do not rewrite them. 
    4. Never modify code or config to match documentation. If docs disagree with code, code wins; flag the discrepancy and update the docs. 
    5. Stop at every phase gate. Each phase produces a written artifact for human review. Do not proceed to the next phase without explicit approval. 
    6. Atomic commits per phase. No mega-commits. Each phase is one (or a small ordered series of) reviewable commits. 
    7. No silent destruction. Every file deleted, moved, or rewritten is listed in the phase's manifest with its disposition. 

Target structure (the "after" shape)
The repo's documentation surface should look like this when finished. Adapt names to fit existing conventions but preserve the role of each slot.
README.md                  # Project overview, what it does, why it exists,
                           # quick-start, links to everything else. The reader's
                           # entry point. Should be readable in under 5 minutes.

docs/
  operating.md             # How to actually use the project end-to-end.
                           # Concrete commands, workflows, expected outputs.
  architecture.md          # Broad system overview. How the major pieces fit.
                           # Diagrams welcome. No deep code references.
  deep-dives/              # One file per non-obvious subsystem. Zoom-in
    <subsystem>.md         # explanations of substantial areas worth their own page.
  gotchas.md               # Footguns, known sharp edges, things to watch out for.
                           # Operational warnings, not bugs (bugs go in the tracker).
  history.md               # Derived development narrative. The story of how
                           # the project got here. Inflection points, reversed
                           # decisions, abandoned approaches, lessons.
                           # Cites archived sources; does not duplicate them.
  adr/                     # All ADRs, verbatim, ordered. Index in adr/README.md.
    NNNN-<slug>.md
  
archive/                   # (If ARCHIVE_STRATEGY=in_repo)
  <YYYY-MM-DD>-pre-cleanup/  # Snapshot folder for this cleanup pass.
    <original paths preserved>
Anything not assigned to a slot above is a candidate for archival.

Phase 1 — Inventory & classify
Walk the entire repo. For every documentation file (.md, .txt, .rst, docs/ contents, any obvious working notes), classify it into exactly one bucket:
    • CURRENT — accurately describes the working state today. Keep. 
    • HISTORICAL_DECISION — captures a decision and its rationale (ADRs, design docs that drove a still-relevant decision). Keep verbatim, route to docs/adr/ or referenced from history.md. 
    • HISTORICAL_NARRATIVE — session reports, pass-N docs, retrospectives, post-flight reports, status updates, dev journals. Source material for the derived history.md. Archive after derivation. 
    • SUPERSEDED — was current at some point, has been replaced by a newer doc or by the current state of the code. Archive. 
    • EPHEMERAL — scratch notes, TODO dumps, agent session transcripts with no preserved decisions. Archive (or delete if ARCHIVE_STRATEGY=git_only and the operator confirms). 
    • DUPLICATE — substantially overlaps another doc. Note which doc subsumes it. Archive. 
    • UNCLEAR — cannot confidently classify. Flag for human decision. 
Output: cleanup/01-inventory.md with this shape:
## Classification manifest

| Path | Bucket | Rationale | Disposition |
|------|--------|-----------|-------------|
| ...  | ...    | ...       | ...         |

## Unclear items requiring decision
- <path>: <why uncertain> — <proposed buckets>

## Statistics
- Files scanned: N
- CURRENT: N | HISTORICAL_DECISION: N | HISTORICAL_NARRATIVE: N | SUPERSEDED: N | EPHEMERAL: N | DUPLICATE: N | UNCLEAR: N
- Total bytes of doc: before / projected after
STOP. Wait for operator review and approval of the manifest before proceeding.

Phase 2 — Derive the development narrative
Using only the docs classified HISTORICAL_NARRATIVE and HISTORICAL_DECISION in Phase 1, draft docs/history.md. This is the spine of the project's story.
Rules:
    1. One document. Target length: 1800–3000 words. Compress aggressively. 
    • Chronological, but organized by inflection points rather than by date — the moments where the project changed direction. 
    • For each inflection point: what was tried, what failed, what was learned, what changed. Two to five sentences each. 
    • Every inflection point cites at least one source doc by archived path. 
    • Do not reproduce ADRs. Link to them. 
    • No fabricated continuity. If two source docs leave a gap, name the gap rather than bridging it with invention. 
Output: cleanup/02-history-draft.md plus a provenance table showing every claim and its source doc(s).
STOP. Wait for operator review. The operator must confirm the narrative matches their actual recollection — this is the highest-risk artifact in the pass.

Phase 3 — Restructure & write current-state docs
Now produce the target structure. Each file is grounded in the code and the CURRENT-bucket docs from Phase 1, not in your memory of how the project works.
For each target doc, before writing:
    • Identify the source-of-truth artifacts (code modules, config files, existing current docs). 
    • Read them. Do not skim. 
    • Note what you cannot determine from sources; mark those sections [needs operator input] rather than guessing. 
Write order:
    1. docs/architecture.md — grounded in the code's actual module layout. 
    2. docs/deep-dives/<subsystem>.md — one per genuinely non-obvious area. Skip subsystems that are self-evident from the code. 
    3. docs/operating.md — grounded in actual entrypoints, CLI surfaces, configs. Every command shown must be one that exists. 
    4. docs/gotchas.md — derived from HISTORICAL_NARRATIVE lessons-learned content plus any explicit warnings in code comments. 
    5. README.md — last. It routes to everything else. Should answer: what is this, why does it exist, how do I run it, where do I learn more. 
Then execute the archive moves per Phase 1's manifest.
Output: cleanup/03-restructure-plan.md listing every file written, moved, archived, with a short rationale for each. Plus the actual file changes staged but not committed.
STOP. Wait for operator review.

Phase 4 — Verify
Run these checks and produce a report:
    • Link integrity. Every internal link in every kept doc resolves. List broken links. 
    • Code grounding. Every command, file path, module name, or API reference in operating.md and architecture.md exists in the codebase. List discrepancies. 
    • Source coverage. Every HISTORICAL_NARRATIVE doc archived in Phase 3 either appears in the history.md provenance table or is explicitly noted as "archived without derivation, reason: <reason>". 
    • ADR integrity. Every ADR in docs/adr/ is verbatim from its source. List any that were modified. 
    • No orphaned [unverified] or [needs operator input] markers outside of explicit open-questions sections. 
    • Coverage. Every file in the original inventory has a final disposition. 
Output: cleanup/04-verification.md.
STOP if any check fails. Surface failures to operator; do not silently fix.

Phase 5 — Commit plan
Propose an ordered sequence of atomic commits with messages. Suggested shape:
    1. docs: archive pre-cleanup snapshot — moves all archived files into archive/<date>-pre-cleanup/. 
    2. docs: add ADR index and consolidate decision records — if applicable. 
    3. docs: add derived development history — adds docs/history.md. 
    4. docs: rewrite architecture and operating docs against current code — the bulk of the restructure. 
    5. docs: rewrite README as routing entry point — last. 
    6. chore: tag landmark <name> — if INITIAL_CLEANUP, recommend a docs-cleanup-<date> tag so the pre-state is recoverable by tag. 
Output: cleanup/05-commits.md with the proposed commit list. Execute only on operator approval.

LANDMARK_PASS modifications
If MODE=LANDMARK_PASS, the above phases compress as follows:
    • Phase 1 scans only docs changed, added, or invalidated since the last cleanup landmark (use git log against the prior cleanup tag). 
    • Phase 2 appends a new section to existing history.md rather than rewriting it. New section covers the landmark interval only. 
    • Phase 3 updates only the target docs affected by the landmark. README updated only if quick-start or top-level description changed. 
    • Phase 4 runs unchanged. 
    • Phase 5 produces a smaller commit set, typically 1–3 commits. 

What this prompt will not do
    • Will not modify code. 
    • Will not delete files without operator confirmation per ARCHIVE_STRATEGY. 
    • Will not fabricate decisions, dates, or rationale. 
    • Will not collapse ADRs into summaries. 
    • Will not proceed past a phase gate without explicit approval. 
    • Will not commit anything without an approved commit plan. 

Operator's reading order on completion
After the pass: read cleanup/02-history-draft.md against memory first, then README.md as if you were a reviewer landing cold, then docs/operating.md while actually running the commands. If any of those three readings produces surprise, the pass is not done.
