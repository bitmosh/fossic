# fossic-similarity-hnsw

HNSW-backed implementation of `fossic::SimilaritySearchProvider` via [`hnsw_rs`](https://crates.io/crates/hnsw_rs).

Enables semantic search over fossic event payloads at Cerebra scale (50k–500k vectors) without an in-memory NumPy ceiling. Wire it into a `Store` via `OpenOptions::similarity_provider`.

Full documentation lands at v1.7.4.
