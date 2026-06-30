# Cerebra Read Adapter for lattica-es

*Design document — drafted 2026-06-12. Specifies what the Cerebra-side adapter needs to do, how Cerebra's existing event vocabulary maps to lattica-es event types, and what constraints Cerebra places on the adapter's behavior. Companion to ADR-002 (which establishes that lattica-es uses read adapters rather than migrating module-local event stores). Reference document for cross-Claude coordination during lattica-es design.*

---

## Purpose and scope

ADR-002 establishes that lattica-es does not replace Cerebra's `inspector_events` table. The table remains authoritative for Cerebra's internal cognitive operations. lattica-es instead provides a **read adapter** that surfaces `inspector_events` rows into the unified event vocabulary that Lattica's cross-module timeline and time-travel viewer consume.

This document specifies what the Cerebra-side of that adapter needs to provide. It is not a complete adapter specification — the lattica-es-side has its own concerns (registration, polling vs subscription, projection integration) that are out of scope here. What this document covers is everything the adapter implementer needs to know about Cerebra's event store, vocabulary, and semantic constraints.

The document is written to be passed to whoever is designing lattica-es's adapter layer. It is not an implementation specification for Cerebra itself — Cerebra continues to write to `inspector_events` exactly as it does today, with no schema changes and no API changes required for the adapter to work.

## What Cerebra's event store currently looks like

Cerebra's event store is a single SQLite table named `inspector_events`, accessed via Cerebra's Python code through the `cerebra/inspector/` module. The store has been the substrate for cognitive audit throughout Phases 1-5 of Cerebra's development and is the authoritative record of every cognitive operation Cerebra has performed.

The table's current schema, as of Migration009 with Phase 5 closed at v0.2.7:

```sql
CREATE TABLE inspector_events (
    event_id        TEXT PRIMARY KEY,
    event_type      TEXT NOT NULL,
    subject_type    TEXT NOT NULL,
    subject_id      TEXT NOT NULL,
    session_id      TEXT,
    occurred_at     INTEGER NOT NULL,
    data_json       TEXT NOT NULL,
    schema_version  INTEGER NOT NULL DEFAULT 1
);
```

Several properties matter for adapter design.

The `event_id` is currently a UUID generated at write time. It is not content-addressed in the lattica-es sense; two semantically identical events would have different event_ids. This means the adapter must compute synthetic content-addressed IDs at read time rather than using event_id directly.

The `subject_type` and `subject_id` together identify what the event is about. Cerebra's subject types include source, chunk, record, working_memory_item, tower_item, session, query, lattice_lineage, and others. The subject identifies the entity the event acted on, not the operation. This maps to lattica-es's notion of a stream (the entity being affected) plus the event itself.

The `session_id` is nullable. Events that happen outside an active session (ingestion, classifier operations, system-level events) have NULL session_id. Events within session context (working memory, tower, context) have a non-NULL session_id. This is a Cerebra-internal distinction; lattica-es may not care about it directly but should preserve it in the unified vocabulary so downstream consumers can filter session-scoped vs system-scoped activity.

The `occurred_at` is a Unix epoch in seconds (integer). Cerebra does not store millisecond precision in this column. This is a known limitation and is fine for cognitive operations (which happen at human-perceivable timescales), but the adapter should be aware that timestamp resolution is coarser than what lattica-es native events may provide.

The `data_json` is a JSON-encoded string containing event-type-specific payload. The schema of `data_json` is per-event-type and is documented elsewhere (cerebra/inspector/event.py defines the event types and their expected payload shapes). The adapter must parse `data_json` per event type to extract structured data.

