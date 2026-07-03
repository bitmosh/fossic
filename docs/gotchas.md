# Substrate Gotchas

Properties of the fossic substrate that consumers and integrators reliably encounter in practice. Each entry is grounded in current source: file paths, line numbers, and test references are included so you can verify the behavior directly rather than taking it on faith.

Audience: consumers (Cerebra, Policy Scout, future projects) and substrate integrators (sibling crates, third-party MCP servers, code building on fossic directly).

*Last updated: 2026-06-21. Substrate version at time of writing: v1.8.1.*

---

## 1. CCE Identity Collisions

### Property

Event identity is a BLAKE3 hash of four inputs (`src/cce.rs:133–146`):

```rust
pub fn derive_event_id(
    event_type: &str,
    type_version: u32,
    causation_id: Option<&[u8; 32]>,
    payload: &serde_json::Value,
) -> Result<[u8; 32], CceError> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(CCE_PREFIX);           // "fossic-cce-v1\0"
    encode_string(&mut buf, event_type)?;
    encode_u32_as_i64(&mut buf, type_version);
    encode_optional_bytes(&mut buf, causation_id.map(|b| b.as_slice()));
    encode_value(&mut buf, payload)?;
    Ok(*blake3::hash(&buf).as_bytes())
}
```

`stream_id`, `version`, `timestamp_us`, and `branch` are **not** in the hash. Two appends with the same `event_type`, `type_version`, `causation_id`, and `payload` produce the same event ID regardless of which stream, branch, or point in time they occur.

The underlying SQL is `INSERT OR IGNORE` (`src/append.rs:80–84`). A collision silently no-ops; the existing row is unchanged. The return value signals whether the append was new (`is_new: rows_changed > 0` at `src/append.rs:108`).

### Why

Deterministic identity enables idempotent relay, content-addressed deduplication across stores, and verifiable causation chains without a distributed coordinator. The `external_id` field provides a separate escape hatch for relay-assigned identity when payload dedup isn't the right level of abstraction.

### Where it bites

- **Test loops producing identical payloads.** `for i in 0..N { store.append(Append { event_type: "Tick".into(), payload: json!({}), .. }) }` produces one stored event, not N. Every iteration collides.
- **Production vocabularies missing domain discriminators.** A vocabulary that emits `StepExecuted { "result": "ok" }` from N sibling steps under the same causation parent produces one stored event if the payloads are identical — the second through Nth steps vanish silently.
- **Retry logic re-deriving identical events.** A retry that reconstructs the same event with the same payload re-inserts the same ID. This is correct-by-design (idempotent relay), but it means the retry hasn't "written a new event" — it confirmed the original.
- **Same-parent sibling events with identical observations.** Two parallel agents observing the same value and emitting `ObservationRecorded { "value": 42 }` under the same `causation_id` produce one stored event.

This property was independently rediscovered during Track 1 (v1.1.3 test suite), Track 2 (v1.2.1 reducer state tests), and a Tauri IPC pass (v1.1.8). It surfaces predictably across implementation contexts and is worth internalizing early.

### Mitigation

Include at least one domain-unique field in the payload that varies per logical event:

```rust
// Good: step_id varies per sibling
store.append(Append {
    event_type: "StepExecuted".into(),
    payload: json!({
        "step_id": step_id,   // unique per step
        "result": "ok",
    }),
    ..
})
```

Common discriminators by domain:

| Context | Discriminator |
|---|---|
| Cognitive cycle steps | `step_id` or `step_pos` |
| Agent traces | `cycle_id` + `message_id` |
| Relay heartbeats | `uptime_us` (monotonically increasing microseconds since start) |
| Test fixtures | `{"seq": N}` in the payload |

For relay deduplication (where you *want* collisions to be detected), use `external_id` instead of relying on payload identity — it lets the hub detect "already relayed" without requiring the payload to be identical.

---

## 2. Unbounded Dispatch Channel

### Property

The channel between the append path and the post-commit dispatcher is unbounded (`src/store.rs:241–242`):

```rust
let (dispatch_tx, dispatch_rx) =
    crossbeam_channel::unbounded::<StoredEvent>();
```

`store.append()` sends to this channel and returns immediately without blocking on subscriber delivery. If subscribers fall behind, the channel grows in memory with no cap.

Two observability accessors are available (`src/store.rs:1544–1554`):

