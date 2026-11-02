#!/usr/bin/env bash
# Checks that the workspace builds on the Rust release Cargo.toml's rust-version names, so the floor it
# declares is true. The version is read from Cargo.toml, its one source, and installed with rustup,
# minimal profile, if this machine lacks it. Run by `make check-msrv` and by CI's msrv job.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(awk -F'"' '/^rust-version[[:space:]]*=/ { print $2; exit }' Cargo.toml)
[ -n "$version" ] || { echo "check-msrv.sh: no rust-version in Cargo.toml" >&2; exit 2; }

if ! rustup which --toolchain "$version" cargo >/dev/null 2>&1; then
    echo "installing Rust $version"
    rustup toolchain install "$version" --profile minimal
fi

echo "checking on Rust $version"
# --test slow is named on a line of its own because --all-targets leaves out a test declared
# `test = false`, as check-clippy does.
cargo "+$version" check --locked --workspace --all-features --all-targets
cargo "+$version" check --locked --workspace --all-features --test slow
