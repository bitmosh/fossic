# SPDX-License-Identifier: Apache-2.0
"""Tests for cross-stream aggregate queries and multi-stream reads."""

from __future__ import annotations

import pytest

from fossic import AggregateQuery, Append, ReadQuery, Store
from conftest import unique_ev


def _streams(store: Store) -> None:
    store.declare_stream("cerebra/lattice/alpha", "t")
    store.declare_stream("cerebra/lattice/beta", "t")
    store.declare_stream("cerebra/lattice/gamma", "t")


def test_aggregate_exact_pattern(tmp_store: Store) -> None:
    _streams(tmp_store)
    unique_ev(tmp_store, "cerebra/lattice/alpha")
    unique_ev(tmp_store, "cerebra/lattice/beta")
    events = tmp_store.aggregate(
        AggregateQuery(stream_pattern="cerebra/lattice/alpha")
    )
    assert len(events) == 1
    assert events[0].stream_id == "cerebra/lattice/alpha"


def test_aggregate_wildcard_pattern(tmp_store: Store) -> None:
    _streams(tmp_store)
    for s in ("cerebra/lattice/alpha", "cerebra/lattice/beta", "cerebra/lattice/gamma"):
        unique_ev(tmp_store, s, n=1)
    events = tmp_store.aggregate(
        AggregateQuery(stream_pattern="cerebra/lattice/*")
    )
    assert len(events) == 3


def test_aggregate_double_star_pattern(tmp_store: Store) -> None:
    _streams(tmp_store)
    unique_ev(tmp_store, "cerebra/lattice/alpha")
    events = tmp_store.aggregate(AggregateQuery(stream_pattern="cerebra/**"))
    assert len(events) >= 1


def test_aggregate_event_type_filter(tmp_store: Store) -> None:
    _streams(tmp_store)
    tmp_store.append(
        Append(
            stream_id="cerebra/lattice/alpha",
            event_type="TypeA",
            payload={"n": 1},
        )
    )
    tmp_store.append(
        Append(
            stream_id="cerebra/lattice/beta",
            event_type="TypeB",
            payload={"n": 2},
        )
    )
    events = tmp_store.aggregate(
        AggregateQuery(
            stream_pattern="cerebra/lattice/*",
            event_type_filter="TypeA",
        )
    )
    assert all(e.event_type == "TypeA" for e in events)
    assert len(events) == 1


def test_aggregate_empty_pattern_no_results(tmp_store: Store) -> None:
    events = tmp_store.aggregate(
        AggregateQuery(stream_pattern="no/match/**")
    )
    assert events == []


def test_read_range_event_type_filter(declared_store: Store) -> None:
    declared_store.append(
        Append(stream_id="test/s", event_type="Alpha", payload={"n": 1})
    )
    declared_store.append(
        Append(stream_id="test/s", event_type="Beta", payload={"n": 2})
    )
    events = declared_store.read_range(
        ReadQuery(stream_id="test/s", event_type_filter="Alpha")
    )
    assert len(events) == 1
    assert events[0].event_type == "Alpha"
