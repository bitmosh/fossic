"""Tests for relay primitives: _hub_stream_id, _translate_causation_id,
relay_append, RelayAgent.relay_event."""

from __future__ import annotations

import tempfile
from pathlib import Path

from fossic import Append, ReadQuery, Store
from fossic.relay import (
    RelayAgent,
    RelayConfig,
    _hub_stream_id,
    _translate_causation_id,
    relay_append,
)


# ── Helpers ───────────────────────────────────────────────────────────────────


def make_store(d: str, name: str, *streams: str) -> Store:
    s = Store.open(str(Path(d) / f"{name}.db"))
    for stream in streams:
        s.declare_stream(stream, "test")
    return s


def make_agent(
    local: Store,
    hub: Store,
    source_prefix: str = "cerebra",
    subscribe_pattern: str = "cerebra/*",
    relay_filter: set[str] | None = None,
) -> RelayAgent:
    """Build a RelayAgent with stores already wired (bypasses run())."""
    cfg = RelayConfig(
        local_store_path="",
        hub_store_path="",
        source_prefix=source_prefix,
        subscribe_pattern=subscribe_pattern,
        relay_filter=relay_filter or set(),
    )
    agent = RelayAgent(cfg)
    agent.local_store = local
    agent.hub_store = hub
    return agent


# ── _hub_stream_id ────────────────────────────────────────────────────────────


def test_hub_stream_id_already_prefixed() -> None:
    assert _hub_stream_id("cerebra", "cerebra/agent-trace") == "cerebra/agent-trace"


def test_hub_stream_id_not_prefixed() -> None:
    assert _hub_stream_id("cerebra", "agent-trace") == "cerebra/agent-trace"


def test_hub_stream_id_different_prefix_not_stripped() -> None:
    assert _hub_stream_id("cerebra", "ai-stack/gpu") == "cerebra/ai-stack/gpu"


def test_hub_stream_id_partial_match_not_stripped() -> None:
    # "cerebral/events" starts with "cerebral/" not "cerebra/"
    assert _hub_stream_id("cerebra", "cerebral/events") == "cerebra/cerebral/events"


def test_hub_stream_id_all_four_projects() -> None:
    # Under D.3, streams already carrying their project prefix pass through unchanged
    assert _hub_stream_id("cerebra", "cerebra/agent-trace") == "cerebra/agent-trace"
    assert _hub_stream_id("lumaweave", "lumaweave/graph/events") == "lumaweave/graph/events"
    assert _hub_stream_id("policy-scout", "policy-scout/audit") == "policy-scout/audit"
    assert _hub_stream_id("ai-stack", "ai-stack/gpu") == "ai-stack/gpu"


# ── RelayConfig ───────────────────────────────────────────────────────────────


def test_relay_config_defaults() -> None:
    cfg = RelayConfig(
        local_store_path="/a",
        hub_store_path="/b",
        source_prefix="cerebra",
        subscribe_pattern="cerebra/*",
    )
    assert cfg.relay_filter == set()
    assert cfg.batch_size == 50
    assert cfg.reconnect_delay_ms == 5000
    assert cfg.max_retry_attempts == 5
    assert cfg.retry_backoff_base_ms == 100


# ── _translate_causation_id ───────────────────────────────────────────────────


def test_translate_none_returns_none() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        assert _translate_causation_id(local, hub, "cerebra", None) is None


def test_translate_local_id_to_hub_id() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")

        local_id = local.append(Append("cerebra/signals", "TypeA", {"a": 1}))
        local_ev = local.read_one(local_id)
        assert local_ev is not None
        # Relay via relay_append — adds source_store tag, changing the content hash
        relay_append(
            local_store=local,
            hub_store=hub,
            event=local_ev,
            source_prefix="cerebra",
            hub_stream_id="cerebra/signals",
            payload=local_ev.payload(),
        )
        hub_ev = hub.read_by_external_id("cerebra/signals", local_id.hex())
        assert hub_ev is not None

        result = _translate_causation_id(local, hub, "cerebra", local_id)
        assert result == hub_ev.id
        assert hub.read_one(result) is not None  # returned ID is live in hub


def test_translate_hub_id_passes_through() -> None:
    # An ID that lives only in hub (cross-store trigger, e.g. GraphSnapshotAvailable.id)
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")

        hub_id = hub.append(Append("cerebra/signals", "HubEvent", {"h": 1}))
        # hub_id does not exist in local store
        result = _translate_causation_id(local, hub, "cerebra", hub_id)
        assert result == hub_id


def test_translate_unrelayed_local_id_passes_through() -> None:
    # Local event exists but hasn't been relayed yet → case-1 fallback
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")

        local_id = local.append(Append("cerebra/signals", "UnrelayedEvent", {"u": 1}))
        # Nothing relayed to hub
        result = _translate_causation_id(local, hub, "cerebra", local_id)
        assert result == local_id  # falls back to local id (case-1 degradation)


# ── relay_event ───────────────────────────────────────────────────────────────


def test_relay_event_returns_true() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        agent = make_agent(local, hub)
        assert agent.relay_event(ev) is True