```rust
/// Current undelivered events in the dispatch channel.
pub fn dispatch_channel_pressure(&self) -> usize {
    self.inner.dispatch_tx.len()
}

/// Historical peak depth since this store instance was opened.
pub fn dispatch_channel_high_water_mark(&self) -> usize {
    self.inner.dispatch_channel_high_water_mark.load(Ordering::Relaxed)
}
```

The high-water mark is updated at every dispatch site (`src/store.rs:398–400`, `442–444`, `508–510`). It resets to zero on `Store::open` (the `Arc<AtomicUsize>` is freshly allocated each time). There is no runtime reset method.

### Why

Bounding the dispatch channel would propagate backpressure to `store.append()`, blocking writers during transient subscriber hiccups. Non-blocking append is more valuable than bounded memory for this channel: the cost of a slow subscriber is heap, not write latency or deadlock risk.

### Where it bites

- **Long-running subscribers.** A subscriber that issues network calls, runs ML inference, or hits a slow SQLite write on every event can fall behind under moderate write load. The channel grows without alerting the writer.
- **Burst workloads.** A bulk import or a replay-from-cursor that appends thousands of events quickly can fill the channel before subscribers drain it. The channel will drain eventually, but peak memory can be significant.
- **Unmonitored deployments.** Without polling `dispatch_channel_pressure()`, there is no indication the channel is growing. The first observable symptom is usually OOM or degraded subscriber queues filling.
- **Per-instance accounting.** High-water mark resets on every `Store::open`. A relay agent that reconnects after a `StorageError` gets a fresh mark, erasing the history from the previous session.

### Mitigation

Poll `dispatch_channel_pressure()` in consumer health checks. A value that trends upward over multiple samples indicates a subscriber that processes slower than the write rate:

```rust
let pressure = store.dispatch_channel_pressure();
let peak = store.dispatch_channel_high_water_mark();

if pressure > ALERT_THRESHOLD {
    // subscriber is falling behind; consider circuit-breaking or shedding
}
```

A nonzero `dispatch_channel_high_water_mark()` that never decreases is normal under burst writes. A value that keeps climbing across reconnect cycles indicates structural backpressure.

Planned — Phase 3 (Pressure-Aware Substrate) will surface back-pressure automatically via `PressureBandChanged` events to `_fossic/system`, removing the polling requirement. Planned — Phase 4 (Adaptive Subscription Delivery) will introduce priority classes with gradient mitigation for sustained overload.

---

## 3. PostCommit Subscription Overflow → Permanent Degradation

### Property

Each PostCommit subscriber has a bounded `crossbeam_channel` delivery queue (`src/subscriptions.rs:138–139`):

```rust
SubscriptionMode::PostCommit { queue_size } => {
    let (tx, rx) = crossbeam_channel::bounded::<StoredEvent>(queue_size);
```

When that queue is full and a new event arrives, the subscriber is immediately and permanently degraded (`src/subscriptions.rs:268–270`):

```rust
Err(crossbeam_channel::TrySendError::Full(_)) => {
    entry.degraded.store(true, Ordering::Release);
    newly_degraded.push(*id);
}
```

The degraded flag is sticky: once set, all subsequent `dispatch_post_commit` calls skip the subscriber (`src/subscriptions.rs:244–246`). There is no drain, no retry, no backoff. The subscriber stops receiving events.

The dispatcher thread emits `SubscriptionDegraded` to `_fossic/system` for each newly-degraded subscriber (`src/store.rs:2141–2153`):

```rust
let newly_degraded = registry.dispatch_post_commit(&event);
if let Some(ref mut writer) = sys_writer {
    for sub_id in newly_degraded {
        writer.emit_subscription_degraded(
            sub_id,
            &event.stream_id,
            &event.branch,
            event.version,
        );
    }
}
```

`SubscriptionDegraded` payload fields: `subscription_id`, `stream_id`, `branch`, `dropped_version` (the version of the event that failed to deliver).

`_fossic/system` events are excluded from delivery by default (`src/subscriptions.rs:238–240`). A subscriber must set `include_system: true` in its `SubscribeQuery` to receive them.

### Why

The alternative — blocking the dispatcher until the subscriber queue drains — would propagate subscriber slowness backward through the entire dispatch path, stalling delivery to all other subscribers and eventually blocking `store.append()` when the unbounded dispatch channel becomes the bottleneck. Binary degradation avoids this at the cost of losing the slow subscriber.

### Where it bites