The `schema_version` is currently 1 for all events. Future Cerebra migrations may bump this if event payload schemas change. The adapter must handle multiple schema versions gracefully (probably via a per-event-type upcaster registry, mirroring lattica-es's upcaster pattern).

## Event vocabulary inventory

Cerebra emits the following event types, organized by the cognitive phase that introduced them. The list is current as of v0.2.7 (Phase 5 close + Lattice Step 1) and will grow as further phases ship.

The structure of each entry: the Cerebra event_type name, what cognitive operation produced it, what subject_type it carries, what the data_json payload contains, and the proposed unified-vocabulary mapping.

### Phase 1-2 events (source ingestion and classification)

**SourceIngested**
- Produced when a source document enters the vault via `cerebra ingest`
- Subject type: `source`, subject_id: source_id (a UUID)
- Payload: `{path, content_hash, parse_warnings, chunks_created, token_count}`
- Proposed unified type: `source_ingested`

**ChunkExtracted**
- Produced for each chunk derived from a source during ingestion
- Subject type: `chunk`, subject_id: chunk_id
- Payload: `{source_id, position, content_hash, token_estimate, chunk_text}`
- Proposed unified type: `chunk_extracted`

**RecordClassified**
- Produced when a chunk gets its SKU classification (top-1 commit)
- Subject type: `record`, subject_id: record_id
- Payload: `{chunk_id, sku_address, confidence, classifier_version, model}`
- Proposed unified type: `record_classified`

**ClassificationDistributionRecorded**
- Produced alongside RecordClassified, captures the full distribution
- Subject type: `chunk`, subject_id: chunk_id
- Payload: `{distribution: {category_d1: confidence}, top_1, threshold}`
- Proposed unified type: `classification_distribution_recorded`
- Note: this event is the substrate for the dark matter / shadow capture work; the adapter should preserve the full distribution

### Phase 3 events (storage and index layer)

**EmbeddingComputed**
- Produced when a record gets vector-embedded
- Subject type: `record`, subject_id: record_id
- Payload: `{embedding_model, model_version, dim, latency_ms}`
- Proposed unified type: `embedding_computed`

**LexicalIndexed**
- Produced when a record enters FTS5 lexical index
- Subject type: `record`, subject_id: record_id
- Payload: `{indexer_version, tokens_count}`
- Proposed unified type: `lexical_indexed`

**GraphEdgeAdded**
- Produced when a structural or semantic edge is written
- Subject type: `graph_edge`, subject_id: edge_id
- Payload: `{from_node_id, to_node_id, edge_type, edge_subtype}`
- Proposed unified type: `graph_edge_added`

**IndexStateUpdated**
- Produced when an index's freshness state changes
- Subject type: `index`, subject_id: index_name
- Payload: `{old_state, new_state, records_affected}`
- Proposed unified type: `index_state_updated`

### Phase 4 events (retrieval orchestration)

**QueryReceived**
- Produced when a query enters the retrieval pipeline
- Subject type: `query`, subject_id: query_id
- Payload: `{query_text, query_source, vault_path}`
- Proposed unified type: `query_received`

**QueryPlanned**
- Produced after the planner classifies the query into D1 categories
- Subject type: `query`, subject_id: query_id
- Payload: `{sku_d1, mode, keywords_matched, planner_version}`
- Proposed unified type: `query_planned`

**TraversalStepCompleted**
- Produced after each of the six traversal steps in retrieval
- Subject type: `query`, subject_id: query_id (same as parent QueryPlanned)
- Payload: `{step_number, step_name, candidates_added, candidates_total}`
- Proposed unified type: `traversal_step_completed`
- Note: six of these fire per query; lattica-es should probably treat them as span events on the query span rather than separate top-level events

**SalienceScored**
- Produced after composite salience is computed for all candidates
- Subject type: `query`, subject_id: query_id
- Payload: `{candidates_scored, weight_profile, score_floor}`
- Proposed unified type: `salience_scored`

**ContextPacketBuilt**
- Produced when the final ContextPacket is assembled
- Subject type: `query`, subject_id: query_id
- Payload: `{packet_id, selected_count, packet_version, abstained}`
- Proposed unified type: `context_packet_built`

**RetrievalAbstained**
- Produced when retrieval refuses to commit (best score below floor)
- Subject type: `query`, subject_id: query_id
- Payload: `{best_score_seen, floor_used, abstention_rationale}`
- Proposed unified type: `retrieval_abstained`

### Phase 5 events (working memory and truth tower)

**WorkingMemoryCreated**
- Produced when a new session starts and gets its working memory
- Subject type: `session`, subject_id: session_id
- Payload: `{vault_path, slot_capacities}`
- Proposed unified type: `working_memory_created`

**AttentionItemProposed**
- Produced before a working memory item insert is committed
- Subject type: `working_memory_item`, subject_id: wm_item_id
- Payload: `{session_id, slot_type, record_id, salience_score, is_synthetic}`
- Proposed unified type: `attention_item_proposed`

**AttentionItemPromoted**
- Produced after successful working memory insert
- Subject type: `working_memory_item`, subject_id: wm_item_id
- Payload: `{session_id, slot_type, record_id, salience_score, is_pinned}`
- Proposed unified type: `attention_item_promoted`

**AttentionItemEvicted**
- Produced when a working memory item is evicted
- Subject type: `working_memory_item`, subject_id: wm_item_id
- Payload: `{session_id, slot_type, reason, was_tower_cited}`
- Proposed unified type: `attention_item_evicted`

**AttentionItemDeferred**
- Produced when a promotion is rejected because slot is full of pinned items
- Subject type: `working_memory_item`, subject_id: proposed_wm_item_id
- Payload: `{session_id, slot_type, record_id, reason}`
- Proposed unified type: `attention_item_deferred`

**TowerInitialized**
- Produced exactly once per session, on first T1 promotion
- Subject type: `session`, subject_id: session_id
- Payload: `{session_id, t1_capacity, t2_capacity}`
- Proposed unified type: `tower_initialized`

**TowerItemPromoted**
- Produced when an item is added to the truth tower (T1 or T2)
- Subject type: `tower_item`, subject_id: tower_item_id
- Payload: `{session_id, tier, record_id, wm_item_id, salience_score, is_pinned, t1_citation_id}`
- Proposed unified type: `tower_item_promoted`

**TowerCrossReferenceAdded**
- Produced when a T2 item cites a T1 item (atomic with T2 promotion)
- Subject type: `tower_item`, subject_id: t2_tower_item_id
- Payload: `{session_id, citing_t2_id, cited_t1_id}`
- Proposed unified type: `tower_cross_reference_added`

**TowerItemEvicted**
- Produced when a tower item is evicted
- Subject type: `tower_item`, subject_id: tower_item_id
- Payload: `{session_id, tier, reason, stale_t2_count}`
- Proposed unified type: `tower_item_evicted`

**TowerItemStaled**
- Produced per T2 item marked stale due to T1 eviction
- Subject type: `tower_item`, subject_id: t2_tower_item_id
- Payload: `{session_id, citing_t1_id, evicted_at}`
- Proposed unified type: `tower_item_staled`

**TowerRendered**
- Produced when render_chronological or to_tower_field runs
- Subject type: `session`, subject_id: session_id
- Payload: `{render_format, t1_count, t2_count, stale_count}`
- Proposed unified type: `tower_rendered`

**WorkingMemoryRendered**
- Produced by cerebra memory status
- Subject type: `session`, subject_id: session_id
- Payload: `{format, slot_counts}`
- Proposed unified type: `working_memory_rendered`

### Lattice events (post-Phase-5 architectural addition)

**LatticeCommit**
- Produced when a chunk multi-commits to multiple SKU categories
- Subject type: `chunk`, subject_id: chunk_id
- Payload: `{sibling_record_ids, sibling_count, confidence_distribution, threshold_used, classifier_top_1}`
- Proposed unified type: `lattice_commit`

**LatticeSiblingResolved**
- Will be produced once Lattice Step 2 ships; captures retrieval-time sibling routing
- Subject type: `lattice_lineage`, subject_id: lattice_lineage_id
- Payload: `{siblings_considered, winner_record_id, routing_basis, query_sku_d1, winner_sku_d1, query_id}`
- Proposed unified type: `lattice_sibling_resolved`
- Note: not yet in production (Lattice Step 2 prompt drafted but not implemented as of v0.2.7)

## Synthetic content-addressed ID derivation

lattica-es uses `blake3(event_type + payload + causation_id)` as the content-addressed event ID for native events. Cerebra's events do not have content-addressed IDs; they have UUIDs assigned at write time.

The adapter must produce stable, deterministic content-addressed IDs for Cerebra events so they participate correctly in the unified timeline. The proposed derivation:

```
synthetic_id = blake3(
    unified_event_type +    # the mapped type per the inventory above
    data_json +              # the original payload, byte-identical
    event_id                 # the original UUID, serving as causation surrogate
)
```

The original `event_id` serves as the causation surrogate because it is unique per event in Cerebra's store, even if not content-derived. Including it ensures that two events with identical payloads (which can happen — same query run twice produces identical query_received payloads) get different synthetic IDs. Without this, the adapter would produce duplicate IDs and corrupt the unified timeline.

Once derived, the synthetic_id is stable. Re-running the adapter on the same `inspector_events` row produces the same synthetic_id. This means consumers of the unified timeline can rely on IDs being durable — caching, deduplication, and cross-reference all work correctly.

The adapter must persist the mapping from `event_id → synthetic_id` somewhere so that adapter restarts don't produce different IDs for the same source rows. A simple SQLite table managed by the adapter is sufficient:

```sql
CREATE TABLE cerebra_event_id_mapping (
    event_id         TEXT PRIMARY KEY,
    synthetic_id     BLOB NOT NULL,
    adapter_version  INTEGER NOT NULL,
    mapped_at        INTEGER NOT NULL
);
```

If `adapter_version` ever changes (because the unified vocabulary mapping or derivation logic changes), the adapter can re-derive IDs and migrate the mapping. The `adapter_version` field is the version safety net.

## Stream identification for Cerebra events

lattica-es organizes events into streams. The natural stream identification for Cerebra events follows from the subject_type and subject_id:

- Events about a source go in stream `cerebra/source/<source_id>`
- Events about a chunk go in stream `cerebra/chunk/<chunk_id>`
- Events about a record go in stream `cerebra/record/<record_id>`
- Events about a session go in stream `cerebra/session/<session_id>`
- Events about a query go in stream `cerebra/query/<query_id>`
- Events about a working memory item go in stream `cerebra/wm/<wm_item_id>`
- Events about a tower item go in stream `cerebra/tower/<tower_item_id>`
- Events about a lattice lineage go in stream `cerebra/lattice/<lattice_lineage_id>`

The `cerebra/` prefix namespaces Cerebra's streams against other modules' streams in the unified timeline. Policy Scout's adapter would use `policy_scout/` prefix; LumaWeave's adapter would use `lumaweave/` prefix; native lattica-es streams would use their own naming convention.

This stream identification is consistent with how Cerebra's events naturally aggregate. A query's lifecycle (received → planned → traversal steps → scored → packet built) lives in one stream. A session's lifecycle (created → many working memory operations → tower operations → eventually closed) lives in one stream. The unified timeline can present any of these stream-shaped views to consumers.

## Causation and correlation

Many Cerebra events carry data that implicitly references other events. For example, ContextPacketBuilt's payload includes `packet_id`, but the relevant TraversalStepCompleted events that contributed candidates aren't directly linked except via the shared `query_id` subject. lattica-es supports causation_id as a first-class concept; the adapter should populate it where possible.

The proposed causation mapping for Cerebra events:

For query-lifecycle events (QueryReceived through ContextPacketBuilt), the causation_id is the synthetic_id of the QueryReceived event for that query. Every subsequent query-lifecycle event has the same causation_id, anchoring it to the query that initiated the chain.

For session-lifecycle events (WorkingMemoryCreated through TowerRendered), the causation_id is the synthetic_id of the WorkingMemoryCreated event for that session.

For tower events that cite specific T1 items (TowerCrossReferenceAdded, TowerItemStaled), the causation_id is the synthetic_id of the cited T1's TowerItemPromoted event.

For lattice events that resolve sibling routing (LatticeSiblingResolved, when Step 2 ships), the causation_id is the synthetic_id of the QueryReceived event for the query that triggered the resolution.

For ingestion-lifecycle events (SourceIngested → ChunkExtracted → RecordClassified → EmbeddingComputed → LexicalIndexed), the causation chain follows the natural data dependency: each event's causation_id is the synthetic_id of its immediate upstream event.

The adapter must compute these causation_ids by looking up prior events in its mapping table. This is straightforward for in-session events (the session_id provides natural scoping) but requires a small amount of join logic for cross-event lookups. The lookups are bounded — each event has at most one causation_id, and the lookup is by indexed columns.

## Event ordering and timestamp resolution

Cerebra's `occurred_at` is Unix epoch in seconds. lattica-es native events likely store millisecond or microsecond precision. The adapter must handle the resolution mismatch without introducing ordering bugs.

Within a single second, multiple Cerebra events can occur (the classifier writes RecordClassified and ClassificationDistributionRecorded back-to-back; the retrieval pipeline writes six TraversalStepCompleted events in sequence). The adapter must preserve their original ordering.

The proposed approach: use the original `event_id` UUID as a secondary sort key when `occurred_at` is identical. UUID generation includes a monotonic component that approximates ordering even when timestamps tie. This is not a perfect solution (UUIDs are not strictly monotonic) but it is good enough for Cerebra's actual write patterns, which don't produce true concurrent writes within a single second.

The adapter exposes the original `occurred_at` as the event's timestamp in the unified timeline. Downstream consumers (the time-travel viewer, the diff layer) see Cerebra events at second-precision and lattica-es native events at finer precision. This is acceptable; the cognitive operations Cerebra performs happen at human-perceivable timescales where second-precision is the right granularity.

## What the adapter must NOT do

Several behaviors are explicitly forbidden to preserve Cerebra's authority over its own event store.

The adapter must not write to `inspector_events`. It is strictly read-only against Cerebra's store. Any need to record adapter-specific state (the synthetic ID mapping, the adapter's last-read cursor, error states) goes in adapter-owned tables, not in Cerebra's tables.

