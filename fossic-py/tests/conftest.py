"""Shared fixtures and helpers for fossic Python binding tests."""

from __future__ import annotations

import tempfile
from pathlib import Path
from typing import Generator

import pytest

from fossic import Append, Store


@pytest.fixture
def tmp_store() -> Generator[Store, None, None]:
    """An open Store in a fresh temporary directory."""
    with tempfile.TemporaryDirectory() as d:
        store = Store.open(str(Path(d) / "test.db"))
        yield store


@pytest.fixture
def declared_store(tmp_store: Store) -> Store:
    """A store with a pre-declared ``test/s`` stream."""
    tmp_store.declare_stream("test/s", "test")
    return tmp_store


_counter = 0


def unique_ev(store: Store, stream_id: str, branch: str = "main", **payload: object) -> None:
    """
    Append an event with a unique payload so CCE dedup never conflates events.

    Identical payloads at the same causal position de-duplicate by design.
    Always vary the payload when you need *n* distinct events.
    """
    global _counter
    _counter += 1
    merged = {"_seq": _counter, **payload}
    store.append(
        Append(
            stream_id=stream_id,
            event_type="TestEvent",
            payload=merged,
            branch=branch,
        )
    )
