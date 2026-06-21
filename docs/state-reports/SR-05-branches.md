# SR-05 — Branch System

**Series:** Fossic State Reports · Document 5 of 9  
**Scope:** Stream branching — schema, lifecycle states, chain resolution, cache, API, operational patterns  
**Prerequisites:** SR-02 (storage schema), SR-03 (event lifecycle)

---

## 1. What Branches Are

Fossic branches are stream-level divergence points. Every stream starts with one branch named `main`. Additional branches can be created at any point in a stream's version history, allowing alternate event sequences to be written and explored without affecting the canonical `main` branch.

This is deliberately not like git. A fossic branch is:
- **scoped to a single stream** — there is no cross-stream branching concept
- **non-merging** — promoting a branch does not copy its events anywhere; merging is a consumer responsibility
- **append-only within the branch** — events on a closed (promoted or dead_end) branch cannot be added to or removed

The primary use case is agent speculation. An agent that is about to make a decision or attempt a repair can fork the decision stream at the current tip, write speculative events to the branch, evaluate the outcome, and then either promote the branch (recording that the strategy succeeded) or mark it dead_end (discarding it). The events themselves remain in the store either way — fossic is append-only — but new events can no longer be appended to a closed branch.

---

## 2. Branch Schema

```sql
CREATE TABLE branches (
    id              TEXT    NOT NULL,
    stream_id       TEXT    NOT NULL,
    parent_id       TEXT    NOT NULL,    -- 'main' for root branches; branch ID for nested
    parent_version  INTEGER NOT NULL,    -- 0 for root branches
    description     TEXT,
    created_at      INTEGER NOT NULL,    -- microseconds since Unix epoch
    lifecycle       TEXT    NOT NULL DEFAULT 'ephemeral',
    closed_at       INTEGER,
    closed_reason   TEXT,
    alternatives    TEXT,               -- JSON array of sibling branch IDs
    PRIMARY KEY (stream_id, id)
);

CREATE INDEX idx_branches_stream    ON branches(stream_id);
CREATE INDEX idx_branches_lifecycle ON branches(stream_id, lifecycle);
```

### Column-by-column rationale

**`id`** — Consumer-supplied branch identifier, unique per stream. The store does not generate branch IDs; you choose them. Convention: ULIDs for programmatic creation, semantic names like `"repair-attempt-2026-06-20"` for human-created branches. Must pass validation (see §9 Errors) — no characters that break SQL or ambiguously resemble lifecycle state strings.

**`stream_id`** — The stream this branch belongs to. The composite primary key `(stream_id, id)` means the same branch ID can be reused across different streams without collision.

**`parent_id`** — The branch from which this one was forked. Always `'main'` for branches created directly from the main branch. Can be another branch's `id` for nested branching (a branch of a branch). The literal string `'main'` is a reserved value — `main` is never stored as a row in the `branches` table.

**`parent_version`** — The version number on the parent branch at which this branch forks. Events on the parent branch up to and including `parent_version` form the "shared prefix" of this branch's history. Events appended directly to this branch start version numbering from 0 (independent sequence). For root branches, `parent_version = 0` when the branch forks at the very start; any higher value means there is shared history.

**`description`** — Free-text description. Optional. Survives branch closure.

**`created_at`** — Microsecond Unix timestamp. Set by `now_us()` at create time.

**`lifecycle`** — One of `'ephemeral'`, `'promoted'`, `'dead_end'`. Default `'ephemeral'`. TEXT column — not an enum — because SQLite has no native enum type. Validated in application code.

**`closed_at`** — Microsecond Unix timestamp. Set when lifecycle transitions from `ephemeral`. NULL while the branch is open.

**`closed_reason`** — Consumer-supplied reason string. Optional. Set at promote/dead_end time.

**`alternatives`** — JSON array of sibling branch IDs. Informational only — the store does not enforce any relationship between siblings. Used to track parallel exploration paths: if two agents are simultaneously exploring `strategy-a` and `strategy-b`, each can declare the other in its `alternatives` field, making the parallel structure visible in `list_branches` output.

