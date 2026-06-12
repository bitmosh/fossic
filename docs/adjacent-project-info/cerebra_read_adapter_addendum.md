# Cerebra Read Adapter — Addendum: Event-Sourced Aggregate Proposal

*Addendum to cerebra_read_adapter_design.md — drafted 2026-06-12 following an architectural insight that emerged in design conversation. Proposes that Cerebra's relationship with lattica-es extends beyond a read adapter for inspector_events into a fuller event-sourced aggregate architecture where Cerebra's lattice nodes become aggregates with reducers in the lattica-es sense. Intended to be shared with Lattica Claude as a proposal for cross-Claude reaction.*

---

## What this addendum proposes

The original read adapter design treats the relationship between Cerebra and lattica-es as primarily a *read* relationship. Cerebra writes events to its own inspector_events table; lattica-es provides a read adapter that surfaces those events into the unified timeline. The relationship is one-directional and limited to observability.

This addendum proposes extending that relationship into a fuller event-sourced architecture where Cerebra's lattice nodes (multi-commit memory records and their siblings) become aggregates in the lattica-es sense — entities with identity, reducers, and state derived from accumulated events. The implication is that Cerebra becomes both a producer and consumer of lattica-es events, with its cognitive substrate's behavior shaped by aggregate state rather than by static row values.

This is a substantial architectural commitment. It changes what lattica-es needs to support for Cerebra specifically, and it changes how Cerebra reasons about its own data. The addendum specifies what changes, why it works architecturally, and what specific feedback lattica-es should provide before the proposal is committed.

## Why this proposal exists

The architectural insight emerged from working through several previously-separate concerns in the Cerebra design:

- The interpretive lattice multi-commits records at write time, but currently records are static after commit
- The witness substrate was specified as a separate aggregation layer over inspector events
- The cycle runtime needs continuous cognitive context but the event log provides discrete entries
- The dark matter substrate captured shadows in a parallel data structure to memory records
- Counterfactual cognition needed mechanism that wasn't yet designed

Each concern had a reasonable independent solution. Together they implied substantial architectural sprawl. The insight that resolves them: each is a different consumer of the same underlying event stream. The lattice node updating its state when promoted is observing events. The witness layer aggregating patterns is observing the same stream at coarser granularity. The cycle runtime reading current cognitive field is reading projections that derive from the stream. The dark matter substrate captures additional event types in the same stream. Counterfactual cognition branches the stream.

Once events become first-class and consumption varies, the parallel-layer architecture collapses. There is one event substrate. Multiple consumers operate against it. Each derives what it needs from the same events.

The full articulation of this pattern lives in the event_sourced_cognitive_substrate.md concept document. This addendum focuses on the implications for lattica-es specifically.

## What changes from the original adapter design

The original read adapter design assumes Cerebra continues writing to inspector_events as it does today, and lattica-es provides a read-only adapter that surfaces those events into the unified timeline. Cerebra's internal architecture is unchanged.

Under the proposed event-sourced aggregate architecture, Cerebra continues writing to inspector_events (no migration cost, no change to existing code), but *additionally* registers its lattice nodes as aggregates within lattica-es. The aggregate reducers fold events from the same stream the adapter is surfacing into the unified timeline.

The same event stream serves both purposes. The adapter surfaces events to consumers of the unified timeline. The lattica-es aggregate machinery folds events into reducers for Cerebra's lattice nodes. Cerebra reads aggregate state when its retrieval pipeline needs to know "what does this record know about its own usage history."

This means lattica-es is consumed by Cerebra in two distinct ways:

1. **As an event store and adapter** for the unified timeline (current design)
2. **As an aggregate runtime** that runs reducers over Cerebra's lattice node streams (proposed addition)

The two consumption patterns are independent in implementation. The unified timeline consumers see events; Cerebra's retrieval pipeline reads aggregate state. They don't need to coordinate. But they're consuming the same underlying lattica-es services, which is the elegance of the architecture.

## What lattica-es needs to support

The proposal does not require lattica-es features beyond what ADR-002 specifies. Specifically, it depends on:

**Aggregate identity and stream organization** — Each lattice node has a stream; the stream ID derives deterministically from the lineage identifier. Reading the node's current state means rehydrating the aggregate from its stream. This is core ES toolkit functionality.

**Pure synchronous reducers** — The reducer for a lattice node takes (current_state, event) and produces new_state with no I/O, no side effects, no async. ADR-002 already specifies this as the reducer contract.

**Reactive subscriptions** — The witness layer needs to be notified asynchronously when aggregate state changes. PyO3 exposes this as async generators per the ADR.

**Snapshot support** — Replaying every event from a stream's beginning becomes expensive at scale. Snapshots optimize this. ADR-002 specifies snapshots as optimization that must never be required for correctness.

**Branching** — Counterfactual cognition requires branching the event stream and replaying with modifications. ADR-002 specifies branching as a v1 capability.

All five capabilities are already in the lattica-es design. The proposal uses them in a specific configuration but does not request new features. This is important: the proposal does not extend lattica-es's scope; it intensifies Cerebra's consumption of lattica-es's existing scope.