- **Subscribers that briefly stall.** A GC pause, a slow disk write, or a network hiccup that outlasts the queue size causes permanent degradation. The subscriber would have caught up, but it doesn't get the chance.
- **Burst workloads with a subscriber that would drain normally.** A relay agent that can process 100 events/second falls permanently behind during a 200 events/second burst if the queue fills before the burst ends.
- **Silent failure without `_fossic/system` monitoring.** Without a `SubscriptionDegraded` consumer, degradation is invisible. The subscriber silently stops receiving events; the relay silently stops relaying; the hub silently falls behind.
- **Queue size set too small.** `queue_size: 100` sounds generous for a smooth workload but may saturate in under a second during bursts. There is no dynamic expansion.

### Mitigation

Subscribe to `_fossic/system` with `include_system: true` and watch for `SubscriptionDegraded`:

```rust
let sys_sub = store.subscribe(SubscribeQuery {
    stream_pattern: "_fossic/system".to_string(),
    branch: "main".to_string(),
    include_system: true,   // required — system events are filtered by default
}, SubscriptionMode::PostCommit { queue_size: 64 }, handler);
```

On `SubscriptionDegraded` receipt: read `dropped_version` from the payload, re-subscribe from that cursor, and replay the gap. The `SubscriptionHandle::is_degraded()` accessor lets you also poll without a separate subscription.

For relay agents, the pattern is:

```rust
loop {
    let handle = store.subscribe(pattern, mode, handler);
    // ... process events ...
    if handle.is_degraded() {
        let resume_version = get_last_processed_version();
        // re-subscribe from cursor
        drop(handle);
        continue;
    }
}
```

Size the queue to absorb realistic burst depth: `queue_size` should be at least `peak_write_rate_per_second × expected_max_handler_latency_seconds × safety_factor`. For a handler that may stall 1 s during GC and a write rate of 50 events/s, `queue_size: 500` is a reasonable floor.

Planned — Phase 4 will replace binary degradation with gradient degradation: coalescing and sampling before the hard cut, and per-subscription priority classes so critical system consumers don't saturate under the same queue pressure as background subscribers.

---

## 4. SystemStreamWriter Is Single-Thread-Owned

### Property

`SystemStreamWriter::emit` takes `&mut self` (`src/system_stream.rs:43–47`):

```rust
pub fn emit(
    &mut self,
    event_type: &str,
    payload: &serde_json::Value,
    indexed_tags: Option<&serde_json::Value>,
) {
```

The struct holds a raw `rusqlite::Connection` with no internal synchronization (`src/system_stream.rs:17–19`):

```rust
pub struct SystemStreamWriter {
    conn: Connection,
}
```

Only one caller can call `emit` at a time per instance. Multiple callers sharing one instance must synchronize externally.

**Visibility note:** `SystemStreamWriter` was promoted to `pub` and re-exported from the crate root in v1.7.0 (`pub use system_stream::SystemStreamWriter` in `src/lib.rs:32`). Prior to v1.7.0 it was `pub(crate)` and only usable within the fossic crate itself. As of v1.7.0, sibling crates and external integrators can construct their own instances.

**Established pattern:** the substrate holds four `SystemStreamWriter` instances as of v1.7.0:

| Owner | Location | Synchronization |
|---|---|---|
| Dispatcher thread | `store.rs` `start_dispatcher` fn | Directly owned; no Mutex needed (single thread) |
| Reducer system writer | `store.rs:152` StoreInner field | `parking_lot::Mutex<Option<SystemStreamWriter>>` (lazy init) |
| Project registry writer | `store.rs:161` StoreInner field | `parking_lot::Mutex<Option<SystemStreamWriter>>` (lazy init) |
| Background executor | `executor.rs:228` | `Option<SystemStreamWriter>` (lazy init, executor thread sole owner) |

The dispatcher and background executor own their instances directly (no Mutex) because they are the sole callers on dedicated threads. The store-held writers use `Mutex<Option<>>` because they may be called from arbitrary application threads through public API.

### Why

The system stream is a low-frequency audit channel. Avoiding internal locking keeps `emit` a minimal, best-effort path: a single dedicated connection, one `IMMEDIATE` transaction per event, no contention with the main write path. The `&mut self` signature makes the ownership requirement visible at compile time.

### Where it bites

