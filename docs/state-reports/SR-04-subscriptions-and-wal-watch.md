# SR-04 — Subscription System and WAL Watch

**Series:** Fossic State Reports · Document 4 of 9  
**Scope:** `src/subscriptions.rs`, `src/wal_watch.rs`, `src/glob.rs`, `fossic-py/src/subscriptions.rs`  
**Companion docs:** SR-03 (event lifecycle, append path), SR-06 (reducers — also use the glob system)

---

## 1. Overview

Fossic's subscription system delivers events to registered callbacks after (or during) writes. There are two fundamentally different delivery modes:

- **Synchronous** — callback fires *inside* the write transaction, before `tx.commit()`. The write mutex is held for the full duration of the callback. Zero delivery lag. Can stall subsequent writes.
- **PostCommit** — callback fires *after* `tx.commit()` returns, from a per-subscriber background thread via a bounded channel. Non-zero but typically sub-millisecond delivery lag. Write path is never blocked by subscriber processing time.

The **WAL watcher** extends subscriptions to cross-process writes: a `Store` instance opened in process A will receive events written by process B to the same SQLite file, routed through the same PostCommit path.

Choose mode carefully. The invariants, failure modes, and performance profiles are very different.

---

## 2. SubscribeQuery

```rust
pub struct SubscribeQuery {
    pub stream_pattern: String,  // glob pattern: * = one segment, ** = zero or more
    pub branch: String,          // default "main"
    pub include_system: bool,    // whether to deliver _fossic/system events
}
```

**`stream_pattern`** — a segment-based glob pattern that determines which streams the subscription receives events from. The matching algorithm is documented in §3 below. Common patterns:

| Pattern | Matches |
|---|---|
| `**` | Every stream (including all depths) |
| `cerebra/**` | All streams under `cerebra/` |
| `cerebra/agent-trace/*` | Session streams exactly one level deep |
| `policy-scout/posture` | Exactly this one stream |
| `*/posture` | Any project's `posture` stream |

**`branch`** — subscriptions are per-branch. A subscription on `"main"` does not receive events written to a branch. Default is `"main"` and most consumers never change this.

**`include_system`** — the `_fossic/system` stream carries internal fossic events (`SubscriptionDegraded`, `Purged`). By default these are excluded from delivery (`include_system = false`). Set to `true` only if you specifically need to observe system health events. Note: the dispatcher skips `_fossic/system` when `include_system = false`; this is also what prevents recursive dispatch when writing a `SubscriptionDegraded` event (see §6).

---

## 3. Glob Pattern System

The same glob engine is used for subscriptions, reducer pattern matching, and payload transform pattern matching. It lives in `src/glob.rs` and is worth understanding precisely.

### 3.1 Segment Rules

Patterns and stream IDs are split on `/` into segments. Matching is recursive descent over the segment lists.

