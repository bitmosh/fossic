── PASS COMPLETE · v1.5.0 · 2026-06-21 ──────────────────────

Title: Track 2 close — fossic core substrate-complete

Summary: Version bump v1.4.1 → v1.5.0. No API or behavior changes. Marks the substrate-complete milestone for Track 2: Phases 6, 7, and 8 fully shipped across v1.2.0–v1.4.1.

Project: fossic

Highlights:
· Track 2 arc closed: EveryNEvents (v1.2.0) → ReducerStateLarge + StateAdaptive (v1.2.1) → auto_gc_orphans / Phase 6 close (v1.2.2) → BackgroundExecutor + QuiescenceMonitor (v1.3.0) → EveryNSeconds + recurring GC / Phase 7 close (v1.3.1) → ProjectRegistered + RelayHeartbeat (v1.4.0) → docs (v1.4.1) → close (v1.5.0)
· Open items carried forward: CP-T2-2 (federation protocol spec), fossic-coordinator crate, TaskPriority::High (reserved), EveryNSeconds wall-clock integration test (deferred)
· 1 stable ignored test throughout: doc-test store::append_if (marked ```ignore, intentional)

Learnings:
· None (version bump only)

Commit: (see chore commit)
Tests: 322 passed · 0 failed · 1 ignored
Branch: clean
