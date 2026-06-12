"""Tests for PostCommit subscriptions and the Python worker thread."""

from __future__ import annotations

import threading

import pytest

from fossic import Store, SubscriptionMode
from conftest import unique_ev


def test_subscribe_context_manager(declared_store: Store) -> None:
    with declared_store.subscribe("test/s") as sub:
        unique_ev(declared_store, "test/s", v=1)
        ev = next(sub)
        assert ev.event_type == "TestEvent"


def test_subscribe_receives_multiple_events(declared_store: Store) -> None:
    received = []
    with declared_store.subscribe("test/s") as sub:
        for i in range(3):
            unique_ev(declared_store, "test/s", i=i)
        for _ in range(3):
            received.append(next(sub))
    assert len(received) == 3


def test_subscribe_post_commit_mode(declared_store: Store) -> None:
    with declared_store.subscribe(
        "test/s", mode=SubscriptionMode.post_commit()
    ) as sub:
        unique_ev(declared_store, "test/s", x=42)
        ev = next(sub)
        assert ev.payload()["x"] == 42


def test_subscribe_worker_thread_callback(declared_store: Store) -> None:
    received: list[object] = []
    done = threading.Event()

    def on_event(ev: object) -> None:
        received.append(ev)
        if len(received) >= 2:
            done.set()

    with declared_store.subscribe(
        "test/s",
        mode=SubscriptionMode.post_commit(),
        callback=on_event,
    ):
        unique_ev(declared_store, "test/s", a=1)
        unique_ev(declared_store, "test/s", a=2)
        assert done.wait(timeout=5.0), "worker thread did not deliver both events"

    assert len(received) == 2


def test_unsubscribe_idempotent(declared_store: Store) -> None:
    with declared_store.subscribe("test/s") as sub:
        pass
    sub.unsubscribe()  # second call — should not raise


def test_is_degraded_false_initially(declared_store: Store) -> None:
    with declared_store.subscribe("test/s") as sub:
        assert sub.is_degraded is False


def test_subscribe_stop_iteration_after_unsubscribe(declared_store: Store) -> None:
    sub = declared_store.subscribe("test/s")
    sub.unsubscribe()
    with pytest.raises(StopIteration):
        next(sub)
