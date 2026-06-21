"""Python-side integration tests for fossic.similarity.HnswProvider.

Parity coverage against crates/fossic-similarity-hnsw/tests/integration.rs.
All tests use tmp_path (pytest built-in) for isolation.
"""

from __future__ import annotations

import time

import pytest

from fossic.similarity import HnswProvider, SimilarityQuery


# ── Helpers ───────────────────────────────────────────────────────────────────

def make_event_id(n: int) -> bytes:
    b = bytearray(32)
    b[0] = n & 0xFF
    b[1] = (n >> 8) & 0xFF
    return bytes(b)


def make_provider(tmp_path, dims: int = 4, **kwargs) -> HnswProvider:
    return HnswProvider(str(tmp_path / "store.db"), dimensions=dims, **kwargs)


# ── Basic operations ──────────────────────────────────────────────────────────

def test_empty_index_query_returns_empty(tmp_path):
    p = make_provider(tmp_path)
    results = p.query({"embedding": [1.0, 0.0, 0.0, 0.0], "k": 5})
    assert results == []


def test_index_and_query_roundtrip(tmp_path):
    p = make_provider(tmp_path)
    eid = make_event_id(1)
    p.index(eid, [1.0, 0.0, 0.0, 0.0])
    results = p.query({"embedding": [1.0, 0.0, 0.0, 0.0], "k": 1})
    assert len(results) == 1
    assert results[0]["event_id"] == eid
    assert isinstance(results[0]["score"], float)


def test_index_wrong_dims_raises(tmp_path):
    p = make_provider(tmp_path, dims=4)
    with pytest.raises(Exception):
        p.index(make_event_id(1), [1.0, 0.0])


def test_query_wrong_dims_raises(tmp_path):
    p = make_provider(tmp_path, dims=4)
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    with pytest.raises(Exception):
        p.query({"embedding": [1.0, 0.0], "k": 1})


def test_zero_k_returns_empty(tmp_path):
    p = make_provider(tmp_path)
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    results = p.query({"embedding": [1.0, 0.0, 0.0, 0.0], "k": 0})
    assert results == []


def test_len_and_is_empty(tmp_path):
    p = make_provider(tmp_path)
    assert p.is_empty()
    assert p.len() == 0
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    assert not p.is_empty()
    assert p.len() == 1


def test_event_id_must_be_32_bytes(tmp_path):
    p = make_provider(tmp_path)
    with pytest.raises(Exception, match="32 bytes"):
        p.index(b"\x00" * 16, [1.0, 0.0, 0.0, 0.0])


def test_top_k_results_bounded(tmp_path):
    p = make_provider(tmp_path)
    for i in range(10):
        vec = [0.0] * 4
        vec[i % 4] = float(i + 1)
        p.index(make_event_id(i), vec)
    results = p.query({"embedding": [1.0, 0.0, 0.0, 0.0], "k": 3})
    assert len(results) <= 3


def test_remove_raises_not_supported(tmp_path):
    p = make_provider(tmp_path)
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    with pytest.raises(Exception):
        p.remove(make_event_id(1))


# ── Persistence ───────────────────────────────────────────────────────────────

def test_persistence_round_trip(tmp_path):
    db = str(tmp_path / "store.db")
    eid = make_event_id(42)
    embedding = [1.0, 0.0, 0.0, 0.0]

    p1 = HnswProvider(db, dimensions=4)
    p1.index(eid, embedding)
    assert p1.is_dirty()
    p1.save()
    assert not p1.is_dirty()

    p2 = HnswProvider(db, dimensions=4)
    results = p2.query({"embedding": embedding, "k": 1})
    assert len(results) == 1
    assert results[0]["event_id"] == eid


def test_save_empty_index(tmp_path):
    db = str(tmp_path / "store.db")
    p1 = HnswProvider(db, dimensions=4)
    p1.save()
    # Reload — should come back empty, not error
    p2 = HnswProvider(db, dimensions=4)
    assert p2.is_empty()


def test_dirty_flag_cleared_on_save(tmp_path):
    p = make_provider(tmp_path)
    assert not p.is_dirty()
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    assert p.is_dirty()
    p.save()
    assert not p.is_dirty()


# ── SimilarityQuery helper ────────────────────────────────────────────────────

def test_similarity_query_as_dict(tmp_path):
    p = make_provider(tmp_path)
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    sq = SimilarityQuery(embedding=[1.0, 0.0, 0.0, 0.0], k=1)
    results = p.query(sq.as_dict())
    assert len(results) == 1
    assert results[0]["event_id"] == make_event_id(1)


def test_similarity_query_with_stream_pattern_field():
    sq = SimilarityQuery(embedding=[1.0, 0.0], k=5, stream_pattern="events/*")
    d = sq.as_dict()
    assert d["stream_pattern"] == "events/*"
    assert d["k"] == 5


# ── Stream-pattern filtering ──────────────────────────────────────────────────

def test_index_with_stream_id_filters_correctly(tmp_path):
    p = make_provider(tmp_path)
    eid_a = make_event_id(1)
    eid_b = make_event_id(2)
    p.index_with_stream_id(eid_a, "events/a", [1.0, 0.0, 0.0, 0.0])
    p.index_with_stream_id(eid_b, "other/b", [1.0, 0.0, 0.0, 0.0])

    # Filter to events/* — should return eid_a, not eid_b
    results = p.query({"embedding": [1.0, 0.0, 0.0, 0.0], "k": 5, "stream_pattern": "events/*"})
    ids = [r["event_id"] for r in results]
    assert eid_a in ids
    assert eid_b not in ids


# ── Background save scheduling ────────────────────────────────────────────────

def test_schedule_save_fires_when_dirty(tmp_path):
    db = str(tmp_path / "store.db")
    # quiescence_window_ms=100 so the executor fires within ~700ms
    p = HnswProvider(db, dimensions=4, quiescence_window_ms=100)
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    assert p.is_dirty()

    p.schedule_save(priority="low")
    assert p.is_save_pending()

    # Wait for quiescence (100ms) + executor poll (500ms) + margin
    time.sleep(0.8)

    assert not p.is_dirty()
    hnsw_dir = tmp_path / "hnsw"
    assert (hnsw_dir / "index.hnsw.data").exists()
    assert (hnsw_dir / "index.hnsw.graph").exists()
    assert (hnsw_dir / "mappings.bin").exists()


def test_schedule_save_noop_when_not_dirty(tmp_path):
    db = str(tmp_path / "store.db")
    p = HnswProvider(db, dimensions=4, quiescence_window_ms=100)
    p.index(make_event_id(1), [1.0, 0.0, 0.0, 0.0])
    p.save()
    assert not p.is_dirty()

    p.schedule_save(priority="low")
    assert not p.is_save_pending()


def test_schedule_save_storm_prevention(tmp_path):
    db = str(tmp_path / "store.db")
    p = HnswProvider(db, dimensions=4, quiescence_window_ms=100)
    for i in range(100):
        p.index(make_event_id(i), [float(i % 4 == j) for j in range(4)])
        p.schedule_save()

    # Only one pending flag should be set
    assert p.is_save_pending()
    # And dirty should still be true (save hasn't fired yet)
    assert p.is_dirty()
    # Clean up by letting it drain
    time.sleep(0.8)
    assert not p.is_dirty()