### Indices

`idx_branches_stream` on `(stream_id)` — supports `list_branches(stream_id)`.

`idx_branches_lifecycle` on `(stream_id, lifecycle)` — supports filtering by lifecycle state, e.g. "all open (ephemeral) branches for stream X".

---

## 3. Lifecycle States

```
                  ┌─── promote ───→ promoted  (terminal)
ephemeral ────────┤
                  └─── dead_end ──→ dead_end  (terminal)
```

All branches begin as `ephemeral`. The two transitions are one-way and terminal: there is no transition from `promoted` to `dead_end`, no transition from `dead_end` to `promoted`, and no transition back to `ephemeral`. Once a branch is closed, it stays closed.

### ephemeral

The default state after `create_branch`. Events can be appended to the branch. The branch is fully open. A consumer reading the store's `list_branches` can enumerate all open branches by filtering for `lifecycle == "ephemeral"`.

### promoted

The branch has been accepted. The store enforces no semantic meaning — what "promoted" implies is entirely up to the consumer. The typical meaning: the speculative events on the branch proved correct, and the consumer has now committed corresponding events to `main` (usually with `causation_id` links back to the branch events for traceability). After promotion, no new events can be written to the branch.

### dead_end

The branch has been abandoned. No new events can be appended. The branch events remain physically in the `events` table (fossic is append-only — there is no deletion that removes them en masse, only the `purge_event` escape hatch which operates per event). A dead_end branch is a permanent record that a particular exploration path was tried and rejected.

### Lifecycle error

Attempting an invalid transition (e.g., calling `promote_branch` on a branch that is already `dead_end`) returns:

```
Error::BranchLifecycleError { reason: String }
```

The `reason` string is generated by the store and describes which transition was attempted and from which state.

Transitions are idempotent for the valid direction: calling `promote_branch` on an already-`promoted` branch returns `Ok(())` without modifying the database. Same for `mark_branch_dead_end` on an already-`dead_end` branch.

---

## 4. CreateBranch

The Rust struct:

```rust
pub struct CreateBranch {
    pub stream_id: String,
    pub branch_id: String,
    pub parent_id: String,                    // "main" or another branch ID
    pub parent_version: u64,                  // version at which to fork
    pub description: Option<String>,
    pub alternatives: Option<Vec<String>>,    // sibling branch IDs
}
```

The Python class:

```python
store.create_branch(CreateBranch(
    stream_id="cerebra/decisions",
    branch_id="repair-attempt-1",
    parent_id="main",
    parent_version=42,          # fork point: shared history is events 0..=42 from main
    description="Speculative repair for bug-123",
    alternatives=["repair-attempt-2"],
))
```

**What `parent_version=42` means:** The branch was created after event version 42 existed on main. The branch does not inherit those events automatically — the consumer must stitch them together via `resolve_chain` if needed. The version number is purely metadata that records where the fork happened.

**Validation:**
- `stream_id` must exist in the `streams` table (`StreamNotDeclared` if not).
- `parent_id` must be either `"main"` or an existing branch ID on the same stream (`BranchNotFound` if not).
- `branch_id` must pass character validation (`InvalidBranchId` if not).
- If a branch with the same `(stream_id, branch_id)` already exists, the behavior is a no-op (idempotent INSERT OR IGNORE) or returns an error — callers should treat duplicate creation as idempotent.

The `alternatives` field is serialized to JSON and stored in the `alternatives` TEXT column. If `None`, the column is NULL.

---

## 5. Appending Events to a Branch

Events are appended to a branch by specifying `branch` in the `Append` struct:

```python
store.append(Append(
    stream_id="cerebra/decisions",
    branch="repair-attempt-1",     # the branch to write to
    event_type="RepairProposed",
    payload={"strategy": "patch-line-42", "confidence": 0.87},
))
```

If `branch` is omitted, the default is `"main"`.

