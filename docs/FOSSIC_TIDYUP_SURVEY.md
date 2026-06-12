# Fossic Tidy-Up Survey

**Date:** 2026-06-12  
**Surveyor:** Supervisor Claude (survey-only pass)  
**Scope:** fossic/ · fossic-py/ · crates/fossic-tauri/ · fossic-node/ · .github/workflows/ · scripts/ · docs/implement/

---

## Section 1: Confirmed Issues from Prior Reports

---

### Issue 1 — DynReducer trait gap

**Status: PARTIALLY CONFIRMED — different from the report**

The Rust `BoxedReducer` trait exists (`fossic/src/reducers.rs:36`) and is used
internally for type erasure, but it is `pub(crate)` — not a public API that
Python or Node can reference. The `Reducer` trait uses compile-time associated
types as reported.

The Python binding's `register_reducer` / `read_state` are implemented entirely
in pure Python (`fossic-py/python/fossic/__init__.py:370-414`). Every
`read_state` call does a full event replay from version 0 (or from the
`to_version` bound) with no snapshot caching. The comment in the code
(`__init__.py:29`) explicitly acknowledges this:
> "the store always folds all events on each `read_state` call"

The napi-rs binding has **no `register_reducer` or `read_state` methods at all**.
JavaScript consumers cannot use the reducer/aggregate pattern.

The spec's `OpenOptions::similarity_search` field also does not exist in the
code (`fossic/src/types.rs`) — the `SimilaritySearchProvider` trait is not
declared anywhere in the codebase.

**Proposed fix:** Two separable concerns. (a) For Python snapshot caching: add a
`pub trait DynReducer` (or make `BoxedReducer` pub) so the Python PyO3 binding
can call `take_snapshot` and `compute_state_bytes` after applying Python-managed
state. Document the current limitation clearly in the meantime. (b) For Node.js:
add `register_reducer` / `read_state` to the Node binding if reducers are
in-scope for v1. (c) Declare `SimilaritySearchProvider` as a `pub trait` stub
(the spec promises it; the code should at least compile the type). All three are
additive changes.

**Severity: HIGH** — Python snapshot caching gap is a correctness-vs-performance
issue (not a data-loss bug, but a scaling cliff for any stream with many events).
Node missing reducers entirely is a spec gap. Missing `SimilaritySearchProvider`
means the spec's extension point is fiction.

---

### Issue 2 — Spec section 14 (Tokio integration) is wrong

**Status: CONFIRMED**

`fossic/src/types.rs` has no `tokio_handle` field on `OpenOptions`.
`fossic/Cargo.toml` has no `tokio` dependency. The core uses
`std::thread::spawn` + `crossbeam-channel` throughout. The spec's "two runtimes"
problem description and the `Handle::spawn` solution are fiction — the core never
had a Tokio runtime to conflict with.

The napi-rs binding (`fossic-node/src/store.rs`) does have `tokio = "1"` in its
own Cargo.toml and uses `tokio::task::spawn_blocking` throughout, but that's
napi-rs's own runtime management — it is not coordinated with any host handle.

