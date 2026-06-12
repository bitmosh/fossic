# fossic-py

PyO3 Python bindings for [fossic](../README.md), the local-first event sourcing library.

The Python API mirrors the Rust API with synchronous semantics. An async wrapper (`fossic_aio`) for asyncio consumers is published separately and wraps these bindings with `asyncio.to_thread`.

## Installation

Built with [maturin](https://github.com/PyO3/maturin):

```sh
pip install maturin
maturin develop          # editable install for development
maturin build --release  # build a wheel
```

## Quick start

```python
import os
from fossic import Store, Append, ReadQuery, OpenOptions, SubscriptionMode

# fossic does not expand tilde paths — expand before calling Store.open.
store = Store.open(
    path=os.path.expanduser("~/.fossic/store.db"),
    options=OpenOptions(
        encryption="plaintext",
        on_first_open="create_if_missing",
    ),
)

store.declare_stream("cerebra/lattice/abc123", declared_by="cerebra")

event_id = store.append(Append(
    stream_id="cerebra/lattice/abc123",
    event_type="MemoryRecordCommitted",
    type_version=1,
    payload={"content_hash": "...", "source": "..."},
))

events = store.read_range(ReadQuery(
    stream_id="cerebra/lattice/abc123",
    branch="main",
    from_version=0,
))
```

## Subscription delivery

Callbacks run on a Python-owned worker thread (not a Rust-spawned thread). This preserves `threading.local` state, asyncio contextvars, and logging context. See §4.2 of `docs/implement/FOSSIC_V1_SPEC.md` for the full explanation.

```python
with store.subscribe(
    stream_pattern="cerebra/lattice/*",
    mode=SubscriptionMode.post_commit(queue_size=1024),
) as sub:
    for event in sub:
        process(event)
```

## Requirements

- Python 3.12+
- PyO3 0.29+ (free-threaded Python 3.13+/3.14 supported)
- Rust stable toolchain

## License

MIT OR Apache-2.0
