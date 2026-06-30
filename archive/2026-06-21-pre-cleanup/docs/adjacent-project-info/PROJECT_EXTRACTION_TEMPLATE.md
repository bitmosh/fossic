# ES Consumer Profile — [PROJECT NAME]

You are the [PROJECT] advocate. We're building **ES**, a local-first
event sourcing library: Rust core, PyO3/napi-rs bindings, SQLite-backed,
content-addressed events, branchable history, pure synchronous reducers.
It's being extracted from Lattica's Phase 6 plans because [PROJECT] and
several other projects also want event sourcing capabilities.

Before we lock the v1 API, we need your project's actual shape. Please
answer the questions below as concretely as possible — "I don't know yet"
and "we don't care about this" are both valid and useful answers.

If you need to look at the codebase to answer any of these, do so. Cite
files/lines where helpful.

## Language and runtime
1. Primary language(s)? (Python version if Python, Node/Deno/Bun if JS, etc.)
2. In-process embedded library, long-running daemon, CLI tool, or hybrid?
3. Multi-process? Multi-thread? Async runtime in use (asyncio, tokio, etc.)?

## What you want from ES
4. The three top things you want from event sourcing for this project.
   Be specific — "audit trail", "replay", "branching for experiments",
   "agent trace export", "cross-process pub/sub", etc.
5. Anything from the ES spec (event fabric / branching /
   snapshots / OTel export / agent trace adapter) you DON'T need or
   actively don't want? Saying "we don't care about branches" is as
   valuable as saying "we need branches."

## Scale and shape of writes
6. Estimated write rate at steady state: events/sec, events/minute?
7. Burst profile: do writes come in tight clusters, or evenly spread?
8. Typical payload size? Maximum payload size? Any payloads larger than 64KB?
9. Number of distinct streams you'd expect? Estimated events per stream
   per day?
10. Single writer per stream, or concurrent writers?

## Reads
11. Read patterns: linear replay from version 0? Tail-the-latest?
    Time-range queries? Random access by event ID?
12. Live subscriptions: do consumers need real-time event delivery, or
    is polling acceptable? If real-time, what's the acceptable latency?
13. Cross-stream queries needed (e.g., "all events with this correlation_id
    across all streams")?

## Persistence and lifecycle
14. How long do events need to live? Forever? 90 days? Configurable?
15. Any need to delete individual events (PII removal, compliance,
    poisoned-payload cleanup)? If yes, what's driving it?
16. Acceptable storage growth? Bounded by disk, bounded by retention
    policy, or "we'll cross that bridge later"?
17. Backup/restore expectations? Just-the-SQLite-file is fine, or
    need streaming backup, point-in-time recovery, etc.?

## Security and deployment
18. Any sensitive data in event payloads (PII, secrets, credentials,
    user content)?
19. Encryption at rest required? If yes, who manages the key — OS
    keyring, KMS, env var, configurable?
20. Single-user local-first, or multi-user / multi-tenant?
21. Deployment target: developer workstation, server, container,
    edge device, all of the above?

## Existing event/log infrastructure
22. Does this project already have an event store, audit log, or
    similar? What's the schema? Would you migrate to ES,
    bridge to it (read-only adapter), or both?
23. If migrating: how many existing events would need translation?

## Integration shape
24. How would you want to call ES? In-process API, separate
    daemon with IPC, file-watching bridge, all of the above?
25. Anything in your codebase today that would make integration
    awkward? (e.g., existing async patterns, custom serializers,
    weird dependency conflicts)

## Open questions and concerns
26. What would make you NOT adopt ES? What's the biggest risk
    you see from your project's perspective?
27. Anything in the ES design (see EVENT_FABRIC.md, ADR-002)
    that you'd push back on?

---

Compile the response as a single markdown section under
"## [PROJECT NAME]" suitable for inclusion in
ES_CONSUMER_PROFILES.md.