Lines in the spec that need correction:
- §14 entire section ("Fossic uses Tokio internally for the subscription
  dispatcher, file-watcher, and OTel exporter" — false)
- §4.1 `OpenOptions.tokio_handle` field (does not exist)
- §17 row: `tokio::runtime::Handle option | LumaWeave` (dead reference)

**Proposed fix:** Rewrite spec §14 to describe the actual threading model: a
`std::thread::spawn` dispatcher thread + crossbeam-channel for subscription
dispatch, a `notify`-crate background scan thread for WAL watching. Remove the
`tokio_handle` field from the §4.1 `OpenOptions` struct definition. Remove the
§17 table row. Tauri consumers do not need to pass any handle; the dispatcher
and WAL watcher threads are internal to the store.

**Severity: HIGH** — Spec says to call `OpenOptions { tokio_handle: Some(...) }`
for Tauri consumers; this field does not exist and writing such code will fail to
compile. Any Tauri integration attempt following the spec will hit this
immediately.

---

### Issue 3 — Subscription glob patterns missing

**Status: CONFIRMED**

`fossic/src/subscriptions.rs:20-23`:
```rust
pub struct SubscribeQuery {
    pub stream_id: String,   // exact match only
    pub branch: String,
}
```

All four binding surfaces take exact `stream_id`:
- Python `fossic-py/src/store.rs:235`: `subscribe(stream_id: String, ...)`
- Node `fossic-node/src/store.rs:277`: `SubscribeQuery { stream_id: ... }`
- Tauri `crates/fossic-tauri/src/commands.rs:160`: `stream_id: String`

The spec (§4.2, §4.3, §7) shows `stream_pattern="cerebra/lattice/*"` in
subscription examples. The glob engine in `fossic/src/reducers.rs:179`
(`glob_matches`) is not shared with subscriptions — it is entangled in the
reducer registry module.

**Proposed fix:** Extract `glob_matches` and `pattern_specificity` from
`reducers.rs` into a new `fossic/src/glob.rs` utility module (pub(crate), or pub
if needed). Update `SubscribeQuery` to use a `stream_pattern: String` field.
Update the subscription dispatch path to call `glob_matches(entry.stream_pattern,
event.stream_id)` instead of `entry.stream_id == event.stream_id`. Update all
four binding surfaces. This is a two-phase change: (a) extract glob utils,
(b) update SubscribeQuery and dispatch. They can be done in one pass.

**Severity: HIGH** — The spec guarantees this feature; it is not present. Any
consumer that subscribes to `cerebra/lattice/*` today gets no events. Silent
failure.

---

### Issue 4 — `fossic_read_state_at_version` reducer_name unused

**Status: CONFIRMED — Tauri binding layer only**

`crates/fossic-tauri/src/commands.rs:133`:
```rust
pub fn fossic_read_state_at_version(
    store: State<'_, Store>,
    stream_id: String,
    branch: String,
    version: u64,
    _reducer_name: Option<String>,   // accepted, ignored
) -> Result<serde_json::Value, String>
```

The comment on line 122 says this is "forward-compatibility with v2
cross-language snapshot lookup." The underlying Rust `Store::read_state_at_version`
is generic (`read_state_at_version::<serde_json::Value>`) and does use the
registered reducer to fold events — it just doesn't take a name parameter because
the reducer is looked up by stream_id. The Tauri binding always decodes to
`serde_json::Value`.

This means the Tauri frontend gets JSON regardless of what the registered reducer
produces. For reducers whose state is not JSON-serializable (e.g., uses non-JSON
msgpack types like binary blobs), the `from_slice::<serde_json::Value>` call will
return an error. For well-behaved JSON-compatible reducers it works.

**Proposed fix:** Document the behavior explicitly in the command's doc comment:
"Decodes reducer state as JSON; requires that the reducer's state type is
JSON-serializable via msgpack." The `_reducer_name` parameter should either be
removed (if it is not going to be honored in v1) or documented clearly as
reserved. Low-cost fix: rename to `reducer_name` and add a doc comment noting it
is accepted but currently unused pending v2 cross-language snapshot support.

**Severity: MEDIUM** — Functional for JSON-compatible states; surprising for
others. The underscore prefix + wrong comment ("forward-compatibility") is
misleading. Spec says `reducer_name` selects the reducer; Tauri ignores it.

---

### Issue 5 — `update_wal_cursor` is dead code

**Status: CONFIRMED**

`fossic/src/subscriptions.rs:254-263`:
```rust
pub fn update_wal_cursor(&self, sub_id: u64, new_cursor: i64) { ... }
```

`grep -rn "update_wal_cursor"` across all `src/` files returns only the
definition. No caller exists. The WAL cursor is correctly advanced by
`dispatch_post_commit` (lines 220-233 in the same file) after successful
`try_send`. The `update_wal_cursor` function is the remnant of an older design
where the WAL watcher advanced cursors directly. `cargo clippy --all-targets`
will report this as a dead-code warning.

**Proposed fix:** Delete the function. Before deleting, add a comment to
`dispatch_post_commit` (or to the `SubscriberKind::PostCommit` definition)
documenting the cursor ownership invariant: "The WAL cursor is advanced only
here, in `dispatch_post_commit`, after a successful `try_send`. The WAL watcher
must never advance cursors directly — doing so would allow double-delivery or
skip-delivery bugs." (See also Issue 9.)

**Severity: LOW** — No runtime impact; dead code; will surface as a clippy
warning in CI (`-D warnings` in release.yml).

---

### Issue 6 — PyO3 version pinning

**Status: PARTIALLY CONFIRMED — less severe than reported**

`fossic-py/Cargo.toml`:
```toml
pyo3 = { version = "0.29", features = ["extension-module"] }
```

The default caret constraint in Cargo is `^0.29.0` meaning `>=0.29.0, <0.30.0`.
This is not an open-ended range — it will not jump to 0.30. It will pick up
0.29.x patch releases automatically. PyO3 has historically had minor-bump
breaking changes but within a major PyO3 series (0.29.x) the API is stable.

The spec says "PyO3 ≥ 0.26" as the minimum floor but the binding uses 0.29 APIs
(`Python::attach`, `py.detach()`) which are only available in 0.26+. Using
`^0.29` is stricter than necessary but safe.

**Proposed fix:** Either (a) leave as-is with a comment explaining why 0.29 is
the minimum (the `Python::attach` / `py.detach()` APIs from 0.26 are used but
0.29 is tested), or (b) change to `">=0.26, <0.30"` to match the spec's
stated floor. The `crossbeam-channel = "0.5"` dependency has no pin at all — for
consistency the pyo3 pin is fine. No urgent change needed.

**Severity: LOW** — Functionally correct; the caret range is safe.

---

### Issue 7 — `time = "=0.3.37"` pin in fossic-tauri

**Status: CONFIRMED — pin is present; necessity unclear**

`crates/fossic-tauri/Cargo.toml:21`:
```toml
time = "=0.3.37"
```

The comment says "avoid cookie 0.18.1 coherence conflict (cookie was designed
against time 0.3.36-0.3.37)." The fossic-tauri crate has its own `Cargo.lock`
(it's not in a workspace) so the pin is locally enforced. Whether Tauri 2 has
since dropped cookie 0.18.1 in favor of a newer version that doesn't require this
pin is unknown without checking the Tauri changelog — out of scope for this
survey (would require a web fetch).

**Proposed fix:** Before v1.0.0-rc.1, run `cargo update --precise time` with the
pin removed and verify the build still succeeds. If Tauri 2 has moved on, remove
the pin. If not, the pin is load-bearing and should stay with an updated comment
citing the specific Tauri version that introduced the fix.

**Severity: LOW** — Build still works with the pin. Only matters if an exact-pin
breaks a consumer's workspace that needs a different `time` version.

---

### Issue 8 — Test helper duplication (`unique_ev`)

**Status: CONFIRMED — Rust core has 2 definitions; Python is consolidated**

Occurrences:
- `fossic/tests/subscriptions.rs:34` — `fn unique_ev(stream_id: &str) -> Append`
- `fossic/tests/wal_watch.rs:27` — `fn unique_ev(stream_id: &str) -> Append`

These are two independent identical definitions in the same crate's test suite.
The Python binding uses a single shared definition in
`fossic-py/tests/conftest.py:32` (correct pattern). The Node binding has a
conceptual equivalent inline in `fossic-node/__test__/append-read.spec.ts` rather
than a shared helper.

**Proposed fix:** Create `fossic/tests/common/mod.rs` (or a `tests/helpers.rs`)
with a single `pub fn unique_ev(stream_id: &str) -> Append` and add
`mod common;` / `use common::unique_ev;` in the two test files that define it
independently. The Node binding version is a TypeScript test concern and can
remain inline.

**Severity: LOW** — Cosmetic inconsistency. The two Rust definitions are
identical so no behavioral divergence exists today, but they will drift.

---

### Issue 9 — Cursor ownership invariant undocumented

**Status: CONFIRMED — invariant is real and correct, but undocumented**

The invariant: *"Exactly one code path may advance the in-process subscription
WAL cursor — `dispatch_post_commit` in `subscriptions.rs`, after a successful
`try_send`. The WAL watcher must not advance cursors directly."*

This invariant is correctly implemented: `wal_watch.rs` sends events through
`dispatch_tx → dispatcher thread → dispatch_post_commit` (not directly via
`update_wal_cursor`). But the invariant lives only in conversation history and
the minds of the track authors. There is no comment in either
`subscriptions.rs` or `wal_watch.rs` that articulates it.

The dead-code `update_wal_cursor` function (Issue 5) is the residue of a design
that *would have* violated this invariant. Its existence is a trap for future
maintainers.

**Proposed fix:** (a) Delete `update_wal_cursor` (Issue 5 fix). (b) Add a
one-paragraph comment at the top of `SubscriptionRegistry::dispatch_post_commit`
explaining the cursor ownership rule and *why* it matters (double-delivery /
skip-delivery). (c) Add a brief comment in `wal_watch.rs`'s `run_scan_loop`
explaining why WAL events go through `dispatch_tx` rather than updating cursors
in-place. This is documentation-only work, no behavior change.

**Severity: MEDIUM** — An undocumented invariant in load-bearing subscription
delivery code. A future maintainer who adds a WAL cursor update in `wal_watch.rs`
will introduce a hard-to-reproduce double-delivery or skip-delivery bug.

---

### Issue 10 — Workspace structure: no root Cargo.toml

**Status: CONFIRMED — different from the report (not a workspace problem, just no workspace)**

There is no top-level `Cargo.toml` with a `[workspace]` section. Each crate is
standalone:
- `fossic/Cargo.toml` — fossic core
- `fossic-py/Cargo.toml` — PyO3 binding
- `fossic-node/Cargo.toml` — napi binding (its own Cargo.lock)  
- `crates/fossic-tauri/Cargo.toml` — Tauri companion (its own Cargo.lock)

The CI workflows compensate by using `working-directory:` per job. This works,
but it means:
- `cargo build` or `cargo test` at the repo root fails
- `cargo update` must be run separately per crate
- Shared dependencies (`serde`, `rusqlite`, etc.) have independent version
  locks per crate — drift is possible over time
- Contributor experience is worse than a workspace

**Proposed fix:** Add a root `Cargo.toml` with `[workspace] members = ["fossic",
"fossic-py", "fossic-node", "crates/fossic-tauri"]`. This unifies the lock file
and enables `cargo test --workspace`. The CI `working-directory:` directives can
remain or be simplified. One risk: workspace-level resolver may surface
dependency version conflicts between the crates that the standalone builds hid;
resolve any such conflicts as part of the workspace migration. This is a
non-trivial change if conflicts surface.

**Severity: MEDIUM** — Does not block v1.0.0 functionality. Does increase
long-term maintenance friction and dependency drift risk.

---

## Section 2: New Findings from the Additional Survey

---

### A1 — Node.js binding has no typed error hierarchy

**File:** `fossic-node/src/store.rs` (all methods)  
**What's wrong:** Every error in the Node binding is emitted as
`Error::new(Status::GenericFailure, e.to_string())`. TypeScript callers cannot
distinguish a `StreamNotDeclaredError` from a `BranchNotFoundError` —
programmatic error handling requires string-parsing the error message.  
**Proposed fix:** Add a typed error enum in `fossic-node/src/errors.rs` mirroring
the Python `errors.rs` exception hierarchy, mapped to `napi::Status` variants or
custom error codes. At minimum, expose the error variant name as a `code` property
on the thrown Error object, matching the pattern `@napi-rs` recommends. Full
typed exceptions require some napi-rs boilerplate but the Python binding is a
complete model to copy from.  
**Severity: HIGH** — TypeScript consumers must string-match error messages to
distinguish error types. This is fragile and undocumented. Python has the correct
design; Node should match it.

---

### A2 — `InvalidAlternatives` and `InvalidIndexedTags` missing from Python exception hierarchy

**File:** `fossic-py/src/errors.rs:16-40`  
**What's wrong:** The `to_py_err` match arm catch-all (`_ => StorageError::new_err(msg)`) silently absorbs `Error::InvalidAlternatives` and `Error::InvalidIndexedTags`. Python callers that catch `StorageError` will see these, but they cannot catch them specifically (no class `InvalidAlternativesError` or `InvalidIndexedTagsError` exists).  
**Proposed fix:** Add `pyo3::create_exception!(fossic, InvalidAlternativesError, FossicBaseError)` and `pyo3::create_exception!(fossic, InvalidIndexedTagsError, FossicBaseError)` in errors.rs. Add match arms in `to_py_err`. Register both in `register()`. This is a small copy-paste addition.  
**Severity: LOW** — These are input-validation errors. Callers that need to distinguish them from storage errors (e.g., to surface a user-friendly message for "alternatives must be a JSON array") cannot do so today.

---

### A3 — Tauri binding swallows all errors as `String`

**File:** `crates/fossic-tauri/src/commands.rs` (all commands), `crates/fossic-tauri/src/serialization.rs`  
**What's wrong:** Every Tauri command returns `Result<T, String>`. The frontend receives an error string and cannot programmatically distinguish a `StreamNotDeclaredError` from a `BranchNotFoundError`. This matches how many Tauri apps work but is inconsistent with the Python binding's typed hierarchy.  
**Proposed fix:** Return a structured error type instead of `String`. A minimal approach: define a `FossicTauriError { code: String, message: String }` serde type and return `Result<T, FossicTauriError>` where `code` is the enum variant name. Frontend callers can then branch on `error.code`. This is a breaking change to the command signature if the frontend is already consuming the current string form.  
**Severity: MEDIUM** — The spec describes typed errors; the Tauri surface does not expose them. Frontend consumers doing robust error handling must parse error strings.

---

### B1 — Node binding drops `reason` from `promote_branch` / `mark_branch_dead_end`

**Files:** `fossic-node/src/store.rs:241-262`, `fossic/src/lib.rs`, `fossic-py/src/store.rs`  
**What's wrong:**
- Rust: `promote_branch(stream_id, branch_id, reason: Option<&str>)`
- Python: `promote_branch(stream_id, branch_id, reason=None)` — consistent
- Node: `promote_branch(stream_id, branch_id)` — no `reason` parameter

Same for `mark_branch_dead_end`. The `reason` field in the `branches` table
(`closed_reason`) will always be NULL for branches closed via the Node binding.
**Proposed fix:** Add `reason: Option<String>` to both Node methods and pass
it through. Minor API addition.  
**Severity: MEDIUM** — Silent data loss (the reason field is always NULL from
Node). Not a correctness bug but a missing feature that consumers will notice in
observability tooling.

---

### B2 — Node cursor API has completely different signature

**Files:** `fossic-node/src/store.rs:295-316`, `fossic/src/store.rs:444-463`  
**What's wrong:**
- Rust: `get_cursor(consumer_id, stream_id, branch)` → `Option<u64>`
- Python: `get_cursor(consumer_id, stream_id, branch)` — consistent
- Node: `get_cursor(name)` → `Option<i64>` — completely different; takes a
  single opaque `name` string, returns `i64` (not bigint)

The spec defines `cursors` with a `(consumer_id, stream_id, branch)` primary key.
The Node binding collapsed these into a single `name` parameter, presumably as a
convenience, but the `set_cursor(name, value: i64)` signature loses the ability
to scope cursors per (stream, branch). Callers must manually compose the key.  
**Proposed fix:** Either (a) update the Node binding to match the Rust API:
`getCursor(consumerId, streamId, branch)` and `setCursor(consumerId, streamId,
branch, version: bigint)`, or (b) keep the simplified API but document the
expected naming convention (e.g., `"consumer:stream:branch"`) explicitly. Option
(a) is preferred for spec compliance.  
**Severity: MEDIUM** — Spec-divergent, silently incompatible with the Rust and
Python cursor models. Consumers migrating between bindings will produce corrupt
cursor state.

---

### B3 — `walk_causation` missing `WalkDirection::Both` in Node and Tauri

**Files:** `fossic-node/src/store.rs:186-210`, `crates/fossic-tauri/src/commands.rs:107-115`  
**What's wrong:** Both Node and Tauri parse the `direction` string but only handle
`"forward"` / `"backward"`. Node returns an error for `"both"`; Tauri silently
defaults to `WalkDirection::Forward` (via the `_ =>` arm returning an error for
any other string, including `"both"`). The Rust core supports `WalkDirection::Both`.  
**Proposed fix:** Add `"both"` / `"Both"` case in both binding surfaces.  
**Severity: LOW** — The spec documents `WalkDirection::Both`. Node and Tauri
callers who need bidirectional walk cannot use it today.

---

### C1 — `store.rs` is 877 lines; consider splitting off snapshot/reducer helpers

**File:** `fossic/src/store.rs`  
**What's wrong:** The file is well-organized by section headers but `compute_state_bytes`, `take_snapshot`, `gc_orphaned_snapshots`, `get_reducer`, and the `SnapshotInfo` delegation all live in `store.rs` while the actual snapshot and reducer logic is already in `snapshots.rs` and `reducers.rs`. The delegation wrappers in `store.rs` add length without much logic.  
**Proposed fix:** This is a judgment call. The current structure is readable and the section headers are clear. A split would reduce file size but add import complexity. Recommended: leave as-is, note for v1.1.  
**Severity: LOW** — Cosmetic/organizational; no correctness concern.

---

### D1 — `SimilaritySearchProvider` declared in spec but not in code

**File:** Spec `FOSSIC_V1_SPEC.md` §10.4, §15; `fossic/src/lib.rs`; `fossic/src/types.rs`  
**What's wrong:** The spec defines `SimilaritySearchProvider` as a declared trait
(extension point only). Neither the trait nor `OpenOptions::similarity_search` nor
`Store::similarity_query` exist anywhere in the code. The `similarity` feature flag
in `fossic/Cargo.toml` is declared but empty (`similarity = []`).  
**Proposed fix:** Add the trait stub and the `OpenOptions::similarity_search`
field (behind the `similarity` feature flag). Add `Store::similarity_query` that
returns `Err(Error::NotImplemented { feature: "similarity_query" })` when no
provider is registered, or calls the provider when one is. This is the "designed
for, not yet implemented" pattern the spec describes.  
**Severity: MEDIUM** — The feature flag is in Cargo.toml but the trait it's
supposed to gate doesn't exist. Consumers enabling `--features similarity` get
nothing. More importantly the spec's architectural extension point is invisible
to downstream implementors.

---

### D2 — `BoxedReducer` is `pub(crate)` but Python binding needs it

**File:** `fossic/src/reducers.rs:36`; `fossic-py/src/store.rs:396-403`  
**What's wrong:** The Python `take_snapshot` docstring says:
> "NOTE: requires a Rust reducer registered for stream_id. Python-side reducers
> … do not interact with this method. See the register_reducer docstring and the
> core-change request in FOSSIC-PY-NOTES.md."

This is a known gap that requires making `BoxedReducer` public (or creating a
`DynReducer` public trait) so the binding can call the snapshot machinery with a
Python-provided reducer. Related to Issue 1.  
**Proposed fix:** Make `pub trait DynReducer` that has the same methods as
`BoxedReducer` but is `pub`. Python and Node bindings implement it with their
callback wrappers, then call a new `Store::register_dyn_reducer` that accepts a
`Box<dyn DynReducer>`. This is the same as Issue 1's fix path.  
**Severity: HIGH** (see Issue 1 — same root cause).

---

### E1 — Dependency drift risk: no shared Cargo.lock across the monorepo

**Related to Issue 10.** Because there's no workspace, `serde`, `serde_json`,
`rusqlite`, `blake3`, `crossbeam-channel`, and `rmp-serde` all have independent
version locks in each crate. Current state looks consistent (all use `"1"` for
serde/serde_json/rmp-serde, `"0.31"` for rusqlite, `"0.5"` for
crossbeam-channel, `"1"` for blake3) but divergence is one `cargo update` away.  
**Proposed fix:** Addressed by workspace migration (Issue 10).  
**Severity: LOW** — No current divergence; future risk.

---

### F1 — Release smoke test for Node uses wrong API calls

**File:** `.github/workflows/release.yml:208-220`  
**What's wrong:** The Node smoke test:
```javascript
const store = fossic.openStore(path.join(dir, 'smoke.db'));
store.declareStream('smoke/test', 'ci');
store.append({ streamId: 'smoke/test', ... });
const evts = store.readRange({ streamId: 'smoke/test', branch: 'main' });
```

Three problems:
1. `fossic.openStore(...)` does not exist. The napi-rs binding exposes a class
   `Store` with a factory method; the JS call should be `fossic.Store.open(...)`
   (or the equivalent after `#[napi(factory)]` codegen — needs verification).
2. `declareStream`, `append`, and `readRange` are all `async` (return Promises)
   in the binding, but the smoke test calls them synchronously.
3. `store.readRange(...)` is not a top-level export — it is a method on `Store`.

This smoke test runs on every release tag (the `if:` condition now passes since
`fossic-node/package.json` exists) and will fail.  
**Proposed fix:** Rewrite the smoke test using `async/await`:
```javascript
const { Store } = require('.');
const store = Store.open(path.join(dir, 'smoke.db'));
await store.declareStream('smoke/test', 'ci');
await store.append({ streamId: 'smoke/test', eventType: 'SmokeEvent',
                     typeVersion: 1, payload: { ok: true } });
const evts = await store.readRange({ streamId: 'smoke/test', branch: 'main' });
```
Run with `node --input-type=module -e "..."` or use a `.mjs` heredoc.  
**Severity: BLOCKER** — Every release with fossic-node present will fail at the
smoke test step. The release pipeline aborts before publishing.

---

### F2 — `fossic-node` has no `package-lock.json`; CI uses `npm ci`

**File:** `.github/workflows/ci.yml:113`; `fossic-node/`  
**What's wrong:** The CI Node job uses `npm ci` (which requires `package-lock.json`
to exist), but `fossic-node/package-lock.json` does not exist. The
`devDependencies` in `package.json` (`@napi-rs/cli`, `typescript`, `vitest`) have
never been installed. `npm ci` will fail with "missing package-lock.json".  
**Proposed fix:** Run `npm install` in `fossic-node/` (with dev approval per the
CLAUDE.md safeguard since this installs new packages) to generate
`package-lock.json`, then commit it. Until then, change `npm ci` to `npm install`
in the CI job as a temporary measure.  
**Severity: BLOCKER** — CI for Node tests always fails. This also means the napi
binding has never been built or tested in CI.

