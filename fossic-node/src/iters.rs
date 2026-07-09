// SPDX-License-Identifier: Apache-2.0
use std::sync::{Arc, Mutex};

use fossic::{CausationIter, CorrelationIter, RangeIter};
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::types::StoredEventJs;

// ── FossicRangeIter ───────────────────────────────────────────────────────────

/// Async iterator over a range query. Each `rawNext()` call may acquire and
/// release a pool connection internally (batch-fetch-bounded pattern).
#[napi]
pub struct FossicRangeIter {
    inner: Arc<Mutex<Option<RangeIter>>>,
}

impl FossicRangeIter {
    pub fn new(iter: RangeIter) -> Self {
        FossicRangeIter {
            inner: Arc::new(Mutex::new(Some(iter))),
        }
    }
}

#[napi]
impl FossicRangeIter {
    /// Advance one step. Returns the next `StoredEvent`, or `null` when exhausted.
    #[napi]
    pub async fn raw_next(&self) -> Result<Option<StoredEventJs>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            match guard.as_mut() {
                None => Ok(None),
                Some(iter) => match iter.next() {
                    None => {
                        *guard = None;
                        Ok(None)
                    }
                    Some(Ok(ev)) => Ok(Some(StoredEventJs::from(&ev))),
                    Some(Err(e)) => {
                        *guard = None;
                        Err(Error::new(Status::GenericFailure, e.to_string()))
                    }
                },
            }
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }
}

// ── FossicCorrelationIter ─────────────────────────────────────────────────────

#[napi]
pub struct FossicCorrelationIter {
    inner: Arc<Mutex<Option<CorrelationIter>>>,
}

impl FossicCorrelationIter {
    pub fn new(iter: CorrelationIter) -> Self {
        FossicCorrelationIter {
            inner: Arc::new(Mutex::new(Some(iter))),
        }
    }
}

#[napi]
impl FossicCorrelationIter {
    #[napi]
    pub async fn raw_next(&self) -> Result<Option<StoredEventJs>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            match guard.as_mut() {
                None => Ok(None),
                Some(iter) => match iter.next() {
                    None => {
                        *guard = None;
                        Ok(None)
                    }
                    Some(Ok(ev)) => Ok(Some(StoredEventJs::from(&ev))),
                    Some(Err(e)) => {
                        *guard = None;
                        Err(Error::new(Status::GenericFailure, e.to_string()))
                    }
                },
            }
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }
}

// ── FossicCausationIter ───────────────────────────────────────────────────────

#[napi]
pub struct FossicCausationIter {
    inner: Arc<Mutex<Option<CausationIter>>>,
}

impl FossicCausationIter {
    pub fn new(iter: CausationIter) -> Self {
        FossicCausationIter {
            inner: Arc::new(Mutex::new(Some(iter))),
        }
    }
}

#[napi]
impl FossicCausationIter {
    #[napi]
    pub async fn raw_next(&self) -> Result<Option<StoredEventJs>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            match guard.as_mut() {
                None => Ok(None),
                Some(iter) => match iter.next() {
                    None => {
                        *guard = None;
                        Ok(None)
                    }
                    Some(Ok(ev)) => Ok(Some(StoredEventJs::from(&ev))),
                    Some(Err(e)) => {
                        *guard = None;
                        Err(Error::new(Status::GenericFailure, e.to_string()))
                    }
                },
            }
        })
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?
    }
}
