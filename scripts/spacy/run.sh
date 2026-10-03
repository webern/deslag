#!/usr/bin/env bash
#
# Runs spaCy on deslag's own tokens, for the exam's file-import path. spaCy is the ceiling
# candidate: it runs offline in Python and its output is imported as a file, so it is graded on the
# same tokens as every other tagger. Nothing in the build, the tests or CI reads or runs this.
#
#   run.sh fetch                          install spaCy and its model into .spacy/venv, unless
#                                         .spacy is already that
#   run.sh tag TOKENS OUT [GOLD]          fill the token file `deslag-exam tokens` wrote, and write
#                                         the import file OUT; with GOLD, also print how often
#                                         spaCy agrees with it
#
# requirements.lock pins spaCy, the model and every dependency by version and sha256; see
# requirements.in. pip installs it with --require-hashes into a venv under .spacy at the repo root,
# a cache that git ignores and that nothing but this script reads. .spacy/stamp holds the lock and
# the Python it was installed with; a stamp that matches is the whole check, so a repeat fetch is
# free. A stamp that differs, or none, rebuilds the venv, and a failure removes it, so nothing
# partial is left behind. Set PYTHON to pick the interpreter; it must be 3.11 or newer.
#
# On Linux torch is the CPU build from download.pytorch.org, because the PyPI one drags in several
# GB of CUDA libraries; the lock pins its hashes too.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$HERE/requirements.lock"
CACHE="$ROOT/.spacy"
VENV="$CACHE/venv"
STAMP="$CACHE/stamp"
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"
LOCK_REL="${HERE#"$ROOT/"}/requirements.lock"

fail() {
    cat >&2 <<MSG

Error in:
$SELF

spaCy is the exam's ceiling candidate and an External Asset, installed into a cache that nothing
else reads. Nothing in the build needs it. A failure has occurred in the script that runs it; it
is pinned in $LOCK_REL.

Error message:
$*
MSG
    exit 1
}

# The interpreter that builds the venv, and its version.
python_bin() {
    local bin="${PYTHON:-python3}"
    command -v "$bin" >/dev/null 2>&1 \
        || fail "$bin is not installed, and it runs spaCy. Install Python 3.11 or newer (brew install python, or your package manager), or set PYTHON."
    printf '%s\n' "$bin"
}

python_version() {
    "$1" -c 'import sys; print("%d.%d.%d" % sys.version_info[:3])'
}

# What the venv was installed from: the lock, and the interpreter's version.
wanted_stamp() {
    cat "$LOCK"
    printf '# python %s\n' "$(python_version "$1")"
}

fetch() {
    [ -f "$LOCK" ] || fail "$LOCK_REL is missing."
    local python
    python="$(python_bin)"
    "$python" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' \
        || fail "$python is Python $(python_version "$python"); the lock needs 3.11 or newer. Set PYTHON."

    if [ -f "$STAMP" ] && [ -x "$VENV/bin/python" ] && [ "$(wanted_stamp "$python")" = "$(cat "$STAMP")" ]; then
        echo "spacy: $(model_name) is already installed"
        return 0
    fi

    rm -rf "$CACHE/venv" "$STAMP"
    mkdir -p "$CACHE"
    # The venv goes if this ends badly. Not local: the trap runs when the script exits.
    installing=1
    trap '[ -z "${installing:-}" ] || rm -rf "$CACHE/venv" "$CACHE/stamp"' EXIT

    echo "spacy: installing the pinned packages into ${VENV#"$ROOT/"}; the model is several hundred MB"
    "$python" -m venv "$VENV" || fail "Could not create a venv with $python."
    # --no-deps because the lock already holds every dependency; --only-binary so that nothing
    # builds from source, and no setup.py runs; --require-hashes so every file is checked.
    PIP_DISABLE_PIP_VERSION_CHECK=1 "$VENV/bin/python" -m pip install --quiet --no-input \
        --require-hashes --no-deps --only-binary :all: \
        --extra-index-url https://download.pytorch.org/whl/cpu \
        -r "$LOCK" \
        || fail "pip could not install $LOCK_REL. Needs PyPI, download.pytorch.org and github.com."
    "$VENV/bin/python" -m pip check >/dev/null \
        || fail "The installed packages do not agree with each other; see: $VENV/bin/python -m pip check"

    wanted_stamp "$python" >"$STAMP"
    installing=
    echo "spacy: $(model_name) is in ${VENV#"$ROOT/"}"
}

# The model's package name, as `import` spells it, from the lock's one en-core-web line.
model_name() {
    local name
    name="$(awk '$1 ~ /^en-core-web-/ { print $1; exit }' "$LOCK")"
    [ -n "$name" ] || fail "$LOCK_REL pins no en-core-web model."
    printf '%s\n' "${name//-/_}"
}

tag() {
    local tokens="${1:?usage: run.sh tag TOKENS OUT [GOLD]}" out="${2:?usage: run.sh tag TOKENS OUT [GOLD]}"
    local gold="${3:-}"
    [ -f "$tokens" ] || fail "$tokens does not exist; deslag-exam tokens writes it."
    [ -x "$VENV/bin/python" ] && [ -f "$STAMP" ] || fail "spaCy is not installed; run: $SELF fetch"
    local args=(--model "$(model_name)" --tokens "$tokens" --out "$out")
    [ -z "$gold" ] || args+=(--gold "$gold")
    "$VENV/bin/python" "$HERE/tag.py" "${args[@]}"
}

case "${1:-}" in
fetch) fetch ;;
tag) shift; tag "$@" ;;
*) fail "usage: run.sh fetch | run.sh tag TOKENS OUT [GOLD]" ;;
esac
