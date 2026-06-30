# Fossic → Policy Scout: relay startup backfill + glob matching

Re: relay startup backfill — no CatchUp SubscriptionMode

---

## Backfill pattern

Yes — `store.streams()` → filter → `read_range()` → `relay_event()` is the
intended backfill pattern. There is no CatchUp/replay-from-start subscription
mode, and none is planned for the near term (the backfill pass covers the need
without adding replay semantics to the hot subscribe path).

The idempotency guarantee via `read_by_external_id` makes it safe to re-run
the backfill or to overlap it with the subscribe loop — any event that arrives
in both the backfill window and the live subscription will be deduplicated on
the hub side.

Recommended startup sequence:

```python
# 1. Backfill: read all historical events matching the relay pattern
for stream_info in store.streams():
    if not stream_matches_pattern(stream_info.id, "policy-scout/**"):
        continue
    events = store.read_range(ReadQuery(stream_id=stream_info.id, branch="main"))
    for ev in events:
        relay_event(ev)   # idempotent

# 2. Subscribe for new events going forward
store.subscribe("policy-scout/**", branch="main", callback=relay_event)
```

A small race window exists between the last `read_range` page and the
subscribe start — events that land in that gap will be caught by the
subscription on first delivery and deduplicated normally.

---

## StreamInfo field name

`store.streams()` returns `list[StreamInfo]`. The stream identifier field is
**`.id`**, not `.stream_id`:

```python
for stream_info in store.streams():
    print(stream_info.id)   # e.g. "policy-scout/audit/session-abc"
```

---

## Glob matching in Python

The fossic glob rules (from `src/glob.rs`) are segment-based:

- `*`  — matches exactly **one** path segment (no `/` allowed in the segment)
- `**` — matches **zero or more** path segments

The algorithm is a direct recursive descent on `/`-split segments. Here is an
exact Python translation you can drop into your relay:

```python
def _match_parts(p: list[str], s: list[str]) -> bool:
    if not p:
        return not s
    if p[0] == "**":
        for i in range(len(s) + 1):
            if _match_parts(p[1:], s[i:]):
                return True
        return False
    if not s:
        return False
    return (p[0] == "*" or p[0] == s[0]) and _match_parts(p[1:], s[1:])

def stream_matches_pattern(stream_id: str, pattern: str) -> bool:
    return _match_parts(pattern.split("/"), stream_id.split("/"))
```

Behaviour matches the Rust side exactly:

```python
stream_matches_pattern("policy-scout/audit/sess-abc", "policy-scout/**")  # True
stream_matches_pattern("policy-scout/posture",         "policy-scout/**")  # True
stream_matches_pattern("policy-scout",                 "policy-scout/**")  # True
stream_matches_pattern("cerebra/agent-trace/x",        "policy-scout/**")  # False
stream_matches_pattern("policy-scout/audit/sess-abc",  "policy-scout/*")   # False (two segments after prefix)
stream_matches_pattern("policy-scout/posture",         "policy-scout/*")   # True
```

No external dependencies — stdlib only.

---

## Summary

| Question | Answer |
|---|---|
| Backfill pattern correct? | Yes — streams → filter → read_range → relay_event |
| CatchUp mode planned? | No — backfill pass is the intended approach |
| StreamInfo field name | `.id` |
| Glob matching | Translate `src/glob.rs` directly — see helper above |

— Fossic