**Version numbering on a branch:** Each branch has its own independent version sequence starting at 0. Versions on `repair-attempt-1` are not related to versions on `main`. A branch with three events has versions 0, 1, 2 — the fact that `main` might be at version 9000 is irrelevant.

**Reading branch events only:** `read_range` with `branch="repair-attempt-1"` returns only events appended directly to that branch. The shared history from `parent_id` is not included. This is intentional — see §6 for stitching.

**Validation at append time:** The store checks that the branch is not in a terminal lifecycle state (`promoted` or `dead_end`). Appending to a closed branch returns `BranchLifecycleError`.

The CCE/blake3 event ID derivation does not include branch. The same payload appended to `main` and to `repair-attempt-1` on the same stream with the same event_type, type_version, and causation_id will produce the same EventId. In practice this should not happen (branches are for exploration, not duplication), but the uniqueness constraint is on `(stream_id, branch, version)` not on `id` alone.

---

## 6. BranchSegment and Chain Resolution

Reading the full history of a branch — parent shared history plus branch-specific events — requires resolving the branch chain.

### BranchSegment

```rust
pub struct BranchSegment {
    pub branch_id: String,
    pub to_version: Option<u64>,  // None = read to the current tip of this branch
}
```

A `BranchSegment` says: "read events from this branch, up to (and including) `to_version`, or to the tip if `to_version` is None."

### resolve_chain

```rust
store.resolve_chain(stream_id, branch_id) -> Result<Vec<BranchSegment>, Error>
```

Returns segments in root-to-leaf order. For a simple fork from `main` at version 42:

```
resolve_chain("cerebra/decisions", "repair-attempt-1")
→ [
    BranchSegment { branch_id: "main",           to_version: Some(42) },
    BranchSegment { branch_id: "repair-attempt-1", to_version: None   },
  ]
```

For nested branching (branch B was forked from branch A at version 5, and branch A was forked from `main` at version 42):

```
resolve_chain("cerebra/decisions", "nested-branch-b")
→ [
    BranchSegment { branch_id: "main",      to_version: Some(42) },
    BranchSegment { branch_id: "branch-a",  to_version: Some(5)  },
    BranchSegment { branch_id: "nested-branch-b", to_version: None },
  ]
```

### Using the chain to read full history

```python
def read_full_history(store, stream_id, branch_id):
    """Read all events from root through the full branch chain, in order."""
    chain = store.resolve_chain(stream_id, branch_id)
    events = []
    for seg in chain:
        q = ReadQuery(
            stream_id=stream_id,
            branch=seg.branch_id,
            to_version=seg.to_version,   # None → reads to current tip
        )
        events.extend(store.read_range(q))
    # Events within each segment are version-ordered ascending.
    # Across segments, the order is: root shared history first, then branch events.
    return events
```

The ordering guarantee: within each `read_range` call, events are ordered by `version ASC`. Across segments, the concatenation is chronologically meaningful because `parent_version` is always the fork point — events in segment N with `to_version=K` were written before events in segment N+1 started.

---

## 7. Chain Resolution Algorithm (Implementation)

The `resolve_branch_chain` function in `src/branches.rs` is a recursive descent on the `branches` table:

```
fn resolve_branch_chain(conn, stream_id, branch_id) -> Vec<BranchSegment>:
  1. Query: SELECT parent_id, parent_version FROM branches
             WHERE stream_id = ? AND id = ?
     → (parent_id, parent_version)

  2. If parent_id == "main":
     return [
       BranchSegment { "main", to_version: Some(parent_version) },
       BranchSegment { branch_id, to_version: None },
     ]

  3. Else (parent_id is another branch):
     prefix = resolve_branch_chain(conn, stream_id, parent_id)
     // Set the last segment in prefix to have to_version = parent_version
     // (it was previously None — we now know where it ends)
     prefix.last_mut().to_version = Some(parent_version)
     prefix.push(BranchSegment { branch_id, to_version: None })
     return prefix
```

