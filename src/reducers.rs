use crate::{error::Error, types::SnapshotPolicy};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

// ── Traits ────────────────────────────────────────────────────────────────────

/// Marker trait for types that can be used as reducer state.
///
/// Automatically implemented for any type satisfying the bounds.
pub trait ReducerState: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {}

impl<T> ReducerState for T where T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {}

/// A pure synchronous reducer. Structural constraints enforced by the trait:
///
/// - `&self` (not `&mut self`) — reducers are stateless; all state lives in `State`.
/// - No async — `apply` is synchronous.
/// - No I/O methods exposed — reducers cannot make system calls.
pub trait Reducer: Send + Sync + 'static {
    type State: ReducerState;
    type Event: DeserializeOwned;

    const NAME: &'static str;
    const VERSION: u32;
    const STATE_SCHEMA_VERSION: u32;

    fn initial_state(&self) -> Self::State;

    /// Apply a single event to state. Returns the new state.
    /// Must be a pure function: no I/O, no mutation of `self`, no randomness.
    fn apply(&self, state: Self::State, event: &Self::Event) -> Self::State;
}

// ── Type-erased reducer ───────────────────────────────────────────────────────

pub(crate) trait BoxedReducer: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> u32;
    fn state_schema_version(&self) -> u32;
    /// Serialize the initial state to msgpack.
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error>;
    /// Apply an event (msgpack payload bytes) to state (msgpack bytes).
    /// Returns the new state as msgpack bytes.
    fn apply_bytes(&self, state_bytes: &[u8], event_payload: &[u8]) -> Result<Vec<u8>, Error>;
}

/// A dyn-safe reducer bridge for foreign-language reducers (Python, JS, etc.).
pub trait DynReducer: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> u32;
    fn state_schema_version(&self) -> u32;
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error>;
    fn apply_bytes(&self, state_bytes: &[u8], event_payload: &[u8]) -> Result<Vec<u8>, Error>;
}

struct ErasedReducer<R: Reducer> {
    reducer: R,
}

impl<R: Reducer> BoxedReducer for ErasedReducer<R> {
    fn name(&self) -> &str {
        R::NAME
    }
    fn version(&self) -> u32 {
        R::VERSION
    }
    fn state_schema_version(&self) -> u32 {
        R::STATE_SCHEMA_VERSION
    }
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error> {
        rmp_serde::to_vec(&self.reducer.initial_state()).map_err(Error::MsgpackEncode)
    }
    fn apply_bytes(&self, state_bytes: &[u8], event_payload: &[u8]) -> Result<Vec<u8>, Error> {
        let state: R::State =
            rmp_serde::from_slice(state_bytes).map_err(Error::MsgpackDecode)?;
        let event: R::Event =
            rmp_serde::from_slice(event_payload).map_err(Error::MsgpackDecode)?;
        let new_state = self.reducer.apply(state, &event);
        rmp_serde::to_vec(&new_state).map_err(Error::MsgpackEncode)
    }
}

// ── DynReducer adapter ────────────────────────────────────────────────────────

struct DynReducerAdapter {
    reducer: Box<dyn DynReducer>,
}

