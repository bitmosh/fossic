# SR-08 — Schema Evolution, Deletion, and Error Model

**Series:** Fossic State Reports · Document 8 of 9  
**Covers:** `src/transforms.rs`, `src/upcasters.rs`, `src/deletion.rs`, `src/error.rs`, `fossic-py/src/errors.rs`  
**Status:** Research-grade implementation reference · 2026-06-20

---

## 1. Overview

Three mechanisms address the reality that events change meaning over time or occasionally need removal:

- **PayloadTransforms** — pure functions applied at write time that mutate a payload before CCE encoding and storage. The event id reflects the transformed payload. The original data never enters the store.
- **Upcasters** — pure functions applied at read time that migrate payload bytes from one `type_version` to the next. Stored events are never touched; old bytes are converted on the way out.
- **Deletion** — two escape hatches: `purge_event` (surgical row deletion with a mandatory confirmation string and audit trail) and `shred_stream` (cryptographic inaccessibility via DEK deletion — v1 stub only).

These are the least-often-invoked surfaces in fossic, but when you need them the operational consequences are significant. This document covers each in full, including exact byte sequences, transaction flows, and the complete error taxonomy.

---

## 2. PayloadTransform — Pre-Write Mutation

### 2.1 Trait definition

```rust
pub trait PayloadTransform: Send + Sync + 'static {
    fn transform(&self, event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error>;
}
```

The transform receives **msgpack bytes** and returns **msgpack bytes**. It does not receive a `serde_json::Value` — the payload has already been serialized to msgpack before the transform chain runs. The `event_type` parameter allows a single transform registered on a broad pattern to branch on the specific event type without needing separate per-type registrations.

A transform that returns `Err` causes the append to fail. The IMMEDIATE transaction is rolled back. No partial transform output is ever persisted.

### 2.2 Registration

```rust
store.register_payload_transform(stream_pattern, my_transform)?;
```

The `stream_pattern` uses the standard fossic glob system:
- `*` matches exactly one path segment (no `/` in the matched portion).
- `**` matches zero or more segments.
- Examples: `policy-scout/**`, `cerebra/agent-trace/*`, `**` (all streams).

Multiple transforms may be registered on overlapping patterns. There is no deduplication — if pattern A and pattern B both match `cerebra/agent-trace/sess-1`, both transforms fire.

### 2.3 Chain execution

```rust
pub(crate) fn apply_transforms(
    entries: &[TransformEntry],
    stream_id: &str,
    event_type: &str,
    mut payload_bytes: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    for entry in entries {
        if pattern_matches(&entry.pattern, stream_id) {
            payload_bytes = entry.transform.transform(event_type, &payload_bytes)?;
        }
    }
    Ok(payload_bytes)
}
```

Key properties:
- Iteration order is **registration order**. Transform A registered before Transform B on overlapping patterns: A fires first, B receives A's output.
- If no transforms match, the original bytes are returned by value with no copy (`Vec<u8>` is moved through unchanged).
- A transform failure (`Err`) short-circuits the loop. The error propagates to `append_impl`, which rolls back the IMMEDIATE transaction.

### 2.4 What transforms affect

Transforms run before CCE encoding. This means:

1. The payload bytes passed to CCE are the **post-transform** bytes.
2. The CCE encoding of those bytes is used to derive the event id.
3. The stored msgpack `payload` column contains the **post-transform** bytes.

**Consequence:** The event id reflects the transformed payload, not the caller's original. Two different callers who both pass `{"email": "Alice@Example.com"}` through a lowercase-normalizing transform will produce identical event ids. If they are writing to the same (stream_id, branch), the second append will collide on the primary key (same id) and SQLite will return a conflict. This is usually the desired deduplication behavior.

### 2.5 When to use transforms

**PII stripping:** Strip or hash personally identifiable fields before they touch the store. The id reflects the sanitized payload; the original PII never enters SQLite.

```rust
struct EmailHasher;
impl PayloadTransform for EmailHasher {
    fn transform(&self, _event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        let mut v: serde_json::Value = rmp_serde::from_slice(payload)?;
        if let Some(email) = v.get_mut("email").and_then(|e| e.as_str()) {
            let hashed = blake3::hash(email.as_bytes()).to_hex().to_string();
            v["email"] = serde_json::Value::String(format!("hashed:{hashed}"));
        }
        Ok(rmp_serde::to_vec_named(&v)?)
    }
}
store.register_payload_transform("users/**", EmailHasher)?;
```

**Normalization:** Coerce fields to canonical form so that equivalent inputs produce identical ids.

**Schema projection (relay use case):** A relay receiving events with extra fields can strip the fields before writing to the hub store. This ensures the hub-side event id matches what a hub-native producer would generate for the same logical event.

### 2.6 What transforms are NOT for