This is a clean recursion with depth equal to the nesting level of the branch chain. For typical use (branches from main), depth is 1 and the function makes 1 SQL query. For branches of branches, depth is N and makes N queries.

### Chain Cache

Resolved chains are expensive to recompute for deeply nested branches, and they are read frequently (every `read_full_history` call). The store caches them:

```rust
branch_chain_cache: RwLock<BTreeMap<(String, String), Vec<BranchSegment>>>
```

Key: `(stream_id, branch_id)`. Value: the resolved `Vec<BranchSegment>`.

**Cache read path:** `RwLock::read()` → look up key → if found, clone and return.

**Cache miss path:** Acquire `RwLock::write()` → resolve chain from DB → insert → release write lock → return.

**Cache invalidation:** The entire cache is cleared when any new branch is created (on any stream). This is a conservative global invalidation — branch creation is assumed to be infrequent enough that per-stream invalidation is not worth the additional complexity.

**Consequence for high-throughput branch creation:** If a workload creates many branches rapidly (e.g. one branch per agent task), the cache will be invalidated frequently and will offer little benefit. In practice this is not the expected usage pattern — branches are created for major decision points, not per-event.

---

## 8. Branch Lifecycle Operations

### create_branch

```rust
store.create_branch(&CreateBranch { ... })?;
```

Inserts a row into `branches` with `lifecycle = 'ephemeral'` and `created_at = now_us()`. Invalidates the chain cache.

### promote_branch

```rust
store.promote_branch(stream_id, branch_id, reason: Option<&str>)?;
```

Sets `lifecycle = 'promoted'`, `closed_at = now_us()`, `closed_reason = reason` in a write transaction. Idempotent for already-`promoted` branches. Returns `BranchLifecycleError` if the branch is `dead_end`.

After promotion, any `store.append(... branch=branch_id ...)` returns `BranchLifecycleError`.

### mark_branch_dead_end

```rust
store.mark_branch_dead_end(stream_id, branch_id, reason: Option<&str>)?;
```

Sets `lifecycle = 'dead_end'`, `closed_at = now_us()`, `closed_reason = reason`. Idempotent for already-`dead_end` branches. Returns `BranchLifecycleError` if the branch is `promoted`.

After closing, appends to the branch are blocked. The branch events remain in the `events` table.

### list_branches

```rust
store.list_branches(stream_id) -> Result<Vec<BranchInfo>, Error>
```

Returns all branches for the stream, all lifecycle states. The `BranchInfo` struct:

```rust
pub struct BranchInfo {
    pub id: String,
    pub stream_id: String,
    pub parent_id: String,
    pub parent_version: u64,
    pub description: Option<String>,
    pub created_at: i64,
    pub lifecycle: String,              // "ephemeral" | "promoted" | "dead_end"
    pub closed_at: Option<i64>,
    pub closed_reason: Option<String>,
    pub alternatives: Option<Vec<String>>,
}
```

`alternatives` is deserialized from the JSON array column. If the column is NULL, `alternatives` is `None`.

SQL executed by `list_branches`:

```sql
SELECT id, stream_id, parent_id, parent_version, description,
       created_at, lifecycle, closed_at, closed_reason, alternatives
FROM branches
WHERE stream_id = ?1
ORDER BY created_at ASC
```

---

## 9. Error Variants

| Error | When |
|---|---|
| `BranchNotFound { stream_id, branch_id }` | Branch ID does not exist in the `branches` table |
| `BranchLifecycleError { reason }` | Invalid lifecycle transition (e.g. dead_end → promoted) |
| `InvalidBranchId { id, reason }` | Branch ID fails character/format validation |

`BranchLifecycleError` is also returned when appending to a branch in a terminal lifecycle state.

The `reason` field in `BranchLifecycleError` is a human-readable description generated by the store, e.g.:

```
"cannot promote: branch 'repair-v1' is already dead_end"
"cannot append: branch 'strategy-b' is promoted"
```

---

## 10. Subscriptions and Branches

