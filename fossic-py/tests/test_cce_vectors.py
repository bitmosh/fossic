"""
Python CCE conformance harness.

Loads the canonical test vectors from cce-test-vectors.json at the repo root
and verifies that fossic-py's CCE encoder produces byte-identical output to
each vector's expected bytes.

These vectors are the contract for the CCE specification. The Rust core has a
parallel harness in fossic/tests/cce_vectors.rs that verifies the Rust encoder
against the same vectors. This harness ensures the Python binding stays in sync;
any drift between the Rust core and the Python binding produces an immediate
test failure.
"""

import json
from pathlib import Path

import pytest

from fossic import cce_encode_bytes_raw, cce_encode_f64_bits, cce_encode_value

VECTORS_PATH = Path(__file__).parent.parent.parent / "cce-test-vectors.json"


def load_vectors() -> list:
    if not VECTORS_PATH.exists():
        pytest.skip(f"cce-test-vectors.json not found at {VECTORS_PATH}")
    data = json.loads(VECTORS_PATH.read_text())
    return data["encode_value_vectors"]


@pytest.mark.parametrize(
    "vector",
    load_vectors(),
    ids=lambda v: v["id"],
)
def test_python_cce_matches_canonical(vector: dict) -> None:
    """Python CCE encoder produces byte-identical output to the canonical vector."""
    inp = vector["input"]
    if vector["expected_hex"] is None:
        pytest.skip("expected_hex is null — vector not yet filled in")
    expected = bytes.fromhex(vector["expected_hex"])

    if inp["type"] == "json":
        actual = cce_encode_value(inp["value"])
    elif inp["type"] == "i64_str":
        actual = cce_encode_value(int(inp["value"]))
    elif inp["type"] == "f64_bits":
        actual = cce_encode_f64_bits(inp["bits"])
    elif inp["type"] == "bytes":
        raw = bytes.fromhex(inp["data"]) if inp["data"] else b""
        actual = cce_encode_bytes_raw(raw)
    else:
        pytest.skip(f"unknown input type: {inp['type']}")
        return

    assert actual == expected, (
        f"CCE byte mismatch for vector '{vector['id']}'\n"
        f"  description: {vector.get('description', '')}\n"
        f"  input:    {inp}\n"
        f"  expected: {expected.hex()}\n"
        f"  actual:   {actual.hex()}"
    )
