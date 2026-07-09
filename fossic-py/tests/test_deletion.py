# SPDX-License-Identifier: Apache-2.0
"""Tests for event purge and stream shredding."""

from __future__ import annotations

import pytest

from fossic import Append, EventId, EventNotFoundError, PurgeConfirmationError, ReadQuery, Store
from conftest import unique_ev


def test_purge_event_removes_payload(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Secret", payload={"ssn": "123"})
    )
    # Verify event is readable before purge.
    assert declared_store.read_one(eid) is not None

    declared_store.purge_event(
        eid,
        confirm="I understand this breaks replay-from-zero",
        reason="PII removal",
        purged_by="test",
    )
    # After purge, the event is removed from the read path entirely.
    # read_one returns None; the payload is no longer accessible.
    assert declared_store.read_one(eid) is None


def test_purge_wrong_confirm_raises(declared_store: Store) -> None:
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="Ev", payload={"k": 1})
    )
    with pytest.raises(PurgeConfirmationError):
        declared_store.purge_event(
            eid,
            confirm="wrong",
            reason="test",
            purged_by="test",
        )


def test_purge_nonexistent_event_raises(declared_store: Store) -> None:
    fake = EventId.from_hex("b" * 64)
    with pytest.raises(EventNotFoundError):
        declared_store.purge_event(
            fake,
            confirm="I understand this breaks replay-from-zero",
            reason="r",
            purged_by="t",
        )


@pytest.mark.skip(
    reason=(
        "DESIGN_GAP (Pass 8.6): shred_stream requires encryption mode (spec §9.2). "
        "Re-enable when crypto-shredding ships (OpenOptions::encryption = OsKeyring)."
    )
)
def test_shred_stream_clears_events(declared_store: Store) -> None:
    for _ in range(3):
        unique_ev(declared_store, "test/s")
    declared_store.shred_stream("test/s", reason="GDPR erasure test")
    events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    # All event payloads should be redacted after shredding.
    for ev in events:
        p = ev.payload()
        assert "_seq" not in p


def test_cursor_set_and_get(declared_store: Store) -> None:
    unique_ev(declared_store, "test/s")
    declared_store.set_cursor("consumer-1", "test/s", "main", 1)
    cur = declared_store.get_cursor("consumer-1", "test/s", "main")
    assert cur == 1


def test_cursor_missing_returns_none(declared_store: Store) -> None:
    cur = declared_store.get_cursor("no-consumer", "test/s", "main")
    assert cur is None
