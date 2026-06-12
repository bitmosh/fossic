---
pass: 2
version: v0.2.0
date: "(retroactive estimate, not verified)"
summary: Subscriptions — PostCommit and Synchronous modes, WAL watcher
---

# Blast Radius — Pass 2 (v0.2.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## Files

### Created
- `fossic/src/subscriptions.rs` — SubscriptionRegistry, dispatcher thread, cursor tracking
- `fossic/src/wal_watch.rs` — WAL scan loop, background thread, notify-based file watch

### Modified
- `fossic/src/store.rs` — subscribe() method, SubscriptionMode integration
- `fossic/src/types.rs` — SubscriptionMode, RawSubscriptionHandle, SubscriberKind
- `fossic/tests/subscriptions.rs` — subscription behavior tests (created)
- `fossic/tests/wal_watch.rs` — WAL watcher tests (created)

---

## Public APIs

### Added
- `Store::subscribe(stream_id, branch, mode) -> Result<RawSubscriptionHandle>` — raw handle
- `SubscriptionMode::synchronous()` — fires in the write transaction
- `SubscriptionMode::post_commit()` — fires from bounded dispatch queue after commit
- `RawSubscriptionHandle` — unsubscribe, is_degraded, _wait_for_next_event

---

## Schema changes

None. (Subscription cursor state tracked in-memory in v0.2; `cursors` table added later.)

---

## Configuration changes

None.

---

## Dependency changes

- Added: `crossbeam-channel` — bounded channels for PostCommit dispatcher
- Added: `notify` — file-system events for WAL watcher (retroactive estimate)

---

## Behavior changes

- `append()` now fires registered subscriptions — Synchronous callbacks fire within the
  write transaction; PostCommit callbacks fire from the dispatcher thread after commit.
- PostCommit queue overflow degrades gracefully — `is_degraded` flag set; consumer must
  replay from cursor to recover.

---

## Living report updates

No new entries this pass. No entries resolved. (retroactive — Aseptic not yet active)