- **Enrichment:** Adding data fetched from external sources (HTTP calls, DB lookups) in a transform introduces I/O and non-determinism. If the external call fails, the append fails. Use a separate append with causation_id linkage for enrichment patterns.
- **Routing:** A transform cannot redirect an event to a different stream. Use the caller's code for routing.
- **Conditional append:** A transform cannot cancel the append based on business logic — use `append_if` for that.

### 2.7 Python callable transform

```python
def strip_metadata(event_type: str, payload: dict) -> dict:
    result = dict(payload)
    result.pop("_meta", None)
    return result

store.register_payload_transform("cerebra/**", strip_metadata)
```

The Python bridge (`PyTransform` in `fossic-py/src/store.rs`) wraps the callable:

```
Input:  msgpack bytes
  → rmp_serde::from_slice::<serde_json::Value>
  → json_to_py (serde_json::Value → json.dumps → json.loads → Python dict)
  → callable(event_type, py_dict)
  → Python dict
  → py_to_json (json.dumps → serde_json::Value)
  → rmp_serde::to_vec_named
Output: msgpack bytes
```

The double round-trip (msgpack → JSON → Python → JSON → msgpack) exists because Python's type system and the Rust type system cannot share memory directly. A transform written in Rust has no round-trip overhead. For high-throughput paths where every append goes through a transform, prefer a Rust `PayloadTransform` implementation.

---

## 3. Upcasters — Read-Time Schema Migration

### 3.1 Design principle

Upcasters are the answer to "I changed the shape of `UserCreated` and now old events have the wrong fields." The core rule is:

- **Stored data is immutable.** `type_version` and `payload` bytes in the events table are never modified by upcasters.
- **id is immutable.** The event id always reflects the originally stored payload.
- **Read path is transparent.** Callers of `read_range`, `read_one`, `read_batch`, `read_by_external_id` always receive the current version of the payload, regardless of what version it was stored at.

### 3.2 Trait definition

```rust
pub trait Upcaster: Send + Sync + 'static {
    fn upcast(&self, payload: &[u8]) -> Result<Vec<u8>, Error>;
}
```

Receives and returns msgpack bytes. No access to event metadata (stream_id, event_type, version, etc.) — upcasters are pure functions keyed by `(event_type, from_version)`. If you need to branch on event type, register separate upcasters per type.

### 3.3 Registration

```rust
store.register_upcaster(event_type, from_version, to_version, my_upcaster)?;
```

- `event_type`: the event type string exactly as it appears in the `event_type` column.
- `from_version`: the `type_version` value in the stored event that this upcaster migrates FROM.
- `to_version`: the `type_version` this upcaster produces.

The pair `(event_type, from_version, to_version)` is also written to the `upcasters_registered` table with `registered_at` timestamp. This is an audit trail — it does not drive upcaster logic at read time (the in-memory `UpcasterRegistry` does that).

### 3.4 UpcasterRegistry

```rust
#[derive(Default)]
pub(crate) struct UpcasterRegistry {
    entries: HashMap<String, Vec<UpcasterEntry>>,
}
```

Keyed by event_type string. Each value is a `Vec<UpcasterEntry>` sorted by `from_version` ascending. Sorting happens at registration time (after each `push`, a `sort_by_key(|e| e.from)` runs on the vec).

### 3.5 Chain traversal algorithm

The `apply` method on `UpcasterRegistry`:

```rust
pub fn apply(
    &self,
    event_type: &str,
    stored_version: u32,
    mut payload: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let entries = match self.entries.get(event_type) {
        Some(e) if !e.is_empty() => e,
        _ => return Ok(payload),   // no upcasters for this type — return as-is
    };

    let mut current = stored_version;
    loop {
        let entry = entries.iter().find(|e| e.from == current);
        match entry {
            None => {
                let has_higher = entries.iter().any(|e| e.from > current);
                if has_higher {
                    return Err(Error::UpcasterChainGap {
                        event_type: event_type.to_string(),
                        from: current,
                    });
                }
                break;   // end of chain
            }
            Some(e) => {
                payload = e.upcaster.upcast(&payload)?;
                current = e.to;
            }
        }
    }
    Ok(payload)
}
```

Pseudocode walk:

```
current = stored_version
loop:
    look for entry where entry.from == current
    if not found:
        if any entry has entry.from > current:
            → UpcasterChainGap error (gap in chain)
        else:
            break (current version is the latest, nothing to upcast)
    apply upcaster: payload = upcaster.upcast(payload)?
    current = entry.to
return payload
```

### 3.6 The contiguous chain requirement

Every step in the version sequence must have a registered upcaster. There is no "skip" semantics. If you register v1→v2 and v3→v4 but not v2→v3, reading a v2-stored event returns:

