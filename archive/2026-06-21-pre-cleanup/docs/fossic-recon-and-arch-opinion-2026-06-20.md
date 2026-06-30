# Fossic Reconnaissance + Architectural Opinion Report

**Prepared for:** cross-Claude architectural handoff
**Date:** 2026-06-20
**Source files read:** `src/store.rs`, `src/subscriptions.rs`, `src/wal_watch.rs`, `fossic-py/python/fossic/__init__.py`, `src/reducers.rs`, `cerebra/cognition/catalyst.py`, `cerebra/cognition/clutch.py`, `docs/FOSSIC_TIDYUP_SURVEY.md`, `docs/aseptic/TECH_DEBT.md`

---

## Part A — Reconnaissance

### A1. `src/store.rs`

#### `StoreInner` struct (lines 67–86)

```rust
// lines 67–86
struct StoreInner {
    conn: Mutex<Connection>,          // the single write connection
    #[allow(dead_code)]
    path: PathBuf,                    // stored but never read back — dead field (TIDYUP H1)
    options: OpenOptions,             // full OpenOptions retained for read_pool_size / timeout_ms
    transforms: RwLock<Vec<TransformEntry>>,
    upcasters: RwLock<UpcasterRegistry>,
    sub_registry: Arc<SubscriptionRegistry>,
    dispatch_tx: crossbeam_channel::Sender<StoredEvent>,  // unbounded — see below
    _wal_watcher: Option<WalWatcher>,
    branch_cache: RwLock<BTreeMap<(String, String), Vec<BranchSegment>>>,
    reducers: RwLock<ReducerRegistry>,
    similarity_provider: Option<Arc<dyn crate::similarity::SimilaritySearchProvider>>,
    read_pool_rx: crossbeam_channel::Receiver<Connection>,
    read_pool_tx: crossbeam_channel::Sender<Connection>,
}
```

#### `Store::open` (lines 101–199)

```rust
pub fn open(path: impl AsRef<Path>, options: OpenOptions) -> Result<Self, Error>
```

Defaults baked in vs configurable:
- `pool_size = options.read_pool_size.max(1)` — pool of `pool_size` read connections, all opened with `PRAGMA query_only = ON`. **Configurable.**
- `PRAGMA synchronous = NORMAL`, `busy_timeout = 30000` — hardcoded. Not configurable.
- `encryption` and `checkpoint_mode` are checked and both return `NotImplemented` for anything non-default — configuration exists structurally but is unimplemented.
- WAL watcher failure is soft: `map_err(|e| eprintln!(...)).ok()` — a watcher start failure yields `_wal_watcher: None` and no error to the caller. Silent degradation.

**Key structural note — `dispatch_tx` is unbounded** (line 151):
```rust
let (dispatch_tx, dispatch_rx) = crossbeam_channel::unbounded::<StoredEvent>();
```
Post-commit events from `append` are sent to this channel; the dispatcher thread drains it. Under write bursts, this channel can grow without bound. There is no backpressure from the dispatcher to the append path. This is the correct choice for non-blocking appends, but it means a slow `SubscriptionDegraded` write can cause unbounded queue growth between append and `write_degraded_event`.

#### `SubscriptionRegistry` placement
Held as `Arc<SubscriptionRegistry>` in `StoreInner.sub_registry` (line 74), cloned into the WAL watcher and dispatcher thread at `Store::open`. The Arc ownership is clean — all three paths share one registry.

#### WAL watcher start (lines 155–163)
```rust
let wal_watcher = WalWatcher::start(
    path.clone(),
    dispatch_tx.clone(),
    Arc::clone(&sub_registry),
).map_err(...).ok();
```

#### System-stream emission helpers
Only one: `write_degraded_event` (lines 1158–1209) — writes `SubscriptionDegraded` to `_fossic/system` with `sub_id`, `stream_id`, `branch`, `dropped_version`. Written from the dispatcher thread via a dedicated `sys_conn` opened at dispatcher spawn. No other system-stream emission helpers exist. There is no `SystemStreamWriter` abstraction — this is currently ad-hoc.

