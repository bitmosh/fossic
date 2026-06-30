---
pass: v1.1.0
version: v1.1.0
date: 2026-06-20
summary: Bounded Resource API foundation — types, OpenOptions extensions, dispatch channel observability
---

# Blast Radius — Pass v1.1.0

## Files

### Created
- `tests/bounded_foundation.rs` — 15 foundation-type tests
- `CHANGELOG.md` — project changelog (first entry)
- `docs/aseptic/blast-radius/pass-1.1.0.md` — this file

### Modified
- `Cargo.toml` — version 0.1.0 → 1.1.0; `[[test]] bounded_foundation` entry added
- `src/types.rs` — `BudgetKind`, `ReadOutcome<T>`, `TruncationReason`, `TruncationCursor`,
  `CursorInner` (pub crate), `SamplingMode` added; `OpenOptions` gains `default_max_results`
  and `default_max_bytes` fields (both `Option<usize>`, default `None`)
- `src/error.rs` — `use crate::types::BudgetKind` added; `Error::ReadBudgetExceeded { budget: BudgetKind, limit: usize }` variant added
- `src/store.rs` — `AtomicUsize`/`Ordering` imports added; `StoreInner::dispatch_channel_high_water_mark: Arc<AtomicUsize>` added; HWM tracking at all three `dispatch_tx.send` sites (append, append_batch, append_if); `Store::dispatch_channel_pressure()` and `Store::dispatch_channel_high_water_mark()` methods added
- `src/lib.rs` — `BudgetKind`, `ReadOutcome`, `SamplingMode`, `TruncationCursor`, `TruncationReason` added to public re-exports
- `fossic-py/src/types.rs` — `TryFrom<&PyOpenOptions> for OpenOptions` struct literal: `default_max_results: None, default_max_bytes: None` added (drift fix)

---

## Changes

### `src/types.rs` — new bounded-read types

**`BudgetKind`** (`Copy, Debug, PartialEq, Eq`): `ResultCount | ByteSize`. Discriminant for `Error::ReadBudgetExceeded` and the two new `OpenOptions` fields.

**`ReadOutcome<T>`** (`Debug`): `Complete(T) | Truncated { data: T, cursor: TruncationCursor, reason: TruncationReason }`. No existing API returns this yet; it is the return type of the bounded read methods shipping in v1.1.2+.

**`TruncationReason`** (`Copy, Debug, PartialEq, Eq`): `ResultCount | ByteSize`. Parallel to `BudgetKind` but on the outcome side (which kind triggered the truncation).

**`TruncationCursor`** (`Debug`): opaque `Vec<u8>` wrapper. Public API: `from_bytes(Vec<u8>)`, `into_bytes()`, `as_bytes()`. Internal `pub(crate)` methods: `encode(&CursorInner) -> Result<Self>` and `decode(&self) -> Result<CursorInner>` via msgpack. Dead-code warnings expected until v1.1.2 call sites exist.

**`CursorInner`** (`pub(crate)`, `serde::Serialize + Deserialize`): three variants encoding the resume state for each bounded read shape:
- `Range { stream_id, branch, next_version }` — for `read_range_bounded`
- `Correlation { correlation_id: [u8; 32], after_timestamp_us }` — for `read_by_correlation_bounded`
- `Causation { start_id: [u8; 32], depth, last_seen_id: [u8; 32] }` — for `walk_causation_bounded`

**`SamplingMode`** (`Copy, Debug, PartialEq, Eq`): `Exhaustive | BreadthFirst { max_per_level: usize } | Adaptive { target_count: usize }`. Controls graph-walk truncation strategy.

**`OpenOptions::default_max_results: Option<usize>`** — store-level default result-count ceiling. `None` by default. Callers using `..Default::default()` are unaffected.

**`OpenOptions::default_max_bytes: Option<usize>`** — store-level default byte-size ceiling. `None` by default. Callers using `..Default::default()` are unaffected.

### `src/error.rs` — `ReadBudgetExceeded`

New variant `ReadBudgetExceeded { budget: BudgetKind, limit: usize }`. Not yet raised by any production code path; reserved for v1.1.2+ bounded read implementations. Import added: `use crate::types::BudgetKind`.

### `src/store.rs` — dispatch channel observability

`StoreInner` gains `dispatch_channel_high_water_mark: Arc<AtomicUsize>` initialized to 0. Updated atomically at every `dispatch_tx.send` site using `fetch_max(len + 1, Relaxed)` — tracks the historical peak channel depth.

`Store::dispatch_channel_pressure() -> usize` — returns `dispatch_tx.len()` (current pending count).
`Store::dispatch_channel_high_water_mark() -> usize` — returns the peak seen since store open.

No changes to dispatch semantics, subscriber delivery, or cursor advancement.

### `fossic-py/src/types.rs` — drift fix

Manual `OpenOptions` struct literal in `TryFrom<&PyOpenOptions>` was missing the two new fields. Added `default_max_results: None, default_max_bytes: None`. No Python-visible API change.

---

## Public API changes

**New types (all additive):**
- `BudgetKind` (enum, Copy)
- `ReadOutcome<T>` (enum)
- `TruncationReason` (enum, Copy)
- `TruncationCursor` (struct)
- `SamplingMode` (enum, Copy)

**New error variant:** `Error::ReadBudgetExceeded { budget: BudgetKind, limit: usize }`

**Extended:** `OpenOptions::default_max_results`, `OpenOptions::default_max_bytes`

**New methods:** `Store::dispatch_channel_pressure()`, `Store::dispatch_channel_high_water_mark()`

**Breaking changes:** None. All changes are additive. Manual `OpenOptions` struct literal callers must add the two new fields (fossic-py fixed inline; fossic-node uses `..Default::default()` — unaffected).

---

## Tech debt / polish debt

**No new entries this pass.**

`TruncationCursor::encode/decode` and `CursorInner` emit dead-code warnings; these are expected and documented in CHANGELOG.md. They activate when bounded read methods ship in v1.1.2.

---

## Adjacent project notifications

- **fossic-py**: drift-fixed inline in this pass. No Python-visible API changes.
- **fossic-node**: uses `..Default::default()` for `OpenOptions` — unaffected.
- **fossic-tauri**: uses `..Default::default()` for `OpenOptions` — unaffected.
- **Downstream consumers** (cerebra, lumaweave, policy-scout, ai-stack): no API surface changes visible across the FFI boundary in this pass.