- **Sibling crates sharing a single instance across threads.** A `fossic-coordinator` that receives events from multiple projects and wants to emit `ProjectRegistered` from a dispatch callback shared across threads cannot pass one `SystemStreamWriter` through an `Arc` — it needs external locking or one instance per thread.
- **Integrators constructing a `SystemStreamWriter` in a multi-threaded context.** The type is now accessible (`pub` at v1.7.0), which means integrators can create instances — but the compiler will reject sharing one across threads without `Mutex`.
- **Calling `emit` from an async context.** `Connection` is not `Send` across await points. `emit` is synchronous and best called from a dedicated OS thread or inside `spawn_blocking`.

### Mitigation

For any context where `emit` may be called from more than one thread, use the established substrate pattern: lazy initialization behind a `parking_lot::Mutex<Option<SystemStreamWriter>>`:

```rust
struct MyExtension {
    db_path: PathBuf,
    sys_writer: parking_lot::Mutex<Option<SystemStreamWriter>>,
}

impl MyExtension {
    fn emit_event(&self, event_type: &str, payload: &serde_json::Value) {
        let mut guard = self.sys_writer.lock();
        if guard.is_none() {
            *guard = SystemStreamWriter::new(&self.db_path);
        }
        if let Some(ref mut writer) = *guard {
            writer.emit(event_type, payload, None);
        }
    }
}
```

Lazy initialization defers the connection open until the first emission, which keeps `new` out of hot paths and tolerates stores that may not exist yet at construction time. If open fails, `new` returns `None` (with a WARN log); subsequent calls re-attempt.

---

## 5. Event IDs Are Content Hashes, Not Timestamps

### Property

Event IDs are 32-byte BLAKE3 hashes (`src/cce.rs:145`):

```rust
Ok(*blake3::hash(&buf).as_bytes())
```

They sort lexicographically as bytes. A lexicographic sort of event IDs does not produce a chronological order, a version order, or any order that reflects when events occurred. The relationship between two event IDs is purely structural (one caused the other, or they share a correlation ID) not temporal.

The version column (`events.version`, a monotonic integer per `(stream_id, branch)`) is the correct ordering signal within a stream. `timestamp_us` is the correct cross-stream temporal signal, though it is subject to clock drift and is not monotonic across machines.

### Why

Content-addressed identity provides idempotent append, deterministic relay, and verifiable causation linkage without a distributed coordinator. These properties require the ID to be a function of content, not a function of time or position.

### Where it bites

- **Custom cursor designs using event ID as an ordering cursor.** `WHERE id > ?` in a custom query does not give events after a certain time — it gives events whose BLAKE3 hash is lexicographically later, which has no useful semantics.
- **Pagination assuming "next ID" is chronologically next.** Iterating by event ID to paginate a stream will return events in a non-time, non-version order, likely skipping and duplicating entries from a human perspective.
- **Code comparing event IDs as integers or strings.** `id_a > id_b` is meaningful only in the sense of byte-array comparison; it carries no temporal or logical ordering information.
- **Assuming causation chains are monotonically ID-ordered.** A child event's ID is derived from its payload, which includes the parent's ID as `causation_id`. The parent's hash is a function of the parent's content; the child's hash is a function of the child's content. Neither will generally be numerically larger or smaller.

### Mitigation

Sort and paginate by `(stream_id, branch, version)` within a stream, and by `timestamp_us` for cross-stream ordering:

```rust
// Paginate within a stream — version is the correct cursor
let query = ReadQuery::stream("my/stream")
    .after_version(last_seen_version);

// Order events from multiple streams by time
events.sort_by_key(|e| e.timestamp_us);
```

Use event IDs only for what they're designed for:
- **Identity:** `store.read_one(event_id)` — fetch the exact event
- **Content-addressed lookup:** `store.read_by_external_id(stream_id, event_id.hex())` — idempotency check
- **Causation linkage:** `store.walk_causation(root_id, ...)` — causal graph traversal
- **Correlation grouping:** `store.read_by_correlation(correlation_id)` — events sharing a session/request

The substrate-provided `TruncationCursor` handles ordering correctly internally. Only custom cursor implementations are at risk.

---

## 6. Aggregate Trait Lacks Resume Support

### Property

`aggregate_bounded` returns `ReadOutcome::Truncated` with `cursor: None` on truncation (`src/cross_stream.rs:654–661`):

```rust
if exceed_count || exceed_bytes {
    let reason = if exceed_count {
        TruncationReason::ResultCount
    } else {
        TruncationReason::ByteSize
    };
    let data = agg.clone().finalize();
    return Ok(ReadOutcome::Truncated { data, cursor: None, reason });
}
```

