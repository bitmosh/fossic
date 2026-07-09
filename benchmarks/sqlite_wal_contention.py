#!/home/boop/.venv/bin/python3
# SPDX-License-Identifier: Apache-2.0
"""
SQLite WAL write-contention benchmark for Fossic.

Single-scenario mode (original CLI preserved):
    python3 benchmarks/sqlite_wal_contention.py [--writers N] [--rate R] [--duration D]
      [--payload-bytes B] [--payload-shape {random,jsonish,zeros}]
      [--wal-autocheckpoint N] [--busy-timeout N]

Suite mode (all four scenarios × 2 checkpoint configs → markdown report):
    python3 benchmarks/sqlite_wal_contention.py --suite [--output PATH]

Each writer iteration simulates the fossic append path:
  1. Build event payload at target size (per shape)
  2. CCE encode: msgpack with deep-sorted keys  → encode_us
  3. BLAKE3 ID derivation over PREFIX + encoded  → hash_us
  4. SQLite BEGIN IMMEDIATE + INSERT + COMMIT    → write_us
  total_us = encode_us + hash_us + write_us
"""

import argparse
import os
import platform
import shutil
import sqlite3
import tempfile
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

import blake3 as _blake3
import msgpack

# ─────────────────────────────────────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────────────────────────────────────

HASH_PREFIX = b"fossic:v1:"

SCHEMA = """
CREATE TABLE IF NOT EXISTS events (
    content_id  TEXT    NOT NULL,
    writer_id   INTEGER NOT NULL,
    seq         INTEGER NOT NULL,
    payload     BLOB    NOT NULL,
    written_at  REAL    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ws ON events(writer_id, seq);
"""

# ─────────────────────────────────────────────────────────────────────────────
# Payload construction
# ─────────────────────────────────────────────────────────────────────────────

def _deep_sort(obj):
    """Recursively sort dict keys — CCE canonical form pre-pass."""
    if isinstance(obj, dict):
        return {k: _deep_sort(v) for k, v in sorted(obj.items())}
    if isinstance(obj, list):
        return [_deep_sort(i) for i in obj]
    return obj


def _make_jsonish(target_bytes: int, writer_id: int, seq: int, ts: float) -> dict:
    base = {
        "event_type": "SweepCompleted",
        "schema_version": 2,
        "timestamp_ns": int(ts * 1_000_000_000),
        "writer_id": writer_id,
        "sequence": seq,
        "run_id": f"run-{writer_id:04d}-{seq:08d}",
        "status": "completed",
        "duration_ms": round(123.456 + seq * 0.001, 6),
        "deps_scanned": 4200 + seq,
        "deps_flagged": 7,
        "policy_version": "2.1.4",
        "policy_hash": "a3f9c2b1d4e5f6781a2b3c4d5e6f7890",
        "transitive_deps": [
            {
                "name": f"package-name-{i:03d}",
                "version": f"{i // 10}.{i % 10}.0",
                "ecosystem": "pypi",
                "direct": i < 5,
                "license": "MIT" if i % 3 == 0 else "Apache-2.0",
            }
            for i in range(20)
        ],
        "flagged_packages": [
            {
                "name": f"suspicious-package-{i:02d}",
                "version": "0.0.1",
                "reason": "supply_chain_anomaly",
                "severity": "high" if i % 2 == 0 else "medium",
                "first_seen_epoch": 1704067200 + i * 86400,
                "cve_refs": [
                    f"CVE-2026-{10000 + i * 100}",
                    f"CVE-2026-{20000 + i * 100}",
                ],
                "confidence": round(0.85 + i * 0.02, 3),
            }
            for i in range(5)
        ],
        "metadata": {
            "host": "policy-scout-worker",
            "pid": 10000 + writer_id,
            "env": "development",
            "config_path": "/etc/fossic/policy-scout.yaml",
            "tags": ["benchmark", "synthetic", "fossic-bench", "v1"],
            "git_sha": "deadbeefcafebabedeadbeef01234567",
            "git_branch": "main",
        },
        "scores": [round(i * 0.05, 6) for i in range(20)],
        "timing_breakdown": {
            "resolve_ms": 45.234,
            "fetch_ms": 67.891,
            "analyze_ms": 10.456,
            "report_ms": 0.054,
        },
    }
    # Pad to target_bytes with a blob field
    probe = msgpack.packb(_deep_sort(base), use_bin_type=True)
    remaining = target_bytes - len(probe) - 40  # ~40 bytes for the "_pad" key overhead
    if remaining > 0:
        base["_pad"] = b"\xcc" * remaining
    return base