The adapter must not modify any Cerebra schema. If a future Cerebra migration changes the inspector_events schema, the adapter must update to handle the new schema rather than the other way around.

The adapter must not assume real-time read latency. Cerebra is a CLI-invoked process; the inspector_events table may have writes happening intermittently rather than continuously. The adapter should poll or subscribe with appropriate intervals, not assume sub-second freshness. For the cross-module timeline, a polling interval of 1-5 seconds is acceptable; the time-travel viewer can tolerate looking back into a stream that hasn't quite caught up to the absolute current state.

The adapter must not deduplicate based on payload similarity. Two Cerebra events with identical data_json but different event_ids are different events — they happened separately, even if they look the same. The synthetic_id derivation handles this correctly via the event_id inclusion; the adapter should not add its own deduplication logic on top.

The adapter must not depend on Cerebra-specific Python imports. The adapter is part of lattica-es and may be implemented in Rust (with the adapter logic in the Rust core) or Python (with adapter logic in PyO3-exposed code). Either way, it reads inspector_events through standard SQLite queries, not through Cerebra's Python module imports. This decouples lattica-es's adapter from Cerebra's release cycle.

## What Cerebra commits to

In exchange for the adapter being non-invasive, Cerebra makes a few commitments back.

The inspector_events schema is stable. Cerebra may add new columns in future migrations but will not remove or rename existing columns. New columns will be nullable so the adapter can ignore them gracefully.

