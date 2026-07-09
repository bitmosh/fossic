# SPDX-License-Identifier: Apache-2.0
"""
fossic.similarity — HNSW-backed similarity search for fossic event stores.

Quick start::

    from fossic.similarity import HnswProvider, SimilarityQuery

    provider = HnswProvider("store.db", dimensions=1024)
    provider.index(event_id_bytes, embedding)
    provider.save()

    # Query via dict:
    results = provider.query({"embedding": query_vec, "k": 10})

    # Or via SimilarityQuery helper:
    sq = SimilarityQuery(embedding=query_vec, k=10)
    results = provider.query(sq.as_dict())

    for hit in results:
        print(hit["event_id"].hex(), hit["score"])

Import paths:
    from fossic.similarity import HnswProvider, SimilarityQuery
    from fossic import HnswProvider, SimilarityQuery  # after __init__ re-export
"""

from __future__ import annotations

import dataclasses
from typing import Optional

try:
    from fossic._fossic import HnswProvider  # type: ignore[import]
except ImportError:
    HnswProvider = None  # type: ignore[assignment,misc]


@dataclasses.dataclass
class SimilarityQuery:
    """Typed wrapper for a k-NN query dict.

    Construct and call ``.as_dict()`` to pass to ``HnswProvider.query()``.
    Fields map 1:1 to the Rust ``SimilarityQuery`` struct.

    :param embedding: Query vector (same dimensionality as the index).
    :param k: Number of nearest neighbours to return.
    :param stream_pattern: Optional glob filter — only vectors registered via
        ``index_with_stream_id`` are eligible; plain ``index`` vectors are
        always excluded from filtered queries (CP-D2-2).
    """

    embedding: list
    k: int
    stream_pattern: Optional[str] = None

    def as_dict(self) -> dict:
        """Return the query as the dict ``HnswProvider.query()`` expects."""
        return {
            "embedding": self.embedding,
            "k": self.k,
            "stream_pattern": self.stream_pattern,
        }


__all__ = ["HnswProvider", "SimilarityQuery"]