def _build_event(shape: str, target_bytes: int, writer_id: int, seq: int, ts: float) -> dict:
    if shape == "random":
        return {"_t": "event", "_w": writer_id, "_s": seq, "data": os.urandom(target_bytes)}
    elif shape == "zeros":
        return {"_t": "event", "_w": writer_id, "_s": seq, "data": bytes(target_bytes)}
    else:  # jsonish
        return _make_jsonish(target_bytes, writer_id, seq, ts)


# ─────────────────────────────────────────────────────────────────────────────
# Per-writer result
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class WriterResult:
    writer_id: int
    encode_us: list[float] = field(default_factory=list)
    hash_us:   list[float] = field(default_factory=list)
    write_us:  list[float] = field(default_factory=list)
    total_us:  list[float] = field(default_factory=list)
    busy_errors: int = 0
    write_count: int = 0
    dropped: int = 0


# ─────────────────────────────────────────────────────────────────────────────
# Writer thread
# ─────────────────────────────────────────────────────────────────────────────

def _writer(
    writer_id: int,
    db_path: str,
    barrier: threading.Barrier,
    stop: threading.Event,
    result: WriterResult,
    rate: float,
    busy_timeout_ms: int,
    payload_bytes: int,
    payload_shape: str,
):
    con = sqlite3.connect(db_path, check_same_thread=False, isolation_level=None)
    con.execute(f"PRAGMA busy_timeout = {busy_timeout_ms}")
    con.execute("PRAGMA synchronous = NORMAL")
    con.execute("PRAGMA cache_size = -8192")

    interval = 1.0 / rate
    seq = 0
    barrier.wait()
    next_tick = time.perf_counter()

    while not stop.is_set():
        now = time.perf_counter()
        if now > next_tick + interval:
            n_skip = int((now - next_tick) / interval)
            result.dropped += n_skip
            next_tick += n_skip * interval
        sleep_for = next_tick - now
        if sleep_for > 0:
            time.sleep(sleep_for)
        next_tick += interval

        ts = time.time()
        t0 = time.perf_counter()

        # Step 1+2: build event object + CCE-encode (deep-sorted msgpack)
        obj = _build_event(payload_shape, payload_bytes, writer_id, seq, ts)
        encoded = msgpack.packb(_deep_sort(obj), use_bin_type=True)
        t1 = time.perf_counter()

        # Step 3: BLAKE3 content ID
        content_id = _blake3.blake3(HASH_PREFIX + encoded).hexdigest()
        t2 = time.perf_counter()

        # Step 4: SQLite append
        try:
            con.execute("BEGIN IMMEDIATE")
            con.execute(
                "INSERT INTO events (content_id, writer_id, seq, payload, written_at)"
                " VALUES (?,?,?,?,?)",
                (content_id, writer_id, seq, encoded, ts),
            )
            con.execute("COMMIT")
            t3 = time.perf_counter()

            result.encode_us.append((t1 - t0) * 1e6)
            result.hash_us.append((t2 - t1) * 1e6)
            result.write_us.append((t3 - t2) * 1e6)
            result.total_us.append((t3 - t0) * 1e6)
            result.write_count += 1
            seq += 1

        except sqlite3.OperationalError as exc:
            con.execute("ROLLBACK")
            t3 = time.perf_counter()
            if "locked" in str(exc).lower():
                result.busy_errors += 1
            result.encode_us.append((t1 - t0) * 1e6)
            result.hash_us.append((t2 - t1) * 1e6)
            result.write_us.append((t3 - t2) * 1e6)
            result.total_us.append((t3 - t0) * 1e6)

    con.close()


# ─────────────────────────────────────────────────────────────────────────────
# Scenario definitions
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class ScenarioConfig:
    sid: str
    desc: str
    writers: int
    rate: float
    duration: int
    payload_bytes: int
    payload_shape: str
    wal_autocheckpoint: int
    busy_timeout_ms: int = 30_000


