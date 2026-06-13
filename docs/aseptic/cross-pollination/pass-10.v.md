---
pass: 10.v
version: v0.10.v
date: 2026-06-12
summary: Spec clarification — purge semantics, branch conventions, BranchInfo fields, tilde expansion, upcaster docstring
---

# Cross-Pollination — Pass 10.v (v0.10.v)

> Cross-pollination records what adjacent projects need to know about changes made
> in this pass. Severity: **FYI** = awareness only, **NEEDS-AWARENESS** = read before
> implementing the named feature, **ACTION-REQUIRED** = breaking change; act before
> consuming the updated API.

---

## Cerebra

**Severity: FYI**

- Purge semantics clarified in §9.3. `read_one` returns `None` after purge; the
  `Purged` audit event is in `_fossic/system`, NOT in the original stream. Cerebra
  has not implemented purge workflows yet — no action needed now, but ensure Cerebra's
  purge path (when implemented) reads `_fossic/system` for audit, not the original stream.

---

## Policy Scout

**Severity: FYI**

- `list_branches` behavior documented in §8 ("Default branch convention"): returns empty
  list for streams with no forked branches — not an error, not an indication that main
  doesn't exist. If Policy Scout audits branch state, it should not treat an empty
  `list_branches` result as "no history."

---

## LumaWeave (src/)

**Severity: NEEDS-AWARENESS**

- §4.3 (Node binding) now has an explicit note: `Store.open` expands `~` paths via
  `shellexpand`. If LumaWeave's Tauri backend opens a fossic store with a tilde path
  via the Node binding (unlikely — the Tauri path uses the Rust core directly), do not
  pre-expand the path. The Rust `Store::open` does NOT apply shellexpand (only the
  language bindings do); for the Tauri consumer, use `app.path().app_data_dir()` as
  shown in §4.4.

---

## Bo / discord-bot / ai-stack

**Severity: FYI**

- No impact. Spec clarifications in this pass do not affect Bo's use of fossic (Bo
  is Python, reads events, does not use branches or purge).

---

## Rhyzome

**Severity: FYI**

- `BranchInfo` field names now documented canonically in §8: `.id`, `.lifecycle`
  (not `.branch_id`, `.status`). Rhyzome uses branches heavily — confirm all branch
  code uses the correct field names before implementing new branch features.
