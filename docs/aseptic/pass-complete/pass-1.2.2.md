# Pass Report — v1.2.2

**Date:** 2026-06-21
**Version:** v1.2.2
**Type:** feat — Phase 6 close (auto_gc_orphans drop-time GC fallback)

---

## 1. Deliverables status

| Deliverable | Status | Notes |
|---|---|---|
| `OpenOptions::auto_gc_orphans: bool` (default false) | DONE | `src/types.rs` |
| `impl Drop for Store` — GC fires on last-clone drop when flag set | DONE | `src/store.rs` |
| Arc::strong_count == 1 guard — only last clone triggers GC | DONE | `src/store.rs` |
| CP-T2-1 marker in `src/snapshots.rs` | DONE | above `gc_orphaned_snapshots_impl` |
| fossic-py drift fix — `auto_gc_orphans: false` in manual struct literal | DONE | `fossic-py/src/types.rs` |
| 3 new tests (flag-off, flag-on, last-clone-only) | DONE | `tests/snapshots.rs` |

---

## 2. Test results

286 tests, 0 failures. 3 net new snapshot tests.

```
test auto_gc_orphans_flag_off_no_gc_on_drop ... ok
test auto_gc_orphans_flag_on_gc_fires_on_drop ... ok
test auto_gc_orphans_only_fires_on_last_clone_drop ... ok
```

---

## 3. Files touched

Reference: `docs/aseptic/blast-radius/pass-1.2.2.md`

- Modified in d3e2dcc: `src/types.rs`, `src/store.rs`, `src/snapshots.rs`,
  `fossic-py/src/types.rs`, `tests/snapshots.rs`, `CHANGELOG.md`
- This commit: `Cargo.toml` (version bump), blast-radius, pass-complete

---

## 4. API changes

**New `OpenOptions` field:** `auto_gc_orphans: bool` (default `false`).

**New runtime behavior:** When `true`, `gc_orphaned_snapshots` is called synchronously at
last-clone drop time. Blocking write; callers needing predictable drop latency should keep
flag `false` and call explicitly.

**Breaking changes:** None. `OpenOptions::Default` provides the new field.

---

## 5. Living report updates

No new entries this pass.

---

## 6. Adjacent project impact

No cross-pollination file produced. fossic-py manual struct updated. No adjacent project
impact beyond the standard OpenOptions drift fix.

---

## 7. PASS COMPLETE message ready

```
── PASS COMPLETE · v1.2.2 · 2026-06-21 ──────────────────────

Title: Drop-Time GC Fallback (Phase 6 Close)
Summary: OpenOptions::auto_gc_orphans (default false) triggers gc_orphaned_snapshots at last-Store-clone drop time, purging snapshots for reducers no longer registered; Arc::strong_count guards against intermediate-clone false-fires.
Project: fossic

Highlights:
· impl Drop for Store — when auto_gc_orphans=true and Arc::strong_count==1, calls gc_orphaned_snapshots(); errors discarded (best-effort); callers needing a count call the method explicitly
· CP-T2-1 marker in snapshots.rs — Phase 7 will supplement with BackgroundExecutor::schedule(GcOrphanSnapshots, TaskPriority::Low); drop-time call retained as final-shutdown cleanup
· Phase 6 closed: v1.2.0 (EveryNEvents), v1.2.1 (ReducerStateLarge + StateAdaptive), v1.2.2 (auto_gc_orphans); v1.3.0 opens Phase 7

Learnings:
· Drop-time GC on Arc-backed types: Arc::strong_count(&self.inner) == 1 inside Drop for Store correctly identifies the last clone — the Arc's refcount has not yet been decremented when our Drop runs, so count==1 means "we are the only holder"

Commit: [PENDING_SHA]
Tests: 286 passed · 0 failed · 0 skipped
Branch: clean
```
