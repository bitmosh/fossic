use crate::{
    error::Error,
    types::{EventId, ReadQuery, StoredEvent},
};
use rusqlite::Connection;

pub(crate) fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    let indexed_tags_json: Option<String> = row.get(11)?;
    let indexed_tags = indexed_tags_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                11,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;

    Ok(StoredEvent {
        id: row.get(0)?,
        stream_id: row.get(1)?,
        branch: row.get(2)?,
        version: row.get::<_, i64>(3)? as u64,
        timestamp_us: row.get(4)?,
        causation_id: row.get(5)?,
        correlation_id: row.get(6)?,
        event_type: row.get(7)?,
        type_version: row.get::<_, i64>(8)? as u32,
        payload: row.get(9)?,
        external_id: row.get(10)?,
        indexed_tags,
    })
}

pub(crate) const SELECT_COLS: &str =
    "id, stream_id, branch, version, timestamp_us, causation_id, correlation_id, \
     event_type, type_version, payload, external_id, indexed_tags";

/// `SELECT_COLS` with `events.` prefix — use in JOIN queries to avoid
/// "ambiguous column name" errors when the joined subquery also has an `id` column.
pub(crate) const PREFIXED_SELECT_COLS: &str =
    "events.id, events.stream_id, events.branch, events.version, events.timestamp_us, \
     events.causation_id, events.correlation_id, events.event_type, events.type_version, \
     events.payload, events.external_id, events.indexed_tags";

pub(crate) fn read_range_impl(
    conn: &Connection,
    q: ReadQuery,
) -> Result<Vec<StoredEvent>, Error> {
    let from = q.from_version.unwrap_or(0) as i64;
    let to = q.to_version.map(|v| v as i64).unwrap_or(i64::MAX);
    let limit = q.limit.map(|l| l as i64).unwrap_or(i64::MAX);

    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE stream_id = ?1 AND branch = ?2 AND version >= ?3 AND version <= ?4 \
         AND (?6 IS NULL OR event_type = ?6) \
         ORDER BY version ASC LIMIT ?5"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![q.stream_id, q.branch, from, to, limit, q.event_type_filter],
        row_to_event,
    )?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

pub(crate) fn read_one_impl(
    conn: &Connection,
    id: EventId,
) -> Result<Option<StoredEvent>, Error> {
    let sql = format!("SELECT {SELECT_COLS} FROM events WHERE id = ?1");
    match conn.query_row(&sql, rusqlite::params![id], row_to_event) {
        Ok(event) => Ok(Some(event)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Sqlite(e)),
    }
}

/// Fetch multiple events by their CCE event IDs in a single query.
///
/// Results are ordered by `timestamp_us ASC`. IDs not found in the store are
/// silently omitted — callers that need to detect missing IDs should compare
/// the returned count against the input length.
///
/// **SQLite parameter limit:** SQLite allows at most 32,766 bound parameters
/// per statement. Callers are responsible for keeping batch sizes well below
/// this ceiling; a reasonable operational limit is ≤ 4,096 IDs per call.
/// Upcasters are applied by the `Store::read_batch` wrapper after this returns.
pub(crate) fn read_batch_impl(
    conn: &Connection,
    ids: &[EventId],
) -> Result<Vec<StoredEvent>, Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: String = (1..=ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE id IN ({placeholders}) \
         ORDER BY timestamp_us ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids), row_to_event)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

pub(crate) fn read_by_external_id_impl(
    conn: &Connection,
    stream_id: &str,
    external_id: &str,
) -> Result<Option<StoredEvent>, Error> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events WHERE stream_id = ?1 AND external_id = ?2 LIMIT 1"
    );
    match conn.query_row(&sql, rusqlite::params![stream_id, external_id], row_to_event) {
        Ok(event) => Ok(Some(event)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(Error::Sqlite(e)),
    }
}
