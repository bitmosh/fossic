"""
fossic — local-first event sourcing with content-addressed event identity.

Exception hierarchy
-------------------
FossicError
  StreamNotDeclaredError
  InvalidStreamIdError
  InvalidEventIdError
  StoreNotFoundError
  SchemaMismatchError
  NotImplementedError
  BranchNotFoundError
  BranchLifecycleError
  InvalidBranchIdError
  ReducerPatternAmbiguousError
  ReducerNotFoundError
  NoEventsToSnapshotError
  PurgeConfirmationError
  EventNotFoundError
  UpcasterChainGapError
  StorageError           (wraps SQLite / I/O / msgpack / CCE errors)

Python-reducer support
----------------------
``Store.register_reducer`` registers a Python object as a DynReducer, backed by the
Rust snapshot-caching infrastructure.  The Python reducer protocol::

    name: str                           # unique reducer name (snapshot key)
    version: int                        # reducer code version
    state_schema_version: int           # state schema version

    def initial_state(self) -> Any: ...
    def apply(self, state: Any, event_payload: Any) -> Any: ...

``Store.read_state(stream_id, branch)`` folds events through the registered reducer
starting from the most recent snapshot.
``Store.take_snapshot(stream_id, branch)`` persists the current state to SQLite.
"""

from __future__ import annotations

import threading
import warnings
from typing import Any, Callable, Optional

# Re-export the Rust extension module types so consumers can import directly
# from `fossic` without knowing about `fossic._fossic`.
try:
    from fossic._fossic import (  # type: ignore[import]
        AggregateQuery,
        Append,
        BranchInfo,
        BranchSegment,
        CreateBranch,
        EventId,
        FossicError,
        InvalidBranchIdError,
        BranchLifecycleError,
        BranchNotFoundError,
        EventNotFoundError,
        InvalidEventIdError,
        InvalidStreamIdError,
        NoEventsToSnapshotError,
        NotImplementedError,
        PurgeConfirmationError,
        RawSubscriptionHandle,
        ReadQuery,
        ReducerNotFoundError,
        ReducerPatternAmbiguousError,
        ReducerNotFoundByNameError,
        ReducerCallError,
        SchemaMismatchError,
        SnapshotInfo,
        StorageError,
        Store as _RustStore,
        StoredEvent,
        StreamInfo,
        StreamNotDeclaredError,
        StoreNotFoundError,
        SubscriptionMode,
        UpcasterChainGapError,
        OpenOptions,
        cce_encode_value,
        cce_encode_bytes_raw,
        cce_encode_f64_bits,
    )
    _RUST_AVAILABLE = True
except ImportError:
    # Extension not built yet (e.g. running mypy / IDEs).
    _RUST_AVAILABLE = False
    _RustStore = None  # type: ignore[assignment]
    ReadQuery = None  # type: ignore[assignment,misc]

from fossic._worker import SubscriptionWorker


# ── Python-level SubscriptionHandle ──────────────────────────────────────────


