// SPDX-License-Identifier: Apache-2.0
pub mod cce;
pub mod glob;
pub mod subscriptions;

mod append;
mod branches;
mod cross_stream;
mod cursors;
mod deletion;
mod error;
mod executor;
mod read;
mod reducers;
mod registry;
mod schema;
mod similarity;
mod snapshots;
mod store;
mod stream;
mod system_stream;
mod transforms;
mod types;
mod upcasters;
mod wal_watch;

pub use branches::BranchSegment;
pub use cross_stream::{Aggregate, AggregateQuery, WalkDirection};
pub use error::{CceError, Error};
pub use executor::{BackgroundExecutor, BacklogTask, TaskKind, TaskPriority};
pub use reducers::{DynReducer, Reducer, ReducerState};
pub use similarity::{SimilarityHit, SimilarityQuery, SimilaritySearchProvider};
pub use store::{CausationIter, CorrelationIter, RangeIter, Store};
pub use subscriptions::{
    SubscribeQuery, SubscriptionHandle, SubscriptionHandler, SubscriptionMode,
};
pub use system_stream::SystemStreamWriter;
pub use transforms::PayloadTransform;
pub use types::{
    Append, BranchInfo, BudgetKind, CheckpointMode, CreateBranch, EncryptionMode, EventId,
    FirstOpenPolicy, OpenOptions, ReadOutcome, ReadQuery, SamplingMode, SnapshotInfo,
    SnapshotPolicy, StoredEvent, StreamInfo, TruncationCursor, TruncationReason,
};
pub use upcasters::Upcaster;
