# SPDX-License-Identifier: Apache-2.0
"""
Python-owned subscription worker thread.

The worker thread is created by Python's threading module so that
threading.local, contextvars, logging context, and Django thread-locals all
behave correctly.  Rust only produces events into a sync queue; the worker
reads them and invokes the callback on a stable, Python-owned thread.

See FOSSIC_V1_SPEC.md §4.2 — "Python-owned worker pattern".
"""

from __future__ import annotations

import threading
from typing import TYPE_CHECKING, Callable

if TYPE_CHECKING:
    from fossic._fossic import RawSubscriptionHandle, StoredEvent

_POLL_TIMEOUT = 0.5  # seconds — how long to block in each recv_timeout call


class SubscriptionWorker:
    """
    Manages a Python-owned background thread that reads from the Rust queue
    and invokes *callback* on each event.

    Usage::

        worker = SubscriptionWorker(raw_handle, my_callback)
        worker.start()
        # ... later ...
        worker.stop()
    """

    def __init__(
        self,
        handle: RawSubscriptionHandle,
        callback: Callable[[StoredEvent], None],
    ) -> None:
        self._handle = handle
        self._callback = callback
        self._stopping = False
        self._thread: threading.Thread = threading.Thread(
            target=self._loop,
            name="fossic-subscription-worker",
            daemon=True,
        )

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        """Signal the worker to stop and wait for it to exit."""
        self._stopping = True
        self._thread.join()

    def _loop(self) -> None:
        """
        Main loop — runs on the Python-owned worker thread.

        Blocks in `_wait_for_next_event` (GIL released during the wait), so
        other threads, asyncio, etc. are not starved.
        """
        try:
            while not self._stopping:
                event = self._handle._wait_for_next_event(_POLL_TIMEOUT)
                if event is None:
                    # Timeout — check stopping flag and retry.
                    continue
                self._callback(event)
        except StopIteration:
            # Channel closed (subscription removed / store dropped).
            pass
