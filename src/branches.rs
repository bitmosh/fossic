use crate::{
    error::Error,
    schema::now_us,
    types::{BranchInfo, CreateBranch},
};
use rusqlite::{Connection, OptionalExtension};

// ── BranchSegment ─────────────────────────────────────────────────────────────

/// One segment in a resolved branch ancestor chain.
///
/// Events from this segment: `WHERE branch = branch_id AND version <= to_version`
/// (or all events if `to_version` is `None` — the terminal branch in the chain).
#[derive(Debug, Clone)]
pub struct BranchSegment {
    pub branch_id: String,
    /// Inclusive upper bound. `None` means no upper bound (terminal branch).
    pub to_version: Option<u64>,
}

// ── Validation ────────────────────────────────────────────────────────────────

fn validate_branch_id(id: &str) -> Result<(), Error> {
    if id.is_empty() {
        return Err(Error::InvalidBranchId {
            id: id.to_string(),
            reason: "cannot be empty".into(),
        });
    }
    if id.len() > 128 {
        return Err(Error::InvalidBranchId {
            id: id.to_string(),
            reason: format!("max 128 chars, got {}", id.len()),
        });
    }
    for c in id.chars() {
        if !c.is_alphanumeric() && c != '-' && c != '_' && c != '/' {
            return Err(Error::InvalidBranchId {
                id: id.to_string(),
                reason: format!("invalid character '{c}'"),
            });
        }
    }
    // Reject leading/trailing/double slashes.
    if id.starts_with('/') || id.ends_with('/') || id.contains("//") {
        return Err(Error::InvalidBranchId {
            id: id.to_string(),
            reason: "no leading/trailing/consecutive slashes".into(),
        });
    }
    Ok(())
}

// ── CRUD operations ───────────────────────────────────────────────────────────

pub(crate) fn create_branch_impl(conn: &Connection, b: &CreateBranch) -> Result<(), Error> {
    if b.branch_id == "main" {
        return Err(Error::InvalidBranchId {
            id: "main".into(),
            reason: "'main' is reserved".into(),
        });
    }
    validate_branch_id(&b.branch_id)?;

    if let Some(ref alt) = b.alternatives {
        if !alt.is_array() {
            return Err(Error::InvalidAlternatives);
        }
    }

    let now = now_us();
    let alternatives_json = b.alternatives.as_ref().map(|a| a.to_string());

    conn.execute(
        "INSERT INTO branches \
         (id, stream_id, parent_id, parent_version, description, created_at, lifecycle, alternatives) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ephemeral', ?7)",
        rusqlite::params![
            b.branch_id,
            b.stream_id,
            b.parent_id,
            b.parent_version as i64,
            b.description,
            now,
            alternatives_json,
        ],
    )?;
    Ok(())
}