---

### G1 — One production `expect()` in `cce.rs`

**File:** `fossic/src/cce.rs:44`  
**What's wrong:**
```rust
let f = n.as_f64().expect("serde_json Number must be i64, u64, or f64");
```
This is logically infallible — `serde_json::Number` has exactly three
representations (i64, u64, f64) and if neither `as_i64()` nor `as_u64()`
returns `Some`, then `as_f64()` must succeed. The expect message documents the
reasoning.  
**Proposed fix:** Replace with `unreachable!("serde_json Number variant exhausted")` which better communicates "this cannot happen" versus "this might happen but we'd like it not to". No behavioral change.  
**Severity: LOW** — Logically infallible; the expect message is adequate.

---

### H1 — `#[allow(dead_code)]` on `path` field in `StoreInner`

**File:** `fossic/src/store.rs:46`  
**What's wrong:**
```rust
#[allow(dead_code)]
path: PathBuf,
```
The `path` field is stored at construction time but never read. The intent is
likely diagnostic (a debugger can inspect it), but in a shipped library this adds
noise. The actual path is needed only for the WAL watcher and the dispatcher
thread, both of which receive it at construction time and don't need to access it
from `StoreInner` afterward.  
**Proposed fix:** Remove the field. If diagnostic access to the path is needed,
add a `pub fn path(&self) -> &Path` accessor instead.  
**Severity: LOW** — Cosmetic.

