---
pass: 9
version: v0.9.0
date: "(retroactive estimate, not verified)"
impacts: [cerebra, policy-scout, bo]
---

# Cross-Pollination — Pass 9 (v0.9.0)

> All items in this file are retroactive estimates created at the Aseptic bootstrap
> (Pass v0.10.x). Verify against git log before trusting as precise record.

## cerebra

**Severity:** NEEDS-AWARENESS

**What changed:** `Store.open()` in the Python binding now expands tilde (`~`) paths
before passing to SQLite. A call like `Store.open("~/.fossic/store.db")` previously
created a file at a literal path starting with `~`; it now opens the correct
home-relative path.

**Action required:** Verify any existing `Store.open()` calls in cerebra. If cerebra
uses `os.path.expanduser()` before passing a path, the behavior is unchanged (double
expansion is safe — expanduser on an already-expanded path is a no-op). If cerebra
passes a raw `~` path, behavior has changed to be correct.

**Advocate-agent message:**
> fossic v0.9.0 shipped. `Store.open()` in the Python binding now handles tilde
> expansion internally.
>
> Impact for cerebra: any `Store.open()` call with a `~`-prefixed path now correctly
> resolves to the home directory. If you were using `os.path.expanduser()` as a
> workaround, you can remove it (double expansion is safe) or leave it — no breaking
> change either way.
>
> Severity: NEEDS-AWARENESS. Verify your Store.open() call sites; most likely no
> action required.

---

## policy-scout

**Severity:** FYI

**What changed:** Same tilde expansion note as cerebra. Policy Scout's store paths
likely use absolute paths or env vars; tilde expansion is probably irrelevant but
confirmed no breaking change.

**Advocate-agent message:**
> fossic v0.9.0: Store.open() now handles ~ paths. Impact for policy-scout: FYI only.
> If your store path uses an absolute path or an env var, no change. No action needed.

---

## bo

**Severity:** FYI — same as policy-scout. No action needed.

---

## lumaweave, ai-stack, rhyzome, bons.ai

No impact. lumaweave uses Node binding (unchanged). ai-stack is indirect. rhyzome and
bons.ai are benched.
