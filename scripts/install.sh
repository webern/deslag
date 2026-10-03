#!/usr/bin/env bash
# Installs the Installed Software that preflight.sh checks for and this
# machine lacks, where a tool the repo can drive is there to do it. rustup,
# cargo and git come first and from outside: a machine without them gets the
# same hint preflight gives, and nothing is downloaded behind the user's back
# for them. typos is a cargo install, which takes a few minutes from source;
# CI uses a prebuilt binary instead, which is why the version is not pinned
# here and tracks the latest release like CI does.

set -euo pipefail

installed=0

have() {
    command -v "$1" >/dev/null 2>&1
}

if ! have rustup || ! have cargo; then
    echo "rustup and cargo are not something this script installs: see https://rustup.rs," >&2
    echo "then run make install again." >&2
    exit 1
fi

if ! have git; then
    echo "git is not something this script installs: https://git-scm.com/downloads" >&2
    exit 1
fi

if ! have typos; then
    echo "installing typos with cargo"
    cargo install typos-cli --locked
    installed=1
fi

if [ "$installed" -eq 0 ]; then
    echo "nothing to install"
fi