---

### H2 — `TODO` in `deletion.rs` for logging framework

**File:** `fossic/src/deletion.rs:40`  
**What's wrong:** `// TODO: upgrade to tracing::warn! or log::warn! when a logging framework is adopted.`
The entire codebase uses `eprintln!` for all logging (store.rs, wal_watch.rs,
deletion.rs). There is no logging framework. This is a known gap but has no
tracking beyond this one comment.  
**Proposed fix:** This is a future concern. For v1, `eprintln!` is acceptable.
Document in `LATTICA_NOW.md` or the v1.1 roadmap. No code change needed before
v1.0.  
**Severity: LOW** — v1 deliverable; flagged for v1.1.

---

### I1 — Invariant 4 (snapshots don't affect correctness) has no property test

**Files:** `fossic/tests/snapshots.rs`; `fossic/tests/reducers.rs`  
**What's wrong:** The spec (§16, invariant 4) states: "Reading aggregate state
without any snapshot always produces the same result as reading with snapshots."
No test directly asserts `read_state(no_snapshot) == read_state(with_snapshot)`
for a varied event sequence. The snapshot tests exercise that snapshots are
written and read, but not that they are equivalent to full replay.  
**Proposed fix:** Add a test in `fossic/tests/snapshots.rs`:
```rust
// Append N events; take snapshot at midpoint; call read_state
// before and after snapshot exists; assert outputs identical.
```
This is a single test, ~30 lines.  
**Severity: MEDIUM** — Missing test for a named spec invariant. Not a known bug,
but an uncovered guarantee.

