# The build entry points. The /deslag-build-doctrine skill governs this file:
# target names are a verb then a scope, the bare verb does everything, help
# lists build, test, check, clean in that order, and set targets come first.

.DEFAULT_GOAL := help

SCRIPTS := scripts
BLOBSTORE := $(SCRIPTS)/blobstore
EWT := $(SCRIPTS)/ewt
HARPER := $(SCRIPTS)/harper
SPACY := $(SCRIPTS)/spacy

# The treebank's dev file, in the release ewt.lock pins.
EWT_DEV := .ewt/$(shell awk '$$1 == "release" { print $$2 }' $(EWT)/ewt.lock)/en_ewt-ud-dev.conllu

# Flags for every cargo call. `ci` adds --locked so a stale Cargo.lock fails
# there instead of being rewritten.
CARGO_FLAGS ?=

.PHONY: help \
        build build-batches build-release \
        test test-blobs test-ewt test-exam test-scripts test-spacy \
        check check-clippy check-deslag check-doc check-fmt check-publish check-typos \
        clean clean-blobs clean-ewt clean-harper clean-spacy \
        ci \
        fix fix-blobs fix-catalog fix-clippy fix-fmt fix-golden fix-test-output \
        preflight install \
        fetch-blobs fetch-ewt fetch-harper fetch-spacy generate-spacy publish-blobs

help:
	@echo "build            build deslag and the crates under tools/ with the debug profile"
	@echo "build-batches    build the batches in $(BLOBSTORE)/batches/ the big tier lacks; network, so not in build"
	@echo "build-release    build with the release profile"
	@echo "test             run every test that needs no network, doctests included, and the exam's gates"
	@echo "test-blobs       fetch the corpus's big tier, test it, and fail if tagging takes over its budget"
	@echo "                 of the time to read it; needs the network, so not in test"
	@echo "test-ewt         fail if deslag's tagger scores under the pinned counts on the treebank's dev"
	@echo "                 set; fetches the treebank, so the network, and not in test or ci"
	@echo "test-exam        fail if the golden tag stream changed, or deslag's tagger is under a gate on"
	@echo "                 the dev or holdout gold; the holdout prints pass or fail per metric"
	@echo "test-scripts     test how batches are built and published; offline, local repositories"
	@echo "test-spacy       score spaCy on the treebank's dev set with deslag-exam; generates the import"
	@echo "                 first, so minutes, and not in test or ci"
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
	@echo "clean-harper     remove the fetched Harper model"
	@echo "clean-spacy      remove the installed spaCy and what it wrote"
	@echo "ci               what CI runs: preflight, check, build, test, test-blobs, with --locked"
	@echo "fix              apply every automatic fix: fmt, clippy, golden set, test output"
	@echo "fix-blobs        rewrite everything derived from the pinned image: the catalogue and the golden"
	@echo "                 file of list_growth; fetches the image first, needs the network, so not in fix"
	@echo "fix-catalog      rewrite banned_phrases' catalogue counts, and the image it names, from the big tier"
	@echo "fix-clippy       apply clippy's suggested fixes"
	@echo "fix-fmt          rustfmt in place"
	@echo "fix-golden       rewrite tests/golden from what each lint finds in the corpus, and"
	@echo "                 tools/*/tests/golden from what deslag-corpus and deslag-exam print"
	@echo "fix-test-output  rewrite the .stderr and .json files of tests/cases"
	@echo "preflight        report what must be installed before a build can succeed"
	@echo "install          install what preflight reports missing, where cargo can; the rest by hand"
	@echo "fetch-blobs      unpack the image $(BLOBSTORE)/blobs.lock pins into .blobs/unpacked"
	@echo "fetch-ewt        fetch the UD English Web Treebank that $(EWT)/ewt.lock pins into .ewt"
	@echo "fetch-harper     fetch the Harper tagger model that $(HARPER)/harper.lock pins into .harper"
	@echo "fetch-spacy      install the spaCy and model $(SPACY)/requirements.lock pins into .spacy; a few GB"
	@echo "generate-spacy   tag the treebank's dev set with spaCy into .spacy, for deslag-exam's --import;"
	@echo "                 fetches the treebank and spaCy first; minutes, so not in ci"
	@echo "publish-blobs    push .blobs/unpacked as the next image and pin it in blobs.lock"

# ---------------------------------------------------------------------------
# build

build: preflight
	cargo build $(CARGO_FLAGS) --workspace --all-features

# Harvests, stages and packs each batch a manifest names into .blobs/unpacked,
# completing a seed first, and fails unless it comes to what the manifest
# expects. Needs GH_TOKEN or a gh login for GitHub sources. The publish-blobs
# workflow runs this before it publishes; see $(BLOBSTORE)/blobs.md.
build-batches: fetch-blobs
	@$(BLOBSTORE)/batches.sh build

