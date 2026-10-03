# The build entry points. The /deslag-build-doctrine skill governs this file:
# target names are a verb then a scope, the bare verb does everything, help
# lists build, test, check, clean in that order, and set targets come first.

.DEFAULT_GOAL := help

SCRIPTS := scripts
BLOBSTORE := $(SCRIPTS)/blobstore
EWT := $(SCRIPTS)/ewt

# Flags for every cargo call. `ci` adds --locked so a stale Cargo.lock fails
# there instead of being rewritten.
CARGO_FLAGS ?=

.PHONY: help \
        build build-release \
        test test-blobs \
        check check-clippy check-deslag check-doc check-fmt check-publish check-typos \
        clean clean-blobs clean-ewt \
        ci \
        fix fix-catalog fix-clippy fix-fmt fix-golden fix-test-output \
        preflight install \
        fetch-blobs fetch-ewt publish-blobs

help:
	@echo "build            build deslag and the crates under tools/ with the debug profile"
	@echo "build-release    build with the release profile"
	@echo "test             run every test that needs no network, doctests included"
	@echo "test-blobs       fetch the corpus's big tier, test it and time reading and tagging it; needs"
	@echo "                 the network, so not in test"
	@echo "check            run every check that gates CI: fmt, clippy, deslag, doc, typos"
	@echo "check-clippy     clippy with warnings denied, tests included"
	@echo "check-deslag     run deslag on this repository's own Markdown"
	@echo "check-doc        build the docs with warnings denied"
	@echo "check-fmt        rustfmt in check mode"
	@echo "check-publish    cargo publish --dry-run; slow, so not part of check"
	@echo "check-typos      spell check the tree"
	@echo "clean            remove everything make created"
	@echo "clean-blobs      remove the fetched big tier, edits not yet published too, and crane"
	@echo "clean-ewt        remove the fetched treebank"
	@echo "ci               what CI runs: preflight, check, build, test, test-blobs, with --locked"
	@echo "fix              apply every automatic fix: fmt, clippy, golden set, test output"
	@echo "fix-catalog      rewrite banned_phrases' catalogue counts from the big tier"
	@echo "fix-clippy       apply clippy's suggested fixes"
	@echo "fix-fmt          rustfmt in place"
	@echo "fix-golden       rewrite tests/golden from what each lint finds in the corpus, and"
	@echo "                 tools/*/tests/golden from what deslag-corpus and deslag-exam print"
	@echo "fix-test-output  rewrite the .stderr and .json files of tests/cases"
	@echo "preflight        report what must be installed before a build can succeed"
	@echo "install          install what preflight reports missing, where cargo can; the rest by hand"
	@echo "fetch-blobs      unpack the image $(BLOBSTORE)/blobs.lock pins into .blobs/unpacked"
	@echo "fetch-ewt        fetch the UD English Web Treebank that $(EWT)/ewt.lock pins into .ewt"
	@echo "publish-blobs    push .blobs/unpacked as the next image and pin it in blobs.lock"

# ---------------------------------------------------------------------------
# build

build: preflight
	cargo build $(CARGO_FLAGS) --workspace --all-features

build-release: preflight
	cargo build $(CARGO_FLAGS) --workspace --all-features --release

# ---------------------------------------------------------------------------
# test

test: preflight
	cargo test $(CARGO_FLAGS) --workspace --all-features

# The big tier's tests are ignored by a plain cargo test, so that test runs
# offline and with no login. The time that follows asserts nothing: it prints
# how long reading and tagging the tier take into the CI log, for a budget.
test-blobs: preflight fetch-blobs
	cargo test $(CARGO_FLAGS) --all-features --test blobs -- --ignored
	cargo run $(CARGO_FLAGS) -p deslag-corpus -- --tier blobs time

# ---------------------------------------------------------------------------
# check

check: check-fmt check-clippy check-deslag check-doc check-typos

check-clippy: preflight
	cargo clippy $(CARGO_FLAGS) --workspace --all-features --all-targets -- -D warnings

# The debug build of deslag, run against .agents/deslag.toml. list_growth judges
# the change from where origin/main and HEAD meet, which is empty on main.
check-deslag: preflight
	cargo run $(CARGO_FLAGS) --quiet -- check --base origin/main

# rustdoc has warnings of its own, broken links say, that clippy never sees.
check-doc: preflight
	RUSTDOCFLAGS="-D warnings" cargo doc $(CARGO_FLAGS) --workspace --all-features --no-deps

check-fmt: preflight
	cargo fmt -- --check

# Builds from the packaged crate, which catches a file that `exclude` dropped
# but the build needs. Too slow for `check`; the release workflow runs it.
# deslag is the one package that publishes; the crates under tools/ never do.
check-publish: preflight
	cargo publish $(CARGO_FLAGS) --dry-run --all-features -p deslag

check-typos: preflight
	typos

# ---------------------------------------------------------------------------
# clean

clean: clean-blobs clean-ewt
	cargo clean

# .blobs is what fetch-blobs unpacks and .tools is where blobs.sh installs crane.
clean-blobs:
	rm -rf .blobs .tools

# .ewt is what fetch-ewt downloads, and .ewt.new.* what a killed fetch leaves.
clean-ewt:
	rm -rf .ewt .ewt.new.*

# ---------------------------------------------------------------------------
# ci, fix, preflight, fetch, publish

# A target-specific variable reaches the prerequisites, so every cargo call
# under ci is --locked.
ci: CARGO_FLAGS += --locked
ci: preflight check build test test-blobs

fix: fix-fmt fix-clippy fix-golden fix-test-output

fix-clippy: preflight
	cargo clippy $(CARGO_FLAGS) --workspace --all-features --all-targets --fix --allow-dirty --allow-staged

fix-fmt: preflight
	cargo fmt

# Accepts whatever the lints find and deslag-corpus prints now, so read the diff
# before committing it.
fix-golden: preflight
	DESLAG_FIX_GOLDEN=1 cargo test $(CARGO_FLAGS) --workspace --all-features --test golden

# Rewrites the counts of banned_phrases' catalogue from the big tier, so read
# the diff before committing it.
fix-catalog: preflight fetch-blobs
	DESLAG_FIX_CATALOG=1 cargo test $(CARGO_FLAGS) --all-features --test blobs -- --ignored the_catalogue_counts

# Accepts whatever deslag prints now, so read the diff before committing it.
fix-test-output: preflight
	DESLAG_FIX_TEST_OUTPUT=1 cargo test $(CARGO_FLAGS) --all-features --test cases

preflight:
	@$(SCRIPTS)/preflight.sh

# ---------------------------------------------------------------------------
# install

install:
	@$(SCRIPTS)/install.sh

# The corpus's big tier, from the OCI image $(BLOBSTORE)/blobs.lock pins; see
# $(BLOBSTORE)/blobs.md. A stamp that matches the lock is the whole check.
fetch-blobs:
	@$(BLOBSTORE)/blobs.sh fetch

# The treebank the exam grades on, from the release $(EWT)/ewt.lock pins. Nothing in ci reads it. A
# stamp that matches the lock is the whole check.
fetch-ewt:
	@$(EWT)/fetch.sh fetch

publish-blobs:
	@$(BLOBSTORE)/blobs.sh publish