---

### I2 — Chained upcaster composition test may be missing

**Files:** `fossic/tests/upcasters.rs`  
**What's wrong:** The spec (§13) describes chaining: "an event at type_version=1
with registered upcasters 1→2 and 2→3 is upcast through both before reaching
the reducer." This should be directly tested with a three-version chain (store v1
event, register v1→v2 and v2→v3, read and assert v3 shape). Needs verification
against the actual test file (not read in this survey).  
**Proposed fix:** Add a three-version chain test if one does not exist.  
**Severity: MEDIUM** — If the chain test is absent, a future change to
`apply_upcaster` could silently break chaining.

---

### J1 — `_fossic/system` event types use inconsistent naming conventions

**Files:** `fossic/src/deletion.rs:11`; `fossic/src/store.rs:847, 873`  
**What's wrong:**
- Purge audit event: `PURGED_EVENT_TYPE = "fossic.Purged"` — dot-namespaced
- Subscription degraded: `"SubscriptionDegraded"` — bare PascalCase, no namespace

The spec (§9.2) mentions `ShreddedStreamMarker` (bare PascalCase, no namespace).
Three event types, three different implied naming conventions. Consumers querying
`_fossic/system` must know the exact string for each type.  
**Proposed fix:** Pick one convention for all system stream events and document it
in the spec. Recommendation: bare PascalCase with no namespace (consistent with
`SubscriptionDegraded` and `ShreddedStreamMarker`). Rename `"fossic.Purged"` to
`"Purged"` and update the constant in `deletion.rs`. This is a breaking change
for any consumer who has already filtered on `"fossic.Purged"` — but the system
stream has no external consumers yet.  
**Severity: MEDIUM** — Inconsistent; confusing for anyone instrumenting the
system stream. Simple fix while there are no downstream consumers.

