"""Python-level verification that compute_event_id() matches stored event IDs.

PD-007: expose derive_event_id via PyO3.  These tests confirm the binding is
wired correctly by appending events and checking that the pre-computed ID is
byte-identical to the one the store assigned.
"""

from __future__ import annotations

from fossic import Append, EventId, Store, compute_event_id


# ── round-trip: compute matches stored ───────────────────────────────────────


def test_compute_matches_stored_simple(declared_store: Store) -> None:
    """compute_event_id produces the same ID the store assigns on append."""
    payload = {"action": "login", "user": "alice"}
    eid = declared_store.append(
        Append(stream_id="test/s", event_type="UserLoggedIn", payload=payload)
    )
    computed = compute_event_id("UserLoggedIn", payload)
    assert computed == eid, (
        f"pre-computed ID {computed.hex()} != stored ID {eid.hex()}"
    )


def test_compute_matches_stored_with_causation(declared_store: Store) -> None:
    """compute_event_id with causation_id matches the stored ID."""
    cause_id = declared_store.append(
        Append(stream_id="test/s", event_type="OrderPlaced", payload={"item": "x"})
    )
    payload = {"reason": "fraud"}
    eid = declared_store.append(
        Append(
            stream_id="test/s",
            event_type="OrderCancelled",
            payload=payload,
            causation_id=cause_id,
        )
    )
    computed = compute_event_id("OrderCancelled", payload, causation_id=cause_id)
    assert computed == eid


def test_compute_matches_stored_type_version_2(declared_store: Store) -> None:
    """compute_event_id respects a non-default type_version."""
    payload = {"v": 2}
    eid = declared_store.append(
        Append(
            stream_id="test/s",
            event_type="ThingHappened",
            payload=payload,
            type_version=2,
        )
    )
    computed = compute_event_id("ThingHappened", payload, type_version=2)
    assert computed == eid


# ── determinism ───────────────────────────────────────────────────────────────


def test_compute_is_deterministic() -> None:
    """Same inputs always produce the same EventId."""
    a = compute_event_id("Ping", {"seq": 42})
    b = compute_event_id("Ping", {"seq": 42})
    assert a == b


def test_compute_returns_event_id_instance() -> None:
    """Return value is an EventId with a 64-char hex representation."""
    eid = compute_event_id("Ping", {})
    assert isinstance(eid, EventId)
    assert len(eid.hex()) == 64
    assert len(eid.as_bytes()) == 32


# ── sensitivity ──────────────────────────────────────────────────────────────


def test_compute_sensitive_to_payload() -> None:
    """Different payloads produce different IDs."""
    a = compute_event_id("Ev", {"x": 1})
    b = compute_event_id("Ev", {"x": 2})
    assert a != b


def test_compute_sensitive_to_event_type() -> None:
    """Different event types produce different IDs."""
    a = compute_event_id("TypeA", {"x": 1})
    b = compute_event_id("TypeB", {"x": 1})
    assert a != b


def test_compute_sensitive_to_type_version() -> None:
    """Different type_version values produce different IDs."""
    a = compute_event_id("Ev", {"x": 1}, type_version=1)
    b = compute_event_id("Ev", {"x": 1}, type_version=2)
    assert a != b


def test_compute_sensitive_to_causation_id(declared_store: Store) -> None:
    """Presence or absence of causation_id changes the computed ID."""
    cause = declared_store.append(
        Append(stream_id="test/s", event_type="Root", payload={})
    )
    without = compute_event_id("Child", {"k": "v"})
    with_cause = compute_event_id("Child", {"k": "v"}, causation_id=cause)
    assert without != with_cause
