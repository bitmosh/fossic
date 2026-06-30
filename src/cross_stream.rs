use crate::{
    error::Error,
    read::{row_to_event, PREFIXED_SELECT_COLS, SELECT_COLS},
    types::{
        CursorInner, ReadOutcome, SamplingMode, StoredEvent, TruncationCursor, TruncationReason,
    },
    upcasters::{apply_upcaster, UpcasterRegistry},
    EventId,
};
use rusqlite::Connection;
use std::collections::HashSet;

// ── WalkDirection ─────────────────────────────────────────────────────────────

/// Direction for `walk_causation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
/// **Stream glob:** `stream_pattern` follows the same rules as subscriptions —
/// `*` matches one path segment, `**` matches zero or more segments.
/// SQL pre-filtering uses SQLite `GLOB` (an over-approximation); a Rust
/// post-filter enforces exact glob semantics so callers get the stream set
/// they expect.
///
/// **`indexed_tags_filter`:** optional flat-AND exact-match filter pushed down
/// to SQL. Each key-value pair in the object must match the stored tag exactly.
/// Supported value types: string, bool, integer, float, null. All pairs are
/// combined with AND; OR and range predicates belong in `Aggregate::fold`.
/// Keys must be alphanumeric + underscore (no dots, slashes, or quotes).
pub struct AggregateQuery {
    pub stream_pattern: String,
    pub branch: String,
    pub event_type_filter: Option<String>,
    pub from_timestamp_us: Option<i64>,
    pub to_timestamp_us: Option<i64>,
    /// Flat AND exact-match filter on `indexed_tags`. See type-level docs.
    pub indexed_tags_filter: Option<serde_json::Value>,
}