## Specific architectural commitments the proposal implies

If lattica-es accepts this proposal as part of Cerebra's consumption pattern, several specific commitments follow:

**Cerebra's lattice nodes have streams in lattica-es.** Stream naming convention: `cerebra/lattice/<lineage_id>` per the adapter design, with all events affecting nodes in that lineage written to that stream. Single-commit (non-lattice) records also get streams: `cerebra/record/<record_id>`. The two patterns coexist; lattice members use the lineage stream, non-lattice members use the record stream.

**Cerebra registers reducers for these streams.** The reducer logic is Cerebra-side code, written in Python via the PyO3 adapter. lattica-es invokes the reducers when aggregate state is queried or when events arrive. The reducer is pure and synchronous per ADR-002's contract.

**Aggregate state is queryable through lattica-es.** Cerebra's retrieval pipeline asks lattica-es "what is the current state of aggregate X" and receives the folded state. Internally this might involve replay from a snapshot or full replay from stream beginning; the consumer doesn't need to know which.

**Subscriptions for the witness layer.** The witness layer subscribes to lattica-es streams asynchronously, receives event notifications, and updates its own witness-layer aggregates accordingly. Subscriptions are independent — the witness's subscription doesn't affect Cerebra's reducers, and vice versa.

**Snapshot cadence is calibrated.** Lattice nodes might accumulate many events over their lifetime. Snapshot cadence should be tuned so that aggregate rehydration is fast enough for Cerebra's retrieval latency requirements. Probably every 100 events per stream is a reasonable starting point per the original toolkit roadmap.

**Branch support for counterfactual queries.** When the cycle runtime or witness layer wants to ask "what would have happened if event X had been different," lattica-es's branching is the mechanism. The branch is created, replayed with the modification, and aggregate state on the branch is compared to the canonical state.

## What this changes for lattica-es's read adapter design

The original adapter design specified surfacing Cerebra's events into the unified timeline. Under this proposal, the adapter has the same surface but operates within a richer context.

The adapter's responsibilities remain:
- Read inspector_events rows from Cerebra's SQLite store
- Compute synthetic content-addressed IDs per the specified derivation
- Map Cerebra's event vocabulary to the unified vocabulary
- Surface events into lattica-es streams with proper causation chains
- Handle backfill of existing rows

The adapter's relationship to aggregates becomes:
- Events surfaced by the adapter feed both (a) the unified timeline consumers and (b) the aggregate reducers for Cerebra's lattice nodes
- The aggregate reducers are Cerebra-side code that lattica-es invokes
- The unified timeline consumers see the events; the aggregate machinery sees the folded state
- These are independent consumption patterns of the same underlying events

No additional adapter infrastructure is needed. The adapter's job is the same. What changes is that the events it surfaces are also consumed by aggregate reducers within lattica-es itself, producing the aggregate-state architecture Cerebra needs.

## Where this proposal might surprise lattica-es Claude

Several aspects of the proposal might warrant pushback or alternative framings:

**The volume of aggregates is substantial.** A typical Cerebra vault has hundreds to thousands of memory records, each potentially its own aggregate. If lattica-es's aggregate runtime is designed for tens of aggregates rather than thousands, the scaling assumptions might be wrong. Worth confirming that lattica-es can handle this volume gracefully.

**The reducer logic is Cerebra-specific.** lattica-es is being designed as a general-purpose event sourcing toolkit. Cerebra-specific reducers might feel like an inappropriate Cerebra concern leaking into the toolkit. The proposed pattern (consumer-registered reducers via PyO3) keeps the toolkit general while letting consumers customize their aggregate behavior; the question is whether this consumer registration pattern matches lattica-es's intended API.

**Async at the consumption boundary needs to be solid.** The proposed architecture relies on async subscriptions for the witness layer and for any other reactive consumer. PyO3 exposes async generators per ADR-002, but the actual subscription semantics (delivery guarantees, ordering across subscribers, backpressure) need to be specified. Worth confirming these are nailed down.

**The relationship to Cerebra's existing inspector_events isn't fully clear.** Cerebra writes events to its own SQLite table; lattica-es reads them via the adapter; the same events feed lattica-es aggregates. There are conceptually two storage layers (Cerebra's inspector_events and lattica-es's events table). The proposal doesn't migrate Cerebra's events into lattica-es's storage. Whether this dual-storage pattern is acceptable to lattica-es's architecture might need explicit confirmation.

**Counterfactual cognition uses branching in a specific way.** When Cerebra branches an event stream for counterfactual exploration, it's not for time-travel debugging (the original use case) but for cognitive operation. The branches might be short-lived (created for a single counterfactual query, then discarded) or persistent (kept around for ongoing reflection). Either way, lattica-es needs to handle the volume of branches the cognitive runtime might create. Worth confirming branching is intended to scale to this use pattern.

## What feedback Cerebra Claude needs from Lattica Claude

For this proposal to be committed, several specific questions need answers from the lattica-es side:

**Can lattica-es support thousands of aggregates per consuming project?** The volume of Cerebra's lattice nodes is non-trivial. If the toolkit assumes tens of aggregates, the design needs revision before Cerebra commits to this pattern.