```
Error::UpcasterChainGap { event_type: "UserCreated", from: 2 }
```

This is a hard error — it surfaces immediately on any read of a v2 event. Fix by registering the missing v2→v3 upcaster before any reads of that event type.

### 3.7 Behavior at or beyond the chain end

If the stored version is at or beyond the highest registered `from_version` and no upcaster has `from == stored_version`, the loop exits normally and the payload is returned unchanged. This is the normal case for events written at the current version (no upcasting needed).

Example with registered chain v1→v2→v3:
- Stored v1 event: upcasted v1→v2→v3, returned as v3 payload.
- Stored v2 event: upcasted v2→v3, returned as v3 payload.
- Stored v3 event: no upcaster has `from == 3`, no higher entries, loop exits, returned unchanged.
- Stored v4 event (hypothetical future version): same as v3 — no upcaster for v4, returned unchanged.

### 3.8 What changes and what doesn't

The `apply_upcaster` wrapper function:

```rust
pub(crate) fn apply_upcaster(
    registry: &UpcasterRegistry,
    mut event: StoredEvent,
) -> Result<StoredEvent, Error> {
    event.payload = registry.apply(&event.event_type, event.type_version, event.payload)?;
    Ok(event)
}
```

Only `event.payload` (the Vec<u8> of msgpack bytes) changes. `event.id`, `event.type_version`, `event.version`, `event.timestamp_us`, and all other fields are unchanged. The `type_version` field in the returned `StoredEvent` still reflects the stored version — it is NOT updated to the upcasted version.