The event_type vocabulary is append-only. New event types may be added; existing event types will not be renamed or repurposed. If a Cerebra event_type is deprecated, it stops being emitted but the existing rows remain valid and the adapter mapping for them continues to work.

The data_json payload schemas are versioned via schema_version. If a payload schema changes, the schema_version increments and the adapter's upcaster registry can handle the migration. The current schema_version is 1 for all events; future increments will be communicated explicitly when they happen.

The list of event_type strings is enumerable. Cerebra's `cerebra/inspector/event.py` module defines the full set as Python constants. The adapter can read this file (or be passed the list at registration time) to know which event types to expect. Unknown event types should be logged but not crash the adapter; Cerebra may emit new types between adapter releases.

The naming convention for new event types follows the existing pattern: PascalCase with the cognitive operation as the noun (TowerItemPromoted, LatticeCommit, AttentionItemDeferred). The adapter can probably handle naming convention parsing if needed but should not have to.

## Migration path for the existing 745 records

When the adapter is first deployed, Cerebra's dev vault already contains thousands of `inspector_events` rows from past cognitive operations. The adapter must handle this backfill gracefully.

The proposed approach: on first run, the adapter performs a one-time backfill pass over all existing inspector_events rows, computing synthetic_ids and writing them to the mapping table. Subsequent runs only process new rows (events with `event_id` not in the mapping table).

