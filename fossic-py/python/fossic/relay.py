from __future__ import annotations

import logging
import threading
import time
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Optional

try:
    from fossic._fossic import Append, StorageError  # type: ignore[import]
except ImportError:
    Append = None  # type: ignore[assignment,misc]
    StorageError = Exception  # type: ignore[assignment,misc]

if TYPE_CHECKING:
    from fossic._fossic import EventId, StoredEvent  # type: ignore[import]
    from fossic import Store


# ── D.3 stream naming ─────────────────────────────────────────────────────────


def _hub_stream_id(source_prefix: str, local_stream_id: str) -> str:
    """Apply the D.3 conditional strip rule.

    If the local stream already starts with '<source_prefix>/', use it as-is
    on the hub. Otherwise prepend '<source_prefix>/'.

    This prevents double-prefixes (e.g. 'cerebra/cerebra/agent-trace/...')
    for projects whose streams already carry their own prefix.
    """
    prefix = source_prefix + "/"
    if local_stream_id.startswith(prefix):
        return local_stream_id
    return f"{prefix}{local_stream_id}"


# ── Causation ID translation ──────────────────────────────────────────────────


def _translate_causation_id(
    local_store: Store,
    hub_store: Store,
    source_prefix: str,
    local_causation_id: Optional[EventId],
) -> Optional[EventId]:
    """Translate a local causation ID to its hub equivalent where possible.

    Three cases:
      - None             → None (root event)
      - local event ID   → hub_event.id via read_by_external_id (same-project chain)
      - hub event ID     → pass through (cross-store trigger already a hub ID)

    Distinguishes local vs. hub IDs by attempting local_store.read_one().
    If not found locally, the ID is assumed to already be a hub ID (e.g. the
    hub-side GraphSnapshotAvailable.id stored by LumaWeave when reacting to a
    hub trigger). If the local event exists but hasn't been relayed yet,
    walk_causation will fail on hub — correct case-1 degradation behaviour.
    """
    if local_causation_id is None:
        return None
    local_cause = local_store.read_one(local_causation_id)
    if local_cause is None:
        return local_causation_id
    hub_stream = _hub_stream_id(source_prefix, local_cause.stream_id)
    hub_cause = hub_store.read_by_external_id(hub_stream, local_causation_id.hex())
    if hub_cause is not None:
        return hub_cause.id
    return local_causation_id


# ── relay_append ──────────────────────────────────────────────────────────────


def relay_append(
    local_store: Store,
    hub_store: Store,
    event: StoredEvent,
    source_prefix: str,
    hub_stream_id: str,
    payload: Any,
    extra_indexed_tags: Optional[dict[str, Any]] = None,
) -> None:
    """Append one event to the hub store, enforcing all relay protocol fields.

    Callers are responsible for the idempotency check (read_by_external_id)
    and event filtering before calling this. `hub_stream_id` must already be
    D.3-processed (use _hub_stream_id). `payload` must already be deserialized
    (use event.payload()).

    Protocol fields enforced here:
      - causation_id translated via _translate_causation_id (S-030 corrected)
      - external_id = event.id.hex() (S-014 idempotency key)
      - branch passed through (S-012)
      - source_store added to indexed_tags (S-013)
    """
    hub_store.append(Append(  # type: ignore[misc]
        stream_id=hub_stream_id,
        event_type=event.event_type,
        payload=payload,
        branch=event.branch,
        type_version=event.type_version,
        causation_id=_translate_causation_id(
            local_store, hub_store, source_prefix, event.causation_id
        ),
        external_id=event.id.hex(),
        indexed_tags={
            **(event.indexed_tags() or {}),
            **(extra_indexed_tags or {}),
            "source_store": source_prefix,
        },
    ))


# ── Configuration ─────────────────────────────────────────────────────────────


@dataclass
class RelayConfig:
    local_store_path: str
    """Absolute path to the project's local fossic store."""

    hub_store_path: str
    """Absolute path to the shared hub store (~/.lattica/fossic/store.db)."""

    source_prefix: str
    """Project identifier used as hub stream namespace and source_store tag value.
    Must be stable — changing it breaks hub stream names and causation routing."""

    subscribe_pattern: str
    """Stream glob passed to local_store.subscribe() (e.g. 'cerebra/**').
    Note: '*' matches one path segment only; use '**' for streams with
    multiple segments (e.g. 'cerebra/agent-trace/<session_id>' requires
    'cerebra/**' or 'cerebra/agent-trace/*').
    Store.subscribe()'s Python parameter is named 'stream_id' but
    the underlying Rust layer treats it as a pattern."""

    relay_filter: set[str] = field(default_factory=set)
    """If non-empty, only event_types in this set are relayed.
    Empty means relay all event_types matched by subscribe_pattern."""

    heartbeat_interval_s: float = 5.0
    """Interval in seconds between RelayHeartbeat events emitted to local store."""

    project_description: str = ""
    """Human-readable description of the project; included in ProjectRegistered."""

    batch_size: int = 50
    """Hint for batched-append implementations; not enforced by RelayAgent."""

    reconnect_delay_ms: int = 5000
    """Milliseconds to wait before reconnecting after a StorageError."""

    max_retry_attempts: int = 5
    """Max retry attempts per event before logging and skipping."""

    retry_backoff_base_ms: int = 100
    """Initial backoff for per-event retry; doubles on each attempt."""


# ── RelayAgent ────────────────────────────────────────────────────────────────


