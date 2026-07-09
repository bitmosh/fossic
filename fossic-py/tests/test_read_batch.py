# SPDX-License-Identifier: Apache-2.0
"""Tests for Store.read_batch() — fetch multiple events by CCE ID."""

from __future__ import annotations

from fossic import Append, EventId, Store


def append_n(store: Store, stream: str, n: int) -> list[EventId]:
    """Append n events to stream, return their IDs."""
    ids = []
    for i in range(n):
        eid = store.append(Append(stream_id=stream, event_type="Ev", payload={"i": i}))
        ids.append(eid)
    return ids


# ── Basic fetch ───────────────────────────────────────────────────────────────


def test_read_batch_empty_input(declared_store: Store) -> None:
    result = declared_store.read_batch([])
    assert result == []


def test_read_batch_single_id(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"x": 1})
    )
    result = declared_store.read_batch([eid])
    assert len(result) == 1
    assert result[0].id == eid


def test_read_batch_multiple_ids(declared_store: Store) -> None:
    ids = append_n(declared_store, "test/s", 3)
    result = declared_store.read_batch(ids)
    assert len(result) == 3
    returned_ids = {e.id for e in result}
    assert returned_ids == set(ids)


def test_read_batch_timestamp_order(declared_store: Store) -> None:
    """Results come back timestamp ASC regardless of input order."""
    ids = append_n(declared_store, "test/s", 3)
    result = declared_store.read_batch(list(reversed(ids)))
    assert [e.id for e in result] == ids


# ── Missing IDs ───────────────────────────────────────────────────────────────


def test_read_batch_missing_id_silently_omitted(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={})
    )
    absent = EventId.from_hex("ff" * 32)
    result = declared_store.read_batch([eid, absent])
    assert len(result) == 1
    assert result[0].id == eid


def test_read_batch_all_missing_returns_empty(declared_store: Store) -> None:
    absent = EventId.from_hex("aa" * 32)
    result = declared_store.read_batch([absent])
    assert result == []


# ── Cross-stream ──────────────────────────────────────────────────────────────


def test_read_batch_across_streams(tmp_store: Store) -> None:
    tmp_store.declare_stream("a", "test")
    tmp_store.declare_stream("b", "test")
    id_a = tmp_store.append(Append(stream_id="a", event_type="Ev", payload={"s": "a"}))
    id_b = tmp_store.append(Append(stream_id="b", event_type="Ev", payload={"s": "b"}))
    result = tmp_store.read_batch([id_a, id_b])
    assert len(result) == 2
    stream_ids = {e.stream_id for e in result}
    assert stream_ids == {"a", "b"}


# ── Deduplication ─────────────────────────────────────────────────────────────


def test_read_batch_duplicate_ids_returns_one(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={})
    )
    result = declared_store.read_batch([eid, eid])
    assert len(result) == 1


# ── Payload integrity ─────────────────────────────────────────────────────────


def test_read_batch_payload_intact(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(
            stream_id="test/s",
            event_type="Rich",
            payload={"x": 42, "label": "hello"},
        )
    )
    result = declared_store.read_batch([eid])
    assert result[0].payload() == {"x": 42, "label": "hello"}


def test_read_batch_event_metadata_intact(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(
            stream_id="test/s",
            event_type="Typed",
            payload={"k": "v"},
            type_version=3,
        )
    )
    result = declared_store.read_batch([eid])
    ev = result[0]
    assert ev.id == eid
    assert ev.stream_id == "test/s"
    assert ev.event_type == "Typed"
    assert ev.type_version == 3