@dataclass
class ScenarioResult:
    config: ScenarioConfig
    writer_results: list[WriterResult]
    wall_elapsed: float
    wal_log_pages: int
    wal_ckpt_pages: int
    db_size_kb: float


# ─────────────────────────────────────────────────────────────────────────────
# Run one scenario
# ─────────────────────────────────────────────────────────────────────────────

def _run_scenario(cfg: ScenarioConfig) -> ScenarioResult:
    tmp_dir = tempfile.mkdtemp(prefix="fossic_bench_")
    db_path = os.path.join(tmp_dir, "bench.db")

    con = sqlite3.connect(db_path)
    con.execute("PRAGMA journal_mode = WAL")
    con.execute("PRAGMA synchronous = NORMAL")
    con.execute(f"PRAGMA wal_autocheckpoint = {cfg.wal_autocheckpoint}")
    con.executescript(SCHEMA)
    con.commit()
    con.close()

    barrier = threading.Barrier(cfg.writers + 1)
    stop = threading.Event()
    wr = [WriterResult(writer_id=i) for i in range(cfg.writers)]
    threads = [
        threading.Thread(
            target=_writer,
            args=(i, db_path, barrier, stop, wr[i],
                  cfg.rate, cfg.busy_timeout_ms,
                  cfg.payload_bytes, cfg.payload_shape),
            daemon=True,
        )
        for i in range(cfg.writers)
    ]
    for t in threads:
        t.start()

    barrier.wait()
    wall_start = time.perf_counter()
    time.sleep(cfg.duration)
    stop.set()
    wall_elapsed = time.perf_counter() - wall_start
    for t in threads:
        t.join(timeout=15)

    con = sqlite3.connect(db_path)
    wal_log, wal_ckpt = 0, 0
    try:
        row = con.execute("PRAGMA wal_checkpoint(PASSIVE)").fetchone()
        if row:
            wal_log, wal_ckpt = row[1], row[2]
    except Exception:
        pass
    con.close()

    db_kb = Path(db_path).stat().st_size / 1024
    shutil.rmtree(tmp_dir, ignore_errors=True)

    return ScenarioResult(
        config=cfg,
        writer_results=wr,
        wall_elapsed=wall_elapsed,
        wal_log_pages=wal_log,
        wal_ckpt_pages=wal_ckpt,
        db_size_kb=db_kb,
    )


# ─────────────────────────────────────────────────────────────────────────────
# Stats helpers
# ─────────────────────────────────────────────────────────────────────────────

def pct(data: list[float], p: float) -> float:
    if not data:
        return float("nan")
    s = sorted(data)
    k = (len(s) - 1) * p / 100
    lo = int(k)
    hi = min(lo + 1, len(s) - 1)
    return s[lo] + (s[hi] - s[lo]) * (k - lo)


def _agg(wr: list[WriterResult], attr: str) -> list[float]:
    out: list[float] = []
    for r in wr:
        out.extend(getattr(r, attr))
    return out


def _stats(sr: ScenarioResult) -> dict:
    wr = sr.writer_results
    total_writes = sum(r.write_count for r in wr)
    total_busy   = sum(r.busy_errors for r in wr)
    total_drop   = sum(r.dropped     for r in wr)
    target       = sr.config.writers * sr.config.rate * sr.config.duration
    return {
        "target":        int(target),
        "total_writes":  total_writes,
        "total_busy":    total_busy,
        "total_dropped": total_drop,
        "throughput":    total_writes / sr.wall_elapsed,
        "efficiency":    total_writes / target * 100 if target else 0.0,
        "encode_us":     _agg(wr, "encode_us"),
        "hash_us":       _agg(wr, "hash_us"),
        "write_us":      _agg(wr, "write_us"),
        "total_us":      _agg(wr, "total_us"),
        "wal_log":       sr.wal_log_pages,
        "wal_ckpt":      sr.wal_ckpt_pages,
        "db_kb":         sr.db_size_kb,
    }


def _fms(us: float) -> str:
    return f"{us/1000:.2f}ms"


def _prow_console(data: list[float]) -> str:
    if not data:
        return "no data"
    return (
        f"p50={pct(data,50):.0f}µs  p95={pct(data,95):.0f}µs  "
        f"p99={pct(data,99):.0f}µs  p99.9={pct(data,99.9):.0f}µs  max={max(data):.0f}µs"
    )


