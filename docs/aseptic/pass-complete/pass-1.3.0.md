── PASS COMPLETE · v1.3.0 · 2026-06-21 ──────────────────────

Title: Background executor and quiescence-gated task scheduling

Summary: fossic now starts a background thread (fossic-bg) at Store::open that drains a priority heap of maintenance tasks only during quiet windows, with clean shutdown via grace timeout and a done channel.

Project: fossic

Highlights:
· Store::append / append_batch / append_if now stamp a quiescence clock after each write; the dispatcher stamps a second clock after each post-commit delivery — tasks only run when both are idle for the configured window (default 2s)
· BackgroundExecutor shuts down cleanly on Store drop: stop-flag + grace timeout + done channel; tasks with persist_on_drop=true emit DeferredTaskDropped system events instead of being silently discarded
· Weak<dyn StoreOps> pattern prevents the executor from keeping the store alive — upgrade() returning None silently skips the task, no panic

Learnings:
· Arc::strong_count==1 in Drop (v1.2.2) and Weak<dyn Trait> downgrade after Arc construction (v1.3.0) compose cleanly: drop-time GC fires first, then StoreInner drops BackgroundExecutor, which signals the bg thread — no coordination code needed

Commit: 2233cd7
Tests: 317 passed · 0 failed · 1 skipped
Branch: clean