def test_relay_event_event_appears_in_hub() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        agent = make_agent(local, hub)
        agent.relay_event(ev)
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        assert hub_ev is not None
        assert hub_ev.event_type == "Signal"


def test_relay_event_idempotent() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        agent = make_agent(local, hub)
        assert agent.relay_event(ev) is True
        assert agent.relay_event(ev) is False  # already present
        assert len(hub.read_range(ReadQuery("cerebra/signals"))) == 1


def test_relay_event_source_store_tag_added() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        make_agent(local, hub).relay_event(ev)
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        assert hub_ev.indexed_tags()["source_store"] == "cerebra"


def test_relay_event_branch_preserved() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}, branch="main"))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        make_agent(local, hub).relay_event(ev)
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        assert hub_ev.branch == "main"


def test_relay_event_filter_blocks_unmatched_type() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "OtherType", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        agent = make_agent(local, hub, relay_filter={"AllowedType"})
        assert agent.relay_event(ev) is False
        assert len(hub.read_range(ReadQuery("cerebra/signals"))) == 0


def test_relay_event_filter_allows_matched_type() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "AllowedType", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        agent = make_agent(local, hub, relay_filter={"AllowedType"})
        assert agent.relay_event(ev) is True


def test_relay_event_d3_stream_already_prefixed() -> None:
    # Stream name already carries source_prefix/ → hub stream unchanged
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/lattice")
        hub = make_store(d, "hub", "cerebra/lattice")
        local.append(Append("cerebra/lattice", "GraphEvent", {"g": 1}))
        ev = local.read_range(ReadQuery("cerebra/lattice"))[0]
        make_agent(local, hub).relay_event(ev)
        # Hub stream name is "cerebra/lattice" (not "cerebra/cerebra/lattice")
        hub_ev = hub.read_by_external_id("cerebra/lattice", ev.id.hex())
        assert hub_ev is not None


def test_relay_event_d3_stream_not_prefixed() -> None:
    # Stream name lacks source_prefix/ → hub stream gets prefix prepended
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "lattice/events")
        hub = make_store(d, "hub", "cerebra/lattice/events")  # 3 segments
        local.append(Append("lattice/events", "GraphEvent", {"g": 1}))
        ev = local.read_range(ReadQuery("lattice/events"))[0]
        agent = make_agent(local, hub, source_prefix="cerebra")
        agent.relay_event(ev)
        hub_ev = hub.read_by_external_id("cerebra/lattice/events", ev.id.hex())
        assert hub_ev is not None


# ── relay_append — indexed_tags ───────────────────────────────────────────────


def test_relay_append_event_indexed_tags_preserved() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append(
            "cerebra/signals", "Signal", {"x": 1},
            indexed_tags={"session_id": "abc", "cycle_id": "def"},
        ))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        relay_append(
            local_store=local,
            hub_store=hub,
            event=ev,
            source_prefix="cerebra",
            hub_stream_id="cerebra/signals",
            payload=ev.payload(),
        )
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        tags = hub_ev.indexed_tags()
        assert tags["session_id"] == "abc"
        assert tags["cycle_id"] == "def"
        assert tags["source_store"] == "cerebra"


def test_relay_append_source_store_not_overridable_by_event_tags() -> None:
    # Even if the event has a source_store tag, the relay's source_prefix wins
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append(
            "cerebra/signals", "Signal", {"x": 1},
            indexed_tags={"source_store": "wrong"},
        ))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        relay_append(
            local_store=local,
            hub_store=hub,
            event=ev,
            source_prefix="cerebra",
            hub_stream_id="cerebra/signals",
            payload=ev.payload(),
        )
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        assert hub_ev.indexed_tags()["source_store"] == "cerebra"


def test_relay_append_extra_indexed_tags_merged() -> None:
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        relay_append(
            local_store=local,
            hub_store=hub,
            event=ev,
            source_prefix="cerebra",
            hub_stream_id="cerebra/signals",
            payload=ev.payload(),
            extra_indexed_tags={"run_id": "r42"},
        )
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        tags = hub_ev.indexed_tags()
        assert tags["run_id"] == "r42"
        assert tags["source_store"] == "cerebra"


def test_relay_append_no_indexed_tags_on_event() -> None:
    # Event with no indexed_tags — hub event still gets source_store
    with tempfile.TemporaryDirectory() as d:
        local = make_store(d, "local", "cerebra/signals")
        hub = make_store(d, "hub", "cerebra/signals")
        local.append(Append("cerebra/signals", "Signal", {"x": 1}))
        ev = local.read_range(ReadQuery("cerebra/signals"))[0]
        assert ev.indexed_tags() is None
        relay_append(
            local_store=local,
            hub_store=hub,
            event=ev,
            source_prefix="cerebra",
            hub_stream_id="cerebra/signals",
            payload=ev.payload(),
        )
        hub_ev = hub.read_by_external_id("cerebra/signals", ev.id.hex())
        assert hub_ev.indexed_tags()["source_store"] == "cerebra"
