// SPDX-License-Identifier: Apache-2.0
use crate::error::Error;

/// A transform that mutates a msgpack-encoded payload before CCE encoding.
///
/// Transforms are pure synchronous functions — no I/O, no state mutations.
/// A transform that needs to fail must return `Err`; the append fails atomically.
///
/// Multiple transforms registered against overlapping patterns chain in registration
/// order: each transform's output is the next transform's input.
pub trait PayloadTransform: Send + Sync + 'static {
    fn transform(&self, event_type: &str, payload: &[u8]) -> Result<Vec<u8>, Error>;
}

pub(crate) struct TransformEntry {
    pub pattern: String,
    pub transform: Box<dyn PayloadTransform>,
}

/// Returns `true` if `pattern` matches `stream_id`.
///
/// Delegates to `crate::glob::matches` — `*` matches one segment, `**` matches
/// zero or more segments. Consistent with the subscription glob system.
pub(crate) fn pattern_matches(pattern: &str, stream_id: &str) -> bool {
    crate::glob::matches(pattern, stream_id)
}

/// Apply all transforms whose pattern matches `stream_id`, chaining in registration order.
///
/// If no transforms match, the original bytes are returned without any copy.
pub(crate) fn apply_transforms(
    entries: &[TransformEntry],
    stream_id: &str,
    event_type: &str,
    mut payload_bytes: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    for entry in entries {
        if pattern_matches(&entry.pattern, stream_id) {
            payload_bytes = entry.transform.transform(event_type, &payload_bytes)?;
        }
    }
    Ok(payload_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_exact_match() {
        assert!(pattern_matches("policy-scout/audit", "policy-scout/audit"));
        assert!(!pattern_matches("policy-scout/audit", "policy-scout/other"));
    }

    #[test]
    fn pattern_wildcard_last_segment() {
        assert!(pattern_matches(
            "cerebra/lattice/*",
            "cerebra/lattice/abc123"
        ));
        assert!(!pattern_matches("cerebra/lattice/*", "cerebra/lattice"));
        assert!(!pattern_matches(
            "cerebra/lattice/*",
            "cerebra/other/abc123"
        ));
    }

    #[test]
    fn pattern_wildcard_first_segment() {
        assert!(pattern_matches(
            "*/agent-trace/events",
            "cerebra/agent-trace/events"
        ));
        assert!(!pattern_matches(
            "*/agent-trace/events",
            "cerebra/other/events"
        ));
    }

    #[test]
    fn pattern_no_match_segment_count() {
        assert!(!pattern_matches("a/b", "a/b/c"));
        assert!(!pattern_matches("a/b/c", "a/b"));
    }

    #[test]
    fn pattern_double_star() {
        assert!(pattern_matches("cerebra/**", "cerebra/lattice/abc"));
        assert!(pattern_matches(
            "cerebra/**",
            "cerebra/agent-trace/sess_123"
        ));
        assert!(pattern_matches("cerebra/**", "cerebra"));
        assert!(!pattern_matches("cerebra/**", "other/lattice/abc"));
    }
}
