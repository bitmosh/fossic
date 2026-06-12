---
pass: 9
version: v0.9.0
date: "(retroactive estimate, not verified)"
summary: Tilde expansion in Python binding open(); FOSSIC_V1_SPEC.md initial authoring or major revision
---

# Blast Radius — Pass 9 (v0.9.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.
>
> The specific scope of Pass 9 is uncertain. Known outputs: tilde expansion was
> added to the Python binding's `Store.open()` path; the spec document may have
> received a major revision. Other changes may exist that are not captured here.

## Files

### Modified
- `fossic-py/src/store.rs` — `open()` method: expand `~` paths before passing to
  SQLite (`PathBuf::from(path).expand_home()` or equivalent)
- `docs/implement/FOSSIC_V1_SPEC.md` — significant revision or initial authoring
  (retroactive estimate — spec document appears to have been formalized around this
  time; may have been a dedicated doc pass)

---

## Public APIs

### Modified (non-breaking)
- `Store.open(path)` (Python) — tilde in path is now expanded by the binding before
  opening the SQLite file. `~/.fossic/store.db` opens correctly without consumer calling
  `os.path.expanduser()`.

---

## Schema changes

None.

---

## Configuration changes

None.

---

## Dependency changes

None.

---

## Behavior changes

- Python `Store.open("~/.fossic/store.db")` now opens at the home-relative path rather
  than creating a store at a literal path starting with `~`.

---

## Living report updates

New entries:
- POLISH_DEBT: PD-001 — tilde expansion spec examples inconsistent with binding behavior
  (spec examples still show `os.path.expanduser()` after binding was updated to handle tilde)
- DEVIATION: DV-003 — spec §14 Tokio threading model (if this is when the Tokio section
  was written — retroactive estimate)

No entries resolved.