**What does consumer-registered reducer logic look like in PyO3?** The reducer is Cerebra-side Python code, but it must satisfy lattica-es's pure synchronous reducer contract. How does the consumer register the reducer? How does lattica-es enforce purity? How does the consumer access the current aggregate state from non-reducer code (the retrieval pipeline)?

**How are async subscription semantics specified?** Delivery guarantees (at-least-once, at-most-once, exactly-once), ordering (across subscribers, within a single subscriber), backpressure (what happens when a subscriber falls behind), failure modes (subscriber crashes, network partitions if any). These need to be specified before Cerebra builds against them.

**What is the snapshot strategy for high-event-volume aggregates?** Lattice nodes that are heavily used could accumulate hundreds of events quickly. The default snapshot cadence (every 100 events) might or might not be appropriate. Worth understanding lattica-es's snapshot story for this volume.

**How does branching scale to many short-lived branches?** Counterfactual cognition might create branches frequently. Each branch shares storage with its parent per ADR-002, but there's still bookkeeping overhead. Confirm lattica-es expects to handle this branch volume.

**Is there an aggregate-state-changed event type that lattica-es emits, or does each consumer infer state changes from the underlying events?** The witness layer wants to know "this aggregate's state changed." It can either subscribe to all events affecting the aggregate and infer state changes from there, or it can subscribe to a higher-level "aggregate state changed" notification that lattica-es emits. The two patterns have different cost profiles.

These questions don't need to be answered before the proposal is shared with Lattica Claude. They're the substantive points where Cerebra's needs intersect with lattica-es's design choices, and they should be discussed as part of integrating this proposal into lattica-es's overall plan.

## What happens if lattica-es can't or won't support this pattern

The proposal is substantial enough that lattica-es might reasonably push back. If for any reason this isn't the right architecture for lattica-es to support:

**Cerebra can implement the aggregate pattern internally.** Cerebra-side Python code can subscribe to inspector_events (via the lattica-es adapter or via direct SQLite reads), maintain its own aggregate state in Cerebra-owned tables, and serve aggregate-state queries from Cerebra's own code rather than from lattica-es. This produces a worse architecture (Cerebra rebuilds infrastructure lattica-es could provide) but it remains workable.

**The witness layer can be Cerebra-internal.** Instead of being a lattica-es-aware service that observes aggregate state, the witness can be Cerebra-internal code reading inspector_events directly. Less elegant but functional.

**The shadow substrate falls back to the parallel-table design.** The dark matter document originally specified a separate `lattice_shadows` table; that design works if shadows aren't events in the unified stream.

The proposal is the *better* architecture, but Cerebra can fall back to less integrated architectures if needed. Worth flagging so Lattica Claude knows the proposal isn't a hard requirement — it's an opportunity to consolidate that benefits both projects.

## What Cerebra Claude commits to providing

In exchange for lattica-es supporting this consumption pattern, Cerebra makes specific commitments:

The reducer logic for lattice node aggregates is Cerebra-owned and Cerebra-maintained. lattica-es provides the runtime; Cerebra provides the cognitive substrate semantics.

The aggregate state schema is Cerebra-owned. lattica-es doesn't need to understand what state Cerebra's reducers produce; it just runs them and serves the results back.

The event vocabulary that affects Cerebra aggregates is documented in the read adapter design. New event types added by future Cerebra phases will be added to the vocabulary documentation.

The reducer contract (pure, synchronous, no I/O) is honored. Cerebra-side reducers will not violate this contract. If a cognitive operation needs I/O, it happens outside the reducer.

The aggregate volume and event volume are within lattica-es's expected scaling envelope. If Cerebra discovers it's producing more aggregates or events than lattica-es can handle, Cerebra will redesign the aggregation pattern (perhaps coalescing related records into single aggregates) rather than asking lattica-es to scale beyond its design.

## Summary

The proposal extends Cerebra's relationship with lattica-es from "read adapter consumer" to "event-sourced aggregate consumer." Cerebra's lattice nodes become aggregates with reducers that fold events into continuous state. The witness layer observes aggregate state. The cycle runtime operates against aggregate state. Counterfactual cognition uses branching.

The proposal does not require new lattica-es features; it intensifies Cerebra's use of features ADR-002 already specifies. It implies specific commitments around aggregate volume, reducer registration, async subscription semantics, snapshot cadence, and branch handling.

Specific feedback from lattica-es Claude is requested on the questions in the "What feedback Cerebra Claude needs from Lattica Claude" section. The proposal can be committed once those questions are resolved or, if lattica-es pushes back on aspects of the architecture, the alternatives in "What happens if lattica-es can't or won't support this pattern" provide fallback paths.

Either way, the conversation surfaces architectural commitments that benefit from explicit treatment before implementation begins.

---

*This addendum extends cerebra_read_adapter_design.md with the event-sourced aggregate proposal. The full articulation of the architectural pattern lives in event_sourced_cognitive_substrate.md. A reader new to either document should read the original read adapter design first for the contract Cerebra commits to providing, then the concept document for the architectural rationale, then this addendum for the lattica-es-side implications.*