---

### K1 — Tilde (`~`) in paths is not expanded by any binding

**Files:** Spec `FOSSIC_V1_SPEC.md` §4.2 (Python example); `fossic-py/src/store.rs:92`  
**What's wrong:** The Python example in the spec uses:
```python
store = Store.open(path="~/.fossic/store.db", ...)
```
but no binding performs tilde expansion. The Python binding passes the string
directly to `Store::open` → `Connection::open`, which passes it to SQLite. SQLite
does not expand `~`. The store would be created at a literal path starting with
`~` rather than the home directory.  
**Proposed fix:** Either (a) expand `~` in the Python binding's `open` method
using `Path(path).expanduser()`, or (b) change the spec example to use
`os.path.expanduser("~/.fossic/store.db")`. Option (b) is lower-risk (no hidden
behavior change). Document the behavior explicitly in all binding READMEs.  
**Severity: MEDIUM** — The spec example is actively wrong. Any consumer copying
it verbatim will create a store in the current directory under a file named
`~/.fossic/store.db` instead of the home directory path.

---

### L1 — No READMEs for any binding crate

**Files:** `fossic-py/`, `fossic-node/`, `crates/fossic-tauri/`  
**What's wrong:** None of the three binding crates have a `README.md`. The spec
(§3 of BUILD_AND_DISTRIBUTION.md) says "The crate's README is the same as the
main spec's 'API surface' section but reformatted for crates.io discovery."
crates.io / PyPI / npm all surface the README as the package landing page.  
**Proposed fix:** Write minimal READMEs for each crate (2-3 paragraphs + quick
start example). The spec examples in §4.2, §4.3, §4.4 are the quick-start
content.  
**Severity: MEDIUM** — Required for viable publishing. Without a README, PyPI and
npm pages show nothing useful.