The backfill must be resumable. If it fails partway, restart picks up where it left off. The adapter's mapping table can have a `backfill_complete` flag column or equivalent state to track this.

The backfill should be ordered by `occurred_at` ascending, so the unified timeline gets historical events in their original temporal order. Downstream consumers that care about ordering (the time-travel viewer especially) can rely on the events arriving in causal order during backfill.

Performance estimate: with ~5000 inspector_events rows in a typical dev vault and ~1ms per blake3 computation, the backfill completes in a few seconds. Not an operational concern.

## What this design does and does not specify

This document specifies the Cerebra-side contract: what events exist, what their payloads look like, what the mapping to the unified vocabulary should be, how synthetic IDs derive, what causation patterns make sense, what the adapter must and must not do.

This document does not specify the lattica-es-side implementation: how the adapter registers, where adapter code physically lives in the repo, how the polling-vs-subscription model works, how projections built on top of the unified vocabulary are organized, what the API surface for consumers of the unified stream looks like. Those decisions belong to lattica-es's design and may be made independently of Cerebra's constraints.

The two sides meet at the unified event vocabulary. Cerebra commits to producing events that map cleanly into that vocabulary; lattica-es commits to consuming the mapped events without requiring further structure from Cerebra. The vocabulary itself is the contract.

## Open questions for cross-Claude coordination