#### Pressure-relevant signals
**None.** No queue-depth metrics on `dispatch_tx`, no timing measurements, no counters on the read pool. `SubscriptionHandle::queue_depth()` and `queue_capacity()` expose per-subscription PostCommit channel depth (delegating to `queue_info` on the registry), but nothing aggregate. No instrumentation point exists above the per-subscription level today.

---

### A2. `src/subscriptions.rs`

#### `SubscriptionRegistry` (lines 107–110)
```rust
pub(crate) struct SubscriptionRegistry {
    entries: parking_lot::RwLock<HashMap<u64, SubscriberEntry>>,  // NOT std::sync
    next_id: AtomicU64,
}
```
Uses `parking_lot::RwLock`, not `std::sync::RwLock`. This is a meaningful choice — parking_lot is not in the `fossic` core Cargo.toml as a named dependency (check: it must be a transitive dep). If `parking_lot` is ever removed upstream, this silently breaks.

#### `SubscriptionHandler` trait (lines 42–44)
```rust
pub trait SubscriptionHandler: Send + Sync + 'static {
    fn on_event(&self, event: &StoredEvent);
}
```
Simple, no async, no error return. Panics are caught in `dispatch_sync` via `catch_unwind`; there is no panic handling in the PostCommit per-subscription thread (the thread just dies if the handler panics, which closes the channel and silently stops delivery without marking degraded).

#### `SubscriptionMode` (lines 10–17) — confirmed
```rust
pub enum SubscriptionMode {
    Synchronous,
    PostCommit { queue_size: usize },
}
```
Exactly two variants. Nothing else.

#### `dispatch_sync` (lines 178–216)
Iterates all `Synchronous` entries matching branch + glob + non-system filter. Wraps each call in `catch_unwind`. On panic: marks `degraded = true`, logs to stderr. Does NOT return newly-degraded IDs (returns nothing). No cursor advancement — sync subscriptions have no cursor.

#### `dispatch_post_commit` (lines 227–308)
Two-pass design: read-lock pass to try_send + collect `delivered` vec; then write-lock pass to advance cursors. Returns `Vec<u64>` of newly-degraded IDs.

```rust
// lines 259–275 — the overflow → degrade path
match tx.try_send(event.clone()) {
    Ok(_) => { delivered.push(...); }
    Err(TrySendError::Full(_)) => {
        entry.degraded.store(true, Ordering::Release);
        newly_degraded.push(*id);
    }
    Err(TrySendError::Disconnected(_)) => { /* silently skip */ }
}
```

On `Full`: subscription is immediately and permanently marked degraded. There is no retry, no backoff, no partial drain. Once degraded, the subscription is dead until the consumer detects `is_degraded()` and re-subscribes.

**Cursor ownership invariant** — explicitly documented in the comment at line 222–226. Confirmed: only `dispatch_post_commit` advances cursors. Glob subscriptions use per-(stream_id, branch) `stream_cursors` map; exact subscriptions use `wal_cursor`.

**`update_wal_cursor` — absent.** The dead-code function cited in TIDYUP Issue 5 has been removed. The invariant comment is now the documentation.

**Scalability concern with glob cursors:** `post_commit_cursors()` returns `min(stream_cursors.values())` for glob subscriptions (lines 338–344). If any matched stream has cursor -1 (e.g., a new stream seeded at subscription time with no events), the WAL watcher will fetch from version -1 on all matching streams every WAL tick. The per-stream cursor filter in `dispatch_post_commit` prevents double-dispatch, but the SQLite fetch cost is still O(all events since version -1 on those streams). This is a latent O(N) fetch per WAL tick for glob subscriptions with uneven stream depths.

---

### A3. `src/wal_watch.rs`