- **`*`** — matches exactly **one** segment. The matched segment may not contain `/` (that's enforced by the split). `cerebra/*` matches `cerebra/x` but NOT `cerebra/x/y`.
- **`**`** — matches **zero or more** consecutive segments. `cerebra/**` matches `cerebra`, `cerebra/x`, `cerebra/x/y`, and `cerebra/x/y/z`.
- **Literal** — matches exactly that one segment, case-sensitive.

### 3.2 Algorithm (recursive descent)

```
match_parts(pattern_segments: &[&str], stream_segments: &[&str]) -> bool:

  if pattern_segments is empty:
    return stream_segments is empty

  if pattern_segments[0] == "**":
    for i in 0 ..= len(stream_segments):
      if match_parts(pattern_segments[1..], stream_segments[i..]):
        return true
    return false

  if stream_segments is empty:
    return false

  head_matches = (pattern_segments[0] == "*") OR (pattern_segments[0] == stream_segments[0])
  if head_matches:
    return match_parts(pattern_segments[1..], stream_segments[1..])
  
  return false
```

The `**` case tries every possible consumption: zero segments (`i=0`), one segment (`i=1`), etc., up to consuming all remaining stream segments. This is what allows `**` to match zero segments (a trailing `**` matches the end of the stream ID without consuming anything more).

### 3.3 Behavioral examples

```
matches("cerebra/**",         "cerebra")          → true   (* zero segments *)
matches("cerebra/**",         "cerebra/x")        → true
matches("cerebra/**",         "cerebra/x/y")      → true
matches("cerebra/**",         "other/x")          → false
matches("cerebra/*",          "cerebra/x")        → true
matches("cerebra/*",          "cerebra/x/y")      → false  (* * = exactly one *)
matches("cerebra/*",          "cerebra")          → false  (* needs one more segment *)
matches("a/**/b",             "a/b")              → true   (* ** = zero segments *)
matches("a/**/b",             "a/x/b")            → true
matches("a/**/b",             "a/x/y/b")          → true
matches("**",                 "anything/at/all")  → true
matches("**",                 "")                 → true   (* edge: empty string → [""] *)
matches("policy-scout/posture", "policy-scout/posture") → true
matches("policy-scout/posture", "policy-scout/posture/extra") → false
```

### 3.4 `specificity_score`

Used by the reducer registry to select the most-specific matching pattern when multiple reducers could match a stream:

```rust
fn specificity_score(pattern: &str) -> usize
```

Returns the count of **leading literal (non-wildcard) segments** before the first `*` or `**`:

| Pattern | Score |
|---|---|
| `cerebra/agent-trace/*` | 2 |
| `cerebra/**` | 1 |
| `*/posture` | 0 |
| `**` | 0 |
| `policy-scout/posture` | 2 |
| `a/b/c/d` | 4 |

When two registered reducer patterns match the same stream at the same specificity score, `ReducerPatternAmbiguous { a, b }` is returned. This is a hard error at registration time (see SR-06).

### 3.5 `validate_pattern`

Called at subscription and reducer registration time. Returns an error if:
- Pattern is empty.
- Any segment contains characters not permitted in stream IDs.
- A `**` appears mid-segment (e.g. `foo/**bar`).

### 3.6 Python translation

For relay agents that need server-side glob matching in Python before subscribing (e.g. filtering `store.streams()` during backfill):

```python
def _match_parts(p: list[str], s: list[str]) -> bool:
    if not p:
        return not s
    if p[0] == "**":
        for i in range(len(s) + 1):
            if _match_parts(p[1:], s[i:]):
                return True
        return False
    if not s:
        return False
    return (p[0] == "*" or p[0] == s[0]) and _match_parts(p[1:], s[1:])

def stream_matches_pattern(stream_id: str, pattern: str) -> bool:
    return _match_parts(pattern.split("/"), stream_id.split("/"))
```

This is a byte-for-byte translation of `src/glob.rs` and produces identical results on all inputs.

---

## 4. SubscriptionMode::Synchronous

```rust
pub enum SubscriptionMode {
    Synchronous,
    PostCommit { queue_size: usize },
}
```

### 4.1 Delivery timing

With `Synchronous`, the `SubscriptionHandler::on_event` callback is called from within `append_impl`, *before* `tx.commit()`. The write `Mutex<Connection>` is still held. The transaction has inserted the event row but has not yet committed.

Sequence:

```
acquire write lock
begin IMMEDIATE transaction
... validate, transform, derive id, insert row ...
→ dispatch_sync(event):
    for each Synchronous subscriber matching stream:
        on_event(&event)   ← YOUR CALLBACK RUNS HERE, INSIDE WRITE LOCK
tx.commit()
release write lock
→ dispatch_post_commit(event)  ← PostCommit subscribers fire here
```

### 4.2 What the callback can observe

Because the event row is in the not-yet-committed transaction, the callback *can* see it if it reads from the write connection (which it receives via the same Mutex). However, readers using pool connections will NOT see it yet (uncommitted). In practice, Synchronous callbacks should be self-contained (update an in-memory structure, write to a secondary data structure, etc.) rather than reading back from the store.

### 4.3 Performance implications

The write lock is held for the entire callback duration. If 10 subscribers are registered in Synchronous mode, they fire sequentially. A 5ms callback adds 5ms to every single write. This mode is intentionally expensive to discourage overuse.

**Correct use cases:**
- In-process caches that must be consistent with the write (no window for a stale read between commit and cache update).
- Counters or accumulators that must be atomically updated with the event.
- Test fixtures that need guaranteed delivery ordering without race conditions.

**Incorrect use cases:**
- Network calls.
- Disk I/O.
- Reducer state computation.
- Anything that could block.

### 4.4 Panic handling in Synchronous mode

If a Synchronous callback panics, `dispatch_sync` wraps the call in `std::panic::catch_unwind`. On panic:

1. The panic is caught; the panic payload is discarded.
2. The subscription is marked degraded (`degraded` AtomicBool → `true`).
3. A `SubscriptionDegraded` event is written to `_fossic/system` (outside the current transaction — this is a separate write).
4. The current write transaction **still commits**. The panic does not abort the append.

This "degrade and continue" behavior preserves write availability even if a subscriber is broken.

---

## 5. SubscriptionMode::PostCommit

### 5.1 Delivery timing

With `PostCommit { queue_size }`, delivery happens *after* `tx.commit()` returns. The write lock has been released by the time the callback fires.

Full sequence:

```
acquire write lock
begin IMMEDIATE transaction
... validate, transform, derive id, insert row ...
dispatch_sync(event)  ← Synchronous subscribers run here (if any)
tx.commit()
release write lock
→ dispatch_post_commit(event):
    for each PostCommit subscriber matching stream:
        channel.try_send(event.clone())  ← non-blocking push
        if try_send fails: mark degraded, emit SubscriptionDegraded
per-subscriber background thread:
    loop:
        event = channel.recv()
        handler.on_event(&event)
```

### 5.2 Channel architecture

Each PostCommit subscriber owns:
- A `crossbeam_channel::bounded<StoredEvent>(queue_size)` channel.
- A dedicated background thread that reads from the channel and calls `on_event`.

The dispatcher holds the `Sender<StoredEvent>` side. The background thread holds the `Receiver<StoredEvent>` side.

`queue_size` controls the buffer depth between the dispatcher and the handler thread. This decouples write throughput from handler throughput: a burst of writes fills the buffer, and the handler drains it at its own pace.

### 5.3 Channel sizing guidance

| Handler characteristic | Recommended `queue_size` |
|---|---|
| In-memory only (fast) | 256–1024 |
| Local disk I/O | 1024–4096 |
| Network I/O (relay, HTTP calls) | 4096–16384 |
| Batch processor (drains periodically) | 16384+ |

The default (used when not specified) is 1024. The Python default is also 1024 when `mode` is `None`.

If you observe degraded subscriptions under load, increase `queue_size` before blaming slow handlers — a burst of writes can transiently overflow even a fast handler.

### 5.4 `try_send` semantics

The dispatcher uses `try_send` (non-blocking). If the channel is full:

- `try_send` returns `Err(TrySendError::Full(event))`.
- The event is dropped for that subscriber.
- The subscriber is marked degraded (see §6).

`try_send` is used instead of `send` (blocking) because the dispatcher runs on the write thread — blocking here would stall subsequent appends. This is an intentional design tradeoff: write throughput is never sacrificed for slow subscribers.

---

## 6. Degraded Subscriptions

A subscription becomes degraded when:
- **PostCommit:** `try_send` returns `Err(TrySendError::Full(_))` — the channel is full.
- **Synchronous:** the `on_event` callback panics.

### 6.1 Degradation mechanics

When degradation occurs:

1. `subscription.degraded.store(true, Ordering::Release)` — sets the AtomicBool flag.
2. The event is dropped for that subscriber (it will not be delivered, ever).
3. `write_degraded_event(subscription_id)` is called: this appends a `SubscriptionDegraded` event to `_fossic/system` with payload:
   ```json
   {
     "subscription_id": "...",
     "degraded_at_us": 1718000000000000
   }
   ```

### 6.2 Degraded is permanent (v1)

Once a subscription is marked degraded, it stays degraded. There is no automatic recovery. The subscription continues to exist and may receive future events *if the channel drains* (for PostCommit), but there is no mechanism to re-deliver missed events.

To recover:
1. Check `handle.is_degraded()` and detect the condition.
2. Call `handle.unsubscribe()` (or drop the handle).
3. Re-subscribe with a fresh `store.subscribe(...)` call.
4. Backfill by reading events from the last known-good cursor position.

### 6.3 System stream recursion prevention

The `_fossic/system` stream is where `SubscriptionDegraded` events are written. If subscriptions on `_fossic/system` were also degraded (triggering more `SubscriptionDegraded` events on `_fossic/system`, triggering more degradation, etc.), this would be an infinite loop.

The dispatcher explicitly skips `_fossic/system` events when iterating PostCommit and Synchronous subscribers. The `include_system = false` default is the consumer-facing expression of this; the dispatcher enforces it regardless of `include_system` for the degradation write path itself.

### 6.4 Monitoring

To detect degradation in production:
- Periodically call `handle.is_degraded()`.
- Subscribe to `_fossic/system` with `include_system = true` and watch for `SubscriptionDegraded` events.
- The `SubscriptionDegraded` event payload includes the `subscription_id` to correlate with the handle.

---

## 7. SubscriptionHandle — RAII Lifecycle

```rust
pub struct SubscriptionHandle {
    id: SubscriptionId,
    registry: Arc<SubscriptionRegistry>,
    degraded: Arc<AtomicBool>,
}
```

### 7.1 Creation

`Store::subscribe(query, mode, handler)` registers the subscription in `SubscriptionRegistry` and returns `Ok(SubscriptionHandle)`. The handle is the only way to unsubscribe or check degraded state.

### 7.2 Drop = unsubscribe

On `Drop`, `SubscriptionHandle` calls `registry.unsubscribe(self.id)`:

1. The subscription entry is removed from the registry's active subscriber list.
2. For PostCommit: the `Sender<StoredEvent>` is dropped. This closes the channel. The background thread sees `RecvError::Disconnected` on its next `recv()` call and exits cleanly.
3. For Synchronous: the callback is simply no longer invoked.

The handle must be kept alive for as long as you want to receive events. Dropping it is the correct way to unsubscribe — there is no separate `unsubscribe()` method required.

### 7.3 `is_degraded()`

```rust
pub fn is_degraded(&self) -> bool {
    self.degraded.load(Ordering::Acquire)
}
```

Reads the AtomicBool. Thread-safe. Can be called from any thread at any time.

### 7.4 Arc sharing for degraded flag

The `degraded` AtomicBool is in an `Arc` shared between:
- The `SubscriptionHandle` (returned to the caller).
- The internal subscription registry entry.

This is how the dispatcher (running on the write thread) can set `degraded = true` and the caller (checking `handle.is_degraded()` on their thread) immediately sees the update, without any locking.

---

## 8. Cursor Ownership Invariant

This is the most load-bearing invariant in the subscription system. Violating it causes events to be skipped.

### 8.1 The invariant

**Only `dispatch_post_commit` may advance subscription cursors. The WAL watcher must never advance cursors directly.**

### 8.2 What are subscription cursors?

Each active PostCommit subscription maintains in-memory cursors: a map from `(stream_id, branch)` to the last `version` successfully enqueued to that subscriber's channel. These cursors live inside `SubscriptionRegistry`, not in the SQLite `cursors` table.

These internal cursors are used by the WAL watcher to know which events are new (not yet delivered). The WAL watcher reads events `WHERE version > cursor_position` to find unseen events.

### 8.3 Why the WAL watcher must not advance cursors

The WAL watcher detects new events and feeds them to `dispatch_post_commit`. If the WAL watcher also advanced cursors:

```
Timeline (broken scenario):
T=0: In-process write: event version=5 appended
T=1: WAL watcher fires (external commit): sees data_version changed
T=2: WAL watcher reads events newer than cursor=4 → finds version=5 (the in-process event)
T=3: WAL watcher advances cursor to 5  ← WRONG
T=4: dispatch_post_commit from in-process write tries to deliver version=5
T=5: cursor is already 5 → skips version=5 → event DROPPED
```

By keeping cursor advancement exclusively in `dispatch_post_commit`, the sequence is:

```
T=0: In-process write: event version=5 appended
T=1: dispatch_post_commit(v5) → try_send(v5) → cursor advances to 5
T=2: WAL watcher fires for unrelated external change
T=3: WAL watcher reads events newer than cursor=5 → finds nothing new → no-op
```

The invariant prevents double-delivery from the in-process path and the WAL watcher both trying to deliver the same event.

### 8.4 `group_min` cursor aggregation

The WAL watcher must read events that are new to *any* active subscriber. Different subscribers may have different cursor positions (a slow subscriber might be at version 3 while a fast subscriber is at version 7).

`group_min` aggregates: for each `(stream_id, branch)`, take the minimum cursor position across all active subscribers. The WAL watcher reads events `WHERE version > group_min` for each stream/branch pair. This ensures the watcher reads events needed by the slowest subscriber, without missing anything.

The group_min is recomputed from the in-memory cursor map on each WAL watcher scan.

---

## 9. WAL Watcher — Cross-Process Change Detection

### 9.1 Purpose

When multiple processes open the same SQLite file, WAL mode allows them all to read and write independently. But fossic subscriptions are in-process: a `Store` in process A doesn't automatically know about commits from process B.

The WAL watcher closes this gap. It monitors the SQLite file for foreign commits and routes newly discovered events through the normal PostCommit dispatch path.

### 9.2 Implementation

The WAL watcher runs in a dedicated thread launched during `Store::open`. It uses:

- **`notify::RecommendedWatcher`** — the platform-appropriate filesystem watcher:
  - Linux: inotify
  - macOS: FSEvents
  - Windows: ReadDirectoryChanges
- **Parent directory watch** — the watcher monitors the *directory* containing the db file, not the file itself. Reason: some platforms do not emit file-level events for WAL writes to the associated `-wal` file; directory-level events are more reliably triggered.

### 9.3 Scan loop

```
run_scan_loop(db_path, registry, read_pool):

  setup notify::RecommendedWatcher on parent_dir(db_path)
  last_data_version = None

  loop:
    wait for filesystem event (or poll timeout)
    
    conn = read_pool.acquire()
    current_data_version = conn.query_row("PRAGMA data_version", ...).get::<i64>()
    
    if current_data_version == last_data_version:
      continue  // no new commits, skip
    
    last_data_version = current_data_version
    
    min_cursors = registry.group_min_cursors()  // per-(stream_id, branch)
    
    for each (stream_id, branch, min_version) in min_cursors:
      new_events = conn.read_range(stream_id, branch, from_version = min_version + 1)
      for event in new_events:
        dispatch_post_commit(&registry, &event)
        // dispatch_post_commit advances cursors — WAL watcher does NOT
```

### 9.4 `PRAGMA data_version`

`PRAGMA data_version` returns an integer that SQLite increments every time another connection commits a write to the database (including WAL commits from other processes). It does not increment for reads or for commits made by the *same connection* that queries it.

This makes it an efficient way to detect foreign commits without polling the events table directly. The WAL watcher only does the more expensive "read events" query when `data_version` has changed.

### 9.5 Latency characteristics

| Platform | Typical WAL watcher latency |
|---|---|
| Linux (inotify) | < 1ms |
| macOS (FSEvents) | 1–5ms (coalesced) |
| Windows (ReadDirectoryChanges) | 1–10ms |

There is a polling fallback for cases where filesystem events are missed (e.g., network filesystems, some containers). The poll interval is implementation-defined but typically several hundred milliseconds.

**For relay agents:** WAL watcher latency is the dominant source of cross-process event delivery lag. For latency-sensitive relays, prefer running the relay in the same process as the store (or using direct in-process subscriptions).

### 9.6 Startup race

There is a brief window between `Store::open` completing and the WAL watcher's first scan where an external process could commit events. These events will be discovered on the first WAL watcher scan (either triggered by a filesystem event or the first poll tick). Consumers that require completeness should run a backfill (`read_range` from the beginning) before relying on WAL watcher delivery.

---

## 10. SubscriptionHandler Trait

```rust
pub trait SubscriptionHandler: Send + 'static {
    fn on_event(&self, event: &StoredEvent);
}
```

Key constraints:
- **`Send + 'static`** — the handler is moved into the subscription registry and may be called from the write thread (Synchronous) or a background thread (PostCommit). It must be safe to send across thread boundaries.
- **`&self`** (shared reference) — the handler is called via `&self`, so internal mutability (`Mutex`, `RwLock`, `Arc<Mutex<...>>`) is required for any state the handler mutates.
- **No return value** — errors inside `on_event` cannot be propagated. Either handle them internally (log, increment a counter, push to another channel) or panic (which triggers degradation).
- **No `async`** — the trait is synchronous. For async work, the handler should forward to a `tokio::sync::mpsc` or similar channel.

### 10.1 PostCommit handler threading

Each PostCommit subscriber gets its own background thread. If you have 10 PostCommit subscribers, there are 10 handler threads. This gives each subscriber independent processing and prevents a slow subscriber from blocking others.

The handler thread's loop:
```
loop:
    match channel.recv():
        Ok(event)  → handler.on_event(&event)
        Err(Disconnected) → break  // handle dropped, exit
```

### 10.2 Synchronous handler constraints

Synchronous handlers run on the write thread holding the write Mutex. They must:
- Complete quickly (microseconds, not milliseconds).
- Not acquire any locks that could be held by the write path (deadlock risk).
- Not call `store.append()` (recursive write lock attempt → deadlock or `SQLITE_BUSY`).

---

## 11. Python Subscription API

### 11.1 `Store.subscribe()`

```python
handle = store.subscribe(
    stream_pattern: str,
    branch: str = "main",
    mode: Optional[SubscriptionMode] = None,  # defaults to PostCommit(queue_size=1024)
    include_system: bool = False,
) -> RawSubscriptionHandle
```

Returns a `RawSubscriptionHandle` — the raw Rust binding. The higher-level `fossic.SubscriptionHandle` (pure Python) wraps this with context manager support and an iterator protocol.

### 11.2 `RawSubscriptionHandle`

```python
class RawSubscriptionHandle:
    def _wait_for_next_event(self, timeout_secs: float) -> Optional[StoredEvent]:
        """
        Block until an event arrives (GIL released while waiting).
        Returns StoredEvent on success.
        Returns None on timeout.
        Raises StopIteration when channel is closed (unsubscribed or store dropped).
        """

    def unsubscribe(self) -> None:
        """
        Drop the SubscriptionHandle. Closes the Rust-side subscription.
        Subsequent _wait_for_next_event calls raise StopIteration.
        Idempotent.
        """

    def is_degraded(self) -> bool:
        """Read the degraded AtomicBool."""
```

### 11.3 GIL release

`_wait_for_next_event` releases the Python GIL while blocking on `channel.recv_timeout(timeout)`:

```rust
fn _wait_for_next_event(&self, py: Python<'_>, timeout_secs: f64) -> PyResult<Option<PyStoredEvent>> {
    let rx = self.rx.clone();
    let timeout = Duration::from_secs_f64(timeout_secs);
    let result = py.detach(|| rx.recv_timeout(timeout));  // GIL released here
    match result {
        Ok(event) => Ok(Some(PyStoredEvent::from(event))),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(PyStopIteration::new_err("subscription closed")),
    }
}
```

`py.detach()` releases the GIL for the duration of the Rust closure. Other Python threads (including those calling `store.append()`) can run freely while a Python subscription thread is blocked waiting for events. This is critical for systems where the producer and consumer are in the same Python process.

### 11.4 Python channel architecture

The Python binding inserts an extra channel layer:

```
fossic write path
  └─ dispatch_post_commit
       └─ PyQueueHandler::on_event()
            └─ crossbeam::unbounded::tx.send(event.clone())
                 └─ [unbounded channel]
                      └─ rx.recv_timeout() ← _wait_for_next_event polls here
```

`PyQueueHandler` implements `SubscriptionHandler` and simply forwards events to an `unbounded` crossbeam channel. The `unbounded` channel is used here (not `bounded`) because:
- The PostCommit bounded channel (size = `queue_size`) already buffers between the dispatcher and `PyQueueHandler::on_event`.
- Adding a second bounded channel would create a second backpressure point that could cause the PostCommit channel to fill (triggering degradation) even if the Python side is reading events quickly.

The `SubscriptionMode` still controls when `PyQueueHandler::on_event` fires (Synchronous vs PostCommit). The unbounded channel is a passthrough; backpressure is handled at the PostCommit channel layer.

### 11.5 SubscriptionMode in Python

```python
from fossic import SubscriptionMode

# PostCommit with custom queue size
mode = SubscriptionMode.post_commit(queue_size=4096)

# Synchronous (use with care — see §4)
mode = SubscriptionMode.synchronous()

# Default (PostCommit, queue_size=1024) — omit mode parameter
handle = store.subscribe("cerebra/**")
```

### 11.6 Context manager pattern (high-level Python)

The public `fossic.SubscriptionHandle` (pure Python, wrapping `RawSubscriptionHandle`) provides:

```python
with store.subscribe("cerebra/**") as sub:
    for event in sub:
        # event is a StoredEvent
        process(event)
# Exiting context manager calls sub.unsubscribe()
```

The iterator calls `_wait_for_next_event(timeout_secs=1.0)` in a loop, allowing `KeyboardInterrupt` to propagate (the loop wakes every second to check for interruption even if no events arrive).

---

## 12. Dispatcher Internals

### 12.1 `SubscriptionRegistry`

An in-process data structure (inside `Arc<StoreInner>`) containing:
- `Vec<SubscriptionEntry>` — all active subscriptions.
- Per-subscription cursor map: `HashMap<(StreamId, Branch), u64>` tracking last delivered version.
- Per-subscription channel sender (PostCommit) or direct handler reference (Synchronous).

Protected by a `RwLock` or `Mutex` (implementation-defined; reads during dispatch are frequent so read-biased locking is preferred).

### 12.2 `dispatch_sync`

Called from within the write transaction (before commit):

```
dispatch_sync(registry, event):
  for each entry in registry where entry.mode == Synchronous:
    if not matches(entry.pattern, event.stream_id): continue
    if event.stream_id starts with "_fossic/" and not entry.include_system: continue
    catch_unwind:
      entry.handler.on_event(&event)
    on panic:
      entry.degraded.store(true)
      write_degraded_event(entry.id)
```

### 12.3 `dispatch_post_commit`

Called after commit returns:

```
dispatch_post_commit(registry, event):
  for each entry in registry where entry.mode == PostCommit:
    if not matches(entry.pattern, event.stream_id): continue
    if event.stream_id starts with "_fossic/" and not entry.include_system: continue
    
    match entry.channel.try_send(event.clone()):
      Ok(()) →
        entry.cursor.insert((event.stream_id, event.branch), event.version)
        // ↑ cursor advance happens HERE, and ONLY HERE
      Err(Full) →
        entry.degraded.store(true)
        write_degraded_event(entry.id)
```

The cursor advance inside the `Ok(())` branch is the implementation of the cursor ownership invariant: advancing the cursor is tied to successful channel delivery.

---

## 13. Practical Patterns

### 13.1 Backfill then subscribe

The standard pattern for consumers that need both historical and live events:

```python
# Step 1: backfill historical events from a known cursor or beginning
last_cursor = store.get_cursor("my-consumer", "cerebra/agent-trace/sess-1", "main")
from_version = (last_cursor + 1) if last_cursor is not None else 0

events = store.read_range(ReadQuery(
    stream_id="cerebra/agent-trace/sess-1",
    branch="main",
    from_version=from_version,
))
for event in events:
    process(event)
    store.set_cursor("my-consumer", "cerebra/agent-trace/sess-1", "main", event.version)

# Step 2: subscribe for new events
# Race window: events landing between last read_range and subscribe start
# will arrive via subscription. Use external_id deduplication if idempotency matters.
with store.subscribe("cerebra/agent-trace/*") as sub:
    for event in sub:
        process(event)
        store.set_cursor("my-consumer", event.stream_id, event.branch, event.version)
```

### 13.2 Subscription health monitoring

```python
import time

handle = store.subscribe("**")
last_check = time.time()

while True:
    event = handle._wait_for_next_event(timeout_secs=5.0)
    
    if time.time() - last_check > 30.0:
        last_check = time.time()
        if handle.is_degraded():
            logger.error("Subscription degraded — restarting with backfill")
            handle.unsubscribe()
            # ... re-subscribe logic ...
            break
    
    if event is not None:
        process(event)
```

### 13.3 Fan-out to multiple handlers

If you need multiple handlers for the same stream pattern, each handler needs its own subscription (each gets its own channel and cursor state):

```python
# Correct: two subscriptions, two independent channels
audit_handle = store.subscribe("policy-scout/**")
metrics_handle = store.subscribe("policy-scout/**")

# Wrong: one subscription trying to call two handlers
# (would require one handler that calls both, sharing one queue)
```

### 13.4 Synchronous + PostCommit hybrid

You can register both modes on the same stream. Common pattern: Synchronous for a write-through cache that must be consistent, PostCommit for downstream processing that can tolerate slightly delayed delivery:

```rust
// Synchronous: update in-memory cache atomically with write
store.subscribe(
    SubscribeQuery { stream_pattern: "cerebra/**".to_string(), .. },
    SubscriptionMode::Synchronous,
    CacheUpdater::new(cache.clone()),
)?;

// PostCommit: fan-out to relay, audit log, etc.
store.subscribe(
    SubscribeQuery { stream_pattern: "cerebra/**".to_string(), .. },
    SubscriptionMode::PostCommit { queue_size: 4096 },
    RelayHandler::new(hub_store.clone()),
)?;
```

Both subscriptions receive the same events. The Synchronous one fires first (inside the transaction). The PostCommit one fires after commit.

---

## 14. Known Issues and Limitations

**TD: No redelivery for missed events (degraded subscriptions)**  
If a subscription is degraded and misses events, there is no built-in mechanism to replay missed events through the subscription. The consumer must detect degradation, unsubscribe, and re-subscribe with a manual backfill.

**TD: WAL watcher startup race**  
Events committed by external processes between `Store::open` returning and the first WAL watcher scan are caught on the next scan, but only if another event (or the poll timeout) triggers a scan. Consumers that need completeness from time zero should read the stream from `version=0` before subscribing.

**TD: group_min cursor aggregation cost**  
`group_min` iterates all active subscribers on every WAL watcher scan. With many subscribers, this iteration has O(n_subscribers × n_streams) cost. This is unlikely to matter in practice (typical subscriber counts are single-digit) but is worth noting for future optimization.

**In-scope for v1:** PostCommit mode, Synchronous mode, WAL watcher, degraded detection, RAII handle.  
**Out of scope for v1:** Durable subscriptions (surviving process restarts without backfill), event replay from a subscription handle, subscription filtering by indexed_tags.
