use crossbeam_channel as cc;
use fossic::{StoredEvent, SubscriptionHandle, SubscriptionHandler};
use pyo3::prelude::*;
use std::time::Duration;

use crate::types::PyStoredEvent;

// ── Queue-based subscription handler ─────────────────────────────────────────

/// `SubscriptionHandler` that forwards events to a crossbeam channel.
/// The Python side reads events from the channel via `_wait_for_next_event`.
pub(crate) struct PyQueueHandler {
    tx: cc::Sender<StoredEvent>,
}

impl PyQueueHandler {
    pub fn new(tx: cc::Sender<StoredEvent>) -> Self {
        PyQueueHandler { tx }
    }
}

impl SubscriptionHandler for PyQueueHandler {
    fn on_event(&self, event: &StoredEvent) {
        // Best-effort; if the Python side has closed or is gone, drop silently.
        let _ = self.tx.send(event.clone());
    }
}

// ── Python-facing subscription handle ────────────────────────────────────────

/// Handle returned to Python from `Store.subscribe()`.
///
/// Exposes `_wait_for_next_event(timeout_secs)` for the Python worker thread
/// and the iteration protocol.  Dropping this handle (or calling `unsubscribe()`)
/// closes the channel, which wakes any blocked Python thread.
#[pyclass(name = "RawSubscriptionHandle")]
pub struct PyRawSubscriptionHandle {
    rx: cc::Receiver<StoredEvent>,
    /// Kept alive so that dropping it un-registers the subscription and closes the sender.
    handle: Option<SubscriptionHandle>,
}

impl PyRawSubscriptionHandle {
    pub fn new(rx: cc::Receiver<StoredEvent>, handle: SubscriptionHandle) -> Self {
        PyRawSubscriptionHandle { rx, handle: Some(handle) }
    }
}

#[pymethods]
impl PyRawSubscriptionHandle {
    /// Block until the next event arrives (up to `timeout_secs`), releasing the GIL
    /// while waiting.
    ///
    /// Returns:
    /// - `StoredEvent` on success
    /// - `None` on timeout
    /// - raises `StopIteration` when the subscription channel is closed
    fn _wait_for_next_event(
        &self,
        py: Python<'_>,
        timeout_secs: f64,
    ) -> PyResult<Option<PyStoredEvent>> {
        let rx = self.rx.clone();
        let timeout = Duration::from_secs_f64(timeout_secs);
        // Release the GIL while blocking so other Python threads can run.
        let result = py.detach(|| rx.recv_timeout(timeout));
        match result {
            Ok(event) => Ok(Some(PyStoredEvent::from(event))),
            Err(cc::RecvTimeoutError::Timeout) => Ok(None),
            Err(cc::RecvTimeoutError::Disconnected) => {
                // Channel closed (subscription removed / store dropped).
                Err(pyo3::exceptions::PyStopIteration::new_err("subscription closed"))
            }
        }
    }

    /// Explicitly unsubscribe.  Idempotent: safe to call multiple times.
    fn unsubscribe(&mut self) {
        // Dropping the SubscriptionHandle triggers registry.unsubscribe(id),
        // which closes the sender, which causes _wait_for_next_event to return
        // StopIteration the next time it's called.
        self.handle.take();
    }

    /// True if the subscription was marked degraded (queue overflow on PostCommit).
    fn is_degraded(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| h.is_degraded())
            .unwrap_or(false)
    }
}
