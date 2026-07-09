# SPDX-License-Identifier: Apache-2.0
"""Bounded read surface: ReadOutcome, TruncationCursor, SamplingMode, pagination."""
from __future__ import annotations

import pytest

from fossic import (
    Append,
    FossicError,
    ReadOutcome,
    ReadQuery,
    SamplingMode,
    Store,
    TruncationCursor,
)


def append_n(store: Store, n: int, stream: str = "s") -> None:
    store.declare_stream(stream, "main")
    for i in range(n):
        store.append(Append(stream_id=stream, event_type="Evt", payload={"i": i}))


def append_correlated(store: Store, n: int) -> object:
    store.declare_stream("corr", "main")
    root = store.append(Append(stream_id="corr", event_type="Root", payload={}))
    for i in range(n):
        store.append(
            Append(
                stream_id="corr",
                event_type="Child",
                payload={"i": i},
                correlation_id=root,
            )
        )
    return root


# ── TruncationCursor round-trip ───────────────────────────────────────────────


def test_truncation_cursor_bytes_round_trip(tmp_store: Store) -> None:
    append_n(tmp_store, 5)
    q = ReadQuery(stream_id="s")
    outcome = tmp_store.read_range_bounded(q, max_results=2)
    assert outcome.is_truncated
    cursor = outcome.next_cursor
    assert cursor is not None
    raw = cursor.to_bytes()
    reconstructed = TruncationCursor.from_bytes(raw)
    assert reconstructed.to_bytes() == raw


def test_truncation_cursor_empty_bytes() -> None:
    c = TruncationCursor.from_bytes(b"")
    assert c.to_bytes() == b""


# ── SamplingMode constructors ─────────────────────────────────────────────────


def test_sampling_mode_exhaustive() -> None:
    m = SamplingMode.exhaustive()
    assert "exhaustive" in repr(m).lower()


def test_sampling_mode_breadth_first_carries_limit() -> None:
    m = SamplingMode.breadth_first(max_per_level=10)
    assert "10" in repr(m)


def test_sampling_mode_adaptive_carries_target() -> None:
    m = SamplingMode.adaptive(target_count=250)
    assert "250" in repr(m)


# ── ReadOutcome shape ─────────────────────────────────────────────────────────


def test_read_outcome_complete_properties(tmp_store: Store) -> None:
    append_n(tmp_store, 3)
    outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"))
    assert outcome.complete is True
    assert outcome.is_truncated is False
    assert outcome.reason is None
    assert outcome.next_cursor is None
    assert len(outcome.results) == 3


def test_read_outcome_truncated_properties(tmp_store: Store) -> None:
    append_n(tmp_store, 5)
    outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"), max_results=2)
    assert outcome.is_truncated is True
    assert outcome.complete is False
    assert outcome.reason == "result_count"
    assert len(outcome.results) == 2


# ── read_range_bounded ────────────────────────────────────────────────────────


def test_range_bounded_no_budget_returns_complete(tmp_store: Store) -> None:
    append_n(tmp_store, 5)
    outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"))
    assert outcome.complete
    assert len(outcome.results) == 5


def test_range_bounded_truncates_at_result_count(tmp_store: Store) -> None:
    append_n(tmp_store, 10)
    outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"), max_results=3)
    assert outcome.is_truncated
    assert len(outcome.results) == 3
    assert outcome.reason == "result_count"


def test_range_bounded_complete_when_exactly_at_limit(tmp_store: Store) -> None:
    append_n(tmp_store, 5)
    outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"), max_results=5)
    assert outcome.complete
    assert len(outcome.results) == 5


def test_range_bounded_truncates_at_byte_budget(tmp_store: Store) -> None:
    append_n(tmp_store, 10)
    # Budget of 1 byte: at-least-one guarantee ensures first event is included,
    # then byte budget fires.
    outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"), max_bytes=1)
    assert outcome.is_truncated
    assert len(outcome.results) == 1
    assert outcome.reason == "byte_size"


def test_range_bounded_resume_continues_from_cursor(tmp_store: Store) -> None:
    append_n(tmp_store, 6)
    q = ReadQuery(stream_id="s")
    page1 = tmp_store.read_range_bounded(q, max_results=3)
    assert page1.is_truncated
    assert [e.version for e in page1.results] == [0, 1, 2]

    page2 = tmp_store.read_range_bounded(q, max_results=3, cursor=page1.next_cursor)
    assert [e.version for e in page2.results] == [3, 4, 5]


def test_range_bounded_resume_full_pagination(tmp_store: Store) -> None:
    append_n(tmp_store, 7)
    q = ReadQuery(stream_id="s")
    all_versions: list[int] = []
    cursor = None
    while True:
        outcome = tmp_store.read_range_bounded(q, max_results=3, cursor=cursor)
        all_versions.extend(e.version for e in outcome.results)
        if outcome.complete:
            break
        cursor = outcome.next_cursor
    assert all_versions == list(range(7))


@pytest.mark.skip(reason="OpenOptions.default_max_results not yet exposed to Python")
def test_range_bounded_uses_store_default_max_results() -> None:
    pass


@pytest.mark.skip(reason="OpenOptions.default_max_results not yet exposed to Python")
def test_range_bounded_per_call_overrides_store_default() -> None:
    pass


# ── read_by_correlation_bounded ───────────────────────────────────────────────


def test_correlation_bounded_no_budget_returns_complete(tmp_store: Store) -> None:
    root = append_correlated(tmp_store, 4)
    outcome = tmp_store.read_by_correlation_bounded(root)
    assert outcome.complete
    assert len(outcome.results) == 4


def test_correlation_bounded_truncates_at_result_count(tmp_store: Store) -> None:
    root = append_correlated(tmp_store, 6)
    outcome = tmp_store.read_by_correlation_bounded(root, max_results=3)
    assert outcome.is_truncated
    assert len(outcome.results) == 3
    assert outcome.reason == "result_count"


def test_correlation_bounded_resume_continues_from_cursor(tmp_store: Store) -> None:
    root = append_correlated(tmp_store, 6)
    all_ids: list[object] = []
    cursor = None
    while True:
        outcome = tmp_store.read_by_correlation_bounded(root, max_results=3, cursor=cursor)
        all_ids.extend(e.id for e in outcome.results)
        if outcome.complete:
            break
        cursor = outcome.next_cursor
    assert len(all_ids) == 6
    # ids must be monotonically increasing (BLOB lexicographic order)
    assert all_ids == sorted(all_ids, key=lambda e: e.as_bytes())  # type: ignore[attr-defined]


def test_correlation_bounded_no_events_returns_complete_empty(tmp_store: Store) -> None:
    tmp_store.declare_stream("lone", "main")
    lone = tmp_store.append(Append(stream_id="lone", event_type="Lone", payload={}))
    outcome = tmp_store.read_by_correlation_bounded(lone, max_results=10)
    assert outcome.complete
    assert len(outcome.results) == 0


def test_correlation_bounded_wrong_cursor_type_returns_error(tmp_store: Store) -> None:
    append_n(tmp_store, 3)
    root = append_correlated(tmp_store, 3)
    # Acquire a Range cursor by truncating a range read.
    range_outcome = tmp_store.read_range_bounded(ReadQuery(stream_id="s"), max_results=1)
    assert range_outcome.is_truncated
    range_cursor = range_outcome.next_cursor
    # Passing a Range cursor to a correlation query must raise.
    with pytest.raises(FossicError):
        tmp_store.read_by_correlation_bounded(root, max_results=10, cursor=range_cursor)