pub(crate) fn promote_branch_impl(
    conn: &Connection,
    stream_id: &str,
    branch_id: &str,
    reason: Option<&str>,
) -> Result<(), Error> {
    let lifecycle: Option<String> = conn
        .query_row(
            "SELECT lifecycle FROM branches WHERE stream_id = ?1 AND id = ?2",
            rusqlite::params![stream_id, branch_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::BranchNotFound {
            stream_id: stream_id.into(),
            branch_id: branch_id.into(),
        })?;

    match lifecycle.as_deref() {
        Some("promoted") => return Ok(()), // idempotent
        Some("dead_end") => {
            return Err(Error::BranchLifecycleError {
                reason: format!(
                    "branch '{branch_id}' is already marked dead_end and cannot be promoted"
                ),
            })
        }
        _ => {}
    }

    let now = now_us();
    conn.execute(
        "UPDATE branches SET lifecycle = 'promoted', closed_at = ?1, closed_reason = ?2 \
         WHERE stream_id = ?3 AND id = ?4",
        rusqlite::params![now, reason, stream_id, branch_id],
    )?;
    Ok(())
}

pub(crate) fn mark_branch_dead_end_impl(
    conn: &Connection,
    stream_id: &str,
    branch_id: &str,
    reason: Option<&str>,
) -> Result<(), Error> {
    let lifecycle: Option<String> = conn
        .query_row(
            "SELECT lifecycle FROM branches WHERE stream_id = ?1 AND id = ?2",
            rusqlite::params![stream_id, branch_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::BranchNotFound {
            stream_id: stream_id.into(),
            branch_id: branch_id.into(),
        })?;

    match lifecycle.as_deref() {
        Some("dead_end") => return Ok(()), // idempotent
        Some("promoted") => {
            return Err(Error::BranchLifecycleError {
                reason: format!(
                    "branch '{branch_id}' is already promoted and cannot be marked dead_end"
                ),
            })
        }
        _ => {}
    }

    let now = now_us();
    conn.execute(
        "UPDATE branches SET lifecycle = 'dead_end', closed_at = ?1, closed_reason = ?2 \
         WHERE stream_id = ?3 AND id = ?4",
        rusqlite::params![now, reason, stream_id, branch_id],
    )?;
    Ok(())
}

pub(crate) fn list_branches_impl(
    conn: &Connection,
    stream_id: &str,
) -> Result<Vec<BranchInfo>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, stream_id, parent_id, parent_version, description, created_at, \
         lifecycle, closed_at, closed_reason, alternatives \
         FROM branches WHERE stream_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![stream_id], |row| {
        let alt_json: Option<String> = row.get(9)?;
        Ok(BranchInfo {
            id: row.get(0)?,
            stream_id: row.get(1)?,
            parent_id: row.get(2)?,
            parent_version: row.get::<_, i64>(3)? as u64,
            description: row.get(4)?,
            created_at: row.get(5)?,
            lifecycle: row.get(6)?,
            closed_at: row.get(7)?,
            closed_reason: row.get(8)?,
            alternatives: alt_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

// ── Chain resolution ──────────────────────────────────────────────────────────

/// Resolve the ancestor chain for a branch from root to `branch_id`.
///
/// Returns segments in order: first element is the root (typically "main"),
/// last element is `branch_id`. Each segment's `to_version` is the fork point
/// where the next branch diverges; the last segment has `to_version = None`.
///
/// For "main": returns a single segment `[BranchSegment { branch_id: "main", to_version: None }]`.
///
/// The result is suitable for use in `BTreeMap` caching by the caller.
pub(crate) fn resolve_branch_chain(
    conn: &Connection,
    stream_id: &str,
    branch_id: &str,
) -> Result<Vec<BranchSegment>, Error> {
    if branch_id == "main" {
        return Ok(vec![BranchSegment {
            branch_id: "main".to_string(),
            to_version: None,
        }]);
    }

    // Walk from leaf to root, collecting (branch_id, to_version) pairs in reverse.
    // In the final chain, segment[i].to_version = the parent_version stored on segment[i+1].
    let mut chain_rev: Vec<(String, Option<u64>)> = Vec::new();
    let mut current = branch_id.to_string();
    // `fork_version` is the parent_version of the child that diverged from `current`.
    // For the leaf (branch_id itself), there's no fork below, so to_version = None.
    let mut fork_version: Option<u64> = None;

    loop {
        if current == "main" {
            chain_rev.push(("main".to_string(), fork_version));
            break;
        }

        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT parent_id, parent_version FROM branches WHERE stream_id = ?1 AND id = ?2",
                rusqlite::params![stream_id, current],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let (parent_id, parent_version) = row.ok_or_else(|| Error::BranchNotFound {
            stream_id: stream_id.to_string(),
            branch_id: current.clone(),
        })?;

        // The segment for `current` has to_version = fork_version (from the child below it).
        chain_rev.push((current.clone(), fork_version));
        // The segment for `parent_id` will have to_version = parent_version (where `current` forked).
        fork_version = Some(parent_version as u64);
        current = parent_id;
    }

    chain_rev.reverse();
    Ok(chain_rev
        .into_iter()
        .map(|(b, tv)| BranchSegment {
            branch_id: b,
            to_version: tv,
        })
        .collect())
}
