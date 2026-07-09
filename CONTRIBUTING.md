# Contributing to fossic

Thanks for your interest in contributing.

## License

By submitting a pull request, you agree that your contribution will be
licensed under the [Apache License, Version 2.0][apache-2.0] — the same
license as this project.

You do not lose the rights to your contribution by submitting it; the
Apache-2.0 license grants the project (and downstream users) the same
rights as everyone else has to use, modify, and distribute your work.

## Developer Certificate of Origin (DCO)

All commits must be signed off, attesting to the [Developer Certificate of
Origin][dco]:

```
Signed-off-by: Your Name <your.email@example.com>
```

Use `git commit -s` (or `git commit --signoff`) to add this line
automatically. This is a lightweight alternative to a full Contributor
License Agreement and serves the same provenance function: it says that
you have the right to contribute the code you're submitting, and that
you're submitting it under this project's license.

If you forget to sign off, `git commit --amend --signoff` (or, for many
commits, `git rebase --signoff HEAD~N`) can fix it before pushing.

## How to contribute

1. Fork the repository.
2. Create a branch for your change.
3. Make your change. Add tests where relevant.
4. Ensure the existing test suite passes.
5. Sign off your commits (`git commit -s`).
6. Open a pull request. Describe what your change does and why.

Small, focused changes are easier to review. If you're planning a large
change, please open an issue first to discuss the approach.

## System dependencies (Linux)

`fossic-tauri` (workspace member) requires Tauri 2 system libraries.
Before running `just test`, `cargo test --workspace --all-features`,
or `cargo check --workspace`, install:

```sh
sudo apt-get install -y \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libssl-dev pkg-config
```

Without these, workspace builds fail on `gdk-sys`.

## Code style

See the project's existing conventions in `.pre-commit-config.yaml` and
the tool-config blocks in the workspace `Cargo.toml`. Run the local
linters and formatters before pushing:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Bench validation

`.github/workflows/bench-validation.yml` is currently disabled. It
scaffolds a p99 regression check, but the underlying bench script
(`benchmarks/sqlite_wal_payload_sweep.py`) has not been implemented.
Re-enable the schedule once the script lands and GitHub runner
noise doesn't invalidate the p99 measurements.

## Status

This project is in active alpha. APIs may change without notice between
minor versions. Contributions of any size are welcome, but please
understand that responses may be slow as this is a single-maintainer
project.

[apache-2.0]: https://www.apache.org/licenses/LICENSE-2.0
[dco]: https://developercertificate.org/
