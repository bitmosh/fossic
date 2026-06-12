use crate::{
    error::Error,
    schema::now_us,
    types::StreamInfo,
};
use rusqlite::Connection;

const MAX_STREAM_ID_LEN: usize = 256;
const MAX_STREAM_SEGMENTS: usize = 4;

/// Validate a stream ID against the rules in FOSSIC_V1_SPEC §6.
pub(crate) fn validate_stream_id(id: &str) -> Result<(), Error> {
    if id.is_empty() {
        return Err(Error::InvalidStreamId {
            id: id.to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    if id.len() > MAX_STREAM_ID_LEN {
        return Err(Error::InvalidStreamId {
            id: id.to_string(),
            reason: format!("exceeds {} character limit", MAX_STREAM_ID_LEN),
        });
    }
    for c in id.chars() {
        if !c.is_alphanumeric() && !matches!(c, '-' | '_' | '/') {
            return Err(Error::InvalidStreamId {
                id: id.to_string(),
                reason: format!("contains disallowed character '{}'", c),
            });
        }
    }
    let segments: Vec<&str> = id.split('/').collect();
    if segments.len() > MAX_STREAM_SEGMENTS {
        return Err(Error::InvalidStreamId {
            id: id.to_string(),
            reason: format!(
                "too many path segments ({}, max {})",
                segments.len(),
                MAX_STREAM_SEGMENTS
            ),
        });
    }
    for seg in &segments {
        if seg.is_empty() {
            return Err(Error::InvalidStreamId {
                id: id.to_string(),
                reason: "contains empty path segment (leading/trailing slash or double slash)"
                    .to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn declare_stream_impl(
    conn: &Connection,
    stream_id: &str,
    declared_by: &str,
    description: Option<&str>,
) -> Result<(), Error> {
    validate_stream_id(stream_id)?;
    conn.execute(
        "INSERT OR IGNORE INTO streams(id, declared_by, declared_at, description) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![stream_id, declared_by, now_us(), description],
    )?;
    Ok(())
}

pub(crate) fn stream_exists_impl(conn: &Connection, stream_id: &str) -> Result<bool, Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM streams WHERE id = ?1",
        rusqlite::params![stream_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

pub(crate) fn streams_impl(conn: &Connection) -> Result<Vec<StreamInfo>, Error> {
    let mut stmt = conn.prepare(
        // Exclude internal _fossic/* system streams from the user-facing list.
        "SELECT id, declared_by, declared_at, description FROM streams \
         WHERE id NOT LIKE '_fossic/%' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(StreamInfo {
            id: r.get(0)?,
            declared_by: r.get(1)?,
            declared_at: r.get(2)?,
            description: r.get(3)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ids() {
        for id in &[
            "events",
            "cerebra/lattice/abc123",
            "policy-scout/audit",
            "bo/conversation/channel_42",
            "a/b/c/d",
        ] {
            validate_stream_id(id).unwrap_or_else(|e| panic!("{} failed: {}", id, e));
        }
    }

    #[test]
    fn invalid_ids() {
        assert!(validate_stream_id("").is_err());
        assert!(validate_stream_id("a b").is_err());
        assert!(validate_stream_id("/leading").is_err());
        assert!(validate_stream_id("trailing/").is_err());
        assert!(validate_stream_id("a//b").is_err());
        assert!(validate_stream_id("a/b/c/d/e").is_err()); // 5 segments > 4
        assert!(validate_stream_id("has'quote").is_err());
        assert!(validate_stream_id(&"x".repeat(257)).is_err());
    }
}
