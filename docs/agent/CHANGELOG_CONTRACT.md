# The report format — `CHANGELOG_CONTRACT.md`

The load-bearing interface between your agent and `bumper`. Your agent posts this format to the
changelog channel at the end of a task; `bumper`'s parser reads it. If the contract is stable, the
parser stays a few dozen lines of string-matching. If it drifts, the parser fights it forever.

The changelog channel is **write-once by the agent** (the end-of-task report). `bumper` only
reads it.

---

## The format

```
── PASS COMPLETE · v<version> · YYYY-MM-DD ──────────────────────

Title: <4–8 words, blog-suitable, not a commit message>
Summary: <one sentence, 20–300 chars — becomes the post description>
Project: <enrolled project name>     ← optional; omit to use the config fallback module

Highlights:
· <concrete behavioral change — what changed, not which file>
· <concrete behavioral change>
· <concrete behavioral change>

Learnings:
· <optional — methodology/architecture insight>
· <optional>

Commit: <7-char commit SHA>
Tests: <N> passed · <M> failed · <K> skipped
Branch: clean
```

Bare fields (no leading dashes), `· ` bullets.

### Field rules

| Field | Required | Rule |
|---|---|---|
| Header `v<version>` | yes | `vX[.Y[.Z]]` with an optional trailing letter — `v98`, `v98.5`, `v100.0.9`, `v100.0.9a`. The letter slots an unplanned pass between numbered ones. Drives `version`, `tags`, and the slug prefix. |
| Header `YYYY-MM-DD` | yes | **Decorative.** The post date comes from the message's server timestamp, not this. |
| `Title:` | yes | 4–8 words. Blog-suitable post title, independent of the commit subject. |
| `Summary:` | yes | One sentence, **20–300 chars**. Becomes `description`. Watch multibyte chars near the cap. |
| `Project:` | no | An enrolled project name (resolved via the registry). Omitted → the config fallback module. |
| `Highlights:` | yes | 3–5 `·` bullets. Rendered into the post as literal text. |
| `Learnings:` | no | `·` bullets. Captured but not rendered live (kept as a hidden commentary block). |
| `Commit:` | yes | Merge SHA, 7 hex chars (`[0-9a-f]{7}`). Drives `commit` + the idempotency key. |
| `Tests:` | no | Observability only — logged to the debug channel, not written to the post. |
| `Branch:` | no | Observability only. |

> **A too-short `Summary:` passes parsing but fails validation.** The parser accepts it; the schema
> then refuses it (20-char minimum on `description`). You'll see a validation refusal, not a parse
> error. Write a real sentence.

---

## How fields map to the post

- **Version** → the post `version`, the seed for `tags`, and the slug prefix
  (`v98.5` + "blog bumper launch" → `v98-5-blog-bumper-launch`; a trailing letter rides along:
  `v100.0.9a` → `v100-0-9a-...`).
- **Date/time** → from the **Discord message's server timestamp**, converted to the configured
  timezone — NOT from the header date. The header date is decorative; the server timestamp is
  canonical (it can't be hallucinated, an agent-written date can drift). Both `date` and `time`
  derive from the same instant, so they always agree on the day.
- **Title** → the post `title`.
- **Summary** → the post `description`.
- **Project** → the post `module` (resolved through the registry; falls back to config).
- **Highlights** → rendered into the post body as literal bullets.
- **Learnings** → kept as a hidden commentary block (not shown on the page by default).
- **Commit** → the post `commit` and the idempotency key (a repeat of the same commit is skipped).
- **Tests / Branch** → never reach the post; logged to the debug channel for run observability.

---

## Setting up your agent to post this

The integration is **loose**: your agent posts this report as the final step of its workflow, and
that's all. Do **not** wire a `bumper` invocation into the agent — `bumper` runs separately and
reads the channel.

Add an instruction to your agent's end-of-task routine along these lines:

```
As the final step of every completed task, after the work is merged, post a report to
the changelog channel (and ONLY that channel) in exactly this format:

── PASS COMPLETE · v{version} · {date} ──────────────────────

Title: {4–8 word blog-suitable title}
Summary: {one sentence, 20–300 chars}
Project: {enrolled project name, or omit for the default module}

Highlights:
· {3–5 concrete behavioral changes — what changed, not file names}

Learnings:
· {optional insights}

Commit: {7-char merge SHA}
Tests: {N} passed · {M} failed · {K} skipped
Branch: clean

Rules:
- post AFTER the merge (the Commit field needs the merge SHA)
- version = the pass identifier; an optional trailing letter marks an unplanned pass (v98.5a)
- title = blog-suitable, NOT the commit subject
- summary = one readable sentence; this is the post's description (20–300 chars)
- highlights = behavior, not implementation
- do NOT put bare #channel-name references in the text
- post nothing else to this channel
```

> **Sequencing tip:** prove the full `bumper` pipeline end-to-end (with a hand-posted report and
> `--dry`) **before** turning on the agent's automatic posting — so you're not generating real
> reports into a channel whose downstream you haven't tested.

---

## Versioning the format

The header signature lets the format evolve without breaking old parsing. A future variant would
use a distinct header (e.g. `── PASS COMPLETE v2 ·`) and `bumper` would dispatch to a matching
parser. For now there's one format — the one above. If you customize it, keep the header
recognizable and update the parser to match. (The parser's header regex and the frontmatter
schema's version pattern both carry the optional `[a-z]` suffix — if you change the version shape,
change both.)

---

## Frontmatter / Field-mapping, four color fields:

The post may also carry up to four optional color fields, all `#rrggbb` hex, which override the
site's theme colors for that post's surfaces:

| Frontmatter field | Colors | Source |
|---|---|---|
| `colorPrimary` | the version label | per-module override → config `post_color_primary` → omit |
| `colorForeground` | the title | → `post_color_foreground` → omit |
| `colorMuted` | module label, date, description | → `post_color_muted` → omit |
| `colorAccent` | the accent border | → `post_color_accent` → omit |

These are **not** set in the report — they're resolved at bump time from config/registry and
written into frontmatter automatically. The agent's report never contains colors. When all are
unset, the post carries no color fields and renders with the site's theme tokens (the default).

---

**Back to:** [README](../README.md) · [INSTALL](INSTALL.md) · [OPERATION](OPERATION.md) ·
[CONFIG](CONFIG.md) · [ARCHITECTURE](ARCHITECTURE.md)
