── PASS COMPLETE · v1.4.1 · 2026-06-21 ──────────────────────

Title: Documentation — project registration for federated deployments

Summary: README.md gains a Project Registration section with the full manual registration spec table and RelayConfig heartbeat example. FOSSIC_V1_SPEC.md §9.4 adds ProjectRegistered and RelayHeartbeat to the _fossic/system event type table. No behavior changes; 322 tests pass unchanged.

Project: fossic

Highlights:
· README "Project Registration" section: four-field spec table (source_store, local_store_path, subscribe_pattern, project_description), RelayConfig example with heartbeat_interval_s, indexed_tags discipline note, forward-links to §15 (fossic-coordinator) and §9.4
· Spec §9.4: ProjectRegistered and RelayHeartbeat rows added with trigger, payload fields, and indexed_tags schema — the existing event-type table is the correct home; no new federation section added
· CP-T2-2 filed: full federation protocol section (hub coordinator discovery, multi-project relay topology) deferred to fossic-coordinator crate work

Learnings:
· None (docs-only pass)

Commit: 87c2822
Tests: 322 passed · 0 failed · 1 ignored
Branch: clean