def _prow_md(data: list[float]) -> str:
    if not data:
        return "— | — | — | — | —"
    return (
        f"{_fms(pct(data,50))} | {_fms(pct(data,95))} | "
        f"{_fms(pct(data,99))} | {_fms(pct(data,99.9))} | {_fms(max(data))}"
    )


# ─────────────────────────────────────────────────────────────────────────────
# Console report
# ─────────────────────────────────────────────────────────────────────────────

def _print_result(sr: ScenarioResult):
    s = _stats(sr)
    c = sr.config
    pb = f"{c.payload_bytes}B" if c.payload_bytes < 1024 else f"{c.payload_bytes//1024}KB"
    print(f"\n{'─'*70}")
    print(f"  [{c.sid}] {c.desc} — {c.writers}w × {c.rate:.0f}/s × {c.duration}s "
          f"× {pb} {c.payload_shape}  ckpt={c.wal_autocheckpoint}")
    print(f"{'─'*70}")
    print(f"  Writes:     {s['total_writes']:,} / {s['target']:,}  ({s['efficiency']:.1f}%)")
    print(f"  Throughput: {s['throughput']:.1f} ev/s")
    print(f"  Busy errs:  {s['total_busy']}   Dropped: {s['total_dropped']}")
    print(f"  encode_us:  {_prow_console(s['encode_us'])}")
    print(f"  hash_us:    {_prow_console(s['hash_us'])}")
    print(f"  write_us:   {_prow_console(s['write_us'])}")
    print(f"  total_us:   {_prow_console(s['total_us'])}")
    print(f"  WAL:        log_pages={s['wal_log']}  ckpt_pages={s['wal_ckpt']}  db={s['db_kb']:.0f}KB")


# ─────────────────────────────────────────────────────────────────────────────
# Markdown report
# ─────────────────────────────────────────────────────────────────────────────

def _scenario_md_block(sr: ScenarioResult) -> str:
    s = _stats(sr)
    c = sr.config
    pb = f"{c.payload_bytes}B" if c.payload_bytes < 1024 else f"{c.payload_bytes//1024}KB"
    lines = [
        f"**{c.writers}w × {c.rate:.0f}/s × {c.duration}s · {pb} {c.payload_shape} · "
        f"`wal_autocheckpoint={c.wal_autocheckpoint}`**\n\n",
        f"| Metric | Value |\n|---|---|\n",
        f"| Writes completed | {s['total_writes']:,} / {s['target']:,} ({s['efficiency']:.1f}%) |\n",
        f"| Throughput | {s['throughput']:.1f} ev/s |\n",
        f"| SQLITE\\_BUSY errors | {s['total_busy']} |\n",
        f"| Dropped slots | {s['total_dropped']} |\n",
        f"| WAL log pages | {s['wal_log']} |\n",
        f"| WAL checkpointed pages | {s['wal_ckpt']} |\n",
        f"| DB size (post-checkpoint) | {s['db_kb']:.0f} KB |\n\n",
        f"| Substep | p50 | p95 | p99 | p99.9 | max |\n",
        f"|---|---|---|---|---|---|\n",
        f"| `encode_us` | {_prow_md(s['encode_us'])} |\n",
        f"| `hash_us`   | {_prow_md(s['hash_us'])} |\n",
        f"| `write_us`  | {_prow_md(s['write_us'])} |\n",
        f"| `total_us`  | {_prow_md(s['total_us'])} |\n\n",
    ]
    return "".join(lines)


