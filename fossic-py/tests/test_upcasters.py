"""Tests for upcaster chain registration and payload transformation on read."""

from __future__ import annotations

from typing import Any

import pytest

from fossic import Append, ReadQuery, Store, UpcasterChainGapError


def test_upcaster_transforms_payload(declared_store: Store) -> None:
    declared_store.append(
        Append(
            stream_id="test/s",
            event_type="UserCreated",
            payload={"name": "alice"},
            type_version=1,
        )
    )

    def v1_to_v2(payload: dict[str, Any]) -> dict[str, Any]:
        return {"full_name": payload["name"], "schema_version": 2}

    declared_store.register_upcaster("UserCreated", 1, 2, v1_to_v2)
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert events[0].payload()["full_name"] == "alice"


def test_upcaster_chain(declared_store: Store) -> None:
    declared_store.append(
        Append(
            stream_id="test/s",
            event_type="Item",
            payload={"v": 1},
            type_version=1,
        )
    )

    def v1_v2(p: dict[str, Any]) -> dict[str, Any]:
        p["step"] = "v1->v2"
        return p

    def v2_v3(p: dict[str, Any]) -> dict[str, Any]:
        p["step"] = "v2->v3"
        return p

    declared_store.register_upcaster("Item", 1, 2, v1_v2)
    declared_store.register_upcaster("Item", 2, 3, v2_v3)
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert events[0].payload()["step"] == "v2->v3"


def test_upcaster_not_applied_to_other_types(declared_store: Store) -> None:
    declared_store.append(
        Append(
            stream_id="test/s",
            event_type="OtherType",
            payload={"name": "bob"},
            type_version=1,
        )
    )

    def bad(_: dict[str, Any]) -> dict[str, Any]:
        raise AssertionError("should not be called")

    declared_store.register_upcaster("UserCreated", 1, 2, bad)
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    assert events[0].payload()["name"] == "bob"


def test_upcaster_chain_gap_raises(declared_store: Store) -> None:
    declared_store.append(
        Append(
            stream_id="test/s",
            event_type="Gapped",
            payload={"x": 1},
            type_version=1,
        )
    )
    # Register 1->2 and 3->4 but not 2->3 — expect a chain gap error
    declared_store.register_upcaster("Gapped", 1, 2, lambda p: p)
    declared_store.register_upcaster("Gapped", 3, 4, lambda p: p)
    with pytest.raises(UpcasterChainGapError):
        declared_store.read_range(ReadQuery(stream_id="test/s"))
