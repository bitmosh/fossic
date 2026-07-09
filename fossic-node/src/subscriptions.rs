// SPDX-License-Identifier: Apache-2.0
use fossic::StoredEvent;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::Status;
use napi_derive::napi;

use crate::types::StoredEventJs;

// ── FossicSubscription ────────────────────────────────────────────────────────

/// An active fossic subscription. Exposes `rawNext()` / `rawDispose()` / `unsubscribe()`
/// to Rust-land; the JS wrapper in index.js adds `[Symbol.asyncIterator]` and
/// `[Symbol.asyncDispose]` on top.
///
/// Architecture: a single long-lived dispatcher thread drives the crossbeam
/// receiver and pushes events through a ThreadsafeFunction. Each call to
/// `rawNext()` registers a one-shot TSFN callback; the dispatcher delivers at
/// most one event per registered callback, preventing the concurrent-next() race
/// that the old spawn_blocking-per-call pattern suffered from.
#[napi]
pub struct FossicSubscription {
    handle: Option<fossic::SubscriptionHandle>,
    /// Sender side used to register pending JS callbacks with the dispatcher.
    pending_tx: crossbeam_channel::Sender<
        ThreadsafeFunction<Option<StoredEventJs>, (), Option<StoredEventJs>, Status, false>,
    >,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl FossicSubscription {
    pub(crate) fn new(
        handle: fossic::SubscriptionHandle,
        rx: crossbeam_channel::Receiver<StoredEvent>,
        closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let (pending_tx, pending_rx) = crossbeam_channel::unbounded::<
            ThreadsafeFunction<Option<StoredEventJs>, (), Option<StoredEventJs>, Status, false>,
        >();

        let closed_clone = closed.clone();
        std::thread::spawn(move || {
            dispatcher_loop(rx, pending_rx, closed_clone);
        });

        FossicSubscription {
            handle: Some(handle),
            pending_tx,
            closed,
        }
    }
}

#[napi]
impl FossicSubscription {
    /// Await the next event. Called by the JS wrapper's `next()` method.
    ///
    /// Returns `{value, done: false}` with the event, or `{done: true}` if the
    /// subscription is closed and the queue is empty.
    #[napi]
    pub fn raw_next(
        &self,
        callback: ThreadsafeFunction<
            Option<StoredEventJs>,
            (),
            Option<StoredEventJs>,
            Status,
            false,
        >,
    ) {
        use std::sync::atomic::Ordering;
        if self.closed.load(Ordering::Acquire) {
            callback.call(None, ThreadsafeFunctionCallMode::NonBlocking);
            return;
        }
        // Register the callback with the dispatcher. It will fire it when the
        // next event arrives (or immediately if already closed).
        if self.pending_tx.send(callback).is_err() {
            // Dispatcher thread already exited — subscription is gone.
        }
    }

    /// Unsubscribe from the stream.
    #[napi]
    pub fn unsubscribe(&mut self) {
        use std::sync::atomic::Ordering;
        self.closed.store(true, Ordering::Release);
        self.handle.take(); // Drop unsubscribes via SubscriptionHandle::drop
    }

    /// Called by the JS wrapper's `[Symbol.asyncDispose]()`.
    ///
    /// # Safety
    /// napi-rs requires `unsafe` for async `&mut self` methods; no unsafe invariants are violated.
    #[napi]
    pub async unsafe fn raw_dispose(&mut self) {
        self.unsubscribe();
    }
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Long-lived thread: reads from the event channel, fires pending JS callbacks
/// one-for-one. When closed, drains remaining pending callbacks with `None`.
fn dispatcher_loop(
    rx: crossbeam_channel::Receiver<StoredEvent>,
    pending_rx: crossbeam_channel::Receiver<
        ThreadsafeFunction<Option<StoredEventJs>, (), Option<StoredEventJs>, Status, false>,
    >,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use crossbeam_channel::RecvTimeoutError;
    use std::sync::atomic::Ordering;

    loop {
        // Wait for a JS caller to register a pending callback.
        let tsfn = match pending_rx.recv() {
            Ok(t) => t,
            Err(_) => break, // pending_tx dropped → subscription gone
        };

        if closed.load(Ordering::Acquire) {
            tsfn.call(None, ThreadsafeFunctionCallMode::NonBlocking);
            continue;
        }

        // Wait for an event, polling closed flag every 100ms.
        let event = loop {
            if closed.load(Ordering::Acquire) {
                break None;
            }
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(ev) => break Some(ev),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break None,
            }
        };

        let js_val = event.as_ref().map(StoredEventJs::from);
        tsfn.call(js_val, ThreadsafeFunctionCallMode::NonBlocking);

        if event.is_none() {
            // Drain remaining pending callbacks with done=None
            while let Ok(t) = pending_rx.try_recv() {
                t.call(None, ThreadsafeFunctionCallMode::NonBlocking);
            }
            break;
        }
    }
}