The `SubscribeQuery` has a `branch` field:

```rust
pub struct SubscribeQuery {
    pub stream_pattern: String,
    pub branch: String,        // default "main"
    pub include_system: bool,
}
```

A subscription with `branch="repair-attempt-1"` receives only events written directly to that branch. It does not receive the shared history from `main`.

There is no subscription mode that follows a branch chain automatically. If a consumer needs a unified stream of events across a branch chain (e.g. for a reducer that must fold all history), it must:

1. Use `resolve_chain` + `read_range` to backfill the shared history.
2. Subscribe to the leaf branch for live events.
3. Stitch the two together in application code.

In Python:

```python
# Backfill + subscribe pattern for branch chain
chain = store.resolve_chain(stream_id, branch_id)

# Backfill: read all history from root to current tip
for seg in chain:
    q = ReadQuery(stream_id=stream_id, branch=seg.branch_id, to_version=seg.to_version)
    for ev in store.read_range(q):
        process(ev)

# Live: subscribe to the leaf branch for new events
handle = store.subscribe(
    stream_pattern=stream_id,
    branch=branch_id,
    mode=SubscriptionMode.post_commit(queue_size=256),
)
```

---

## 11. Practical Patterns

### Pattern A — Speculative Repair

```python
import time

# 1. Read current tip of the decision stream
events = store.read_range(ReadQuery(stream_id="decisions", branch="main"))
current_tip = events[-1].version if events else 0

# 2. Create a branch at the current tip
store.create_branch(CreateBranch(
    stream_id="decisions",
    branch_id="repair-2026-06-20",
    parent_id="main",
    parent_version=current_tip,
    description="Speculative fix for OOM in worker pool",
))

# 3. Write speculative events
ev_id = store.append(Append(
    stream_id="decisions",
    branch="repair-2026-06-20",
    event_type="RepairProposed",
    payload={"action": "increase-pool-limit", "new_limit": 16},
))

# ... agent executes the repair and evaluates outcome ...

# 4a. If successful: promote
store.promote_branch(
    "decisions",
    "repair-2026-06-20",
    reason="OOM resolved; pool limit 16 validated in staging",
)

# 4b. If failed: mark dead_end
store.mark_branch_dead_end(
    "decisions",
    "repair-2026-06-20",
    reason="pool limit change caused 2x latency; strategy rejected",
)
```

### Pattern B — Parallel Exploration

```python
# Two agents explore competing strategies simultaneously
tip = store.read_range(ReadQuery(stream_id="decisions", branch="main"))[-1].version

store.create_branch(CreateBranch(
    stream_id="decisions",
    branch_id="strategy-a",
    parent_id="main",
    parent_version=tip,
    description="Strategy A: patch the allocator",
    alternatives=["strategy-b"],
))

store.create_branch(CreateBranch(
    stream_id="decisions",
    branch_id="strategy-b",
    parent_id="main",
    parent_version=tip,
    description="Strategy B: restructure the pipeline",
    alternatives=["strategy-a"],
))

# Agent 1 writes to strategy-a; Agent 2 writes to strategy-b independently.
# Evaluation goroutine picks the winner.

winner = evaluate_strategies(store)
loser = "strategy-b" if winner == "strategy-a" else "strategy-a"

store.promote_branch("decisions", winner, reason="strategy validated")
store.mark_branch_dead_end("decisions", loser, reason="superseded by " + winner)
```

### Pattern C — Nested Branching (Branch of a Branch)

```python
# Branch A is an exploration from main at version 10.
# Within that exploration, Branch B is a sub-exploration from Branch A at version 3.

store.create_branch(CreateBranch(
    stream_id="planning",
    branch_id="plan-a",
    parent_id="main",
    parent_version=10,
))
# ... append events to plan-a, reaching version 3 ...

store.create_branch(CreateBranch(
    stream_id="planning",
    branch_id="plan-a-subtest",
    parent_id="plan-a",           # nested: forked from plan-a, not main
    parent_version=3,
))

# Resolving the chain:
chain = store.resolve_chain("planning", "plan-a-subtest")
# → [
#     BranchSegment(branch_id="main",          to_version=10),
#     BranchSegment(branch_id="plan-a",        to_version=3),
#     BranchSegment(branch_id="plan-a-subtest", to_version=None),
#   ]
```