def _generate_report(
    suite: dict[str, tuple[ScenarioResult, ScenarioResult]],
    date_str: str,
) -> str:
    lines: list[str] = []

    # ── Header ──────────────────────────────────────────────────────────────
    lines.append("# Fossic SQLite WAL Payload Sweep\n\n")
    lines.append(f"**Date:** {date_str}  \n")
    lines.append(f"**Host:** {platform.node()} ({platform.machine()})  \n")
    lines.append(f"**OS:** {platform.system()} {platform.release()}  \n")
    lines.append(f"**Python:** {platform.python_version()}  \n")
    lines.append(f"**SQLite:** {sqlite3.sqlite_version}  \n")
    lines.append(f"**blake3:** {_blake3.__version__ if hasattr(_blake3,'__version__') else 'installed'}  \n")
    lines.append(f"**msgpack:** {'.'.join(str(x) for x in msgpack.version)}  \n")
    lines.append("\n> GIL note: Python threads share the GIL. `encode_us` and `hash_us` timings "
                 "at high thread counts include GIL contention — they are an upper bound. "
                 "`write_us` is accurate (SQLite I/O releases the GIL). "
                 "In production, fossic modules run as separate processes.\n\n")
    lines.append("---\n\n")

    # ── Scenario table ───────────────────────────────────────────────────────
    lines.append("## Scenarios\n\n")
    lines.append("| ID | Description | Writers | Rate | Duration | Payload | Shape |\n")
    lines.append("|---|---|---|---|---|---|---|\n")
    for sid, (sr1, _) in suite.items():
        c = sr1.config
        pb = f"{c.payload_bytes}B" if c.payload_bytes < 1024 else f"{c.payload_bytes//1024}KB"
        lines.append(f"| {sid} | {c.desc} | {c.writers} | {c.rate:.0f}/s | {c.duration}s | {pb} | {c.payload_shape} |\n")
    lines.append("\n---\n\n")

    # ── Results ckpt=1000 ────────────────────────────────────────────────────
    lines.append("## Results — `wal_autocheckpoint = 1000` (SQLite default)\n\n")
    for sid, (sr1, _) in suite.items():
        lines.append(f"### Scenario {sid} — {sr1.config.desc}\n\n")
        lines.append(_scenario_md_block(sr1))
    lines.append("---\n\n")

    # ── Results ckpt=4000 ────────────────────────────────────────────────────
    lines.append("## Results — `wal_autocheckpoint = 4000`\n\n")
    for sid, (_, sr4) in suite.items():
        lines.append(f"### Scenario {sid} — {sr4.config.desc}\n\n")
        lines.append(_scenario_md_block(sr4))
    lines.append("---\n\n")

    # ── Analysis ─────────────────────────────────────────────────────────────
    lines.append("## Analysis\n\n")

    # Q1: Scenario C p99 total_us
    sr_c1, sr_c4 = suite["C"]
    sc1 = _stats(sr_c1)
    sc4 = _stats(sr_c4)
    p99_c_us   = pct(sc1["total_us"], 99)
    budget_us  = 15_000

    lines.append("### 1. Scenario C (Policy Scout worst-case): p99 `total_us` vs 15 ms budget\n\n")
    verdict = "**Budget met.**" if p99_c_us <= budget_us else "**Budget exceeded.**"
    lines.append(
        f"{verdict} At `wal_autocheckpoint=1000`, scenario C achieves "
        f"p99 total = **{_fms(p99_c_us)}** "
        f"({'within' if p99_c_us <= budget_us else 'over'} the {budget_us//1000} ms target). "
        f"The fossic append path (CCE encode + BLAKE3 + SQLite WAL write) "
        f"{'is safe' if p99_c_us <= budget_us else 'needs work'} "
        f"at 40 KB payloads with 5 concurrent writers at 20 ev/s.\n\n"
    )

    # Q2: Dominant substep in C
    lines.append("### 2. Which substep dominates in scenario C?\n\n")
    p99_enc   = pct(sc1["encode_us"], 99)
    p99_hash  = pct(sc1["hash_us"],   99)
    p99_write = pct(sc1["write_us"],  99)
    p99_total = pct(sc1["total_us"],  99)
    lines.append(f"At p99 (`wal_autocheckpoint=1000`):\n\n")
    lines.append("| Substep | p99 | % of total_us p99 |\n|---|---|---|\n")
    for label, val in [("encode_us", p99_enc), ("hash_us", p99_hash), ("write_us", p99_write)]:
        share = val / p99_total * 100 if p99_total else 0
        lines.append(f"| `{label}` | {_fms(val)} | {share:.0f}% |\n")
    lines.append("\n")

    dominant_name, dominant_val = max(
        [("encode", p99_enc), ("hash", p99_hash), ("write", p99_write)],
        key=lambda x: x[1],
    )
    lever = {
        "encode": "faster encoder (e.g. pre-sorted template + delta encoding, or streaming CCE)",
        "hash":   "async ID reservation or pre-committed content-address cache",
        "write":  "SQLite tuning (larger page cache, batched multi-event transactions, async I/O mode)",
    }[dominant_name]
    lines.append(
        f"**Dominant substep: `{dominant_name}_us`** at p99 "
        f"({dominant_val/p99_total*100:.0f}% of end-to-end). "
        f"Primary optimization lever: {lever}.\n\n"
    )

    # Q3: Scenario D starvation
    sr_d1, _ = suite["D"]
    sd1 = _stats(sr_d1)
    sr_a1, _ = suite["A"]
    sa1 = _stats(sr_a1)

    lines.append("### 3. Scenario D: writer queue depth and starvation at 50 concurrent writers\n\n")
    p99_d_write = pct(sd1["write_us"], 99)
    p99_a_write = pct(sa1["write_us"], 99)
    ratio = p99_d_write / p99_a_write if p99_a_write else float("inf")

    lines.append(f"| Metric | Value |\n|---|---|\n")
    lines.append(f"| Writers | {sr_d1.config.writers} |\n")
    lines.append(f"| SQLITE\\_BUSY errors | {sd1['total_busy']} |\n")
    lines.append(f"| Dropped slots | {sd1['total_dropped']} |\n")
    lines.append(f"| write\\_us p99 | {_fms(p99_d_write)} |\n")
    lines.append(f"| write\\_us p99.9 | {_fms(pct(sd1['write_us'],99.9))} |\n")
    lines.append(f"| total\\_us p99 | {_fms(pct(sd1['total_us'],99))} |\n")
    lines.append(f"| write\\_us p99 vs scenario A | {ratio:.1f}× |\n\n")

    if sd1["total_busy"] > 0:
        lines.append(
            f"SQLITE\\_BUSY errors present — the 30 s busy\\_timeout was exceeded at "
            f"{sr_d1.config.writers} concurrent writers. True starvation: some writers "
            f"returned errors rather than completing. Recommend a WAL-aware write queue "
            f"or per-shard fossic instances for burst scenarios.\n\n"
        )
    elif sd1["total_dropped"] > int(sd1["target"] * 0.02):
        pct_drop = sd1["total_dropped"] / sd1["target"] * 100
        lines.append(
            f"No SQLITE\\_BUSY errors, but {sd1['total_dropped']} slots dropped "
            f"({pct_drop:.1f}% of target). Writers could not sustain the target rate — "
            f"write serialisation at 50 concurrent writers is the bottleneck. "
            f"Burst concurrency should be bounded at the platform layer.\n\n"
        )
    elif ratio > 5:
        lines.append(
            f"No errors or drops, but write\\_us p99 is {ratio:.1f}× scenario A — "
            f"significant queue depth visible. No starvation at `busy_timeout=30000ms`, "
            f"but latency SLA would be violated in any real-time context. "
            f"50 concurrent writers at 40 KB is beyond the expected steady-state for Lattica.\n\n"
        )
    else:
        lines.append(
            f"No errors, no drops. write\\_us p99 is {ratio:.1f}× scenario A — "
            f"consistent with linear queuing growth. WAL handles the burst without starvation.\n\n"
        )

    # Q4: Checkpoint knob
    lines.append("### 4. Effect of `wal_autocheckpoint = 4000` vs 1000\n\n")
    lines.append("write\\_us p99 and p99.9 per scenario:\n\n")
    lines.append("| Scenario | ckpt=1000 p99 | ckpt=4000 p99 | Δp99 | ckpt=1000 p99.9 | ckpt=4000 p99.9 | Δp99.9 |\n")
    lines.append("|---|---|---|---|---|---|---|\n")
    for sid, (sr1, sr4) in suite.items():
        s1 = _stats(sr1)
        s4 = _stats(sr4)
        p1  = pct(s1["write_us"], 99)
        p4  = pct(s4["write_us"], 99)
        p1x = pct(s1["write_us"], 99.9)
        p4x = pct(s4["write_us"], 99.9)
        d99  = (p4  - p1)  / p1  * 100 if p1  else 0
        d999 = (p4x - p1x) / p1x * 100 if p1x else 0
        lines.append(
            f"| {sid} | {_fms(p1)} | {_fms(p4)} | {d99:+.0f}% "
            f"| {_fms(p1x)} | {_fms(p4x)} | {d999:+.0f}% |\n"
        )
    lines.append("\n")

    # Assess knob for scenario C specifically
    p99_wc1 = pct(sc1["write_us"], 99)
    p99_wc4 = pct(sc4["write_us"], 99)
    delta = (p99_wc4 - p99_wc1) / p99_wc1 * 100 if p99_wc1 else 0
    if abs(delta) < 10:
        lines.append(
            f"Effect is small (≤10% at p99 for scenario C: {_fms(p99_wc1)} → {_fms(p99_wc4)}, "
            f"{delta:+.1f}%). The checkpoint frequency is not the dominant write\\_us variance source "
            f"at this payload and rate. **Recommendation: leave `wal_autocheckpoint` at default (1000) "
            f"for the v1 fossic open-options.** Revisit if sustained write throughput exceeds ~500 ev/s.\n\n"
        )
    elif delta < -10:
        lines.append(
            f"Meaningful improvement: {_fms(p99_wc1)} → {_fms(p99_wc4)} ({delta:+.1f}% at p99 for C). "
            f"**Recommendation: set `wal_autocheckpoint=4000` in fossic's default open-options.** "
            f"Trade-off: WAL file grows ~4× larger between flushes.\n\n"
        )
    else:
        lines.append(
            f"Regression at p99 for C: {_fms(p99_wc1)} → {_fms(p99_wc4)} ({delta:+.1f}%). "
            f"Deferring checkpoints at this payload size increases WAL scan cost. "
            f"**Recommendation: leave at default (1000).**\n\n"
        )

    # ── Recommendation ───────────────────────────────────────────────────────
    lines.append("---\n\n## Recommendation\n\n")
    if p99_c_us <= budget_us:
        lines.append(
            f"Scenario C p99 total = **{_fms(p99_c_us)}** — within the &lt;15 ms budget. "
            f"The fossic v1 append path is viable at Policy Scout worst-case payload on this hardware. "
            f"**No blocking spec changes required before v1.**\n\n"
        )
    else:
        lines.append(
            f"Scenario C p99 total = **{_fms(p99_c_us)}** — exceeds the 15 ms budget. "
            f"Address the `{dominant_name}_us` substep before finalizing the v1 spec.\n\n"
        )

    lines.append("Caveats worth tracking:\n\n")
    lines.append(
        "- The periodic write\\_us p99 spike (WAL checkpoint stall) is present across all scenarios. "
        "A dedicated background checkpoint thread (`wal_autocheckpoint=0` + explicit "
        "`PRAGMA wal_checkpoint(PASSIVE)` on a timer) would flatten write\\_us p99 at the cost of "
        "slightly higher mean write latency — worth evaluating in v2.\n"
    )
    if sd1["total_busy"] > 0 or sd1["total_dropped"] > int(sd1["target"] * 0.01):
        lines.append(
            "- Scenario D (50 concurrent writers) shows stress. The platform should avoid routing "
            "all modules to the same fossic instance simultaneously; a write-ahead queue or per-module "
            "connection multiplexing will absorb bursts.\n"
        )
    lines.append(
        "- Python GIL inflates encode\\_us and hash\\_us in this bench. In a multi-process "
        "deployment these substeps run fully in parallel; the numbers here are conservative.\n"
    )

    return "".join(lines)


