# The build entry points. The /deslag-build-doctrine skill governs this file:
# target names are a verb then a scope, the bare verb does everything, help
# lists build, test, check, clean in that order, and set targets come first.

.DEFAULT_GOAL := help

SCRIPTS := scripts
BLOBSTORE := $(SCRIPTS)/blobstore
EWT := $(SCRIPTS)/ewt
HARPER := $(SCRIPTS)/harper
SPACY := $(SCRIPTS)/spacy
TRAIN := $(SCRIPTS)/train

# The treebank's dev file, in the release ewt.lock pins.
EWT_DEV := .ewt/$(shell awk '$$1 == "release" { print $$2 }' $(EWT)/ewt.lock)/en_ewt-ud-dev.conllu

# Flags for every cargo call. `ci` and `ci-fast` add --locked so a stale
# Cargo.lock fails there instead of being rewritten.
CARGO_FLAGS ?=

.PHONY: help \
        build build-batches build-release \
        test test-blobs test-brill test-brill-deslag test-brill-percept test-ewt test-exam \
        test-owner test-percept test-python test-spacy test-ticlist-brill-deslag \
        test-ticlist-brill-percept test-ticlist-percept \
        check check-clippy check-deslag check-doc check-fmt check-publish check-typos \
        clean clean-blobs clean-ewt clean-harper clean-spacy clean-train \
        ci ci-fast \
        fix fix-blobs fix-catalog fix-clippy fix-fmt fix-golden fix-test-output \
        preflight install \
        fetch-blobs fetch-ewt fetch-harper fetch-spacy generate-brill generate-brill-deslag \
        generate-brill-percept generate-percept generate-spacy publish-blobs

help:
	@echo "build            build deslag and the crates under tools/ with the debug profile"
	@echo "build-batches    build the batches in $(BLOBSTORE)/batches/ the big tier lacks; network, so not in build"
	@echo "build-release    build with the release profile"
	@echo "test             run every Rust test that needs no network, doctests included, and the exam's"
	@echo "                 gates and the owner set's metrics; not test-python, which runs when the scripts it"
	@echo "                 tests change"
	@echo "test-blobs       fetch the corpus's big tier, test it, and fail if tagging takes over its budget"
	@echo "                 of the time to read it; needs the network, so not in test"
	@echo "test-brill       train the Brill tagger on the treebank's train set, grade it on the dev and owner sets with"
	@echo "                 deslag-exam and compare it with deslag, its initial tagger and the perceptron;"
	@echo "                 generates first, so minutes, not in test or ci; run test-percept first for the"
	@echo "                 perceptron's comparison"
	@echo "test-brill-deslag"
	@echo "                 train the Brill tagger that starts from deslag's own readings, grade it on the"
	@echo "                 dev and owner sets with deslag-exam, judge it against the dev gates and compare it"
	@echo "                 with deslag, the perceptron and the first Brill tagger; generates first, so minutes,"
	@echo "                 not in test or ci"
	@echo "test-brill-percept"
	@echo "                 the same for the Brill tagger that starts from the perceptron, a diagnostic; trains"
	@echo "                 the perceptron first, then five more for the folds, so about five minutes"
	@echo "test-ewt         fail if deslag's tagger scores under the pinned counts on the treebank's dev"
	@echo "                 set; fetches the treebank, so the network, and not in test or ci"
	@echo "test-exam        fail if the golden tag stream changed, or deslag's tagger is under a gate on"
	@echo "                 the dev or holdout gold or misses a word of the must-pass list; the holdout"
	@echo "                 prints pass or fail per metric; then the Metrics of test-owner"
	@echo "test-owner       print deslag's score on the owner's hand-tagged gold, tests/gold/owner.conllu, as its own"
	@echo "                 report-only set, never pooled with dev; fails only when the exam cannot run"
	@echo "test-percept     train the perceptron on the treebank's train set, grade it on the dev and owner sets with"
	@echo "                 deslag-exam and compare it with deslag; generates first, so minutes, not in test or ci;"
	@echo "                 the learning curve is $(TRAIN)/run.sh curve"
	@echo "test-python      test how batches are built and published, and the exam's trainers and run.sh; offline,"
	@echo "                 local repositories, about two minutes, so not in test or ci: its own workflow runs it"
	@echo "                 when they change"
	@echo "test-spacy       score spaCy on the treebank's dev set with deslag-exam; generates the import"
	@echo "                 first, so minutes, and not in test or ci"
	@echo "test-ticlist-brill-deslag"
	@echo "                 score the Brill tagger that starts from deslag's readings on the tic list; generates"
	@echo "                 first, so minutes, and not in test or ci"
	@echo "test-ticlist-brill-percept"
	@echo "                 the same for the Brill tagger that starts from the perceptron"
	@echo "test-ticlist-percept"
	@echo "                 score the perceptron on the tic list, tests/gold/ticlist.tsv, with deslag-exam; generates"
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
	@echo "clean-train      remove what the generate-* targets for the learners and their tests wrote"
	@echo "ci               what the GitHub ci job runs: preflight, check, build, test, test-blobs, with"
	@echo "                 --locked; not test-python, which the python workflow runs"
	@echo "ci-fast          the gate to run before a push, about 20 seconds warm: preflight, check, the"
	@echo "                 exam's gates, the owner set's metrics and the Rust tests but the slowest, at once;"
	@echo "                 ci runs the rest"
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
	@echo "generate-brill   train the Brill tagger on the treebank's train set and tag the dev and owner sets into .train,"
	@echo "                 for deslag-exam's --import; fetches the treebank first; minutes, so not in ci"
	@echo "generate-brill-deslag"
	@echo "                 train the Brill tagger on the treebank's train set from deslag's own readings and tag"
	@echo "                 the dev and owner sets into .train; fetches the treebank first; a minute, so not in ci"
	@echo "generate-brill-percept"
	@echo "                 the same from the perceptron's tags, cross-fitted over five folds by document; a few"
	@echo "                 minutes, so not in ci"
	@echo "generate-percept train the perceptron on the treebank's train set and tag the dev and owner sets into .train,"
	@echo "                 for deslag-exam's --import; fetches the treebank first; minutes, so not in ci"
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