The `Aggregate` trait (`src/cross_stream.rs:30–34`) has no `restore` method:

```rust
pub trait Aggregate: Send + Sync + 'static {
    type Output;
    fn fold(&mut self, event: &StoredEvent);
    fn finalize(self) -> Self::Output;
}
```

Resuming a fold from partial state would require injecting the partial aggregator into a new instance — a capability the trait doesn't currently express. The source comment (`src/cross_stream.rs:558–561`) notes this explicitly: "No resume cursor is produced (`cursor: None`). Fold-resume requires re-feeding partial state into a new aggregator instance — not yet supported by `Aggregate`."

This gap is known and deferred to v2.x. Adding `restore(state_bytes: &[u8]) -> Result<Self>` to `Aggregate` (plus snapshot serialization in the bounded implementation) is the required shape, but it's a trait-break that belongs in the next major version.

### Why

The current `Aggregate` trait is intentionally minimal: `fold` + `finalize`, no serialization. Adding resume requires either a serializable state format (making the trait heavier) or an external state-injection mechanism. Neither is trivial; both have downstream implications for all existing `Aggregate` implementations.

### Where it bites

- **Large-dataset aggregations that exceed the result count.** An aggregation over a multi-month event log that truncates at 10,000 events has no way to continue from where it stopped. The caller must either raise the limit (accepting potential OOM) or restructure.
- **Aggregations where `ReadOutcome::Truncated` is treated as complete.** Code that doesn't check `is_truncated()` silently undercomputes the aggregate.
- **Aggregations where the byte budget triggers early.** Large payloads can cause truncation after very few events, and there's no way to continue without re-scanning from the beginning.
- **Test assertions on aggregate completeness.** An aggregate test that passes at data scale 100 and truncates at scale 10,000 gives a false green.

### Mitigation

Use streaming iterators with external state accumulation for any fold that may exceed limits:

```rust
// Instead of aggregate_bounded:
let mut count = 0u64;
let mut sum = 0i64;

for event in store.read_range_iter(ReadQuery::stream("my/stream")) {
    let event = event?;
    let payload: serde_json::Value = rmp_serde::from_slice(&event.payload)?;
    count += 1;
    sum += payload["value"].as_i64().unwrap_or(0);
}
```

`read_range_iter` and `walk_causation_iter` implement `Iterator<Item = Result<StoredEvent>>`. Each `next()` fetches a batch of 100 events and releases the read-pool connection between batches — a pool of size 1 can serve concurrent readers while an iterator is live.

For pause/resume, track the last-consumed event version externally and construct a fresh `ReadQuery` from that cursor:

```rust
let mut last_version: i64 = 0;

// First pass
for event in store.read_range_iter(ReadQuery::stream("my/stream")) {
    let event = event?;
    last_version = event.version as i64;
    fold(event);
}

// Resume after restart
for event in store.read_range_iter(
    ReadQuery::stream("my/stream").after_version(last_version as u64)
) {
    // ...
}
```

If you need a true bounded fold with budget enforcement, wrap the iterator: check budget after each `fold()` call and break manually — your state is already in the accumulator.

---

## Future Additions

The CP (Checkpoint) marker convention tracks documentation-pending items upstream. The following CPs are open at the time of this writing:

- **CP-INTEGRATORS-2** — Multi-store coordination patterns for integrators building on `SystemStreamWriter` (now `pub` at v1.7.0): stream ownership, event routing discipline, and the `_fossic/system` protocol for custom event types.
- **CP-FOSSIC-2** — OpenOptions Python binding gap: `default_max_results` and `default_max_bytes` are not yet exposed via the Python `OpenOptions` wrapper. Consumers using fossic-py cannot set store-level budget defaults from Python.
- **CP-PATTERN-1** — Relay protocol documentation: the D.3 conditional-strip rule, external_id idempotency contract, `source_store` indexed_tag convention, and causation-id translation across store boundaries. Currently undocumented outside source comments in `fossic-py/python/fossic/relay.py`.
- **CP-T2-2** — Federation protocol spec: full `fossic-coordinator` subscriber model, multi-store topology, and ProjectRegistered/RelayHeartbeat consumer contract. Currently documented only as event type rows in `docs/implement/FOSSIC_V1_SPEC.md §9.4`.

This document is updated as substrate properties surface during development. Open a CP marker and reference it here when a new property warrants permanent documentation.

---

*Document version: 1.0 (2026-06-21). Substrate version at time of writing: v1.8.1.*