class SubscriptionHandle:
    """
    Subscription handle returned by ``Store.subscribe()``.

    Supports both the context-manager pattern and direct iteration::

        # Context manager + iteration (most common):
        with store.subscribe("cerebra/lattice/abc", mode=SubscriptionMode.post_commit()) as sub:
            for event in sub:
                process(event)

        # Context manager + callback (worker thread):
        with store.subscribe("stream", callback=handle_event, mode=...) as sub:
            sub.join()   # block until externally stopped

        # Explicit lifecycle:
        sub = store.subscribe("stream")
        sub.start()   # same as __enter__
        ...
        sub.unsubscribe()
    """

    def __init__(
        self,
        inner: "RawSubscriptionHandle",
        callback: Optional[Callable[["StoredEvent"], None]] = None,
    ) -> None:
        self._inner = inner
        self._callback = callback
        self._worker: Optional[SubscriptionWorker] = None
        self._unsubscribed = False
        self._lock = threading.Lock()

    # ── Context manager ───────────────────────────────────────────────────────

    def __enter__(self) -> "SubscriptionHandle":
        if self._callback is not None:
            self._worker = SubscriptionWorker(self._inner, self._callback)
            self._worker.start()
        return self

    def __exit__(self, *_: Any) -> None:
        self.unsubscribe()

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def start(self) -> "SubscriptionHandle":
        """Alias for ``__enter__``; starts the worker thread if a callback was given."""
        return self.__enter__()

    def unsubscribe(self) -> None:
        """Unsubscribe and stop the worker thread.  Idempotent."""
        with self._lock:
            if self._unsubscribed:
                return
            self._unsubscribed = True

        if self._worker is not None:
            self._worker.stop()
            self._worker = None
        self._inner.unsubscribe()

    def __del__(self) -> None:
        if not self._unsubscribed:
            warnings.warn(
                "SubscriptionHandle was garbage collected without being unsubscribed. "
                "Use `with store.subscribe(...) as sub:` or call `.unsubscribe()` explicitly.",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self.unsubscribe()
            except Exception:  # noqa: BLE001
                pass

    # ── Iteration protocol (worker thread bypassed) ───────────────────────────

    def __iter__(self) -> "SubscriptionHandle":
        return self

    def __next__(self) -> "StoredEvent":
        """Block until the next event, releasing the GIL during the wait."""
        try:
            while True:
                event = self._inner._wait_for_next_event(1.0)
                if event is None:
                    # Timeout — check if we should stop.
                    if self._unsubscribed:
                        raise StopIteration
                    continue
                return event  # type: ignore[return-value]
        except StopIteration:
            raise  # subscription channel closed

    # ── Properties ───────────────────────────────────────────────────────────

    @property
    def is_degraded(self) -> bool:
        """True if the PostCommit queue overflowed; replay from cursor to recover."""
        return self._inner.is_degraded()


class Store:
    """
    Fossic event store — the public Python API.

    Wraps ``_fossic.Store`` (the Rust extension) and adds:
    * Python-reducer registration and ``read_state`` / ``read_state_at_version``
    * Subscription lifecycle (context manager, worker thread)
    """

    def __init__(self, inner: Any) -> None:
        self._inner = inner

    # ── Class methods ─────────────────────────────────────────────────────────

    @classmethod
    def open(
        cls,
        path: str,
        options: Optional["OpenOptions"] = None,
    ) -> "Store":
        """Open (or create) a fossic store at *path*."""
        rust_store = _RustStore.open(path, options)  # type: ignore[union-attr]
        return cls(rust_store)

    # ── Pass-through store methods ────────────────────────────────────────────

    def declare_stream(
        self,
        stream_id: str,
        declared_by: str = "python",
        description: Optional[str] = None,
    ) -> None:
        self._inner.declare_stream(stream_id, declared_by, description)

    def streams(self) -> "list[StreamInfo]":
        return self._inner.streams()  # type: ignore[return-value]

    def stream_exists(self, stream_id: str) -> bool:
        return self._inner.stream_exists(stream_id)  # type: ignore[return-value]

    def append(self, a: "Append") -> "EventId":
        return self._inner.append(a)  # type: ignore[return-value]

    def append_batch(self, appends: "list[Append]") -> "list[EventId]":
        return self._inner.append_batch(appends)  # type: ignore[return-value]

    def read_range(self, query: Any) -> "list[StoredEvent]":
        """Read events from a single stream.

        Pass a ``ReadQuery`` to control which events are returned.
        ``ReadQuery.event_type_filter`` (optional) limits results to events
        whose ``event_type`` matches exactly; ``None`` (default) returns all types.
        """
        return self._inner.read_range(query)  # type: ignore[return-value]

    def read_one(self, event_id: "EventId") -> "Optional[StoredEvent]":
        return self._inner.read_one(event_id)  # type: ignore[return-value]

    def read_by_external_id(
        self, stream_id: str, external_id: str
    ) -> "Optional[StoredEvent]":
        return self._inner.read_by_external_id(stream_id, external_id)  # type: ignore[return-value]

    def read_by_correlation(
        self, correlation_id: "EventId"
    ) -> "list[StoredEvent]":
        return self._inner.read_by_correlation(correlation_id)  # type: ignore[return-value]

    def walk_causation(
        self,
        start: "EventId",
        direction: str = "forward",
        max_depth: int = 100,
    ) -> "list[StoredEvent]":
        return self._inner.walk_causation(start, direction, max_depth)  # type: ignore[return-value]

    def aggregate(self, query: "AggregateQuery") -> "list[StoredEvent]":
        return self._inner.aggregate(query)  # type: ignore[return-value]

    def register_upcaster(
        self,
        event_type: str,
        from_version: int,
        to_version: int,
        callable: Callable[[Any], Any],
    ) -> None:
        """Register a callable that upcasts event payloads at read time.

        The callable signature is ``(payload: dict) -> dict`` — NOT
        ``(event_type, payload)``. It receives the deserialized payload dict
        and must return the upcast payload dict.

        **Registration is per (event_type, from_version, to_version) triple.**
        Register one upcaster per version gap. Upcasters chain automatically:
        an event at ``type_version=1`` with registered upcasters ``1→2`` and
        ``2→3`` is upcast through both before reaching a reducer.

        **Upcasters fire at read time, not write time.** Stored events keep
        their original bytes and CCE-derived identity. Upcasting changes what
        a reducer sees but does not alter the stored payload or the event ``id``.

        **Chain gaps raise at read time.** If there is no upcaster covering an
        intermediate version step (e.g. ``1→2`` and ``3→4`` are registered but
        ``2→3`` is missing), reading an event at version 1 raises
        ``UpcasterChainGapError``.
        """
        self._inner.register_upcaster(event_type, from_version, to_version, callable)

    def register_payload_transform(
        self,
        stream_pattern: str,
        callable: Callable[[str, Any], Any],
    ) -> None:
        """Register a callable that transforms event payloads at append time.

        The callable signature is ``(event_type: str, payload: dict) -> dict``.
        It fires before CCE encoding when an event is appended to a stream
        matching *stream_pattern*. The returned dict becomes the stored payload.

        **Register before appending.** Transforms fire during ``Store.append()``,
        not during ``read_range()``. A transform registered after an event is stored
        has no effect on that event's payload.

        The first argument is the event's ``event_type`` string (e.g. ``"UserCreated"``),
        not the stream ID.
        """
        self._inner.register_payload_transform(stream_pattern, callable)

    def purge_event(
        self,
        event_id: "EventId",
        confirm: str,
        reason: str,
        purged_by: str,
    ) -> None:
        self._inner.purge_event(event_id, confirm, reason, purged_by)

    def shred_stream(self, stream_id: str, reason: str) -> None:
        self._inner.shred_stream(stream_id, reason)

    def get_cursor(
        self, consumer_id: str, stream_id: str, branch: str
    ) -> Optional[int]:
        return self._inner.get_cursor(consumer_id, stream_id, branch)  # type: ignore[return-value]

    def set_cursor(
        self, consumer_id: str, stream_id: str, branch: str, version: int
    ) -> None:
        self._inner.set_cursor(consumer_id, stream_id, branch, version)

    def create_branch(self, b: "CreateBranch") -> None:
        self._inner.create_branch(b)

    def promote_branch(
        self, stream_id: str, branch_id: str, reason: Optional[str] = None
    ) -> None:
        self._inner.promote_branch(stream_id, branch_id, reason)

    def mark_branch_dead_end(
        self, stream_id: str, branch_id: str, reason: Optional[str] = None
    ) -> None:
        self._inner.mark_branch_dead_end(stream_id, branch_id, reason)

    def list_branches(self, stream_id: str) -> "list[BranchInfo]":
        """Return only explicitly created diverged branches.

        The implicit 'main' trunk is NOT included — it has no stored row.
        An empty list means the stream exists but no branches have been forked yet.
        """
        return self._inner.list_branches(stream_id)  # type: ignore[return-value]

    def resolve_chain(
        self, stream_id: str, branch_id: str
    ) -> "list[BranchSegment]":
        return self._inner.resolve_chain(stream_id, branch_id)  # type: ignore[return-value]

    def snapshot_info(
        self, stream_id: str, branch: str, reducer_name: str
    ) -> "Optional[SnapshotInfo]":
        return self._inner.snapshot_info(stream_id, branch, reducer_name)  # type: ignore[return-value]

    def take_snapshot(self, stream_id: str, branch: str) -> "SnapshotInfo":
        return self._inner.take_snapshot(stream_id, branch)  # type: ignore[return-value]

    def gc_orphaned_snapshots(self) -> int:
        return self._inner.gc_orphaned_snapshots()  # type: ignore[return-value]

    # ── Subscriptions ─────────────────────────────────────────────────────────

    def subscribe(
        self,
        stream_id: str,
        branch: str = "main",
        mode: Optional["SubscriptionMode"] = None,
        callback: Optional[Callable[["StoredEvent"], None]] = None,
    ) -> SubscriptionHandle:
        """
        Subscribe to a stream.

        Returns a :class:`SubscriptionHandle` that acts as both a context
        manager and an iterator.

        If *callback* is provided the handle starts a Python worker thread
        when used as a context manager (``with store.subscribe(..., callback=fn) as sub:``).
        Without a callback, use ``for event in sub:`` to iterate directly.
        """
        raw = self._inner.subscribe(stream_id, branch, mode)
        return SubscriptionHandle(raw, callback)

    # ── Reducer support ───────────────────────────────────────────────────────

    def register_reducer(self, pattern: str, reducer: Any) -> None:
        """
        Register a Python reducer for all streams matching *pattern*.

        The reducer object must implement:

            name: str
            version: int
            state_schema_version: int
            def initial_state(self) -> Any: ...
            def apply(self, state: Any, event_payload: Any) -> Any: ...

        State is persisted as snapshots in SQLite; ``read_state`` starts from the
        most recent snapshot rather than replaying all events from scratch.
        """
        self._inner.register_reducer(pattern, reducer)

    def read_state(self, stream_id: str, branch: str = "main") -> Any:
        """Compute the current state for *stream_id* through the registered reducer."""
        return self._inner.read_state(stream_id, branch)

    def read_state_at_version(
        self, stream_id: str, branch: str, version: int
    ) -> Any:
        """Like ``read_state`` but folds only events up to *version* inclusive."""
        return self._inner.read_state_at_version(stream_id, branch, version)


__all__ = [
    # Store
    "Store",
    "OpenOptions",
    # Types
    "Append",
    "ReadQuery",
    "AggregateQuery",
    "CreateBranch",
    "SubscriptionMode",
    "SubscriptionHandle",
    # Value types
    "EventId",
    "StoredEvent",
    "StreamInfo",
    "BranchInfo",
    "BranchSegment",
    "SnapshotInfo",
    # Exceptions
    "FossicError",
    "StreamNotDeclaredError",
    "InvalidStreamIdError",
    "InvalidEventIdError",
    "StoreNotFoundError",
    "SchemaMismatchError",
    "NotImplementedError",
    "BranchNotFoundError",
    "BranchLifecycleError",
    "InvalidBranchIdError",
    "ReducerPatternAmbiguousError",
    "ReducerNotFoundError",
    "ReducerNotFoundByNameError",
    "ReducerCallError",
    "NoEventsToSnapshotError",
    "PurgeConfirmationError",
    "EventNotFoundError",
    "UpcasterChainGapError",
    "StorageError",
    # CCE encoding (testing / tooling)
    "cce_encode_value",
    "cce_encode_bytes_raw",
    "cce_encode_f64_bits",
]