test: preflight test-exam
	cargo test $(CARGO_FLAGS) --workspace --all-features

# The big tier's tests are ignored by a plain cargo test, so that test runs
# offline and with no login. The time that follows prints how long reading and
# tagging the tier take, and fails if tagging's share of reading is over the
# budget of the profile built (40.0% in debug, which is what ci runs).
test-blobs: preflight fetch-blobs
	cargo test $(CARGO_FLAGS) --all-features --test blobs -- --ignored
	cargo run $(CARGO_FLAGS) -p deslag-corpus -- --tier blobs time --check

# The Brill tagger's unit tests, then the exam's full report on the dev and owner sets for it and for its
# initial tagger, and `compare` against deslag's tagger, the initial tagger and, if test-percept
# ran, the perceptron, from the import files generate-brill wrote. The runs are saved beside them.
# Not part of test: it needs the treebank and minutes.
test-brill: generate-brill
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh test-brill

# The same for the Brill tagger that starts from deslag's readings and the one that starts from the
# perceptron: the unit tests, then the exam's full report on the dev and owner sets, `compare` against deslag,
# the perceptron and the first Brill tagger as far as their runs exist, and for deslag's dev set the
# gates of tests/gold/gates.toml and the must-pass list, whose misses are printed and do not stop
# the target. Not part of test: it needs the treebank and minutes.
test-brill-deslag: generate-brill-deslag
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh test-brill-deslag

test-brill-percept: generate-brill-percept
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh test-brill-percept

# The treebank's dev set against the counts tests/gold/gates.toml pins for deslag's tagger. Any
# drop fails. Not part of test or ci: it needs the network to fetch the treebank.
test-ewt: preflight fetch-ewt
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- gate --gates tests/gold/gates.toml ewt-dev

# The whole golden binary, not a name filter: a filter that matches nothing passes silently. Then
# the gates of tests/gold/gates.toml on the dev gold, the must-pass list and the holdout gold, which
# print a table of counts for dev, the words missed for mustpass and a pass or fail per metric for
# holdout. A gate is raised by hand, in the change that earns it; there is no fix- target.
test-exam: preflight
	cargo test $(CARGO_FLAGS) -p deslag --all-features --test golden
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- gate --gates tests/gold/gates.toml dev mustpass holdout
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(SCRIPTS)/owner-score.sh --metrics

# The owner's hand-tagged gold, scored on its own and never pooled with the dev gold: it was drawn
# differently (`deslag-gold rank`, the sentences the tagger was least sure of), so pooling biases
# both. It gates nothing; the set is small and its intervals are wide, so this prints a report and
# fails only when the exam cannot run. Not a gates.toml set: `gate` prints counts, with no strata and
# no intervals. Claude proposed the tags of some of these sentences before the owner checked them, so
# a Claude adjudicator's score on this set may read high. test-exam and ci-fast print its Metrics block
# after the gate, so the gate result stays on screen.
test-owner: preflight
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(SCRIPTS)/owner-score.sh

# The perceptron's unit tests, then the exam's full report on the dev and owner sets for it, and `compare`
# against deslag's tagger, from the import files generate-percept wrote. The runs are saved beside
# them. Not part of test: it needs the treebank and minutes. The curve is run by hand.
test-percept: generate-percept
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh test

# The scripts under scripts/blobstore, run against git repositories the tests
# make: no network, no login. Minutes of git, so not in test or ci; the python
# workflow runs it when scripts/blobstore, scripts/llm-detection or scripts/train change. The
# trainers' tests use made-up sentences and stand-ins for cargo, so they need no treebank.
test-python: preflight
	python3 -m unittest discover -b -s $(BLOBSTORE) -p 'test_*.py'
	python3 -m unittest discover -b -s $(TRAIN) -p 'test_*.py'

# The exam's full report for spaCy on the treebank's dev set: the import file from generate-spacy,
# scored on deslag's own tokens. The saved run goes beside it, for `deslag-exam compare`. Not part
# of test: it needs the network, a few GB and minutes.
test-spacy: generate-spacy
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- score --gold $(EWT_DEV) --import .spacy/ewt-dev.import.conllu --save .spacy/ewt-dev.run.json

