---
pass: 3
version: v0.3.0
sha: 0ed84a0
date: 2026-06-12
summary: Subscriptions and WAL-backed live event delivery
---

# Blast Radius — Pass 3 (v0.3.0)

> Retroactive file, aligned to actual commit in Pass v0.10.w.
> Content completely rewritten — original bootstrap estimate described CCE
> (which is v0.1.0). Actual commit 0ed84a0 added subscriptions and WAL watch.

## Files

### Created

4 files in commit 0ed84a0:

- `src/subscriptions.rs` — `SubscriptionRegistry`, dispatcher thread, cursor tracking, subscription lifecycle
- `src/wal_watch.rs` — WAL scan loop, background std::thread, frame-count detection
- `tests/subscriptions.rs` — subscription behavior tests, concurrent subscriber tests, degradation tests
- `tests/wal_watch.rs` — WAL watcher tests, frame detection tests

---

## Public APIs

### Added

- `Store::subscribe(stream_id: &str, branch: &str, mode: SubscriptionMode) -> Result<RawSubscriptionHandle>` — register a live subscription
- `SubscriptionMode::synchronous()` — callback fires within the write transaction (low latency, blocks writer)
- `SubscriptionMode::post_commit(queue_depth: usize)` — callback fires from bounded dispatch queue after commit (non-blocking writer)
- `RawSubscriptionHandle` — returned by `subscribe()`; methods: `unsubscribe()`, `is_degraded() -> bool`, `_wait_for_next_event(timeout) -> Option<StoredEvent>` (test helper)
- `SubscriptionRegistry` — internal type managing active subscriptions and dispatch state

---

## Schema changes

None. Subscription cursor state tracked in-memory in v0.3; `cursors` table added in v0.4.0.

---

## Configuration changes

None.

---

## Dependency changes

- Added: `crossbeam-channel` — bounded channels for PostCommit dispatcher
- Added: `notify` (or equivalent) — file-system events for WAL watcher frame detection (retroactive — exact crate may differ)

---

## Behavior changes

- `append()` now fires registered subscriptions: Synchronous callbacks fire within the write transaction; PostCommit callbacks fire from the dispatcher thread after commit.
- PostCommit queue overflow degrades gracefully — `is_degraded()` flag set on the handle; consumer must replay from cursor to recover.
- **Threading model:** std::thread + crossbeam-channel throughout. No Tokio dependency. The subscription dispatcher and WAL watcher are background `std::thread` handles, not async tasks. (DV-003 origin: spec §14 incorrectly described Tokio; resolved in v0.11.0.)

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