This is intentional: callers that need to know the current schema version must track it themselves (or check the latest registered upcaster's `to_version`).

### 3.9 Transforms vs Upcasters comparison

| Property | PayloadTransform | Upcaster |
|---|---|---|
| Fires at | Write time (before CCE) | Read time (after row fetch) |
| Stored payload | Modified by transform | Unchanged forever |
| Stored type_version | Unchanged | Unchanged |
| Event id | Reflects transformed payload | Reflects original stored payload |
| Primary use | PII, normalization, projection | Schema evolution |
| Pure function | Yes | Yes |
| Event metadata access | event_type only | None |
| Chain order | Registration order | from_version ascending |
| Error on chain gap | N/A (each fires independently) | UpcasterChainGap |

### 3.10 Python callable upcaster

```python
def migrate_v1_to_v2(payload: dict) -> dict:
    """Rename 'origin' field to 'source', added in schema v2."""
    result = dict(payload)
    if "origin" in result and "source" not in result:
        result["source"] = result.pop("origin")
    elif "source" not in result:
        result["source"] = "unknown"
    return result

store.register_upcaster(
    event_type="UserCreated",
    from_version=1,
    to_version=2,
    callable=migrate_v1_to_v2,
)
```

The Python bridge (`PyUpcaster` in `fossic-py/src/store.rs`) wraps the callable with the same msgpack ↔ Python dict round-trip as `PyTransform`:

```
Input:  msgpack bytes
  → rmp_serde::from_slice::<serde_json::Value>
  → json_to_py → Python dict
  → callable(py_dict)
  → Python dict
  → py_to_json → serde_json::Value
  → rmp_serde::to_vec_named
Output: msgpack bytes
```

### 3.11 Upcaster chain deployment sequence

When you change the schema of an event type:

1. Increment `type_version` on new appends (pass `type_version=2` in the `Append` struct).
2. Register the v1→v2 upcaster **before** any code reads old events. Order matters: if old events are read before the upcaster is registered, they return v1 payload and callers that expect v2 shape will break.
3. Never remove a upcaster as long as v1 events remain in any store that code might read.
4. When adding v3: register v2→v3 alongside v1→v2 (both must be present for old v1 events).

---

## 4. purge_event — Surgical Deletion

### 4.1 Why this exists

Fossic is append-only by design. The id of every event is a content-addressed hash; events are meant to be permanent. But real-world systems occasionally need to remove data: a PII incident, a test event accidentally written to production, a compliance requirement.

`purge_event` is the escape hatch. It is intentionally made difficult to invoke.

### 4.2 The confirmation string

```rust
const PURGE_CONFIRM: &str = "I understand this breaks replay-from-zero";
```

This exact string — including capitalization and spacing — must be passed as the `confirm` parameter:

```rust
store.purge_event(
    id,
    "I understand this breaks replay-from-zero",
    "pii-incident-2026-06-20: email field contained raw PII",
    "ops-engineer@example.com",
)?;
```

Any other value returns immediately with no I/O:
```
Error::PurgeConfirmationError {
    got: "whatever you passed",
}
```

The error message is: `purge_event confirmation mismatch; confirm must be exactly "I understand this breaks replay-from-zero", got: "{got}"`.

**The friction is the point.** The string is not a token or hash — it is a human-readable statement of consequence. Reading it aloud forces acknowledgment that after this call, the affected stream cannot be replayed from event 0 and produce the same result.

### 4.3 The `reason` and `purged_by` fields

Both are free-form strings. They appear verbatim in:
- The stderr WARN log entry.
- The audit event payload written to `_fossic/system`.

Convention:
- `reason`: describe the incident, compliance requirement, or purpose (e.g., "GDPR erasure request #4421 — user requested deletion of account events").
- `purged_by`: the identity performing the operation (email, service account name, username).

### 4.4 Full transaction flow

Step-by-step execution of `purge_event_impl`:

**Step 1: Confirmation check**
```rust
if confirm != PURGE_CONFIRM {
    return Err(Error::PurgeConfirmationError { got: confirm.to_string() });
}
```
No lock acquired. No I/O. Returns immediately on mismatch.

**Step 2: Get current timestamp**
```rust
let purged_at_us = now_us();
```

**Step 3: Acquire write connection**
Single Mutex<Connection> for all writes. Blocks until available.

**Step 4: Open IMMEDIATE transaction**
```rust
let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
```

**Step 5: Fetch event metadata (not payload)**
```sql
SELECT event_type, stream_id, timestamp_us FROM events WHERE id = ?1
```
If no row → rollback (implicit on Err return) and return `EventNotFound`.

**Step 6: Build audit payload**
```rust
let purged_payload = serde_json::json!({
    "event_id_purged": id.to_hex(),          // 64 lowercase hex chars
    "original_event_type": original_event_type,
    "original_stream_id": original_stream_id,
    "original_timestamp_us": original_timestamp_us,
    "reason": reason,
    "purged_at_us": purged_at_us,
    "purged_by": purged_by,
});
```

The original payload is intentionally excluded. The audit record documents the purge; it is not a backup of the purged content.

**Step 7: Ensure `_fossic/system` stream exists**
```sql
INSERT OR IGNORE INTO streams(id, declared_by, declared_at, description)
VALUES ('_fossic/system', 'fossic-internal', 0, 'Internal fossic system events')
```
Idempotent. The stream is bootstrapped at store open, but this defensive insert handles edge cases.

**Step 8: Get next version on `_fossic/system`**
```sql
SELECT COALESCE(MAX(version), -1) + 1 FROM events
WHERE stream_id = '_fossic/system' AND branch = 'main'
```
Versions start at 0 for the first system event.

**Step 9: Derive the Purged audit event id**
```rust
let purged_id_bytes = derive_event_id("Purged", 1, None, &purged_payload)?;
let purged_id = EventId::from_bytes(purged_id_bytes);
```
The audit event is content-addressed. Its id includes `purged_at_us`, so two separate purges at different times produce different audit event ids even if purging the same original event.

**Step 10: Serialize audit payload to msgpack**
```rust
let purged_payload_bytes = rmp_serde::to_vec(&purged_payload)?;
```

**Step 11: INSERT the Purged audit event**
```sql
INSERT OR IGNORE INTO events
(id, stream_id, branch, version, timestamp_us, event_type, type_version, payload)
VALUES (?1, '_fossic/system', 'main', ?3, ?4, 'Purged', 1, ?6)
```
`INSERT OR IGNORE` means a duplicate `Purged` event (identical id) is silently skipped. Since the id includes `purged_at_us`, genuine duplicates are essentially impossible in practice.

**Step 12: DELETE the original event**
```sql
DELETE FROM events WHERE id = ?1
```

**Step 13: Commit**
```rust
tx.commit()?;
```
Steps 11 and 12 are committed atomically. Either both the Purged audit record is created and the original is deleted, or neither happens.

**Step 14: Emit stderr WARN**
```rust
eprintln!(
    "[fossic WARN] purge_event called: id={id} reason=\"{reason}\" \
     purged_by=\"{purged_by}\" purged_at_us={purged_at_us}"
);
```
Note: this fires **after** the commit. If the process crashes between commit and eprintln, the purge still happened but the stderr log may be missing. The `_fossic/system` Purged event is the authoritative durable record. The stderr log is a belt-and-suspenders observable for monitoring systems that ingest stderr.

### 4.5 What the Purged audit event looks like

When later read from `_fossic/system`:

```python
event = store.read_range(ReadQuery(stream_id="_fossic/system", branch="main"))[-1]
payload = event.payload  # decoded dict
# {
#   "event_id_purged": "a3b4c5...64chars",
#   "original_event_type": "UserCreated",
#   "original_stream_id": "users/registrations",
#   "original_timestamp_us": 1718880000000000,
#   "reason": "GDPR erasure request #4421",
#   "purged_at_us": 1718880060000000,
#   "purged_by": "ops@example.com",
# }
```

The `event_id_purged` field is the 64-char hex-encoded id of the deleted event. You can verify deletion:

```python
deleted = store.read_one(EventId.from_hex(payload["event_id_purged"]))
assert deleted is None  # event is gone
```

### 4.6 What is and is not purgeable

**Purgeable:** Any event in any user-declared stream on any branch.

**Not purgeable via this API:**
- Events on `_fossic/system` (the internal system stream) — there is no explicit guard in the source preventing it, but `_fossic/system` is reserved for internal use and purging audit events would defeat the audit trail.
- Multiple events at once — `purge_event` is single-event, single-transaction. For batch removal, call it N times. Each call produces one audit event.
- Stream metadata (the `streams` table) — no purge API for stream declarations.
- Branch metadata (the `branches` table) — no purge API for branch records.
- Snapshot rows — no purge API; use `gc_orphaned_snapshots` for controlled cleanup.

### 4.7 Downstream effects of purging

After `purge_event`:
1. **Subscriptions:** PostCommit subscribers that missed the event (were offline) will not receive it on next delivery — the WAL watcher will not re-deliver a deleted row.
2. **Reducers/state:** If any reducer processed the purged event and a snapshot was taken after that point, the snapshot's derived state includes the purged event's contribution. The snapshot is now inconsistent with replay-from-zero. Clear affected snapshots and re-run `read_state` to get a consistent state.
3. **Causation walks:** Events that had the purged event as their `causation_id` remain in the store. Their `causation_id` BLOB still holds the old id. A `walk_causation` from a child event backwards will find no row for the purged ancestor (it's deleted). The walk terminates there without error.
4. **Correlation queries:** `read_by_correlation` scanning for the purged event's `correlation_id` will not return the deleted row.

---

## 5. shred_stream — Crypto-Shredding (v1 Stub)

### 5.1 Current behavior

```rust
store.shred_stream(stream_id, reason)?;
```

In v1, this always returns `Error::NotImplemented` regardless of input:
- If `encryption == Plaintext`: `"shred_stream requires encryption mode; open the store with OpenOptions::encryption = OsKeyring or EnvVar"`
- If `encryption == OsKeyring` or `EnvVar`: `"shred_stream (crypto-shredding implementation is a future track)"`

The API signature exists in the public surface. The `stream_deks` table is in the schema. Neither the Rust crate nor the Python bindings have a working implementation in v1.

### 5.2 stream_deks schema

```sql
CREATE TABLE stream_deks (
    stream_id       TEXT    NOT NULL PRIMARY KEY,
    key_id          TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    shredded_at     INTEGER,          -- NULL until shredded
    shredded_reason TEXT              -- populated at shred time
);
```

Design: one Data Encryption Key per stream, identified by `key_id` (a reference into an external keyring — OS keyring or env-var-specified key store). When the DEK is shredded, `shredded_at` and `shredded_reason` are set.

### 5.3 Design intent for v1.1+

When implemented:
1. Each stream opened in encryption mode gets a DEK created and stored in `stream_deks`.
2. Payload bytes written to the `events` table are encrypted with the stream's DEK.
3. `shred_stream(stream_id, reason)` sets `shredded_at` and deletes the DEK from the external keyring. The `events` rows remain — fossic stays append-only at the row level.
4. Any subsequent read attempt on that stream decrypts with the (now-deleted) DEK and fails, making the payload content cryptographically inaccessible.

The `stream_deks.shredded_at` field is the durable record that the DEK was intentionally destroyed. Unlike `purge_event` which deletes a row, `shred_stream` is O(1) regardless of how many events the stream has — you delete one DEK, all events become inaccessible simultaneously.

**GDPR use case:** A user's data is isolated in their own stream (e.g., `users/u-12345/events`). On erasure request, `shred_stream("users/u-12345/events", "GDPR right-to-erasure #1234")` — all their events become inaccessible in one operation.

### 5.4 Why not just delete the rows?

Several reasons:
1. Fossic's immutability guarantee — row deletion breaks replay semantics.
2. Performance — deleting N rows requires N write-lock cycles; DEK deletion is one row.
3. Storage reclaim — encrypted row bytes remain (disk space is not freed). This is acceptable for the use case: the data is inaccessible but the storage cost is known and bounded.
4. Auditability — the `stream_deks` record shows when and why a stream was shredded.

---

## 6. Full Error Taxonomy

All variants of `fossic::Error` (from `src/error.rs`), with trigger conditions and recommended responses.

### 6.1 Stream errors

```rust
StreamNotDeclared { stream_id: String }
```
**Trigger:** `append` or `append_if` called with a stream_id not in the `streams` table.  
**Message:** `"stream not declared: {stream_id}"`  
**Recovery:** Call `declare_stream(stream_id, declared_by, description)` before the first append. For relay patterns, declare streams in the relay startup sequence.

```rust
InvalidStreamId { id: String, reason: String }
```
**Trigger:** Stream ID fails validation (null bytes, empty string, leading/trailing whitespace, invalid use of `_fossic/` prefix).  
**Message:** `"invalid stream ID '{id}': {reason}"`  
**Recovery:** Use valid stream IDs: slash-separated path segments, printable ASCII, no null bytes.

### 6.2 Event errors

```rust
InvalidEventId(String)
```
**Trigger:** `EventId::from_hex` received a string that is not exactly 64 lowercase hex characters.  
**Message:** `"invalid event ID: {0}"`  
**Recovery:** Validate that the string is exactly 64 chars and contains only `[0-9a-f]`.

```rust
EventNotFound { id: String }
```
**Trigger:** `purge_event` was called with an EventId that has no row in `events`.  
**Message:** `"event not found: {id}"`  
**Recovery:** Verify the event exists with `read_one(id)` before calling `purge_event`.

### 6.3 Store errors

```rust
StoreNotFound { path: String }
```
**Trigger:** `Store::open` called with `first_open_policy = MustExist` and no SQLite file at `path`.  
**Message:** `"store not found at '{path}'"`  
**Recovery:** Either create the store first (using `CreateIfMissing`), or check that the path is correct and the process has read permission.

```rust
SchemaMismatch { stored: u32, required: u32 }
```
**Trigger:** `PRAGMA user_version` in the database file is higher than `CURRENT_SCHEMA_VERSION` in this build (e.g., the db was created by a newer fossic binary).  
**Message:** `"schema version {stored} is newer than this build supports ({required}); upgrade fossic"`  
**Recovery:** Upgrade the fossic library to a version that supports the stored schema.

### 6.4 Branch errors

```rust
BranchNotFound { stream_id: String, branch_id: String }
```
**Trigger:** `promote_branch`, `mark_branch_dead_end`, `resolve_chain`, or `list_branches` called with a branch_id that does not exist for the given stream.  
**Message:** `"branch not found: {stream_id}/{branch_id}"`  
**Recovery:** Check spelling; verify with `list_branches(stream_id)`.

```rust
BranchLifecycleError { reason: String }
```
**Trigger:** Attempted an invalid lifecycle transition (e.g., promoting a dead_end branch).  
**Message:** `"branch lifecycle error: {reason}"`  
**Recovery:** Check the current lifecycle state with `list_branches` before attempting a transition.

```rust
InvalidBranchId { id: String, reason: String }
```
**Trigger:** Branch ID fails validation.  
**Message:** `"invalid branch ID '{id}': {reason}"`  
**Recovery:** Use branch IDs consisting of alphanumeric characters, hyphens, and underscores.

### 6.5 Reducer errors

```rust
ReducerPatternAmbiguous { a: String, b: String }
```
**Trigger:** `register_reducer` was called with a pattern that has the same specificity score as an already-registered pattern, and the two patterns could potentially match the same stream.  
**Message:** `"reducer patterns '{a}' and '{b}' are ambiguous (both match the same streams at equal specificity)"`  
**Recovery:** Use more specific patterns (add literal path segments) to break the tie, or redesign the pattern assignments so they don't overlap.

```rust
ReducerNotFound { stream_id: String }
```
**Trigger:** `read_state`, `read_state_at_version`, or `take_snapshot` called for a stream_id that no registered pattern matches.  
**Message:** `"no reducer registered matching stream '{stream_id}'"`  
**Recovery:** Register a reducer with a pattern that covers the stream before calling state operations.

```rust
ReducerNotFoundByName { name: String }
```
**Trigger:** Snapshot operation looked up a reducer by name (e.g., during snapshot GC) and found no registered reducer with that name.  
**Message:** `"no reducer registered with name '{name}'"`  
**Recovery:** Register the reducer before calling snapshot operations.

```rust
ReducerError { message: String }
```
**Trigger:** A reducer's `apply` method returned `Err`, or the Python DynReducer's `apply()` raised an exception, or `initial_state()` failed.  
**Message:** `"reducer error: {message}"`  
**Recovery:** Investigate the reducer logic. Check the `message` field for the underlying error from the reducer.

### 6.6 Snapshot errors

```rust
NoEventsToSnapshot { stream_id: String, branch: String }
```
**Trigger:** `take_snapshot` called on a (stream_id, branch) pair that has zero events.  
**Message:** `"no events to snapshot for stream '{stream_id}' branch '{branch}'"`  
**Recovery:** Verify that events exist (`read_range` returns non-empty) before calling `take_snapshot`.

### 6.7 Deletion errors

```rust
PurgeConfirmationError { got: String }
```
**Trigger:** `purge_event` called with a `confirm` string other than exactly `"I understand this breaks replay-from-zero"`.  
**Message:** `purge_event confirmation mismatch; confirm must be exactly "I understand this breaks replay-from-zero", got: "{got}"`  
**Recovery:** Use the exact confirmation string. Copy-paste from documentation.

```rust
NotImplemented { feature: &'static str }
```
**Trigger:** `shred_stream` called in v1 (regardless of encryption mode), or other reserved API surfaces.  
**Message:** `"not implemented in v1: {feature}"`  
**Recovery:** Do not call v1 stubs. Track the fossic changelog for implementation in v1.1+.

### 6.8 CCE errors

```rust
Error::Cce(CceError)
```
Sub-error type `CceError`:

```rust
CceError::U64Overflow(u64)
```
**Trigger:** A u64 value in the payload exceeds `i64::MAX` (9,223,372,036,854,775,807).  
**Message:** `"u64 value {0} exceeds i64::MAX; CCE integers are signed i64"`  
**Recovery:** Don't use u64 values above i64::MAX in event payloads. If you genuinely need large unsigned integers, store them as strings (with a schema note) and parse at read time.

```rust
CceError::DuplicateKeys
```
**Trigger:** The payload JSON object has duplicate keys after CCE encoding. JSON technically allows duplicate keys (though it's malformed) and some JSON producers emit them.  
**Message:** `"duplicate map keys after CCE encoding"`  
**Recovery:** Ensure payload dicts have unique keys. Standard JSON parsers usually deduplicate (last-key-wins), but if your producer emits duplicates, normalize them before appending.

```rust
CceError::StringTooLarge(usize)
```
**Trigger:** A string field in the payload exceeds 64 MiB (67,108,864 bytes) after NFC normalization.  
**Message:** `"string exceeds 64 MiB limit ({0} bytes)"`  
**Recovery:** Store large text blobs externally (object store, filesystem) and include only a reference (URL, path, content hash) in the event payload.

### 6.9 Upcaster errors

```rust
UpcasterChainGap { event_type: String, from: u32 }
```
**Trigger:** `UpcasterRegistry::apply` found no upcaster for `from` version but found upcasters for higher versions, indicating a gap in the chain.  
**Message:** `"upcaster chain gap: no upcaster registered for {event_type} from version {from}"`  
**Recovery:** Register a upcaster for the missing `(event_type, from_version)` pair. The chain must be contiguous with no gaps.

### 6.10 Validation errors

```rust
InvalidIndexedTags { got: &'static str }
```
**Trigger:** `indexed_tags` in an `Append` is not `None` and not a JSON object (e.g., it's a JSON array, string, or number).  
**Message:** `"indexed_tags must be a JSON object, got {got}"` where `got` is the type name (`"array"`, `"string"`, etc.).  
**Recovery:** Always pass a dict/object for `indexed_tags`, or pass `None` to omit it.

```rust
InvalidAlternatives
```
**Trigger:** `CreateBranch::alternatives` is not a JSON array.  
**Message:** `"alternatives must be a JSON array"`  
**Recovery:** Pass `alternatives` as a `Vec<String>` (Rust) or a list of strings (Python).

### 6.11 Infrastructure errors

```rust
PoolExhausted { pool_size: usize, timeout_ms: u64 }
```
**Trigger:** All `read_pool_size` read connections were busy and `read_pool_timeout_ms` elapsed with no connection becoming available.  
**Message:** `"read pool exhausted: all {pool_size} connections busy after {timeout_ms}ms; increase OpenOptions::read_pool_size"`  
**Recovery:** Increase `OpenOptions::read_pool_size` (default 4). If the timeout is the issue, increase `OpenOptions::read_pool_timeout_ms` (default 30,000). Investigate why all connections are being held simultaneously — `ReadGuard` drops should return connections promptly.

```rust
Internal(String)
```
**Trigger:** An unexpected internal state that should not occur under normal operation.  
**Message:** `"internal error: {0}"`  
**Recovery:** File a bug report with the fossic repository. This error indicates a logic defect in the library.

```rust
Sqlite(rusqlite::Error)
```
**Trigger:** A SQLite operation returned an error not otherwise handled — disk full, permission denied, file locked by OS (not SQLITE_BUSY), database corruption.  
**Message:** `"SQLite error: {0}"` (the underlying rusqlite error message).  
**Recovery:** Check disk space, file permissions, and database integrity (`PRAGMA integrity_check`).

```rust
MsgpackEncode(rmp_serde::encode::Error)
MsgpackDecode(rmp_serde::decode::Error)
```
**Trigger:** Payload serialization/deserialization failure. Encoding: a type in the payload is not msgpack-serializable (rare with `serde_json::Value`). Decoding: stored bytes are not valid msgpack (indicates database corruption or a write-path bug).  
**Message:** `"msgpack encode error: {0}"` / `"msgpack decode error: {0}"`  
**Recovery:** For encode errors, check the payload value for non-serializable types. For decode errors, run `PRAGMA integrity_check` and check for database corruption.

```rust
Io(std::io::Error)
```
**Trigger:** Filesystem I/O failure (file not found, permission denied during WAL operations, etc.).  
**Message:** `"I/O error: {0}"`  
**Recovery:** Check filesystem permissions and disk health.

### 6.12 CceError in Python

`CceError` variants wrap into `Error::Cce(CceError)`, which maps to Python's `StorageError` (the catch-all base-class exception for all non-specifically-mapped errors). There is no dedicated Python exception class for CCE errors. If you need to distinguish a CCE error in Python:

```python
try:
    store.append(...)
except fossic.StorageError as e:
    if "u64 value" in str(e):
        # handle U64Overflow
    elif "duplicate map keys" in str(e):
        # handle DuplicateKeys
    elif "64 MiB limit" in str(e):
        # handle StringTooLarge
```

---

## 7. Python Exception Hierarchy

The fossic Python module exposes a typed exception hierarchy (from `fossic-py/src/errors.rs`):

```
Exception
└── FossicError  (base class — catch all fossic errors with this)
    ├── StreamNotDeclaredError
    ├── InvalidStreamIdError
    ├── InvalidEventIdError
    ├── StoreNotFoundError
    ├── SchemaMismatchError
    ├── NotImplementedError
    ├── BranchNotFoundError
    ├── BranchLifecycleError
    ├── InvalidBranchIdError
    ├── ReducerPatternAmbiguousError
    ├── ReducerNotFoundError
    ├── ReducerNotFoundByNameError
    ├── ReducerCallError
    ├── NoEventsToSnapshotError
    ├── PurgeConfirmationError
    ├── EventNotFoundError
    ├── UpcasterChainGapError
    └── StorageError  (catch-all for Sqlite, Msgpack, Io, Cce, Internal, PoolExhausted)
```

18 exception classes. All inherit from `fossic.FossicError`, so catching `fossic.FossicError` catches any library error. The mapping from Rust `Error` variants to Python exceptions is one-to-one for all named variants; the `_` catch-all in `to_py_err` maps everything else to `StorageError`.

```python
import fossic

try:
    store.append(Append(stream_id="undeclared/stream", ...))
except fossic.StreamNotDeclaredError:
    store.declare_stream("undeclared/stream", declared_by="my-service")
    store.append(...)  # retry
except fossic.FossicError as e:
    logger.error(f"Unexpected fossic error: {e}")
    raise
```

---

## 8. Operational Checklists

### After a production purge_event call

1. Check `_fossic/system` for the `Purged` audit event:
   ```python
   system_events = store.read_range(ReadQuery(stream_id="_fossic/system", branch="main"))
   purge_records = [e for e in system_events if e.event_type == "Purged"]
   # Find the one matching event_id_purged == your_purged_id.hex()
   ```
2. Verify the original event is gone: `assert store.read_one(purged_id) is None`.
3. If any reducer has a snapshot that was taken after the purged event's version: clear those snapshots and re-derive state via `read_state` (which will replay without the purged event). There is no automatic snapshot invalidation on purge.
4. Notify downstream consumers (relay agents, dashboards) that the stream's history has a gap and replay-from-zero will diverge.
5. Retain the `Purged` audit event's hex id for the compliance/incident record.

### After a type_version bump and upcaster deployment

1. Register all upcasters (v1→v2, v2→v3 if applicable) **before** any code reads from the store.
2. Verify the chain is contiguous: test by calling `store.read_one(some_old_v1_event_id)` and confirming the returned payload has the expected v2 shape.
3. Increment `type_version` in the `Append` struct for all new writes.
4. Do NOT remove old upcasters while old events remain in any store that this code may read. Old upcasters are permanent.
5. Consider bumping `STATE_SCHEMA_VERSION` on affected reducers if the state shape changed due to the new event shape.
6. Call `gc_orphaned_snapshots` if `STATE_SCHEMA_VERSION` was bumped (old-schema snapshots are now stale).

### After upgrading reducer STATE_SCHEMA_VERSION

1. Register the reducer with the new `state_schema_version`.
2. Old snapshots (with the previous `state_schema_version`) are still in the table but will not be returned by `find_latest_snapshot` (which filters by schema version). They are harmless but waste space.
3. Call `gc_orphaned_snapshots` to delete them.
4. The next `read_state` call will do a full replay from events (no snapshot to start from). Take a snapshot immediately after to prime the cache: `store.take_snapshot(stream_id, branch)`.

---

*SR-08 of 9 — see SR-07 for cross-stream queries, SR-09 for Python bindings deep-dive.*