# The tic list read by each Brill tagger: the token skeleton of the English fixtures, with deslag's
# readings of it for the tagger that starts from them, goes through the tagger generate-brill-deslag
# or generate-brill-percept trained, and `deslag-exam ticlist score` reads the import it writes,
# beside the saved run and report in .train. Not part of test or ci: it needs the treebank and
# minutes.
test-ticlist-brill-deslag: generate-brill-deslag
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh ticlist-brill-deslag

test-ticlist-brill-percept: generate-brill-percept
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh ticlist-brill-percept

# The tic list, tests/gold/ticlist.tsv, read by the perceptron: the token skeleton of the English
# fixtures goes through the perceptron generate-percept trained, and `deslag-exam ticlist score` reads
# the import it writes, beside the saved run and report in .train. Not part of test or ci: it needs the
# treebank and minutes.
test-ticlist-percept: generate-percept
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh ticlist

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

clean: clean-blobs clean-ewt clean-harper clean-spacy clean-train
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

# .train is what the generate-* targets for the learners and their tests write: the baselines, the weights,
# the rules, the import files and the saved runs. All of it derives from the treebank and none of it is committed.
clean-train:
	rm -rf .train

# ---------------------------------------------------------------------------
# ci, fix, preflight, fetch, publish

# A target-specific variable reaches the prerequisites, so every cargo call
# under ci is --locked.
ci: CARGO_FLAGS += --locked
ci: preflight check build test test-blobs

# The pre-push gate. Preflight stays first, as in every gate: it costs nothing and
# is how a new machine or worktree learns what it lacks. The gates and
# test-fast.sh use Cargo.toml's fast profile; test-fast.sh runs every Rust test
# binary but the slowest at once. The slowest binaries, the doctests, the big tier
# and test-python are left to CI.
ci-fast: CARGO_FLAGS += --locked
ci-fast: preflight check
	cargo run $(CARGO_FLAGS) --profile fast --quiet -p deslag-exam -- gate --gates tests/gold/gates.toml dev mustpass holdout
	@CARGO_FLAGS="$(CARGO_FLAGS) --profile fast" $(SCRIPTS)/owner-score.sh --metrics
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(SCRIPTS)/test-fast.sh

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

# The baselines (deslag's tagger and the most-common tag, saved as runs), then the Brill tagger
# trained on the treebank's train set, its rules cut off on the treebank's dev set, tagging deslag's
# own tokens of the dev and owner sets into .train/*.brill.import.conllu, and its initial tagger alone into
# .train/*.brillinit.import.conllu, for `deslag-exam score --import`. The model is
# .train/brill.model.json and the rules .train/brill.rules.txt. Nothing in ci reads or runs it.
generate-brill: preflight fetch-ewt
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh generate-brill

# The baselines, then deslag's own readings of the treebank's train set and the dev and owner sets, from
# `deslag-exam readings`, and the Brill tagger trained on them: deslag's `Sure` words frozen, a rule
# picking only among the tags deslag keeps, its rules cut and its confidence counted on the
# treebank's dev set. It tags the dev and owner sets into .train/*.brilldeslag.import.conllu; the model is
# .train/brilldeslag.model.json, its rules .train/brilldeslag.rules.txt and what its confidence
# rests on .train/brilldeslag.evidence.tsv. Nothing in ci reads or runs it.
generate-brill-deslag: preflight fetch-ewt
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh generate-brill-deslag

# The perceptron generate-percept trains, then the Brill tagger that starts from its tags: each
# training sentence is tagged by a perceptron trained on the other four of five folds, split by
# document, and the rules are cut and the confidence counted on the treebank's dev set. The files
# are .train/*.brillpercept.*. A diagnostic only, since the perceptron ships weights. Nothing in ci
# reads or runs it.
generate-brill-percept: preflight fetch-ewt
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh generate-brill-percept

# The baselines (deslag's tagger and the most-common tag, saved as runs), then the perceptron trained
# on the treebank's train set, its confidence tuned on the treebank's dev set, tagging deslag's own
# tokens of the dev and owner sets into .train/*.percept.import.conllu, for `deslag-exam score --import`. The
# weights are .train/percept.weights.json. Nothing in ci reads or runs it, and training takes a
# minute or two.
generate-percept: preflight fetch-ewt
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh generate

# The exam's file-import path, run on the treebank's dev set: deslag's own tokens go to spaCy, and
# its tags come back as .spacy/ewt-dev.import.conllu, for `deslag-exam score --import`. Nothing in
# ci reads or runs it, and the model takes minutes.
generate-spacy: preflight fetch-ewt fetch-spacy
	@mkdir -p .spacy
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- tokens --gold $(EWT_DEV) --out .spacy/ewt-dev.tokens.conllu
	@$(SPACY)/run.sh tag .spacy/ewt-dev.tokens.conllu .spacy/ewt-dev.import.conllu $(EWT_DEV)

publish-blobs:
	@$(BLOBSTORE)/blobs.sh publish
