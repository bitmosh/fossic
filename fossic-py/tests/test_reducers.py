"""Tests for Python-layer reducer registration and state folding."""

from __future__ import annotations

from typing import Any

import pytest

from fossic import Append, ReadQuery, ReducerNotFoundError, Store
from conftest import unique_ev


class _CountReducer:
    name = "counter"
    version = 1
    state_schema_version = 1

    def initial_state(self) -> dict[str, Any]:
        return {"count": 0}

    def apply(self, state: dict[str, Any], _payload: dict[str, Any]) -> dict[str, Any]:
        return {"count": state["count"] + 1}


class _SumReducer:
    name = "summer"
    version = 1
    state_schema_version = 1

    def initial_state(self) -> dict[str, Any]:
        return {"total": 0}

    def apply(self, state: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
        return {"total": state["total"] + payload.get("value", 0)}


def test_register_reducer_and_read_state(declared_store: Store) -> None:
    declared_store.register_reducer("test/s", _CountReducer())
    unique_ev(declared_store, "test/s")
    unique_ev(declared_store, "test/s")
    state = declared_store.read_state("test/s")
    assert state["count"] == 2


def test_read_state_no_events(declared_store: Store) -> None:
    declared_store.register_reducer("test/s", _CountReducer())
    state = declared_store.read_state("test/s")
    assert state["count"] == 0


def test_read_state_at_version(declared_store: Store) -> None:
    declared_store.register_reducer("test/s", _CountReducer())
    unique_ev(declared_store, "test/s")
    unique_ev(declared_store, "test/s")
    unique_ev(declared_store, "test/s")
    all_events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    v_mid = all_events[1].version
    state = declared_store.read_state_at_version("test/s", "main", v_mid)
    assert state["count"] == 2


def test_wildcard_reducer_pattern(tmp_store: Store) -> None:
    tmp_store.declare_stream("cerebra/lattice/a", "t")
    tmp_store.declare_stream("cerebra/lattice/b", "t")
    tmp_store.register_reducer("cerebra/lattice/*", _CountReducer())
    unique_ev(tmp_store, "cerebra/lattice/a")
    unique_ev(tmp_store, "cerebra/lattice/b")
    assert tmp_store.read_state("cerebra/lattice/a")["count"] == 1
    assert tmp_store.read_state("cerebra/lattice/b")["count"] == 1


def test_double_star_reducer_pattern(tmp_store: Store) -> None:
    tmp_store.declare_stream("a/b/c/d", "t")
    tmp_store.register_reducer("**", _CountReducer())
    unique_ev(tmp_store, "a/b/c/d")
    assert tmp_store.read_state("a/b/c/d")["count"] == 1


def test_most_specific_pattern_wins(tmp_store: Store) -> None:
    tmp_store.declare_stream("cerebra/lattice/special", "t")
    tmp_store.register_reducer("**", _CountReducer())
    tmp_store.register_reducer("cerebra/lattice/special", _SumReducer())
    unique_ev(tmp_store, "cerebra/lattice/special", value=10)
    state = tmp_store.read_state("cerebra/lattice/special")
    assert "total" in state, "specific reducer should win over **"
    assert state["total"] == 10


def test_no_reducer_raises(declared_store: Store) -> None:
    with pytest.raises(ReducerNotFoundError):
        declared_store.read_state("test/s")


def test_sum_reducer(declared_store: Store) -> None:
    declared_store.register_reducer("test/s", _SumReducer())
    declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"value": 5})
    )
    declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"value": 7})
    )
    state = declared_store.read_state("test/s")
    assert state["total"] == 12
