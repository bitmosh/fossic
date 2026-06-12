"""Tests for store open / declare_stream / stream registry."""

from __future__ import annotations

from pathlib import Path

import pytest

from fossic import (
    InvalidStreamIdError,
    OpenOptions,
    Store,
    StoreNotFoundError,
    StreamNotDeclaredError,
)
from conftest import unique_ev


def test_open_creates_file(tmp_path: Path) -> None:
    Store.open(str(tmp_path / "test.db"))
    assert (tmp_path / "test.db").exists()


def test_open_require_existing_missing_raises(tmp_path: Path) -> None:
    with pytest.raises(StoreNotFoundError):
        Store.open(
            str(tmp_path / "missing.db"),
            OpenOptions(on_first_open="require_existing"),
        )


def test_open_require_existing_present_ok(tmp_path: Path) -> None:
    path = str(tmp_path / "test.db")
    Store.open(path)  # create
    store2 = Store.open(path, OpenOptions(on_first_open="require_existing"))
    assert store2.stream_exists("_fossic/system")


def test_declare_stream_and_list(tmp_store: Store) -> None:
    tmp_store.declare_stream("test/stream/abc", "unit-test")
    streams = tmp_store.streams()
    ids = [s.id for s in streams]
    assert "test/stream/abc" in ids


def test_declare_stream_idempotent(tmp_store: Store) -> None:
    tmp_store.declare_stream("test/s", "a")
    tmp_store.declare_stream("test/s", "a")  # no error


def test_stream_exists_true(tmp_store: Store) -> None:
    tmp_store.declare_stream("test/s", "a")
    assert tmp_store.stream_exists("test/s")


def test_stream_exists_false(tmp_store: Store) -> None:
    assert not tmp_store.stream_exists("test/nonexistent")


def test_declare_stream_bad_whitespace(tmp_store: Store) -> None:
    with pytest.raises(InvalidStreamIdError):
        tmp_store.declare_stream("test/bad stream", "a")


def test_declare_stream_bad_quote(tmp_store: Store) -> None:
    with pytest.raises(InvalidStreamIdError):
        tmp_store.declare_stream("test/bad\"stream", "a")


def test_append_to_undeclared_stream_raises(tmp_store: Store) -> None:
    with pytest.raises(StreamNotDeclaredError):
        unique_ev(tmp_store, "test/undeclared")