---

### M1 — Bench validation uses `p99_total_us` — correct, not write_us

**File:** `.github/workflows/bench-validation.yml:68`  
**What's wrong:** Nothing. The Track F report that the bench compares against
`write_us` instead of `p99_total_us` is **incorrect** — the workflow compares
`bdata["p99_total_us"]` vs `rdata["p99_total_us"]`. This is the correct metric
per BUILD_AND_DISTRIBUTION.md §6.3 ("Regression > 10% on p99 total_us blocks the
release").  
**Severity: NOT AN ISSUE** — Track F was wrong. No action needed.

---

### N1 — `fossic-node` Cargo.toml has dependency request comment but deps are live

**File:** `fossic-node/Cargo.toml`  
**What's wrong:** The file contains a comment block:
```toml
# [DEPENDENCY REQUEST — REQUIRES MANUAL APPROVAL]
# napi = ...
# napi-derive = ...
```
immediately above the `[dependencies]` section where those same packages appear
**uncommented** as actual dependencies. The comment is vestigial from when the
track requested approval; the deps were subsequently added without removing the
comment. It is confusing to future readers (the comment implies the deps are not
installed, when in fact they are in `[dependencies]`).  
**Proposed fix:** Remove the comment block. The deps are live.  
**Severity: LOW** — Confusing only; no functional impact.

---

### N2 — `fossic-node` has no `Cargo.lock` and no `package-lock.json`

**File:** `fossic-node/`  
**What's wrong:** (Related to F2 above.) The napi crates (`napi`, `napi-derive`,
`napi-build`) are listed in `Cargo.toml` but no `Cargo.lock` exists for
`fossic-node` (confirmed by `ls`). Without a lock file, the exact crate versions
are indeterminate, and `cargo build` will download from crates.io on first run.
Additionally, `npm ci` requires `package-lock.json` which also does not exist.

The build.rs (`extern crate napi_build; fn main() { napi_build::setup(); }`) is
correct. Once crate downloads are approved and `npm install` generates the lock
file, the binding should build.  
**Proposed fix:** (a) Approve the napi/napi-derive/napi-build crate downloads
(these are canonical crates.io packages maintained by the napi-rs team). (b) Run
`cargo build` in `fossic-node/` to generate `Cargo.lock`. (c) Run
`npm install` in `fossic-node/` to install `@napi-rs/cli` and generate
`package-lock.json`. (d) Commit both lock files. Until done, the Node binding
cannot be built or tested by CI or contributors.  
**Severity: BLOCKER** — The Node binding is unverified and cannot be built.

---

## Section 3: Summary Tables

### 3.1 Findings by Severity

| Severity | Count | Items |
|----------|-------|-------|
| BLOCKER  | 3     | F1 (smoke test wrong API), F2/N2 (no lock files → CI fails), N2 (Node unbuilt) |
| HIGH     | 5     | Issue 1 (DynReducer), Issue 2 (Tokio spec wrong), Issue 3 (no glob subscriptions), A1 (no typed errors in Node), D1 (SimilaritySearchProvider missing) |
| MEDIUM   | 12    | Issue 4 (reducer_name unused), Issue 9 (cursor invariant), Issue 10 (no workspace), A2, A3, B1, B2, D2, I1, J1, K1, L1 |
| LOW      | 11    | Issues 5, 6, 7, 8, B3, C1, E1, G1, H1, H2, N1 |
| NOT AN ISSUE | 1 | M1 (bench metric — Track F was wrong) |

**Total actionable findings:** 31 (3 BLOCKER + 5 HIGH + 12 MEDIUM + 11 LOW)

### 3.2 Findings by Artifact

| Artifact | BLK | HIGH | MED | LOW | Total |
|----------|-----|------|-----|-----|-------|
| fossic (Rust core) | 0 | 2 (Issues 1†, 3†) | 4 (Issues 5, 9, 10; I1) | 4 (Issues 6†, 7†, 8; G1) | 10 |
| fossic-py | 0 | 0 | 2 (K1, A2) | 1 (Issue 6) | 3 |
| fossic-tauri | 0 | 0 | 2 (Issue 4, A3) | 1 (B3†) | 3 |
| fossic-node | 2 (F2, N2) | 1 (A1) | 3 (B1, B2, D2†) | 3 (B3†, H1†, N1) | 9 |
| Build infra / CI | 1 (F1) | 0 | 2 (L1) | 2 (E1, H2) | 5 |
| Spec docs | 0 | 3 (Issue 2; D1; D2†) | 3 (I2, J1, K1†) | 1 (N1†) | 7 |

† = finding spans multiple artifacts; counted in primary location.

### 3.3 Estimated Cleanup Effort

**Critical path:**