build-release: preflight
	cargo build $(CARGO_FLAGS) --workspace --all-features --release

# ---------------------------------------------------------------------------
# test

test: preflight test-scripts test-exam
	cargo test $(CARGO_FLAGS) --workspace --all-features

# The big tier's tests are ignored by a plain cargo test, so that test runs
# offline and with no login. The time that follows prints how long reading and
# tagging the tier take, and fails if tagging's share of reading is over the
# budget of the profile built (40.0% in debug, which is what ci runs).
test-blobs: preflight fetch-blobs
	cargo test $(CARGO_FLAGS) --all-features --test blobs -- --ignored
	cargo run $(CARGO_FLAGS) -p deslag-corpus -- --tier blobs time --check

# The treebank's dev set against the counts tests/gold/gates.toml pins for deslag's tagger. Any
# drop fails. Not part of test or ci: it needs the network to fetch the treebank.
test-ewt: preflight fetch-ewt
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- gate --gates tests/gold/gates.toml ewt-dev

# The whole golden binary, not a name filter: a filter that matches nothing passes silently. Then
# the gates of tests/gold/gates.toml on the dev and holdout gold, which print a table of counts for
# dev and a pass or fail per metric for holdout. A gate is raised by hand, in the change that earns
# it; there is no fix- target.
test-exam: preflight
	cargo test $(CARGO_FLAGS) -p deslag --all-features --test golden
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- gate --gates tests/gold/gates.toml dev holdout

# The scripts under scripts/blobstore, run against git repositories the tests
# make: no network, no login.
test-scripts: preflight
	python3 -m unittest discover -b -s $(BLOBSTORE) -p 'test_*.py'

# The exam's full report for spaCy on the treebank's dev set: the import file from generate-spacy,
# scored on deslag's own tokens. The saved run goes beside it, for `deslag-exam compare`. Not part
# of test: it needs the network, a few GB and minutes.
test-spacy: generate-spacy
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- score --gold $(EWT_DEV) --import .spacy/ewt-dev.import.conllu --save .spacy/ewt-dev.run.json

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

clean: clean-blobs clean-ewt clean-harper clean-spacy
	cargo clean

# .blobs is what fetch-blobs unpacks and .tools is where blobs.sh installs crane.
clean-blobs:
	rm -rf .blobs .tools

# .ewt is what fetch-ewt downloads, and .ewt.new.* what a killed fetch leaves.
clean-ewt:
	rm -rf .ewt .ewt.new.*

# .harper is what fetch-harper downloads.
clean-harper:
	rm -rf .harper

# .spacy is the venv fetch-spacy installs, with what generate-spacy and test-spacy write beside it.
clean-spacy:
	rm -rf .spacy

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

# What the publish-blobs workflow runs after it publishes, so a new image leaves the branch green: the
# catalogue's counts and measured_on, and the golden file of list_growth, which names the batch of each
# fixture. One cargo run, so the image is read once per test. Accepts whatever the image says, so read
# the diff before committing it. The two files it writes are listed in blobstore/remeasure.sh, which runs it.
fix-blobs: preflight fetch-blobs
	DESLAG_FIX_CATALOG=1 DESLAG_FIX_GOLDEN=1 cargo test $(CARGO_FLAGS) --all-features --test blobs -- --ignored the_catalogue_counts list_growth

# Rewrites the counts of banned_phrases' catalogue from the big tier, and its measured_on from
# blobs.lock, so read the diff before committing it.
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

# Harper's tagger model, from the commit $(HARPER)/harper.lock pins. It is for measuring only and is
# never checked in or shipped. Nothing in ci reads it. A stamp that matches the lock is the whole
# check.
fetch-harper:
	@$(HARPER)/fetch.sh fetch

# spaCy, the exam's ceiling candidate, in a venv under .spacy that only $(SPACY)/run.sh reads, from
# the packages $(SPACY)/requirements.lock pins. A stamp that matches the lock is the whole check.
fetch-spacy:
	@$(SPACY)/run.sh fetch

# The exam's file-import path, run on the treebank's dev set: deslag's own tokens go to spaCy, and
# its tags come back as .spacy/ewt-dev.import.conllu, for `deslag-exam score --import`. Nothing in
# ci reads or runs it, and the model takes minutes.
generate-spacy: preflight fetch-ewt fetch-spacy
	@mkdir -p .spacy
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- tokens --gold $(EWT_DEV) --out .spacy/ewt-dev.tokens.conllu
	@$(SPACY)/run.sh tag .spacy/ewt-dev.tokens.conllu .spacy/ewt-dev.import.conllu $(EWT_DEV)

publish-blobs:
	@$(BLOBSTORE)/blobs.sh publish
