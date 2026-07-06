# Contributing to fossic

## System dependencies (Linux)

fossic-tauri (workspace member) requires Tauri 2 system libraries.
Before running `just test`, `cargo test --workspace --all-features`,
or `cargo check --workspace`, install:

    sudo apt-get install -y \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
      librsvg2-dev libssl-dev pkg-config

Without these, workspace builds fail on gdk-sys.

## Bench validation

`.github/workflows/bench-validation.yml` is currently disabled. It
scaffolds a p99 regression check, but the underlying bench script
(benchmarks/sqlite_wal_payload_sweep.py) has not been implemented.
Re-enable the schedule once the script lands and GitHub runner
noise doesn't invalidate the p99 measurements.