A few specific decisions need resolution between Cerebra-side and lattica-es-side. These are flagged for explicit attention during the lattica-es design pass.

The unified event vocabulary naming convention. This document proposes snake_case unified event types (lattice_commit, tower_item_promoted) versus Cerebra's PascalCase native types (LatticeCommit, TowerItemPromoted). The convention should be consistent across all modules' adapters. If lattica-es prefers a different convention (CamelCase, kebab-case, etc.), the proposal here can be revised to match. The naming should be locked before adapter implementation begins.

The treatment of TraversalStepCompleted events. Six of these fire per query as part of the retrieval pipeline. They are semantically span events on the query span rather than top-level cognitive events. lattica-es may want to model them as nested spans inside the query stream rather than as separate stream entries. The Cerebra-side payload is sufficient to either; the modeling decision belongs to lattica-es.

The OTel export semantics for Cerebra events. Per ADR-002, OTel GenAI span export is supported from Phase 6. For Cerebra events specifically, what becomes a span and what becomes a span event needs to be decided. A reasonable mapping: query streams become parent spans, traversal steps become child spans, individual cognitive events become span events on the appropriate span. The detailed mapping can be deferred to OTel implementation time but worth flagging as a decision point.

The adapter's update cadence. Polling vs SQLite WAL subscription vs periodic full scans. Cerebra writes are intermittent (CLI-invoked, not continuous), so high-frequency polling produces mostly empty results. A polling interval of 1-5 seconds is recommended as a starting point; the actual cadence depends on lattica-es's read-latency requirements which Cerebra does not need to know about.

The migration path for Lattice Step 2's not-yet-emitted events. LatticeSiblingResolved is in the vocabulary inventory above but has not yet been produced by Cerebra (Step 2 prompt drafted but not implemented as of v0.2.7). The adapter should be designed to handle event types that don't yet exist, so that when Cerebra ships Step 2 the adapter doesn't need updating. This is probably already the natural behavior of an event-type-keyed mapping table, but worth confirming.

---

*This document is intended to be passed to whoever is designing lattica-es's adapter layer. Questions about Cerebra's event store, vocabulary, or semantic constraints can be addressed by reading this document; questions about lattica-es's internal design or consumer API are out of scope here. A reader new to the ADR-002 architecture should read that ADR first for the broader context.*