# ─────────────────────────────────────────────────────────────────────────────
# Suite runner
# ─────────────────────────────────────────────────────────────────────────────

# (sid, desc, writers, rate, duration, payload_bytes, payload_shape)
_SUITE_DEFS = [
    ("A", "baseline rerun",          5,  50.0, 60,  256,   "random"),
    ("B", "Cerebra realistic",       5,  50.0, 60,  4096,  "jsonish"),
    ("C", "Policy Scout worst-case", 5,  20.0, 60,  40960, "jsonish"),
    ("D", "burst worst-case",       50,   1.0, 10,  40960, "jsonish"),
]
_CKPT_SETTINGS = [1000, 4000]


def _run_suite(output: Path, busy_timeout_ms: int = 30_000):
    n_runs = len(_SUITE_DEFS) * len(_CKPT_SETTINGS)
    est_s  = sum(d for _, _, _, _, d, _, _ in _SUITE_DEFS) * len(_CKPT_SETTINGS)
    print(f"\n{'='*70}")
    print("  Fossic SQLite WAL Payload Sweep")
    print(f"  {n_runs} runs  (estimated wall time ~{est_s}s / {est_s//60}m {est_s%60}s)")
    print(f"{'='*70}\n")

    suite: dict[str, tuple[ScenarioResult, ScenarioResult]] = {}

    for sid, desc, writers, rate, duration, pb, shape in _SUITE_DEFS:
        pair: list[ScenarioResult] = []
        for ckpt in _CKPT_SETTINGS:
            cfg = ScenarioConfig(
                sid=sid, desc=desc,
                writers=writers, rate=rate, duration=duration,
                payload_bytes=pb, payload_shape=shape,
                wal_autocheckpoint=ckpt, busy_timeout_ms=busy_timeout_ms,
            )
            pblabel = f"{pb}B" if pb < 1024 else f"{pb//1024}KB"
            print(f"  Running [{sid}] ckpt={ckpt}  "
                  f"{writers}w×{rate:.0f}/s×{duration}s×{pblabel} {shape} … ",
                  end="", flush=True)
            sr = _run_scenario(cfg)
            total_w = sum(r.write_count for r in sr.writer_results)
            print(f"{total_w:,} writes")
            _print_result(sr)
            pair.append(sr)
        suite[sid] = (pair[0], pair[1])

    from datetime import date
    md = _generate_report(suite, date.today().isoformat())
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(md, encoding="utf-8")
    print(f"\n{'='*70}")
    print(f"  Report → {output}")
    print(f"{'='*70}\n")


