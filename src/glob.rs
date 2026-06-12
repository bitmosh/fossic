use crate::error::Error;

/// Returns `true` if `stream_id` matches the glob `pattern`.
///
/// - `*`  matches exactly one path segment (no `/`).
/// - `**` matches zero or more path segments.
pub fn matches(pattern: &str, stream_id: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = stream_id.split('/').collect();
    match_parts(&p, &s)
}

/// Number of leading literal (non-wildcard) path segments.
/// Higher score = more specific match.
pub fn specificity_score(pattern: &str) -> usize {
    pattern
        .split('/')
        .take_while(|s| *s != "*" && *s != "**")
        .count()
}

/// Validate a glob subscription pattern.
///
/// Rejects empty patterns, leading/trailing `/`, embedded whitespace, and
/// quote characters.
pub fn validate_pattern(pattern: &str) -> Result<(), Error> {
    if pattern.is_empty() {
        return Err(Error::InvalidStreamId {
            id: pattern.to_string(),
            reason: "pattern must not be empty".to_string(),
        });
    }
    if pattern.starts_with('/') || pattern.ends_with('/') {
        return Err(Error::InvalidStreamId {
            id: pattern.to_string(),
            reason: "pattern must not have a leading or trailing '/'".to_string(),
        });
    }
    for ch in pattern.chars() {
        if ch.is_whitespace() {
            return Err(Error::InvalidStreamId {
                id: pattern.to_string(),
                reason: "pattern must not contain whitespace".to_string(),
            });
        }
        if ch == '"' || ch == '\'' {
            return Err(Error::InvalidStreamId {
                id: pattern.to_string(),
                reason: "pattern must not contain quote characters".to_string(),
            });
        }
    }
    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn match_parts(p: &[&str], s: &[&str]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    if p[0] == "**" {
        for i in 0..=s.len() {
            if match_parts(&p[1..], &s[i..]) {
                return true;
            }
        }
        return false;
    }
    if s.is_empty() {
        return false;
    }
    (p[0] == "*" || p[0] == s[0]) && match_parts(&p[1..], &s[1..])
}
