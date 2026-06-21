── PASS COMPLETE · v1.3.1 · 2026-06-21 ──────────────────────

Title: EveryNSeconds policy and recurring background GC

Summary: SnapshotPolicy::EveryNSeconds is now live — read_state schedules a background snapshot via the BackgroundExecutor during the next quiescent window; auto_gc_orphans now also schedules an hourly recurring GC task at store open.

Project: fossic

Highlights:
· EveryNSeconds(N) registers without error and fires schedule_background_snapshot after N seconds of quiet time; last_snapshot_us is updated optimistically at schedule time to prevent storm-scheduling in hot read_state loops
· BacklogTask::recurring_interval re-queues GcOrphanSnapshots hourly when auto_gc_orphans=true; bg_take_snapshot on StoreInner is now fully implemented, replacing the NotImplemented placeholder
· policy_not_implemented_seconds test renamed to policy_every_n_seconds_accepted; EveryNSeconds(0) now returns SnapshotPolicyInvalid (consistent with EveryNEvents(0))

Learnings:
· Optimistic timestamp update at schedule time (not at execution time) is the right lock discipline for quiescence-gated recurring tasks — execution can lag by up to one quiescence window, so marking "scheduled" immediately prevents N duplicate tasks from piling up

Commit: 4c73e0f
Tests: 318 passed · 0 failed · 1 skipped
Branch: clean
