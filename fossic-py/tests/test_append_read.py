"""Tests for append / read_range / read_one / external_id / CCE dedup."""

from __future__ import annotations

import pytest

from fossic import Append, EventId, ReadQuery, Store, StreamNotDeclaredError
from conftest import unique_ev


def test_append_returns_event_id(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"x": 1})
    )
    assert isinstance(eid, EventId)
    assert len(eid.as_bytes()) == 32


def test_event_id_hex_round_trip(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"n": 1})
    )
    assert EventId.from_hex(eid.hex()) == eid


def test_read_range_returns_appended(declared_store: Store) -> None:
    unique_ev(declared_store, "test/s", a=1)
    unique_ev(declared_store, "test/s", b=2)
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert len(events) == 2


def test_read_range_from_version(declared_store: Store) -> None:
    for i in range(4):
        unique_ev(declared_store, "test/s", i=i)
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    v2 = events[1].version
    later = declared_store.read_range(
        ReadQuery(stream_id="test/s", from_version=v2)
    )
    assert all(e.version >= v2 for e in later)
    assert len(later) == 3


def test_read_range_limit(declared_store: Store) -> None:
    for i in range(5):
        unique_ev(declared_store, "test/s", i=i)
    events = declared_store.read_range(ReadQuery(stream_id="test/s", limit=2))
    assert len(events) == 2


def test_read_one_by_id(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Foo", payload={"k": "v"})
    )
    ev = declared_store.read_one(eid)
    assert ev is not None
    assert ev.id == eid
    assert ev.payload()["k"] == "v"


def test_read_one_missing_returns_none(declared_store: Store) -> None:
    fake_id = EventId.from_hex("a" * 64)
    assert declared_store.read_one(fake_id) is None


def test_external_id_round_trip(declared_store: Store) -> None:
    declared_store.append(
        Append(
            stream_id="test/s",
            event_type="Ex",
            payload={},
            external_id="ext-abc-123",
        )
    )
    ev = declared_store.read_by_external_id("test/s", "ext-abc-123")
    assert ev is not None
    assert ev.external_id == "ext-abc-123"


def test_cce_dedup_returns_same_id(declared_store: Store) -> None:
    a = Append(stream_id="test/s", event_type="Dup", payload={"same": True})
    id1 = declared_store.append(a)
    id2 = declared_store.append(a)
    assert id1 == id2
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert len(events) == 1


def test_append_batch(declared_store: Store) -> None:
    ids = declared_store.append_batch(
        [
            Append(stream_id="test/s", event_type="A", payload={"n": 1}),
            Append(stream_id="test/s", event_type="B", payload={"n": 2}),
        ]
    )
    assert len(ids) == 2
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert len(events) == 2


def test_event_payload_dict(declared_store: Store) -> None:
    declared_store.append(
        Append(
            stream_id="test/s",
            event_type="Data",
            payload={"nested": {"a": [1, 2, 3]}},
        )
    )
    ev = declared_store.read_range(ReadQuery(stream_id="test/s"))[0]
    assert ev.payload() == {"nested": {"a": [1, 2, 3]}}


def test_append_to_undeclared_raises(tmp_store: Store) -> None:
    with pytest.raises(StreamNotDeclaredError):
        tmp_store.append(
            Append(stream_id="no/such/stream", event_type="X", payload={})
        )