impl BoxedReducer for DynReducerAdapter {
    fn name(&self) -> &str {
        self.reducer.name()
    }
    fn version(&self) -> u32 {
        self.reducer.version()
    }
    fn state_schema_version(&self) -> u32 {
        self.reducer.state_schema_version()
    }
    fn initial_state_bytes(&self) -> Result<Vec<u8>, Error> {
        self.reducer.initial_state_bytes()
    }
    fn apply_bytes(&self, state_bytes: &[u8], event_payload: &[u8]) -> Result<Vec<u8>, Error> {
        self.reducer.apply_bytes(state_bytes, event_payload)
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub(crate) struct ReducerEntry {
    pub(crate) pattern: String,
    pub(crate) specificity: usize,
    pub(crate) reducer: Arc<dyn BoxedReducer>,
    pub(crate) policy: SnapshotPolicy,
}

/// In-memory registry of pattern-based reducers.
#[derive(Default)]
pub(crate) struct ReducerRegistry {
    entries: Vec<ReducerEntry>,
}

impl ReducerRegistry {
    /// Register a reducer for the given glob pattern with `SnapshotPolicy::Manual`.
    pub fn register<R: Reducer>(&mut self, pattern: &str, reducer: R) -> Result<(), Error> {
        self.register_with_policy(pattern, reducer, SnapshotPolicy::Manual)
    }

    /// Register a reducer with an explicit snapshot policy.
    pub fn register_with_policy<R: Reducer>(
        &mut self,
        pattern: &str,
        reducer: R,
        policy: SnapshotPolicy,
    ) -> Result<(), Error> {
        validate_snapshot_policy(&policy)?;
        let spec = crate::glob::specificity_score(pattern);
        for existing in &self.entries {
            if patterns_may_overlap(pattern, &existing.pattern) && spec == existing.specificity {
                return Err(Error::ReducerPatternAmbiguous {
                    a: existing.pattern.clone(),
                    b: pattern.to_string(),
                });
            }
        }
        self.entries.push(ReducerEntry {
            pattern: pattern.to_string(),
            specificity: spec,
            reducer: Arc::new(ErasedReducer { reducer }),
            policy,
        });
        Ok(())
    }

    /// Find the most-specific reducer matching `stream_id`.
    pub fn find_arc(&self, stream_id: &str) -> Option<Arc<dyn BoxedReducer>> {
        self.find_arc_with_policy(stream_id)
            .map(|(arc, _)| arc)
    }

    /// Find the most-specific reducer + its policy matching `stream_id`.
    pub fn find_arc_with_policy(
        &self,
        stream_id: &str,
    ) -> Option<(Arc<dyn BoxedReducer>, SnapshotPolicy)> {
        let mut best: Option<&ReducerEntry> = None;
        for entry in &self.entries {
            if crate::glob::matches(&entry.pattern, stream_id) {
                match &best {
                    None => best = Some(entry),
                    Some(b) if entry.specificity > b.specificity => best = Some(entry),
                    _ => {}
                }
            }
        }
        best.map(|e| (Arc::clone(&e.reducer), e.policy.clone()))
    }

    /// Returns all `(reducer_name, state_schema_version)` pairs for GC filtering.
    pub fn active_keys(&self) -> Vec<(String, u32)> {
        self.entries
            .iter()
            .map(|e| {
                (
                    e.reducer.name().to_string(),
                    e.reducer.state_schema_version(),
                )
            })
            .collect()
    }

    /// Register a DynReducer for the given glob pattern with `SnapshotPolicy::Manual`.
    pub fn register_dyn(&mut self, pattern: &str, reducer: Box<dyn DynReducer>) -> Result<(), Error> {
        self.register_dyn_with_policy(pattern, reducer, SnapshotPolicy::Manual)
    }

    /// Register a DynReducer with an explicit snapshot policy.
    pub fn register_dyn_with_policy(
        &mut self,
        pattern: &str,
        reducer: Box<dyn DynReducer>,
        policy: SnapshotPolicy,
    ) -> Result<(), Error> {
        validate_snapshot_policy(&policy)?;
        let spec = crate::glob::specificity_score(pattern);
        for existing in &self.entries {
            if patterns_may_overlap(pattern, &existing.pattern) && spec == existing.specificity {
                return Err(Error::ReducerPatternAmbiguous {
                    a: existing.pattern.clone(),
                    b: pattern.to_string(),
                });
            }
        }
        self.entries.push(ReducerEntry {
            pattern: pattern.to_string(),
            specificity: spec,
            reducer: Arc::new(DynReducerAdapter { reducer }),
            policy,
        });
        Ok(())
    }

    /// Find a registered reducer by its exact name.
    pub fn find_by_name(&self, name: &str) -> Option<Arc<dyn BoxedReducer>> {
        self.entries
            .iter()
            .find(|e| e.reducer.name() == name)
            .map(|e| Arc::clone(&e.reducer))
    }
}

// ── Snapshot policy validation ─────────────────────────────────────────────────

pub(crate) fn validate_snapshot_policy(policy: &SnapshotPolicy) -> Result<(), Error> {
    match policy {
        SnapshotPolicy::Manual => Ok(()),
        SnapshotPolicy::EveryNEvents(0) => Err(Error::SnapshotPolicyInvalid(
            "EveryNEvents requires N >= 1".into(),
        )),
        SnapshotPolicy::EveryNEvents(_) => Ok(()),
        SnapshotPolicy::EveryNSeconds(_) => Err(Error::NotImplemented {
            feature: "SnapshotPolicy::EveryNSeconds (Phase 7 dependency — not yet available)",
        }),
        SnapshotPolicy::StateAdaptive { .. } => Err(Error::NotImplemented {
            feature: "SnapshotPolicy::StateAdaptive (v1.2.1 dependency — not yet available)",
        }),
    }
}

// ── Pattern utilities ─────────────────────────────────────────────────────────

/// Returns `true` if there is any stream_id that could match both patterns.
fn patterns_may_overlap(a: &str, b: &str) -> bool {
    let a_parts: Vec<&str> = a.split('/').collect();
    let b_parts: Vec<&str> = b.split('/').collect();

    let a_has_dstar = a_parts.contains(&"**");
    let b_has_dstar = b_parts.contains(&"**");

    // Without **, segment counts must match.
    if !a_has_dstar && !b_has_dstar && a_parts.len() != b_parts.len() {
        return false;
    }

    // Without **, check for literal conflicts position by position.
    if !a_has_dstar && !b_has_dstar {
        return a_parts
            .iter()
            .zip(b_parts.iter())
            .all(|(ap, bp)| *ap == "*" || *bp == "*" || ap == bp);
    }

    // With **: conservatively assume overlap.
    true
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star_single_segment() {
        assert!(crate::glob::matches("cerebra/lattice/*", "cerebra/lattice/abc"));
        assert!(!crate::glob::matches("cerebra/lattice/*", "cerebra/lattice/abc/sub"));
        assert!(!crate::glob::matches("cerebra/lattice/*", "cerebra/other/abc"));
    }

    #[test]
    fn glob_double_star_any_segments() {
        assert!(crate::glob::matches("cerebra/**", "cerebra/lattice/abc"));
        assert!(crate::glob::matches("cerebra/**", "cerebra/lattice"));
        assert!(crate::glob::matches("cerebra/**", "cerebra"));
        assert!(!crate::glob::matches("cerebra/**", "other/lattice"));
    }

    #[test]
    fn glob_exact_match() {
        assert!(crate::glob::matches("policy-scout/audit", "policy-scout/audit"));
        assert!(!crate::glob::matches("policy-scout/audit", "policy-scout/other"));
    }

    #[test]
    fn specificity_ordering() {
        assert!(
            crate::glob::specificity_score("cerebra/lattice/*")
                > crate::glob::specificity_score("cerebra/*")
        );
        assert_eq!(
            crate::glob::specificity_score("cerebra/lattice/*"),
            crate::glob::specificity_score("cerebra/lattice/**")
        );
    }

    #[test]
    fn ambiguity_same_specificity() {
        assert!(patterns_may_overlap("cerebra/lattice/*", "cerebra/lattice/*"));
    }

    #[test]
    fn no_overlap_different_literals() {
        assert!(!patterns_may_overlap("cerebra/lattice/*", "rhyzome/repair/*"));
    }
}