### Pattern D — Branch Inventory

```python
branches = store.list_branches("decisions")

open_branches = [b for b in branches if b.lifecycle == "ephemeral"]
promoted = [b for b in branches if b.lifecycle == "promoted"]
dead_ends = [b for b in branches if b.lifecycle == "dead_end"]

# Stale open branches (created more than 7 days ago)
seven_days_us = 7 * 24 * 3600 * 1_000_000
now_us = int(time.time() * 1_000_000)
stale = [b for b in open_branches if (now_us - b.created_at) > seven_days_us]

# Note: you can list and close stale branches, but you cannot delete their events.
# The events table is append-only. Branch metadata (the branches table row) is
# updated in place, but event rows are never deleted by lifecycle operations.
for b in stale:
    store.mark_branch_dead_end(
        b.stream_id, b.id,
        reason="auto-closed: stale after 7 days with no resolution"
    )
```

---

## 12. What v1 Does Not Include

**Automatic merge:** Promoting a branch does not copy or link its events to `main`. "Merging" a branch means manually re-appending the desired events to `main` (typically with `causation_id` links back to the branch events for audit trail). The store makes no assumptions about how or whether branch events are incorporated into `main`.

**Branch-aware diff:** There is no built-in query that computes "what events are on branch B that are not on `main`" or "where do branch A and branch B diverge." Consumers must implement this by reading both branches via `read_range` and comparing in application code.

**Cross-stream branches:** A branch is scoped to exactly one stream. There is no concept of a "global branch" that spans multiple streams simultaneously. If a speculative change affects multiple streams, each affected stream needs its own branch, and the consumer is responsible for coordinating their lifecycle.

**Subscription spanning parent + branch:** Subscribing to a branch only receives events appended to that specific branch. There is no subscription mode that automatically delivers the parent shared history first and then live branch events.

**Branch-aware causation walks:** `walk_causation` operates purely on `causation_id` links regardless of which branch the events are on. A causation walk from a branch event may reach events on `main`, and vice versa — the walk does not filter by branch.

**DEK-per-branch encryption:** The `stream_deks` table keys on `stream_id` only, not `(stream_id, branch)`. There is no branch-scoped encryption. If a stream uses crypto-shredding, all branches of that stream share the same DEK fate.

---

## 13. Python API Reference

```python
from fossic import Store, CreateBranch, ReadQuery, BranchInfo, BranchSegment

store = Store.open("~/fossic/store.db")

# Create
store.create_branch(CreateBranch(
    stream_id="my/stream",
    branch_id="exploration-1",
    parent_id="main",
    parent_version=99,
    description="optional",
    alternatives=None,
))

# List
branches: list[BranchInfo] = store.list_branches("my/stream")
# BranchInfo fields: id, stream_id, parent_id, parent_version, description,
#                    created_at, lifecycle, closed_at, closed_reason, alternatives

# Resolve chain
chain: list[BranchSegment] = store.resolve_chain("my/stream", "exploration-1")
# BranchSegment fields: branch_id, to_version (Optional[int])

# Lifecycle transitions
store.promote_branch("my/stream", "exploration-1", reason="succeeded")
store.mark_branch_dead_end("my/stream", "exploration-1", reason="failed")

# Errors raised
from fossic import BranchNotFoundError, BranchLifecycleError, InvalidBranchIdError
```

The Python `BranchInfo` and `BranchSegment` classes are `@pyclass` wrappers around the Rust types. All fields are accessible as Python attributes. `alternatives` is a Python `list[str]` when present, `None` when the column is NULL.

---

*Next: SR-06 — Reducers and Snapshot System*
