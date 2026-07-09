# SPDX-License-Identifier: Apache-2.0
"""Tests for branch creation, promotion, dead-end marking, and chain resolution."""

from __future__ import annotations

import pytest

from fossic import (
    BranchNotFoundError,
    CreateBranch,
    ReadQuery,
    Store,
)
from conftest import unique_ev


def test_create_branch(declared_store: Store) -> None:
    for _ in range(3):
        unique_ev(declared_store, "test/s")
    declared_store.create_branch(
        CreateBranch(
            stream_id="test/s",
            branch_id="feature-x",
            parent_id="main",
        )
    )
    branches = declared_store.list_branches("test/s")
    ids = [b.id for b in branches]
    assert "feature-x" in ids


def test_branch_appends_isolated(declared_store: Store) -> None:
    for _ in range(2):
        unique_ev(declared_store, "test/s")
    declared_store.create_branch(
        CreateBranch(stream_id="test/s", branch_id="exp", parent_id="main")
    )
    unique_ev(declared_store, "test/s", branch="exp", val="branch-only")
    main_events = declared_store.read_range(ReadQuery(stream_id="test/s"))
    branch_events = declared_store.read_range(
        ReadQuery(stream_id="test/s", branch="exp")
    )
    # read_range returns only events committed to the queried branch.
    # Main is unchanged (2 events); the branch has only its own event (1).
    assert len(main_events) == 2
    assert len(branch_events) == 1


def test_promote_branch(declared_store: Store) -> None:
    unique_ev(declared_store, "test/s")
    declared_store.create_branch(
        CreateBranch(stream_id="test/s", branch_id="promo", parent_id="main")
    )
    declared_store.promote_branch("test/s", "promo", reason="looks good")
    branches = declared_store.list_branches("test/s")
    promo = next(b for b in branches if b.id == "promo")
    assert promo.lifecycle == "promoted"


def test_mark_branch_dead_end(declared_store: Store) -> None:
    unique_ev(declared_store, "test/s")
    declared_store.create_branch(
        CreateBranch(stream_id="test/s", branch_id="dead", parent_id="main")
    )
    declared_store.mark_branch_dead_end("test/s", "dead", reason="abandoned")
    branches = declared_store.list_branches("test/s")
    dead = next(b for b in branches if b.id == "dead")
    assert dead.lifecycle == "dead_end"


def test_resolve_chain_includes_main(declared_store: Store) -> None:
    unique_ev(declared_store, "test/s")
    chain = declared_store.resolve_chain("test/s", "main")
    assert len(chain) >= 1
    assert chain[0].branch_id == "main"


def test_resolve_chain_not_found_raises(declared_store: Store) -> None:
    with pytest.raises(BranchNotFoundError):
        declared_store.resolve_chain("test/s", "no-such-branch")


def test_list_branches_empty_stream(declared_store: Store) -> None:
    # "main" is the implicit default branch and is never stored in the branches table.
    # A freshly-declared stream with no explicit branches returns an empty list.
    branches = declared_store.list_branches("test/s")
    assert branches == []
