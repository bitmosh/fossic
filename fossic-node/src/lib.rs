mod iters;
mod store;
mod subscriptions;
mod types;

use napi_derive::napi;

pub use iters::{FossicCausationIter, FossicCorrelationIter, FossicRangeIter};
pub use store::Store;
pub use subscriptions::FossicSubscription;
pub use types::{
    AppendJs, BranchInfoJs, CreateBranchJs, EventId, OpenOptionsJs, ReadOutcomeJs, ReadQueryJs,
    SamplingModeJs, StreamInfoJs, StoredEventJs, SubscribeQueryJs, TruncationCursorJs,
};

/// Fossic v1 napi-rs Node.js binding.
///
/// Entry point: `Store.open(path, options?)` — returns a fully initialized `Store`.
/// All store methods are `async` (Promise-based). Versions are `BigInt`;
/// event IDs are `Uint8Array` (or hex strings where noted).
///
/// Re-exported for convenience — consumers can also import types directly.
#[napi]
pub fn fossic_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