impl Default for AggregateQuery {
    fn default() -> Self {
        AggregateQuery {
            stream_pattern: "*".to_string(),
            branch: "main".to_string(),
            event_type_filter: None,
            from_timestamp_us: None,
            to_timestamp_us: None,
            indexed_tags_filter: None,
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

/// Bounded variant of `read_by_correlation_impl`. Orders by `id ASC` (32-byte BLOB
/// lexicographic) rather than `timestamp_us` so the cursor predicate
/// `id > last_seen_id` gives deterministic, exact resume.
///
/// Always includes at least one event even if its payload alone exceeds the byte budget.
/// `resume_after_id` is decoded from `CursorInner::Correlation::last_seen_id`.
pub(crate) fn read_by_correlation_bounded_impl(
    conn: &Connection,
    correlation_id: EventId,
    resume_after_id: Option<[u8; 32]>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ReadOutcome<Vec<StoredEvent>>, Error> {
    let corr_id_bytes = *correlation_id.as_bytes();
    let resume_eid: Option<EventId> = resume_after_id.map(EventId::from_bytes);

    // (?2 IS NULL OR id > ?2) — when resume_eid is None, rusqlite passes SQL NULL
    // and the IS NULL branch passes, giving an unconstrained lower bound.
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE correlation_id = ?1 AND (?2 IS NULL OR id > ?2) \
         ORDER BY id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![correlation_id, resume_eid], row_to_event)?;

    let mut events: Vec<StoredEvent> = Vec::new();
    let mut byte_count: usize = 0;

    for row in rows {
        let event = row?;
        let event_bytes = event.payload.len();

        let exceed_count = max_results.is_some_and(|n| events.len() >= n);
        let exceed_bytes =
            max_bytes.is_some_and(|b| !events.is_empty() && byte_count + event_bytes > b);

        if exceed_count || exceed_bytes {
            let last_seen_id = events.last().map(|e| *e.id.as_bytes()).unwrap_or([0u8; 32]);
            let cursor = TruncationCursor::encode(&CursorInner::Correlation {
                correlation_id: corr_id_bytes,
                last_seen_id,
            })?;
            let reason = if exceed_count {
                TruncationReason::ResultCount
            } else {
                TruncationReason::ByteSize
            };
            return Ok(ReadOutcome::Truncated {
                data: events,
                cursor: Some(cursor),
                reason,
            });
        }

        byte_count += event_bytes;
        events.push(event);
    }

    Ok(ReadOutcome::Complete(events))
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
    let rows = stmt.query_map(rusqlite::params![start, depth_bound], row_to_event)?;
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
    let rows = stmt.query_map(rusqlite::params![start, depth_bound], row_to_event)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

// ── walk_causation_bounded ────────────────────────────────────────────────────

/// Fetch all events whose `causation_id` is one of the `frontier` IDs — one BFS
/// level forward (children). Results ordered by `id ASC` for determinism.
fn bfs_expand_forward(conn: &Connection, frontier: &[[u8; 32]]) -> Result<Vec<StoredEvent>, Error> {
    if frontier.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=frontier.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE causation_id IN ({placeholders}) ORDER BY id ASC"
    );
    let eids: Vec<EventId> = frontier.iter().map(|b| EventId::from_bytes(*b)).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(eids.iter()), row_to_event)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

/// Fetch the parent events of the `frontier` set — one BFS level backward.
/// Each parent is the event whose `id` matches a frontier event's `causation_id`.
/// Results ordered by `id ASC` for determinism.
fn bfs_expand_backward(
    conn: &Connection,
    frontier: &[[u8; 32]],
) -> Result<Vec<StoredEvent>, Error> {
    if frontier.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=frontier.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {SELECT_COLS} FROM events \
         WHERE id IN ( \
             SELECT causation_id FROM events \
             WHERE id IN ({placeholders}) AND causation_id IS NOT NULL \
         ) ORDER BY id ASC"
    );
    let eids: Vec<EventId> = frontier.iter().map(|b| EventId::from_bytes(*b)).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(eids.iter()), row_to_event)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

/// Expand `frontier` in the requested direction, returning the next BFS level
/// deduplicated and sorted by `id ASC`.
fn expand_frontier(
    conn: &Connection,
    frontier: &[[u8; 32]],
    direction: &WalkDirection,
) -> Result<Vec<StoredEvent>, Error> {
    match direction {
        WalkDirection::Forward => bfs_expand_forward(conn, frontier),
        WalkDirection::Backward => bfs_expand_backward(conn, frontier),
        WalkDirection::Both => {
            let fwd = bfs_expand_forward(conn, frontier)?;
            let bwd = bfs_expand_backward(conn, frontier)?;
            let fwd_ids: HashSet<[u8; 32]> = fwd.iter().map(|e| *e.id.as_bytes()).collect();
            let mut combined = fwd;
            for e in bwd {
                if !fwd_ids.contains(e.id.as_bytes()) {
                    combined.push(e);
                }
            }
            combined.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
            Ok(combined)
        }
    }
}

/// Truncate a level's events to the sampling budget. Events must already be
/// sorted by `id ASC` (expand_frontier guarantees this). Always stable with
/// respect to id order — deterministic, not random.
fn apply_bfs_sampling(
    mut events: Vec<StoredEvent>,
    sampling: SamplingMode,
    max_depth: usize,
) -> Vec<StoredEvent> {
    let max_per_level = match sampling {
        SamplingMode::Exhaustive => return events,
        SamplingMode::BreadthFirst { max_per_level } => max_per_level,
        // Adaptive: distribute target_count evenly across max_depth levels.
        SamplingMode::Adaptive { target_count } => (target_count / max_depth.max(1)).max(1),
    };
    events.truncate(max_per_level);
    events
}

/// Bounded BFS causation walk. Cuts at level boundaries — always yields whole
/// BFS levels; the at-least-one guarantee means the first level is yielded even
/// if it alone exceeds the budget.
///
/// `resume` = `(frontier_ids, depth_consumed)` decoded from a `CursorInner::Causation`.
/// `frontier_ids` are the last-yielded level's event IDs; expand them to get the next level.
#[allow(clippy::too_many_arguments)]
pub(crate) fn walk_causation_bounded_impl(
    conn: &Connection,
    start: EventId,
    direction: &WalkDirection,
    max_depth: usize,
    sampling: SamplingMode,
    resume: Option<(Vec<[u8; 32]>, u32)>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ReadOutcome<Vec<StoredEvent>>, Error> {
    let dir_byte: u8 = match direction {
        WalkDirection::Forward => 0,
        WalkDirection::Backward => 1,
        WalkDirection::Both => 2,
    };

    // Initialize frontier and seen-id set.
    // `seen` prevents re-yielding nodes already returned (within this call).
    let (mut current_frontier, start_depth, mut seen) = match resume {
        Some((frontier, depth_consumed)) => {
            // Frontier IDs were already yielded; mark them seen so they aren't re-yielded
            // if the graph has convergent paths.
            let seen: HashSet<[u8; 32]> = frontier.iter().copied().collect();
            (frontier, (depth_consumed as usize) + 1, seen)
        }
        None => {
            let start_bytes = *start.as_bytes();
            let mut seen = HashSet::new();
            seen.insert(start_bytes);
            (vec![start_bytes], 1_usize, seen)
        }
    };

    let mut results: Vec<StoredEvent> = Vec::new();
    let mut byte_count: usize = 0;
    let mut depth = start_depth;

    while depth <= max_depth && !current_frontier.is_empty() {
        let mut level_events = expand_frontier(conn, &current_frontier, direction)?;

        // Dedup against already-seen nodes (handles convergent paths and resume).
        level_events.retain(|e| !seen.contains(e.id.as_bytes()));

        level_events = apply_bfs_sampling(level_events, sampling, max_depth);

        if level_events.is_empty() {
            break;
        }

        let level_count = level_events.len();
        let level_bytes: usize = level_events.iter().map(|e| e.payload.len()).sum();

        let would_exceed_count = max_results.is_some_and(|n| results.len() + level_count > n);
        let would_exceed_bytes = max_bytes.is_some_and(|b| byte_count + level_bytes > b);

        // Cut before this level only if we already have results (at-least-one guarantee).
        if (would_exceed_count || would_exceed_bytes) && !results.is_empty() {
            let reason = if would_exceed_count {
                TruncationReason::ResultCount
            } else {
                TruncationReason::ByteSize
            };
            let cursor = TruncationCursor::encode(&CursorInner::Causation {
                frontier: current_frontier,
                direction: dir_byte,
                depth_consumed: (depth - 1) as u32,
            })?;
            return Ok(ReadOutcome::Truncated {
                data: results,
                cursor: Some(cursor),
                reason,
            });
        }

        // Yield this level.
        for e in &level_events {
            seen.insert(*e.id.as_bytes());
        }
        byte_count += level_bytes;
        current_frontier = level_events.iter().map(|e| *e.id.as_bytes()).collect();
        results.extend(level_events);
        depth += 1;
    }

    Ok(ReadOutcome::Complete(results))
}

// ── aggregate ─────────────────────────────────────────────────────────────────

/// Returns `true` if `key` is safe to interpolate into a JSON path string.
/// Rejects anything that could escape `'$.{key}'` in the SQL literal.
fn is_safe_tag_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_')
}

pub(crate) fn aggregate_impl<A: Aggregate>(
    conn: &Connection,
    query: AggregateQuery,
    mut agg: A,
    upcasters: &UpcasterRegistry,
) -> Result<A::Output, Error> {
    // Build WHERE clauses and bound params dynamically so we can attach
    // arbitrary indexed_tags conditions without the ?N reuse limitation.
    let mut clauses: Vec<String> = vec!["stream_id GLOB ?".to_string(), "branch = ?".to_string()];
    // Using Box<dyn ToSql> so we can push heterogeneous param types.
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(query.stream_pattern.clone()),
        Box::new(query.branch.clone()),
    ];

    if let Some(ref et) = query.event_type_filter {
        clauses.push("event_type = ?".to_string());
        params.push(Box::new(et.clone()));
    }
    if let Some(from_ts) = query.from_timestamp_us {
        clauses.push("timestamp_us >= ?".to_string());
        params.push(Box::new(from_ts));
    }
    if let Some(to_ts) = query.to_timestamp_us {
        clauses.push("timestamp_us <= ?".to_string());
        params.push(Box::new(to_ts));
    }

    // indexed_tags_filter: flat AND, exact-match on JSON primitives.
    // Booleans are compared as integers (json_extract returns 1/0 for true/false).
    // Keys are validated to prevent SQL injection via the JSON path literal.
    if let Some(serde_json::Value::Object(map)) = &query.indexed_tags_filter {
        for (key, value) in map {
            if !is_safe_tag_key(key) {
                return Err(Error::Internal(format!(
                    "indexed_tags_filter key {key:?} must contain only letters, digits, and underscores"
                )));
            }
            let path = format!("$.{key}");
            match value {
                serde_json::Value::Null => {
                    clauses.push(format!("json_extract(indexed_tags, '{path}') IS NULL"));
                }
                serde_json::Value::Bool(b) => {
                    clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                    params.push(Box::new(if *b { 1i64 } else { 0i64 }));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                        params.push(Box::new(i));
                    } else if let Some(f) = n.as_f64() {
                        clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                        params.push(Box::new(f));
                    }
                }
                serde_json::Value::String(s) => {
                    clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                    params.push(Box::new(s.clone()));
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "indexed_tags_filter value for key {key:?} must be a JSON primitive (string, bool, number, or null)"
                    )));
                }
            }
        }
    }

    let sql = format!(
        "SELECT {SELECT_COLS} FROM events WHERE {} ORDER BY timestamp_us ASC",
        clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        row_to_event,
    )?;

    // Rust post-filter: SQLite GLOB treats `*` as any chars (including `/`),
    // but fossic glob treats `*` as one segment only. Apply our glob here so
    // callers get the stream set they expect regardless of SQLite's semantics.
    for row in rows {
        let event = apply_upcaster(upcasters, row?)?;
        if crate::glob::matches(&query.stream_pattern, &event.stream_id) {
            agg.fold(&event);
        }
    }
    Ok(agg.finalize())
}

