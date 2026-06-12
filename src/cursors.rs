//! Cursor API — lightweight consumer progress pointers.
//!
//! Cursors are scoped by `(consumer_id, stream_id, branch)` and store the last
//! successfully processed event version for a given consumer.
//!
//! **Note:** this table is a convenience facility for consumers that don't have
//! their own transactional sink. Consumers with transactional sinks (Postgres,
//! another SQLite database) should store cursors there instead — atomically
//! with their side effects — rather than using this API.

use crate::{error::Error, schema::now_us};
use rusqlite::Connection;

pub(crate) fn get_cursor_impl(
    conn: &Connection,
    consumer_id: &str,
    stream_id: &str,
    branch: &str,
) -> Result<Option<u64>, Error> {
    let result: rusqlite::Result<i64> = conn.query_row(
        "SELECT version FROM cursors \
         WHERE consumer_id = ?1 AND stream_id = ?2 AND branch = ?3",
        rusqlite::params![consumer_id, stream_id, branch],
        |r| r.get(0),
    );
    match result {
        Ok(v) => Ok(Some(v as u64)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Sqlite(e)),
    }
}

pub(crate) fn set_cursor_impl(
    conn: &Connection,
    consumer_id: &str,
    stream_id: &str,
    branch: &str,
    version: u64,
) -> Result<(), Error> {
    let now = now_us();
    conn.execute(
        "INSERT INTO cursors (consumer_id, stream_id, branch, version, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(consumer_id, stream_id, branch) \
         DO UPDATE SET version = excluded.version, updated_at = excluded.updated_at",
        rusqlite::params![consumer_id, stream_id, branch, version as i64, now],
    )?;
    Ok(())
}