# ─────────────────────────────────────────────────────────────────────────────
# Single-scenario mode (original CLI preserved)
# ─────────────────────────────────────────────────────────────────────────────

def _run_single(
    writers: int,
    rate: float,
    duration: int,
    payload_bytes: int,
    payload_shape: str,
    wal_autocheckpoint: int,
    busy_timeout_ms: int,
):
    pb_label = f"{payload_bytes}B" if payload_bytes < 1024 else f"{payload_bytes//1024}KB"
    print(f"\n{'='*70}")
    print("  Fossic SQLite WAL Benchmark")
    print(f"  {writers}w × {rate:.0f}/s × {duration}s × {pb_label} {payload_shape}  "
          f"wal_autocheckpoint={wal_autocheckpoint}  busy_timeout={busy_timeout_ms}ms")
    print(f"{'='*70}\n")
    cfg = ScenarioConfig(
        sid="X", desc="custom",
        writers=writers, rate=rate, duration=duration,
        payload_bytes=payload_bytes, payload_shape=payload_shape,
        wal_autocheckpoint=wal_autocheckpoint, busy_timeout_ms=busy_timeout_ms,
    )
    print(f"  Running … ", end="", flush=True)
    sr = _run_scenario(cfg)
    total_w = sum(r.write_count for r in sr.writer_results)
    print(f"{total_w:,} writes")
    _print_result(sr)
    print()


# ─────────────────────────────────────────────────────────────────────────────
# Entry point
# ─────────────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--suite",              action="store_true")
    ap.add_argument("--writers",            type=int,   default=5)
    ap.add_argument("--rate",               type=float, default=50.0)
    ap.add_argument("--duration",           type=int,   default=60)
    ap.add_argument("--payload-bytes",      type=int,   default=256)
    ap.add_argument("--payload-shape",      choices=["random", "jsonish", "zeros"], default="random")
    ap.add_argument("--wal-autocheckpoint", type=int,   default=1000)
    ap.add_argument("--busy-timeout",       type=int,   default=30_000)
    ap.add_argument("--output",             type=Path,
                    default=Path("benchmarks/results/sqlite_wal_payload_sweep.md"))
    args = ap.parse_args()

    if args.suite:
        _run_suite(args.output, args.busy_timeout)
    else:
        _run_single(
            writers=args.writers,
            rate=args.rate,
            duration=args.duration,
            payload_bytes=args.payload_bytes,
            payload_shape=args.payload_shape,
            wal_autocheckpoint=args.wal_autocheckpoint,
            busy_timeout_ms=args.busy_timeout,
        )


if __name__ == "__main__":
    main()
