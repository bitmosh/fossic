use crate::{error::Error, schema::now_us, types::SnapshotInfo};
use rusqlite::{Connection, OptionalExtension};

// ── Snapshot lookup ───────────────────────────────────────────────────────────

/// Find the latest snapshot matching the given key, optionally bounded above by `max_version`.
///
/// Returns `(version, state_blob)` if found.
pub(crate) fn find_latest_snapshot(
    conn: &Connection,
    stream_id: &str,
    branch: &str,
    reducer_name: &str,
    state_schema_version: u32,
    max_version: Option<u64>,
) -> Result<Option<(u64, Vec<u8>)>, Error> {
    let result: Option<(i64, Vec<u8>)> = if let Some(mv) = max_version {
        conn.query_row(
            "SELECT version, state_blob FROM snapshots \
             WHERE stream_id = ?1 AND branch = ?2 \
             AND reducer_name = ?3 AND state_schema_version = ?4 \
             AND version <= ?5 \
             ORDER BY version DESC LIMIT 1",
            rusqlite::params![
                stream_id,
                branch,
                reducer_name,
                state_schema_version as i64,
                mv as i64,
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
    } else {
        conn.query_row(
            "SELECT version, state_blob FROM snapshots \
             WHERE stream_id = ?1 AND branch = ?2 \
             AND reducer_name = ?3 AND state_schema_version = ?4 \
             ORDER BY version DESC LIMIT 1",
            rusqlite::params![
                stream_id,
                branch,
                reducer_name,
                state_schema_version as i64,
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
    };

    Ok(result.map(|(v, b)| (v as u64, b)))
}

// ── Snapshot write ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_snapshot(
    conn: &Connection,
    stream_id: &str,
    branch: &str,
    version: u64,
    reducer_name: &str,
    reducer_version: u32,
    state_schema_version: u32,
    state_blob: &[u8],
) -> Result<SnapshotInfo, Error> {
    let now = now_us();
    conn.execute(
        "INSERT OR REPLACE INTO snapshots \
         (stream_id, branch, version, reducer_name, reducer_version, \
          state_schema_version, state_blob, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            stream_id,
            branch,
            version as i64,
            reducer_name,
            reducer_version as i64,
            state_schema_version as i64,
            state_blob,
            now,
        ],
    )?;
    Ok(SnapshotInfo {
        stream_id: stream_id.to_string(),
        branch: branch.to_string(),
        version,
        reducer_name: reducer_name.to_string(),
        reducer_version,
        state_schema_version,
        created_at: now,
    })
}

// ── Snapshot metadata ─────────────────────────────────────────────────────────

pub(crate) fn snapshot_info_impl(
    conn: &Connection,
    stream_id: &str,
    branch: &str,
    reducer_name: &str,
) -> Result<Option<SnapshotInfo>, Error> {
    conn.query_row(
        "SELECT version, reducer_version, state_schema_version, created_at \
         FROM snapshots \
         WHERE stream_id = ?1 AND branch = ?2 AND reducer_name = ?3 \
         ORDER BY version DESC LIMIT 1",
        rusqlite::params![stream_id, branch, reducer_name],
        |r| {
            Ok(SnapshotInfo {
                stream_id: stream_id.to_string(),
                branch: branch.to_string(),
                version: r.get::<_, i64>(0)? as u64,
                reducer_name: reducer_name.to_string(),
                reducer_version: r.get::<_, i64>(1)? as u32,
                state_schema_version: r.get::<_, i64>(2)? as u32,
                created_at: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Error::from)
}

// ── GC ────────────────────────────────────────────────────────────────────────

// CP-T2-1: When Phase 7 (Background Executor) lands, supplement
// this drop-time GC with quiescent-window scheduling via
// BackgroundExecutor::schedule(GcOrphanSnapshots, TaskPriority::Low).
// Keep this drop-time call as final-shutdown cleanup.

/// Delete snapshots whose `(reducer_name, state_schema_version)` is not in `active`.
///
/// Returns the number of rows deleted.
pub(crate) fn gc_orphaned_snapshots_impl(
    conn: &Connection,
    active: &[(String, u32)],
) -> Result<usize, Error> {
    if active.is_empty() {
        let count = conn.execute("DELETE FROM snapshots", [])?;
        return Ok(count);
    }

    // Find all distinct (reducer_name, state_schema_version) pairs in the snapshots table.
    let mut stmt = conn.prepare(
        "SELECT DISTINCT reducer_name, state_schema_version FROM snapshots",
    )?;
    let existing: Vec<(String, u32)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut total = 0usize;
    for (name, ver) in existing {
        if !active.iter().any(|(an, av)| an == &name && *av == ver) {
            let count = conn.execute(
                "DELETE FROM snapshots WHERE reducer_name = ?1 AND state_schema_version = ?2",
                rusqlite::params![name, ver as i64],
            )?;
            total += count;
        }
    }
    Ok(total)
}
