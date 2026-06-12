"""Tests for payload transforms (fires at append time, not read time).

The transform callable signature is ``(event_type: str, payload: dict) -> dict``.
Transforms must be registered BEFORE appending — they fire during append, modifying
the stored payload before CCE encoding. A transform registered after an append has
no effect on already-stored events.
"""

from __future__ import annotations

from typing import Any

from fossic import Append, ReadQuery, Store


def test_transform_decorates_payload(declared_store: Store) -> None:
    def add_flag(event_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        return {**payload, "_tagged": True}

    declared_store.register_payload_transform("test/s", add_flag)
    declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"raw": 1})
    )
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert events[0].payload()["_tagged"] is True


def test_transform_callable_receives_event_type(tmp_store: Store) -> None:
    # The first argument to the callable is event_type (not stream_id).
    tmp_store.declare_stream("proj/events", "t")

    received: list[str] = []

    def capture(event_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        received.append(event_type)
        return payload

    tmp_store.register_payload_transform("proj/events", capture)
    tmp_store.append(
        Append(stream_id="proj/events", event_type="Ev", payload={"v": 1})
    )
    assert received == ["Ev"]


def test_transform_not_applied_to_other_streams(tmp_store: Store) -> None:
    tmp_store.declare_stream("stream/a", "t")
    tmp_store.declare_stream("stream/b", "t")

    def tag(_event_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        return {**payload, "tagged": True}

    tmp_store.register_payload_transform("stream/a", tag)
    tmp_store.append(
        Append(stream_id="stream/a", event_type="Ev", payload={"k": "a"})
    )
    tmp_store.append(
        Append(stream_id="stream/b", event_type="Ev", payload={"k": "b"})
    )
    events_a = tmp_store.read_range(ReadQuery(stream_id="stream/a"))
    events_b = tmp_store.read_range(ReadQuery(stream_id="stream/b"))
    assert events_a[0].payload().get("tagged") is True
    assert "tagged" not in events_b[0].payload()


def test_transform_wildcard_pattern(tmp_store: Store) -> None:
    tmp_store.declare_stream("svc/alpha", "t")
    tmp_store.declare_stream("svc/beta", "t")

    def stamp(_event_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        return {**payload, "transformed": True}

    tmp_store.register_payload_transform("svc/*", stamp)
    # Use distinct payloads — identical payloads across streams share the same CCE hash
    # and the second append would be silently deduped, leaving one stream empty.
    for i, s in enumerate(("svc/alpha", "svc/beta")):
        tmp_store.append(Append(stream_id=s, event_type="Ev", payload={"n": i}))

    for sid in ("svc/alpha", "svc/beta"):
        evs = tmp_store.read_range(ReadQuery(stream_id=sid))
        assert evs[0].payload()["transformed"] is True