/// Bounded variant of `aggregate_impl`.
///
/// Folds events until `max_events` have been scanned or `max_bytes` of payload have
/// accumulated. On truncation, clones the aggregator at the cut point and calls
/// `finalize()` on the clone to produce the `Truncated` result.
///
/// No resume cursor is produced (`cursor: None`). Fold-resume requires re-feeding
/// partial state into a new aggregator instance — not yet supported by `Aggregate`.
/// Deferred to v1.2.x.
pub(crate) fn aggregate_bounded_impl<A: Aggregate + Clone>(
    conn: &Connection,
    query: AggregateQuery,
    mut agg: A,
    upcasters: &UpcasterRegistry,
    max_events: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ReadOutcome<A::Output>, Error> {
    let mut clauses: Vec<String> = vec!["stream_id GLOB ?".to_string(), "branch = ?".to_string()];
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(query.stream_pattern.clone()),
        Box::new(query.branch.clone()),
    ];

    if let Some(ref et) = query.event_type_filter {
        clauses.push("event_type = ?".to_string());
        params.push(Box::new(et.clone()));
    }
    if let Some(from_ts) = query.from_timestamp_us {
        clauses.push("timestamp_us >= ?".to_string());
        params.push(Box::new(from_ts));
    }
    if let Some(to_ts) = query.to_timestamp_us {
        clauses.push("timestamp_us <= ?".to_string());
        params.push(Box::new(to_ts));
    }
    if let Some(serde_json::Value::Object(map)) = &query.indexed_tags_filter {
        for (key, value) in map {
            if !is_safe_tag_key(key) {
                return Err(Error::Internal(format!(
                    "indexed_tags_filter key {key:?} must contain only letters, digits, and underscores"
                )));
            }
            let path = format!("$.{key}");
            match value {
                serde_json::Value::Null => {
                    clauses.push(format!("json_extract(indexed_tags, '{path}') IS NULL"));
                }
                serde_json::Value::Bool(b) => {
                    clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                    params.push(Box::new(if *b { 1i64 } else { 0i64 }));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                        params.push(Box::new(i));
                    } else if let Some(f) = n.as_f64() {
                        clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                        params.push(Box::new(f));
                    }
                }
                serde_json::Value::String(s) => {
                    clauses.push(format!("json_extract(indexed_tags, '{path}') = ?"));
                    params.push(Box::new(s.clone()));
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "indexed_tags_filter value for key {key:?} must be a JSON primitive (string, bool, number, or null)"
                    )));
                }
            }
        }
    }

    let sql = format!(
        "SELECT {SELECT_COLS} FROM events WHERE {} ORDER BY timestamp_us ASC",
        clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        row_to_event,
    )?;

    let mut events_scanned: usize = 0;
    let mut byte_count: usize = 0;

    for row in rows {
        let event = apply_upcaster(upcasters, row?)?;
        if crate::glob::matches(&query.stream_pattern, &event.stream_id) {
            let event_bytes = event.payload.len();

            let exceed_count = max_events.is_some_and(|n| events_scanned >= n);
            // at-least-one: byte budget only fires after at least one event has been folded
            let exceed_bytes =
                max_bytes.is_some_and(|b| events_scanned > 0 && byte_count + event_bytes > b);

            if exceed_count || exceed_bytes {
                let reason = if exceed_count {
                    TruncationReason::ResultCount
                } else {
                    TruncationReason::ByteSize
                };
                let data = agg.clone().finalize();
                return Ok(ReadOutcome::Truncated {
                    data,
                    cursor: None,
                    reason,
                });
            }

            agg.fold(&event);
            events_scanned += 1;
            byte_count += event_bytes;
        }
    }

    Ok(ReadOutcome::Complete(agg.finalize()))
}