#### `notify::RecommendedWatcher` setup (lines 39–56)
```rust
let mut watcher = notify::RecommendedWatcher::new(
    move |res| { let _ = notify_tx.send(res); },
    notify::Config::default()
)?;
let watch_dir = db_path.parent()
    .filter(|p| !p.as_os_str().is_empty())
    ...unwrap_or(PathBuf::from("."));
watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
```
Watches the **parent directory** non-recursively. This catches WAL file creation and modification. The scan thread then filters for events whose path matches the `-wal` suffix (lines 113–119).

#### `data_version` polling (lines 97–129)
```rust
let mut last_data_version: i64 = conn
    .query_row("PRAGMA data_version", [], |r| r.get(0))
    .unwrap_or(-1);
// ...
let current_version: i64 = conn
    .query_row("PRAGMA data_version", [], |r| r.get(0))...;
if current_version == last_data_version { continue; }  // checkpoint/truncation, no new data
last_data_version = current_version;
```
`data_version` increments each time another connection commits a write visible to this reader. Comparing against last-seen guards against false positives from WAL checkpoints (which touch the WAL file but don't bump `data_version`). Clean design.

#### `run_scan_loop` (lines 78–197)
Opens its own read connection at startup. On each relevant WAL event: reads `post_commit_cursors()`, expands globs to actual stream IDs via `list_all_streams`, builds `group_min` map per `(stream_id, branch)`, fetches new events via `fetch_events_after`, fans each event through `dispatch_tx.send(event)`. Exits when `dispatch_tx.send` fails (store shutdown).

**Cursor invariant confirmed:** lines 135–137 carry the explicit comment: "The WAL watcher never advances subscription cursors — `dispatch_post_commit` owns cursor advancement." The watcher sends events through `dispatch_tx` to the dispatcher thread, which routes them through `dispatch_post_commit`.

**Timing measurements: none.** No latency measurement from WAL event to dispatch. No histogram of events-per-scan. No instrumentation.

---

### A4. `fossic-py/python/fossic/__init__.py`

#### Python `register_reducer` / `read_state` (lines 434–453)
```python
def register_reducer(self, pattern: str, reducer: Any) -> None:
    self._inner.register_reducer(pattern, reducer)

def read_state(self, stream_id: str, branch: str = "main") -> Any:
    return self._inner.read_state(stream_id, branch)
```
**Both delegate entirely to the Rust extension.** The TIDYUP survey's Issue 1 (`read_state` doing full Python-side replay with no snapshot caching) has been resolved. The Rust `DynReducer` path is used; snapshot caching runs in Rust. TD-001 remains (the PyO3 bridge overhead of ~47μs/event), but the correctness gap (no caching) is closed.

#### `Subscription` iteration protocol (lines 184–199)
```python
def __next__(self) -> "StoredEvent":
    while True:
        event = self._inner._wait_for_next_event(1.0)  # 1-second timeout, releases GIL
        if event is None:
            if self._unsubscribed:
                raise StopIteration
            continue
        return event
```
Polling loop with 1-second GIL-releasing timeouts. Consequence: after `unsubscribe()` is called, iteration can take up to 1 second to notice. Not a bug but a notable latency bound.

#### Error mapping
15 named exception classes (plus `FossicError` base), all defined in Rust via `pyo3::create_exception!` and registered in the extension module. Python callers can catch specific exceptions: `StreamNotDeclaredError`, `ReducerPatternAmbiguousError`, etc. Exceptions flow directly from Rust `Error` variants via the `to_py_err` function in `fossic-py/src/errors.rs`.

---

### A5. `src/reducers.rs`

#### `pub trait DynReducer` (lines 48–54) — confirmed
```rust
pub trait DynReducer: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> u32;
    fn state_schema_version(&self) -> u32;
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error>;
    fn apply_bytes(&self, state_bytes: &[u8], event_payload: &[u8]) -> Result<Vec<u8>, Error>;
}
```
Msgpack bytes in, msgpack bytes out. Public. This is the FFI boundary for foreign-language reducers.

#### `BoxedReducer` — confirmed `pub(crate)`
Internal type-erasure trait. Python/Node never see this directly; they go through `DynReducerAdapter`.

#### Specificity scoring and ambiguity detection (lines 126–135, 173–188)
```rust
let spec = crate::glob::specificity_score(pattern);
for existing in &self.entries {
    if patterns_may_overlap(pattern, &existing.pattern) && spec == existing.specificity {
        return Err(Error::ReducerPatternAmbiguous {
            a: existing.pattern.clone(),
            b: pattern.to_string()
        });
    }
}
```
`ReducerPatternAmbiguous` fires only when two patterns have equal specificity AND may overlap. The overlap check (`patterns_may_overlap`, lines 203–225) is conservative: any pattern containing `**` is assumed to overlap with everything. You cannot register two `**`-containing patterns at all, even over disjoint subtrees. Overly conservative for complex cases, but safe.

#### `SnapshotPolicy::EveryNEvents(100)` — confirmed absent
No `SnapshotPolicy` enum, no automatic snapshot triggering. `take_snapshot` must be called explicitly. The spec's promise of auto-snapshotting is fiction in the current code. This is the architectural gap a background executor would fill.

---

### A6. `cerebra/cognition/catalyst.py`

#### The actual selection formula (lines 111–127)
```python
# This is NOT standard UCB1 — it is a confidence-weighted diversity scorer
base_reward = arm_stats.mean_reward if arm_stats.count > 0 else _BASE_REWARD_DEFAULT
type_penalty = max(0.5, 1.0 - (recent_types.count(arm.type) * 0.15))  # diversity term
confidence_ramp = min(1.0, arm_stats.count / 5.0)  # linear ramp, not sqrt exploration
score = base_reward * type_penalty * confidence_ramp
```

**Architectural flag:** The docstring says "UCB1" but `CatalystEngine.select()` does not implement UCB1. UCB1 is `μ_i + c√(ln(N)/n_i)`. This formula has no exploration term and no global pull count. The `Bandit` primitive (`exploration_weight=1.4`) has UCB1 internals, but `select()` bypasses the bandit's own `select()` method entirely — it only reads `get_stats()` for count and mean_reward. After the forced-exploration phase (all arms sampled once), the engine is purely exploitative with a diversity nudge. An arm that got lucky early will dominate.

#### Reward signal
```
reward = composite_score × confidence  (both from EvaluationPacket)
```
Reward is derived from cerebra's evaluation pipeline, not directly from fossic events.

#### Arm state persistence
SQL-backed via `catalyst_arm_stats` table in cerebra's SQLite DB. Each `record_reward` call opens and closes a fresh `sqlite3.connect()`. Under high step rates, this is N connection open/close per cycle — not pooled. The `catalyst_recent_selections` table tracks the type-diversity window.

#### Arms defined in `CycleConfig`
`CatalystArm` objects are constructed at cycle config time, not from YAML. The engine receives them as `list[CatalystArm]` at construction — pure code configuration.

#### Catalyst invocation
Catalyst fires when clutch returns `escalate_to_catalyst=True` (i.e., no clutch rule matched). Clutch handles the structured, rule-covered decision space; catalyst handles everything else.

---

### A7. `cerebra/cognition/clutch.py`

#### Cascade structure (lines 234–250)
**First-match-wins.** Rules evaluated in config order (index 0 first). `cascade_depth` records which rule fired (0-indexed). No scoring, no weighted combination.

```python
for idx, rule in enumerate(self.rules):
    predicate = self._predicates[rule.predicate_name]
    if predicate(context, rule.parameters):
        return ClutchDecision(
            action=rule.action,
            rule_matched=rule.name,
            cascade_depth=idx,
            escalate_to_catalyst=False
        )
# No match:
return ClutchDecision(
    action="accept",
    rule_matched="default_no_match",
    cascade_depth=len(self.rules),
    escalate_to_catalyst=True
)
```

#### Rule definition
Rules are defined in `CycleConfig.clutch_rules` — pure Python code at cycle-config construction time. No YAML, no runtime-loaded config.

#### Escalation predicate
`escalate_to_catalyst=True` when and only when no rule matches. The `catalyst_was_invoked` predicate (lines 191–193) lets rules react to the *prior step's* catalyst invocation — it reads `ctx.cycle_state.catalyst_invoked_this_step`, which despite the name tracks whether catalyst ran on the *previous* step (via `ClutchCycleState`).

#### Confirmed built-in predicates (from `BUILTIN_PREDICATES` dict, lines 199–216)
Phase 8 originals: `at_terminal_step`, `composite_below_threshold`, `composite_above_threshold`, `first_step`, `step_index_at`, `always`. Phase 9 additions: `prediction_severe_miss`, `prediction_notable_miss`, `signal_below_threshold`, `signal_above_threshold`, `consecutive_steps_below_floor`, `prior_step_action_was`, `step_at`, `catalyst_was_invoked`. 14 total.

---

## Part B — Architectural Angles

### B1. Cross-platform hardware detection for `OpenOptions` defaults

**Options and tradeoff axis:** Precision of auto-tuning vs. dependency cost vs. portability vs. long-term maintenance surface.

Option (a) `/proc/meminfo` + `/sys/block/.../rotational` is zero-dependency and precise on Linux but completely absent on macOS and Windows. Given Tauri deployment targets, Linux-only is a non-starter.

Option (b) `sysinfo` is cross-platform and accurate but adds ~150KB and a new dependency to a codebase with zero-dep tolerance. Under the active 2025–2026 supply-chain threat model, each new dep is a real attack surface — this cost isn't hypothetical.

Option (c) per-OS `cfg` shims is essentially (a) extended with macOS `sysctl -n hw.memsize` and Windows `GlobalMemoryStatusEx` — achievable with `unsafe` stdlib calls and no new deps, but it's ~200 lines of platform-specific unsafe code you own forever.

**Ship option (d) now, wire option (c) later.** Define `HardwareProfile` as a plain struct: `total_ram_gb: Option<f64>`, `logical_cpus: Option<usize>`, `storage_class: StorageClass` (enum NVMe/SSD/HDD/Unknown). Add `HardwareProfile::detect() -> Self` behind a `#[cfg(not(target_os = "unknown"))]` guard that returns `Unknown` everywhere until implemented. `OpenOptions` gets `hardware_profile: Option<HardwareProfile>` — `None` means use current hardcoded defaults, `Some(p)` means tune from profile.

The reason to defer detection: the *tuning formulas* are more valuable right now than the detection code. Define `read_pool_size = clamp(cpus / 2, 2, 8)`, queue capacity from RAM class, as pure functions of `HardwareProfile`. Test the formulas against manually-constructed profiles. Once formulas are correct, wiring detection is a single function addition with no consumer impact — the API shape is already there. Option (c)'s platform-specific code should land as its own PR with its own test surface, not entangled with tuning logic.

---

### B2. Coalescing mechanics for adaptive subscription delivery

**Options and tradeoff axis:** Cursor correctness vs. implementation invasiveness vs. dispatch latency contract.

Option (a) pre-enqueue squash requires peeking the tail of a bounded `crossbeam_channel`, which has no peek operation. Switching to `VecDeque<StoredEvent>` behind a `Mutex` would work but is a structural change to the queue type across all PostCommit subscriptions — high blast radius.

Option (c) windowed batching changes the delivery contract: consumers currently receive events one-by-one via `on_event`. Batching requires either a new `on_batch` method or a different handler trait shape. Too invasive for an adaptive pressure response.

**Ship option (b): post-enqueue dedup at the receiver thread, with explicit cursor advancement for all coalesced versions.**

Implementation shape: the handler thread drains its channel into a `Vec<StoredEvent>`, groups by `event_type`, keeps only the latest per type, but tracks *all* versions seen. The handler receives only the latest event; the subscription's cursor advances over all coalesced versions. This requires a new method on `SubscriptionRegistry` — `fn advance_cursor(sub_id, stream_id, branch, version)` — called by the handler thread after processing each coalesced batch. `dispatch_post_commit` continues to own cursors for normal delivery; the handler thread owns them only during coalesced delivery. The two paths must not race: the invariant is that cursor always reflects the *maximum version seen or skipped*, never a gap.

Coalescing should be pressure-gated: activate only when `queue_depth() > threshold * queue_capacity()` (e.g., 70%). At low pressure, every event delivers individually, preserving current behavior. The threshold should be exposed via `OpenOptions` so consumers can tune the sensitivity. The mode transition itself (normal → coalescing) should emit a `SubscriptionCoalescingStarted` event on `_fossic/system` — observable without polling.

---

### B3. Sampling cursor semantics

**Current cursor system analysis:** `wal_cursor` (exact subscriptions) and `stream_cursors` (glob subscriptions) represent "the highest version successfully dispatched." There is no concept of "versions advanced past without dispatching." The WAL watcher's `fetch_events_after(min_cursor)` fetches all events since the last dispatched version — skipped events under sampling would be re-fetched and re-delivered on reconnect. Replay diverges from initial delivery.

**Minimal additive change:** Add `skip_cursor` alongside `wal_cursor` in `SubscriberKind::PostCommit`:

```rust
wal_cursor: i64,   // highest version dispatched to handler
skip_cursor: i64,  // highest version sampled-over (≥ wal_cursor under pressure)
```

Sampling logic: when a Background-priority subscription is sampled (1-in-N), advance `skip_cursor = max(skip_cursor, event.version)` without advancing `wal_cursor`. The WAL watcher's `fetch_events_after` uses `max(wal_cursor, skip_cursor)` as the scan floor — sampled events are not re-fetched. Cursor monotonicity is preserved: `skip_cursor` is non-decreasing, and `wal_cursor` never exceeds `skip_cursor`.

**Resume-from-cursor semantics:** When a consumer presents a cursor for reconnect, expose `skip_cursor` to them as `last_version_seen()` on `SubscriptionHandle`, distinct from `is_degraded()`. Consumer code can replay the gap via `read_range(from=wal_cursor+1, to=skip_cursor)` or accept it. Fossic must not silently hide the gap — that erodes the trust model. Emit a single `SubscriptionSamplingStarted` event on `_fossic/system` at the moment sampling engages (not per-skipped-event — that would flood the system stream). This gives consumers a durable signal they can query: "was this subscription ever sampled?" The answer is in the event log forever.

---

### B4. Catalyst persistence across `Store` reopens

**Options and tradeoff axis:** Correctness and cold-start cost vs. architectural purity vs. operational simplicity.

Option (a) in-memory cold-start wastes all learning on every restart. In a system with frequent short cerebra sessions, re-learning the arm landscape constantly is unacceptable for a real adaptive system.

Option (b) sidecar JSON is simple but introduces write-on-every-reward to a file outside SQLite's ACID guarantees. A crash mid-write corrupts state.

Option (c) SQLite table in fossic's store is ACID-safe but puts cerebra's cognitive state inside an event store that is conceptually fossic's substrate — ownership is muddied.

**Ship option (d): emit to `_fossic/system`, reconstruct on open, with periodic snapshots.**

The conceptual purity argument is real and aligns with fossic's identity. The startup cost deserves honest accounting: 10,000 `CatalystArmUpdated` events at ~50K reads/sec is ~200ms cold-start — noticeable. The mitigation is already in fossic's architecture: register a reducer on `_fossic/system` over `CatalystArmUpdated` events and call `take_snapshot` every 100 updates. Cold-start becomes: load snapshot + replay events since snapshot — worst case ~100 events, sub-millisecond.

The secondary argument for (d): arm evolution history is auditable. You can replay bandit state to any point in time and ask "what would catalyst have selected at cycle #500?" Options (b) and (c) destroy this history. For a system built on event sourcing, discarding adaptation history is a missed opportunity — it's exactly the kind of derivable state the architecture was designed to preserve.

`CatalystArmUpdated` belongs in `AGENT_TRACE_VOCABULARY.md`. Cerebra owns the reconstruction logic; fossic stores the events. Clean ownership boundary.

---

### B5. Background executor shutdown discipline

**Options and tradeoff axis:** Data safety vs. predictable `Drop` latency vs. implementation complexity.

Option (a) drain-all is correct but makes `Drop` unbounded. Rust's `Drop` has no async support; blocking `Drop` is a footgun in any async context. A store holding 50 pending auto-snapshots blocks the calling thread for seconds.

Option (d) drop-immediately-log is inappropriate for auto-snapshot or GC tasks — silent data loss is unacceptable when a task was explicitly enqueued.

**Ship option (b) with grace period, plus targeted option (c) for deadline-critical tasks.**

Shutdown contract:
1. Signal the executor thread to stop accepting new tasks.
2. Drain with a configurable timeout — default 2s in tests, 10s in production. Most deferred tasks (snapshot writes, GC) complete in milliseconds; the timeout covers normal load.
3. At timeout, for remaining tasks: if the task carries `persist_on_drop: true`, serialize the task descriptor as a `DeferredTaskDropped` event on `_fossic/system`. On next `Store::open`, read pending `DeferredTaskDropped` events and re-enqueue. Tasks without the flag (e.g., speculative GC passes) are dropped with a log line.

This avoids a custom queue serialization format — it uses the existing event store. The event log already survives crashes; persisted tasks get the same durability guarantee for free.

**Deadline escalation:** Tasks carry `soft_deadline: Instant` and `hard_deadline: Instant`. The executor runs tasks ordered by `soft_deadline`. When `soft_deadline` is imminent (<100ms) and no quiescent window has appeared, the task should escalate to inline execution on the enqueuing thread rather than waiting for the background thread. The condition for safe inline execution: the task must not acquire any lock the enqueuing thread already holds. For auto-snapshots and GC this is true (they acquire their own connections). For anything touching `sub_registry`, verify carefully before marking it inline-safe.

---

### B6. Hub Coordinator project discovery

**Options and tradeoff axis:** Friction vs. trust vs. scalability vs. architectural purity.

Options (a) hardcoded and (d) auto-scan are both wrong. Hardcoded doesn't survive project addition. Auto-scan produces false positives from `.git` internals, test fixtures, and abandoned stores — filesystem spelunking is not a trust model.

Option (e) event-sourced discovery is architecturally the most elegant: on first hub-direct write, a project emits `ProjectRegistered` on `_fossic/system` with its store path, name, and relay config. The coordinator subscribes to `_fossic/system` on the hub and dynamically opens new stores as they announce themselves. This is the highest-trust option and the correct multi-machine federation story — a remote worker with no local filesystem access can still participate by writing `ProjectRegistered` via the relay protocol.

**Ship option (b) now, design toward option (e) as the federation matures.**

For a single-developer workstation with 5–7 known projects, `~/.lattica/fossic/project-registry.json` (already specified in the federation protocol) is the right frictionless bootstrap. The coordinator reads it at startup; projects update it when their relay agent first runs.

The path to (e) is a hybrid, not a replacement: `project-registry.json` becomes the startup hint list. The coordinator also subscribes to `_fossic/system` on the hub for `ProjectRegistered` events. New projects announce themselves at relay-agent startup by appending `ProjectRegistered` to the hub. The coordinator's subscription picks it up and opens the new store — fully dynamic, no file edit required. The file handles the local single-machine bootstrap case; the event-sourced path handles federation and future multi-machine scenarios. Neither path blocks the other.

---

**Additional architectural flag not covered by the questions:**

The glob subscription scalability concern from A2 — `min(stream_cursors.values())` as WAL scan floor — becomes acute at coordinator scale. If the coordinator runs a `**` glob on each project store, any store with a new stream seeded at cursor -1 causes O(all events on that stream) fetch per WAL tick. The coordinator should subscribe to bounded globs (`ai-stack/**`, `cerebra/**`) or exact streams, not `**`, and manage cursor advancement explicitly per project store. This is the single highest-leverage performance constraint to communicate to any implementor of the coordinator before they write the first subscription call.