class RelayAgent:
    """Conformant relay agent. Subclass to override _should_relay() for
    complex filter logic, or relay_event() for post-relay side effects.

    Protocol requirements enforced by relay_event() via relay_append():
      - D.3 conditional strip rule on hub stream_id
      - source_store indexed_tag on every relayed event (S-013)
      - branch passed through verbatim (S-012)
      - external_id = event.id.hex() as idempotency key (S-014)
      - causation_id translated local→hub where possible (S-030 corrected)
      - payload deserialized at local relay boundary (S-015)
    """

    def __init__(self, config: RelayConfig) -> None:
        self.config = config
        self.local_store: Optional[Store] = None
        self.hub_store: Optional[Store] = None
        self.logger = logging.getLogger(f"relay.{config.source_prefix}")
        self._last_event_version: int = -1
        self._start_us: int = 0

    def _hub_stream_id(self, local_stream_id: str) -> str:
        return _hub_stream_id(self.config.source_prefix, local_stream_id)

    def _should_relay(self, event: StoredEvent) -> bool:
        if not self.config.relay_filter:
            return True
        return event.event_type in self.config.relay_filter

    def relay_event(self, event: StoredEvent) -> bool:
        """Relay one event from local store to hub.

        Returns True if appended, False if filtered or already present.
        Raises on unrecoverable append failure (caller retries).
        Must only be called after run() has opened both stores.
        """
        assert self.local_store is not None
        assert self.hub_store is not None

        if not self._should_relay(event):
            return False

        # D.3 must run before idempotency check — read_by_external_id needs hub stream_id
        hub_stream_id = self._hub_stream_id(event.stream_id)

        if self.hub_store.read_by_external_id(hub_stream_id, event.id.hex()) is not None:
            return False

        relay_append(
            local_store=self.local_store,
            hub_store=self.hub_store,
            event=event,
            source_prefix=self.config.source_prefix,
            hub_stream_id=hub_stream_id,
            payload=event.payload(),
        )
        self._last_event_version = event.version
        return True

    def _heartbeat_loop(self, stop_event: threading.Event) -> None:
        while not stop_event.wait(timeout=self.config.heartbeat_interval_s):
            if self.local_store is not None:
                try:
                    uptime_us = int(time.time() * 1_000_000) - self._start_us
                    self.local_store.emit_relay_heartbeat(
                        self.config.source_prefix,
                        self._last_event_version,
                        0,
                        uptime_us,
                    )
                except Exception:
                    self.logger.debug("heartbeat emit failed", exc_info=True)

    def run(self) -> None:
        """Main relay loop. Runs until an unrecoverable error; restarts on StorageError."""
        from fossic import Store  # local import avoids circular import at module level
        self._start_us = int(time.time() * 1_000_000)
        stop_hb = threading.Event()
        hb_thread = threading.Thread(
            target=self._heartbeat_loop,
            args=(stop_hb,),
            daemon=True,
            name=f"relay-hb.{self.config.source_prefix}",
        )
        hb_thread.start()
        try:
            while True:
                try:
                    self.local_store = Store.open(self.config.local_store_path)
                    self.hub_store = Store.open(self.config.hub_store_path)
                    self.local_store.emit_project_registered(
                        self.config.source_prefix,
                        self.config.local_store_path,
                        self.config.subscribe_pattern,
                        self.config.project_description,
                    )
                    self.logger.info(
                        "relay started",
                        extra={
                            "source": self.config.source_prefix,
                            "pattern": self.config.subscribe_pattern,
                        },
                    )
                    with self.local_store.subscribe(self.config.subscribe_pattern) as sub:
                        for event in sub:
                            self._relay_with_retry(event)
                except StorageError as exc:
                    self.logger.warning(
                        "store error — reconnecting",
                        extra={
                            "error": str(exc),
                            "delay_ms": self.config.reconnect_delay_ms,
                        },
                    )
                    time.sleep(self.config.reconnect_delay_ms / 1000)
                except Exception:
                    self.logger.error("relay fatal error", exc_info=True)
                    raise
        finally:
            stop_hb.set()

    def _relay_with_retry(self, event: StoredEvent) -> None:
        for attempt in range(self.config.max_retry_attempts):
            try:
                relayed = self.relay_event(event)
                if relayed:
                    self.logger.debug(
                        "relayed",
                        extra={
                            "event_id": event.id.hex(),
                            "stream": event.stream_id,
                            "event_type": event.event_type,
                        },
                    )
                return
            except Exception as exc:
                if attempt == self.config.max_retry_attempts - 1:
                    self.logger.error(
                        "relay event failed after retries — skipping",
                        extra={
                            "event_id": event.id.hex(),
                            "stream": event.stream_id,
                            "event_type": event.event_type,
                            "error": str(exc),
                            "attempts": self.config.max_retry_attempts,
                        },
                    )
                    return
                backoff_s = (self.config.retry_backoff_base_ms / 1000) * (2 ** attempt)
                self.logger.debug(
                    "relay event failed — retrying",
                    extra={
                        "event_id": event.id.hex(),
                        "attempt": attempt + 1,
                        "backoff_s": backoff_s,
                        "error": str(exc),
                    },
                )
                time.sleep(backoff_s)


# ── Convenience entry point ───────────────────────────────────────────────────


def run_relay(config: RelayConfig) -> None:
    """Open both stores and relay events from local to hub until interrupted."""
    RelayAgent(config).run()
