use crate::{
    error::Error,
    read::{row_to_event, PREFIXED_SELECT_COLS, SELECT_COLS},
    types::StoredEvent,
    EventId,
};
use rusqlite::Connection;

// ── WalkDirection ─────────────────────────────────────────────────────────────

/// Direction for `walk_causation`.
pub enum WalkDirection {
    /// Descendants: events whose `causation_id` is the start event (children, grandchildren…).
    Forward,
    /// Ancestors: follow the `causation_id` chain from start back toward the root.
    Backward,
    /// Union of forward and backward, ordered by minimum depth from the start.
    Both,
}

// ── Aggregate ─────────────────────────────────────────────────────────────────

/// Streaming aggregator over a set of events.
///
/// The consumer creates an instance with its initial state, passes it to
/// `Store::aggregate`, and receives `A::Output` after `finalize` is called.
pub trait Aggregate: Send + Sync + 'static {
    type Output;
    fn fold(&mut self, event: &StoredEvent);
    fn finalize(self) -> Self::Output;
}

/// Query parameters for `Store::aggregate`.
///
/// `stream_pattern` uses SQLite's `GLOB` operator: `*` matches any sequence of
/// characters (including `/`). `indexed_tags` filtering is left to the `Aggregate`
/// implementation — fossic does not synthesize tags from payloads.
pub struct AggregateQuery {
    pub stream_pattern: String,
    pub branch: String,
    pub event_type_filter: Option<String>,
    pub from_timestamp_us: Option<i64>,
    pub to_timestamp_us: Option<i64>,
}

impl Default for AggregateQuery {
    fn default() -> Self {
        AggregateQuery {
            stream_pattern: "*".to_string(),
            branch: "main".to_string(),
            event_type_filter: None,
            from_timestamp_us: None,
            to_timestamp_us: None,
        }
    }
}

// ── read_by_correlation ───────────────────────────────────────────────────────

pub(crate) fn read_by_correlation_impl(
    conn: &Connection,
    correlation_id: EventId,
) -> Result<Vec<StoredEvent>, Error> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE correlation_id = ?1 ORDER BY timestamp_us ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![correlation_id], row_to_event)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

// ── walk_causation ────────────────────────────────────────────────────────────

pub(crate) fn walk_causation_impl(
    conn: &Connection,
    start: EventId,
    direction: WalkDirection,
    max_depth: usize,
) -> Result<Vec<StoredEvent>, Error> {
    match direction {
        WalkDirection::Forward => walk_forward(conn, start, max_depth),
        WalkDirection::Backward => walk_backward(conn, start, max_depth),
        WalkDirection::Both => {
            let mut fwd = walk_forward(conn, start, max_depth)?;
            let bwd = walk_backward(conn, start, max_depth)?;
            // Merge: dedup by event id, forward results first.
            let fwd_ids: std::collections::HashSet<[u8; 32]> =
                fwd.iter().map(|e| *e.id.as_bytes()).collect();
            for e in bwd {
                if !fwd_ids.contains(e.id.as_bytes()) {
                    fwd.push(e);
                }
            }
            Ok(fwd)
        }
    }
}

fn walk_forward(
    conn: &Connection,
    start: EventId,
    max_depth: usize,
) -> Result<Vec<StoredEvent>, Error> {
    if max_depth == 0 {
        return Ok(Vec::new());
    }
    // Use PREFIXED_SELECT_COLS (events.id, ...) to avoid "ambiguous column name: id"
    // when the CTE `fwd` also has a column named `id`.
    // Column 12 (bfs_depth) is present but ignored by row_to_event (reads 0..11).
    let sql = format!(
        "WITH RECURSIVE fwd(id, depth) AS (
            SELECT id, 1 FROM events WHERE causation_id = ?1
            UNION
            SELECT e.id, f.depth + 1
            FROM events e
            INNER JOIN fwd f ON e.causation_id = f.id
            WHERE f.depth < ?2
        )
        SELECT {PREFIXED_SELECT_COLS}, MIN(fwd.depth) AS bfs_depth
        FROM events
        INNER JOIN fwd ON events.id = fwd.id
        GROUP BY events.id
        ORDER BY bfs_depth ASC, events.timestamp_us ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let depth_bound: i64 = i64::try_from(max_depth).unwrap_or(i64::MAX);
    let rows = stmt.query_map(
        rusqlite::params![start, depth_bound],
        row_to_event,
    )?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

fn walk_backward(
    conn: &Connection,
    start: EventId,
    max_depth: usize,
) -> Result<Vec<StoredEvent>, Error> {
    if max_depth == 0 {
        return Ok(Vec::new());
    }
    let sql = format!(
        "WITH RECURSIVE bwd(ancestor_id, depth) AS (
            SELECT causation_id, 1
            FROM events
            WHERE id = ?1 AND causation_id IS NOT NULL
            UNION
            SELECT e.causation_id, b.depth + 1
            FROM events e
            INNER JOIN bwd b ON e.id = b.ancestor_id
            WHERE e.causation_id IS NOT NULL AND b.depth < ?2
        )
        SELECT {PREFIXED_SELECT_COLS}, depths.bfs_depth
        FROM events
        INNER JOIN (
            SELECT ancestor_id AS id, MIN(depth) AS bfs_depth
            FROM bwd
            GROUP BY ancestor_id
        ) AS depths ON events.id = depths.id
        ORDER BY depths.bfs_depth ASC, events.timestamp_us ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let depth_bound: i64 = i64::try_from(max_depth).unwrap_or(i64::MAX);
    let rows = stmt.query_map(
        rusqlite::params![start, depth_bound],
        row_to_event,
    )?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

// ── aggregate ─────────────────────────────────────────────────────────────────

pub(crate) fn aggregate_impl<A: Aggregate>(
    conn: &Connection,
    query: AggregateQuery,
    mut agg: A,
) -> Result<A::Output, Error> {
    // Use NULL-guard pattern so all 5 params are always bound.
    // (?3 IS NULL OR event_type = ?3) short-circuits when filter is absent.
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE stream_id GLOB ?1 \
         AND branch = ?2 \
         AND (?3 IS NULL OR event_type = ?3) \
         AND (?4 IS NULL OR timestamp_us >= ?4) \
         AND (?5 IS NULL OR timestamp_us <= ?5) \
         ORDER BY timestamp_us ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![
            query.stream_pattern,
            query.branch,
            query.event_type_filter,
            query.from_timestamp_us,
            query.to_timestamp_us,
        ],
        row_to_event,
    )?;
    for row in rows {
        let event = row?;
        agg.fold(&event);
    }
    Ok(agg.finalize())
}