1. **Unlock Node binding (BLOCKERs F1, F2, N2):** Approve napi crate downloads,
   run `npm install`, fix smoke test. One focused session (~2h). Must happen
   before any release work.

2. **Spec corrections (Issues 2, 3 spec side; D1):** Documentation-only. One
   focused session (~3h). Can happen in parallel with code work.

3. **Subscription glob patterns (Issue 3):** Extract glob utils, update
   SubscribeQuery, update all four binding surfaces. One focused session (~4h).
   Has downstream test implications.

4. **Error type parity (A1, A3):** Add typed error hierarchy to Node binding;
   add structured error type to Tauri. One focused session (~3h).

5. **Naming/API gaps (B1, B2, K1):** Small additions to Node binding; tilde
   expansion. One focused session (~2h).

6. **Documentation gaps (L1, Issues 9, H2, I1):** READMEs, cursor invariant
   comments, snapshot property test. One focused session (~3h).

7. **DynReducer / SimilaritySearchProvider (Issue 1, D1, D2):** Architectural
   addition. Two sessions at minimum — one to design the public trait surface,
   one to implement + wire into Python/Node. Can be deferred to v1.1 if Python
   snapshot caching is documented as a known limitation.

**Total estimate for BLOCKER + HIGH + most MEDIUM:**  
~2 focused sessions for non-DynReducer items, + 2 sessions for DynReducer/  
SimilaritySearchProvider. LOW items are a single additional light session.

**Items on the critical path to v1.0.0-rc.1 (must be done sequentially):**  
1. Unlock Node binding → 2. Fix CI → 3. Glob subscriptions → 4. Spec corrections.
Items 5-7 can parallelize once CI is green.

---

## Section 4: Recommendations

### Fix as a coherent unit (shared root cause / shared change surface)

**Group A — Node binding bootstrap (Issues F1, F2, N2):**  
All three require a single development session: approve crate downloads, run `npm
install`, commit lock files, fix smoke test. These are prerequisites for any
other Node work.

**Group B — Subscription glob + SubscribeQuery naming (Issue 3, B1, B2):**  
Once `glob_matches` is extracted to `fossic/src/glob.rs`, the subscription
pattern filter, the `reason` parameter additions, and the cursor signature fix can
all be done in the same Node-binding pass.

**Group C — Typed error hierarchy in Node/Tauri (A1, A3):**  
The Python `errors.rs` is the reference implementation. Copy-adapt it for Node,
then decide the Tauri error strategy. One pass touches both.

**Group D — Spec corrections (Issue 2, D1, K1 spec side, J1 naming):**  
All are documentation changes to `FOSSIC_V1_SPEC.md`. Batch them: rewrite §14
(Tokio), add SimilaritySearchProvider stub note, fix the `~/.fossic` example, fix
system stream event type table. One PR.

**Group E — Dead code + invariant documentation (Issues 5, 9):**  
Delete `update_wal_cursor`, add cursor ownership invariant comments in
`subscriptions.rs` and `wal_watch.rs`. One tiny PR.

### Defer to v1.1 without harm

- **DynReducer / Python snapshot caching (Issue 1, D2):** The current behavior
  (full replay on every `read_state`) is correct and safe. It is a performance
  issue for high-event-count streams. Document the limitation clearly in v1 and
  address in v1.1. Do NOT defer without documentation.
- **Workspace Cargo.toml (Issue 10):** Not a v1 blocker. Post-v1 cleanup.
- **`fossic-http-gateway`** — already deferred to v1.1 per spec.
- **Logging framework (H2):** v1.1 concern.
- **`SimilaritySearchProvider` implementation:** v2 per spec. Only the trait stub
  needs to be in v1.

### Findings that surface spec ambiguity (decide-and-document, not fix-the-code)

1. **`_fossic/system` event type naming (J1):** The spec never specifies whether
   system stream events should use `"Purged"` vs `"fossic.Purged"`. The code has
   both. Decision: pick `PascalCase` bare (no namespace) for all three types
   (`Purged`, `SubscriptionDegraded`, `ShreddedStreamMarker`) and update the
   constant. This is a breaking change only for consumers querying the system
   stream (currently none).

2. **Tilde expansion in paths (K1):** The spec example uses `~/.fossic/store.db`.
   The spec does not say whether tilde expansion is the binding's responsibility
   or the consumer's. Decision: document that tilde expansion is the consumer's
   responsibility; update the spec example to use `os.path.expanduser()`.

3. **`_reducer_name` in Tauri `fossic_read_state_at_version` (Issue 4):** The
   spec says the parameter selects the reducer; the Tauri binding ignores it.
   Decision: document in the Tauri command's doc comment that in v1 the parameter
   is accepted for future compatibility but unused; the reducer is always the
   one registered for the stream_id.

4. **Node cursor API simplification (B2):** The Node binding collapsed the
   three-argument cursor API to one. This may be intentional (ergonomic for the
   common single-consumer case) or an oversight. Decision: align with the Rust API
   (three args) for cross-language consistency, or document the deliberate
   simplification.

### Urgent flags

No security issues were found. The purge audit trail correctly records the purge
before deleting the row. The `confirm` string check is correct. Encryption modes
return `NotImplemented` consistently (no silent bypass).

The only operationally urgent items are the three BLOCKERs (F1, F2, N2) which
prevent any release involving the Node binding.

---

*End of survey. 31 actionable findings. No security issues. The Rust core is
solid; most issues are in the binding surfaces and spec documentation.*
