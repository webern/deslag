#!/usr/bin/env bash
# Reports every missing build prerequisite in one pass, with how to install it.
# Runs before every build, so it only checks that tools exist, and with a
# version, such as 3.13, that python3 is that version, for the targets that
# need it. A C compiler and archiver are among them: tools/sweep builds
# tree-sitter's C through cc, and the crates that compile C use the same one.
# rustup installs the toolchain and components rust-toolchain.toml names on
# first use. install.sh sets DESLAG_FROM_INSTALL when it runs this, so the
# closing hint does not send the user back to make install.

# No -e: a missing tool must not stop the scan.
set -uo pipefail

python_version=${1:-}
missing=0

need() {
    local tool=$1 hint=$2
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "missing: $tool  ($hint)" >&2
        missing=1
    fi
}

need rustup "https://rustup.rs"
need cargo "installed by rustup"
need typos "cargo install typos-cli, or brew install typos-cli"
need git "https://git-scm.com/downloads; the --diff tests build repositories"
need python3 "https://www.python.org/downloads; make test-python, and collect.py, need it"

# The compiler cc-using crates run: $CC if set, else cc. $CC may carry flags, so take its first word.
read -r cc _ <<<"${CC:-cc}"
if [ "$(uname -s)" = Darwin ]; then
    cc_hint="xcode-select --install"
else
    cc_hint="a C compiler: build-essential (apt), gcc (dnf), or set CC"
fi
need "$cc" "$cc_hint"
need ar "$cc_hint"
# /usr/bin/cc exists on a Mac without the command line tools, and fails when run.
if [ "$(uname -s)" = Darwin ] && ! xcode-select -p >/dev/null 2>&1; then
    echo "missing: the Xcode command line tools  (xcode-select --install)" >&2
    missing=1
fi

if [ -n "$python_version" ] && command -v python3 >/dev/null 2>&1; then
    found=$(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])')
    if [ "$found" != "$python_version" ]; then
        echo "wrong version: python3 is $found, and this target needs $python_version (https://www.python.org/downloads, or put a python3 of that version first on PATH)" >&2
        missing=1
    fi
fi

if [ "$missing" -ne 0 ]; then
    if [ -n "${DESLAG_FROM_INSTALL:-}" ]; then
        echo "install what is listed above by hand, then run make again" >&2
    else
        echo "run make install to install what it can, or install what is listed above, then run make again" >&2
    fi
    exit 1
fi
