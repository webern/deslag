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
LABEL := $(SCRIPTS)/label

# The tools the generate-label-* targets run, built with the release profile.
LABEL_BIN := $(or $(CARGO_TARGET_DIR),target)/release
GOLD_BIN := $(LABEL_BIN)/deslag-gold
EXAM_BIN := $(LABEL_BIN)/deslag-exam

# The cap on what labelling may spend, in dollars, counted over the whole ledger, which lives in a state
# directory shared by every checkout: $LABEL_STATE, or ${XDG_STATE_HOME:-$HOME/.local/state}/deslag-label,
# so a new worktree starts with the spend of the others (the first time, it imports .label/ledger.tsv);
# LABEL_FLAGS go to label.py, to the step a target runs, and in generate-label-cost to its judge step, so
# `--strict` and `--settle-from NAME` work there; LABEL_TAG_FLAGS are the flags of that target's tag step;
# LABEL_INTO is the directory under .label/<set> that judge writes and the report reads;
# LABEL_REPORT_FLAGS go to the report, such as `--versus merge`.
MAX_USD ?= 8
LABEL_FLAGS ?=
# `--limit N` in LABEL_FLAGS belongs to a tag step, which is the one that takes it: generate-label-cost
# sends it there, and the rest of LABEL_FLAGS to its judge step.
LABEL_FLAGS_JOINED = $(subst --limit ,--limit=,$(LABEL_FLAGS))
LABEL_LIMIT = $(subst =, ,$(filter --limit=%,$(LABEL_FLAGS_JOINED)))
LABEL_JUDGE_FLAGS = $(filter-out --limit=%,$(LABEL_FLAGS_JOINED))
LABEL_TAG_FLAGS ?=
LABEL_INTO ?= merge
LABEL_REPORT_FLAGS ?=

# The silver set's run (scripts/label/README.md, Silver). SILVER_DIR holds the draw's parts, part-01 to
# part-NN, and the batch the assembler writes under batch/; SILVER_PREFIX, SILVER_PARTS, SILVER_MIX and
# SILVER_DRAW_FLAGS are the draw (generate-silver-draw); SILVER_MERGE is the merge directory inside each
# part that `finish --trains yes` wrote; SILVER_NAME, as YYYY-MM-DD-slug, names the batch, and
# SILVER_BUILD_FLAGS reach `silver build`, such as --audit, --archive-sha256 and --noise.
SILVER_DIR ?= .label/silver
SILVER_PREFIX ?= sa
SILVER_PARTS ?= 9
SILVER_MIX ?= 1050,650,350,250,850,550,280,220,40,30,15,15
SILVER_DRAW_FLAGS ?= --per-file 3 --per-repo 12
SILVER_MERGE ?= merge
SILVER_NAME ?=
SILVER_BUILD_FLAGS ?=
# The part generate-silver-part labels, as two digits.
PART ?=
# The parts of the draw there are, in order; expanded when a recipe uses it, after the draw.
SILVER_PART_DIRS = $(patsubst %/manifest.tsv,%,$(sort $(wildcard $(SILVER_DIR)/part-*/manifest.tsv)))

# The treebank's dev file, in the release ewt.lock pins.
EWT_DEV := .ewt/$(shell awk '$$1 == "release" { print $$2 }' $(EWT)/ewt.lock)/en_ewt-ud-dev.conllu

# Flags for every cargo call. `ci` and `ci-fast` add --locked so a stale
# Cargo.lock fails there instead of being rewritten.
CARGO_FLAGS ?=

.PHONY: help \
        build build-batches build-release \
        test test-blobs test-brill test-brill-deslag test-brill-percept test-confinement test-ewt \
        test-exam test-label test-owner test-percept test-python test-silver test-spacy \
        test-ticlist-brill-deslag test-ticlist-brill-percept test-ticlist-percept \
        check check-clippy check-deslag check-doc check-fmt check-publish check-release \
        check-typos \
        clean clean-blobs clean-ewt clean-harper clean-label clean-spacy clean-train \
        ci ci-fast \
        fix fix-blobs fix-catalog fix-clippy fix-fmt fix-golden fix-test-output \
        preflight install \
        fetch-blobs fetch-ewt fetch-harper fetch-spacy generate-brill generate-brill-deslag \
        generate-brill-percept generate-label-audit generate-label-cost generate-label-dev \
        generate-label-judge-dev generate-label-judge-owner generate-label-owner \
        generate-label-report-dev generate-label-report-owner generate-label-spacy \
        generate-percept generate-silver-assemble generate-silver-draw generate-silver-part \
        generate-spacy publish-blobs build-label

help:
	@echo "build            build deslag and the crates under tools/ with the debug profile"
	@echo "build-batches    build the batches in $(BLOBSTORE)/batches/ the big tier lacks; network, so not in build"
	@echo "build-release    build with the release profile"
	@echo "test             run every Rust test that needs no network, doctests included, and the exam's"
	@echo "                 gates and the owner set's metrics; not test-python, which runs when the scripts it"
	@echo "                 tests change"
	@echo "test-blobs       fetch the corpus's big tier, test it, check the silver batches with test-silver, and"
	@echo "                 fail if tagging takes over its budget of the time to read it; needs the network, so"
	@echo "                 not in test"
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
	@echo "test-confinement"
	@echo "                 run the real claude as handoff-run runs it, and fail unless it is confined: it reads"
	@echo "                 no decoy outside its directory, writes none outside it, sees no CLAUDE.md and answers"
	@echo "                 the request it was given; stamps .label/confinement.json; needs claude and its login,"
	@echo "                 so not in test or ci; run it before any Opus round"
	@echo "test-ewt         fail if deslag's tagger scores under the pinned counts on the treebank's dev"
	@echo "                 set; fetches the treebank, so the network, and not in test or ci"
	@echo "test-exam        fail if the golden tag stream changed, or deslag's tagger is under a gate on"
	@echo "                 the dev or holdout gold or misses a word of the must-pass list; the holdout"
	@echo "                 prints pass or fail per metric; then the Metrics of test-owner"
	@echo "test-owner       print deslag's score on the owner's hand-tagged gold, tests/gold/owner.conllu, as its own"
	@echo "                 report-only set, never pooled with dev; fails only when the exam cannot run"
	@echo "test-label       score every voter's tags under .label/dev and .label/owner with deslag-exam, as the"
	@echo "                 gold they came from; reads what generate-label-dev and -owner wrote, calls no model"
	@echo "test-percept     train the perceptron on the treebank's train set, grade it on the dev and owner sets with"
	@echo "                 deslag-exam and compare it with deslag; generates first, so minutes, not in test or ci;"
	@echo "                 the learning curve is $(TRAIN)/run.sh curve"
	@echo "test-python      test how batches are built and published, how sentences are labelled with models, and"
	@echo "                 the exam's trainers and run.sh; offline, local repositories, about two minutes, so not"
	@echo "                 in test or ci: its own workflow runs it when they change"
	@echo "test-silver      check every silver batch of the unpacked image against what it recorded, then hold the"
	@echo "                 live ones to the rules that never lapse; passes when the image has no silver; test-blobs"
	@echo "                 runs it"
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
	@echo "check-release    fail while src/changelog.toml has a next release; the release change renames it, so"
	@echo "                 not part of check, and the release workflow runs it"
	@echo "check-typos      spell check the tree"
	@echo "clean            remove everything make created"
	@echo "clean-blobs      remove the fetched big tier, edits not yet published too, and crane"
	@echo "clean-ewt        remove the fetched treebank"
	@echo "clean-harper     remove the fetched Harper model"
	@echo "clean-label      remove what the generate-label-* targets wrote under .label; the ledger of what they"
	@echo "                 spent is in a state directory outside the checkout, and stays (an old"
	@echo "                 .label/ledger.tsv stays too, to be imported once)"
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
	@echo "generate-label-audit"
	@echo "                 pick 50 sentences of .label/draw500's labels at random for the owner to review"
	@echo "generate-label-cost"
	@echo "                 draw 500 training sentences, tag them with every voter and spaCy, adjudicate, and"
	@echo "                 print dollars and minutes per thousand sentences; needs the big tier, spaCy, a key"
	@echo "                 and MAX_USD of budget, so not in ci; LABEL_FLAGS reach its judge step (--strict),"
	@echo "                 LABEL_TAG_FLAGS its tag step"
	@echo "generate-label-dev"
	@echo "                 have the voters tag deslag's dev gold's sentences, blind, into .label/dev; needs"
	@echo "                 OPENROUTER_API_KEY; MAX_USD caps the ledger, kept in a state directory shared by every"
	@echo "                 checkout; LABEL_FLAGS reach label.py"
	@echo "generate-label-judge-dev"
	@echo "                 merge the voters' tags of .label/dev, have Claude settle the disputes, and finish;"
	@echo "                 needs OPENROUTER_API_KEY; LABEL_INTO names the output; with --spacy, merge-spacy,"
	@echo "                 after the plain merge, whose answers it reuses; LABEL_FLAGS=\"--settle-from merge\" with"
	@echo "                 another LABEL_INTO reuses them only where every voter's codes are the same, and only"
	@echo "                 for a merge with the same model voters; LABEL_FLAGS=--strict exits 3 on open items"
	@echo "generate-label-judge-owner"
	@echo "                 the same for .label/owner"
	@echo "generate-label-owner"
	@echo "                 the same for the owner's gold, tests/gold/owner.conllu, into .label/owner"
	@echo "generate-label-report-dev"
	@echo "                 grade .label/dev's merge against the dev gold: each voter, the agreed words, the"
	@echo "                 adjudicated ones and the pipeline, with sentence-bootstrap intervals"
	@echo "                 LABEL_REPORT_FLAGS=\"--versus merge\" adds the paired difference from that merge"
	@echo "generate-label-report-owner"
	@echo "                 the same for .label/owner against the owner's gold"
	@echo "generate-label-spacy"
	@echo "                 tag .label/dev and .label/owner with spaCy and record it as a run for judge"
	@echo "                 --spacy; needs the sets tagged first, and fetches spaCy; minutes"
	@echo "generate-percept train the perceptron on the treebank's train set and tag the dev and owner sets into .train,"
	@echo "                 for deslag-exam's --import; fetches the treebank first; minutes, so not in ci"
	@echo "generate-silver-assemble"
	@echo "                 put the labelled parts under $(SILVER_DIR) together as the batch SILVER_NAME, into"
	@echo "                 $(SILVER_DIR)/batch/SILVER_NAME, and check it; SILVER_BUILD_FLAGS reach silver build"
	@echo "                 (--audit DIR --archive-sha256 SHA for the final build); calls no model"
	@echo "generate-silver-draw"
	@echo "                 draw the silver set's sentences once, dealt into SILVER_PARTS parts under $(SILVER_DIR);"
	@echo "                 reads the big tier; calls no model"
	@echo "generate-silver-part"
	@echo "                 PART=NN: have the three voters tag part NN at once, then spaCy; needs OPENROUTER_API_KEY;"
	@echo "                 MAX_USD caps the ledger; LABEL_FLAGS reach label.py tag; the Opus round after it is"
	@echo "                 label.py judge and handoff-run"
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

# The release builds the generate-label-* targets run: deslag-gold, which reads, merges and grades, and
# deslag-exam, which makes the token files. Not in help: it is a step of the targets that call it.
build-label: preflight
	cargo build $(CARGO_FLAGS) --release -p deslag-exam --bin deslag-gold --bin deslag-exam

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
test-blobs: preflight fetch-blobs test-silver
	cargo test $(CARGO_FLAGS) --all-features --test blobs -- --ignored
	cargo run $(CARGO_FLAGS) -p deslag-corpus -- --tier blobs time --check

# The silver batches of the unpacked image, which need no checkout but the voters' snapshot each carries:
# `silver check` takes each batch, the retired ones too, against what it recorded, so a batch that passed
# once passes forever; `silver standing` holds the live ones to the two rules that never lapse, no
# repository that is reserved now and no fixture that is excluded now, and says how to clear a failure.
# Both pass when the image has no silver/. test-blobs runs it, after the fetch.
test-silver: preflight fetch-blobs
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam --bin deslag-gold -- silver check
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam --bin deslag-gold -- silver standing

# The probe of the confinement the Opus rounds rely on, with the real claude and the invocation handoff-run
# uses: decoys outside its directory, a write outside it, the user's CLAUDE.md, and a control marker in its
# own request. It stamps .label/confinement.json, which handoff-run reads, and fails unless every assertion
# holds. Needs claude on PATH and its login, so not in test or ci; run it before the first Opus round.
test-confinement: preflight
	python3 $(LABEL)/label.py probe-confinement

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

# Every voter's tags in .label/dev and .label/owner, scored by `deslag-exam score --import` against the
# gold the sample was made from, which is the voter's accuracy on it before any merge. Reads what the
# generate-label-* targets wrote and calls no model. Not part of test: it needs them to have run. A voter
# that abstained on a sentence has no tags for it, which the import cannot score: it says so and goes
# on, and generate-label-report-* grades it on the sentences it answered.
test-label: preflight
	@for set in dev owner; do \
	    for tags in .label/$$set/tags/*.conllu; do \
	        [ -f "$$tags" ] || { echo "no tags under .label/$$set; run generate-label-$$set first" >&2; exit 1; }; \
	        echo "$$tags"; \
	        cargo run $(CARGO_FLAGS) --release --quiet -p deslag-exam -- score --gold tests/gold/$$set.conllu --import "$$tags" \
	            || echo "$$tags: not scored here; it may be missing sentences the voter abstained on, which the report grades"; \
	    done; \
	done

# The perceptron's unit tests, then the exam's full report on the dev and owner sets for it, and `compare`
# against deslag's tagger, from the import files generate-percept wrote. The runs are saved beside
# them. Not part of test: it needs the treebank and minutes. The curve is run by hand.
test-percept: generate-percept
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh test

# The scripts under scripts/blobstore, run against git repositories the tests
# make, scripts/train, run with made-up sentences and stand-ins for cargo, and scripts/label,
# run against a stand-in for OpenRouter and the real deslag-gold: no network, no login, no key,
# no treebank. Minutes of git, so not in test or ci; the python workflow runs it when
# scripts/blobstore, scripts/label, scripts/llm-detection, scripts/train or tools/exam change.
test-python: preflight
	cargo build $(CARGO_FLAGS) --quiet -p deslag-exam --bin deslag-gold --bin deslag-exam
	python3 -m unittest discover -b -s $(BLOBSTORE) -p 'test_*.py'
	python3 -m unittest discover -b -s $(TRAIN) -p 'test_*.py'
	python3 -m unittest discover -b -s $(LABEL) -p 'test_*.py'

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

# Fails while src/changelog.toml has a `next` release. The change that bumps the version in
# Cargo.toml renames it, so `check` cannot run this. It runs every ignored test in
# tests/changelog.rs: those are the release-time checks, so a new one there rides along.
check-release: preflight
	cargo test $(CARGO_FLAGS) --all-features --test changelog -- --ignored

check-typos: preflight
	typos

# ---------------------------------------------------------------------------
# clean

clean: clean-blobs clean-ewt clean-harper clean-label clean-spacy clean-train
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

# .label is what the generate-label-* targets write: the voters' replies, tags, merges and reports. The
# ledger of what the models cost lives in the state directory ($LABEL_STATE, or
# ${XDG_STATE_HOME:-$HOME/.local/state}/deslag-label), outside every checkout, so this leaves it alone;
# delete it there by hand to start the count again. A .label/ledger.tsv from before is kept: it is
# imported into the state directory the first time that does not have a ledger.
clean-label:
	@if [ -d .label ]; then find .label -mindepth 1 -maxdepth 1 ! -name ledger.tsv -exec rm -rf {} +; fi

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

# The 50 sentences of the 500-sentence draw's labels that the owner reviews, drawn at random by a seed
# from .label/draw500/merge/labelled.conllu into .label/draw500/merge/audit.conllu, each its own pick_id.
# Run generate-label-cost first. Calls no model.
generate-label-audit: build-label
	$(GOLD_BIN) --dir .label/draw500 audit --count 50

# 500 training sentences, 205 each from the human and llm tiers (120 prose, 45 list-item, 25 heading, 15
# table-cell) and 90 from the mixed tier (30, 25, 20, 15), which is what the mixed tier can give under
# the per-file and per-repository limits: its whole capacity is about 100, and it holds fewer prose
# sentences than the other contexts once the others are drawn. Non-prose contexts are still over half of
# what is drawn, tagged by every voter
# and spaCy, merged and adjudicated by Claude, then the dollars and minutes each run took per thousand
# sentences in .label/draw500/cost.tsv. The one target that spends on sentences nobody grades, and the
# one that fixes the price of labelling the rest. Reads the big tier. Not in ci: it needs the key, spaCy
# and the network. Its labels are the only ones that may train a model (`--trains yes`, which refuses a
# sentence with the text of one of dev or owner). LABEL_FLAGS reach the judge step, so LABEL_FLAGS=--strict
# stops it with exit 3 when an item is left open, instead of leaving its sentence out; the voters' step
# takes LABEL_TAG_FLAGS, and a `--limit N` in LABEL_FLAGS, which only tag takes, so it limits the voters
# to N batches each (a smoke run, whose tags the judge step then refuses). The last line says how many
# words were left out.
generate-label-cost: build-label fetch-blobs fetch-spacy
	$(GOLD_BIN) --dir .label/draw500 draw --prefix cost --mix 120,45,25,15,120,45,25,15,30,25,20,15
	python3 $(LABEL)/label.py tag --dir .label/draw500 --max-usd $(MAX_USD) --gold-bin $(GOLD_BIN) $(LABEL_TAG_FLAGS) $(LABEL_LIMIT)
	$(LABEL)/spacy.sh .label/draw500
	python3 $(LABEL)/label.py judge --dir .label/draw500 --max-usd $(MAX_USD) --gold-bin $(GOLD_BIN) --trains yes $(LABEL_JUDGE_FLAGS)
	python3 $(LABEL)/label.py cost --dir .label/draw500

# Deslag's dev gold's sentences, as tokens with no tags, tagged blind by each voter of
# $(LABEL)/voters.json through OpenRouter into .label/dev/tags. The spend counts against MAX_USD over
# the whole of the ledger in the state directory. Needs OPENROUTER_API_KEY. LABEL_FLAGS reach label.py: try
# LABEL_FLAGS="--voter qwen --limit 1" first, or --dry-run to see a request and send none.
generate-label-dev: build-label
	@mkdir -p .label/dev
	$(EXAM_BIN) tokens --gold tests/gold/dev.conllu --out .label/dev/sample.conllu
	python3 $(LABEL)/label.py tag --dir .label/dev --max-usd $(MAX_USD) --gold-bin $(GOLD_BIN) $(LABEL_FLAGS)

# The voters' tags of .label/dev merged, the disputed words settled by Claude, and the result finished
# into .label/dev/$(LABEL_INTO)/labelled.conllu. For the variant with spaCy as a fourth voter on the part
# of speech, run generate-label-spacy and then LABEL_INTO=merge-spacy LABEL_FLAGS=--spacy: the adjudicator
# answers each item the plain merge also had once, and that answer is reused, so the two differ by voting.
# For the same voters with one run again, write a merge of its own and keep the first untouched:
# LABEL_INTO=merge-again LABEL_FLAGS="--settle-from merge" reuses the plain merge's answer only for an
# item shown with the very same codes from every voter; any other item goes to the adjudicator. A merge
# whose model voters differ from the first's is refused: its answers were given with other codes in view.
# The spaCy variant of that merge: LABEL_INTO=merge-again-spacy LABEL_FLAGS="--spacy --settle-from merge-again".
# A word counts as agreed only when three model voters answered and agree; spaCy is never one of the
# three, and fewer goes to the adjudicator (LABEL_FLAGS=--min-voters N changes that).
# An item the adjudicator never settles leaves its sentence out of labelled.conllu and is counted; add
# --strict to LABEL_FLAGS to exit 3 instead.
generate-label-judge-dev: build-label
	python3 $(LABEL)/label.py judge --dir .label/dev --into $(LABEL_INTO) --max-usd $(MAX_USD) --gold-bin $(GOLD_BIN) $(LABEL_FLAGS)

generate-label-judge-owner: build-label
	python3 $(LABEL)/label.py judge --dir .label/owner --into $(LABEL_INTO) --max-usd $(MAX_USD) --gold-bin $(GOLD_BIN) $(LABEL_FLAGS)

# The same for the owner's gold, tests/gold/owner.conllu, into .label/owner.
generate-label-owner: build-label
	@mkdir -p .label/owner
	$(EXAM_BIN) tokens --gold tests/gold/owner.conllu --out .label/owner/sample.conllu
	python3 $(LABEL)/label.py tag --dir .label/owner --max-usd $(MAX_USD) --gold-bin $(GOLD_BIN) $(LABEL_FLAGS)

# Each voter, the words they agreed on, the words Claude decided and the pipeline, graded on the dev gold
# with 95% sentence-bootstrap intervals, to .label/dev/$(LABEL_INTO)/report.txt and report.tsv. Calls no
# model. After a spaCy variant, LABEL_INTO=merge-spacy grades it; the paired difference from the plain
# merge is LABEL_INTO=merge-spacy LABEL_REPORT_FLAGS="--versus merge"; likewise merge-gemma against merge.
# The report refuses a voter whose tags file a later run has overwritten since the merge.
generate-label-report-dev: build-label
	$(GOLD_BIN) --dir .label/dev report --gold tests/gold/dev.conllu --into $(LABEL_INTO) $(LABEL_REPORT_FLAGS)

generate-label-report-owner: build-label
	$(GOLD_BIN) --dir .label/owner report --gold tests/gold/owner.conllu --into $(LABEL_INTO) $(LABEL_REPORT_FLAGS)

# spaCy's tags of .label/dev and .label/owner, from deslag's own tokens, recorded as the run `spacy`:
# tags/spacy.conllu, and a row of runs.tsv with how long it took. Run after generate-label-dev and
# generate-label-owner. Minutes, and nothing in ci.
generate-label-spacy: build-label fetch-spacy
	$(LABEL)/spacy.sh .label/dev
	$(LABEL)/spacy.sh .label/owner

# The baselines (deslag's tagger and the most-common tag, saved as runs), then the perceptron trained
# on the treebank's train set, its confidence tuned on the treebank's dev set, tagging deslag's own
# tokens of the dev and owner sets into .train/*.percept.import.conllu, for `deslag-exam score --import`. The
# weights are .train/percept.weights.json. Nothing in ci reads or runs it, and training takes a
# minute or two.
generate-percept: preflight fetch-ewt
	@CARGO_FLAGS="$(CARGO_FLAGS)" $(TRAIN)/run.sh generate

# The silver set, drawn once and dealt into SILVER_PARTS parts, $(SILVER_DIR)/part-01 and on, each a draw of its
# own with the same mix to within a sentence. The sentences of an earlier draw are left out with
# SILVER_DRAW_FLAGS="--per-file 3 --per-repo 12 --exclude-draws FILE". Reads the big tier and calls no model.
# Until the batch is published, rank and queue leave the parts' repositories out of an owner's queue.
generate-silver-draw: build-label fetch-blobs
	$(GOLD_BIN) draw --dir $(SILVER_DIR) --prefix $(SILVER_PREFIX) --parts $(SILVER_PARTS) --mix $(SILVER_MIX) $(SILVER_DRAW_FLAGS)

# One part of the silver draw, PART=NN: the voters of $(LABEL)/voters.json tag it at once, each in a process of
# its own (they lock the sample directory and the ledger, which a test shows), then spaCy tags it and is
# recorded as a run. Needs OPENROUTER_API_KEY; MAX_USD caps the ledger over every checkout; LABEL_FLAGS reach
# label.py tag, so LABEL_FLAGS="--limit 1" is a smoke run and --dry-run sends nothing. A voter that fails is
# named, and spaCy does not run. Then the coordinator drives `label.py status --dir $(SILVER_DIR)/part-NN`, the
# judge step and handoff-run for Opus, and `deslag-gold silver build --check-part`.
generate-silver-part: build-label fetch-spacy
	@case "$(PART)" in [0-9][0-9]) ;; *) echo "PART=NN names the part, as two digits, like PART=01" >&2; exit 2;; esac
	@[ -f "$(SILVER_DIR)/part-$(PART)/sample.conllu" ] || { echo "no draw at $(SILVER_DIR)/part-$(PART); run generate-silver-draw first" >&2; exit 1; }
	@voters=$$(python3 -c 'import json, sys; print(" ".join(json.load(open(sys.argv[1]))["voters"]))' $(LABEL)/voters.json) || exit 1; \
	pids=""; names=""; \
	for voter in $$voters; do \
	    python3 $(LABEL)/label.py tag --dir $(SILVER_DIR)/part-$(PART) --voter $$voter --max-usd $(MAX_USD) \
	        --gold-bin $(GOLD_BIN) $(LABEL_FLAGS) & \
	    pids="$$pids $$!"; names="$$names $$voter"; \
	done; \
	failed=""; set -- $$names; \
	for pid in $$pids; do wait $$pid || failed="$$failed $$1"; shift; done; \
	[ -z "$$failed" ] || { echo "the voters that failed:$$failed" >&2; exit 1; }
	$(LABEL)/spacy.sh $(SILVER_DIR)/part-$(PART)

# Every part under $(SILVER_DIR) put together as the batch SILVER_NAME (YYYY-MM-DD-slug) in
# $(SILVER_DIR)/batch/$(SILVER_NAME), the merge of each part being $(SILVER_MERGE). Sentences whose repository became
# reserved after the draw, or whose text is a gold sentence's, are dropped and counted. The batch is written
# only when `silver check` passes on it. The first build has no audit; the final one adds
# SILVER_BUILD_FLAGS="--audit $(SILVER_DIR)/audit --archive-sha256 SHA", and --noise NAME=FILE for each calibration
# report. Calls no model.
generate-silver-assemble: build-label fetch-blobs
	@[ -n "$(SILVER_NAME)" ] || { echo "SILVER_NAME=YYYY-MM-DD-slug names the batch" >&2; exit 2; }
	@[ -n "$(SILVER_PART_DIRS)" ] || { echo "no draw under $(SILVER_DIR); run generate-silver-draw first" >&2; exit 1; }
	$(GOLD_BIN) silver build --name $(SILVER_NAME) $(foreach dir,$(SILVER_PART_DIRS),--part $(dir):$(SILVER_MERGE)) \
	    --out $(SILVER_DIR)/batch/$(SILVER_NAME) $(SILVER_BUILD_FLAGS)

# The exam's file-import path, run on the treebank's dev set: deslag's own tokens go to spaCy, and
# its tags come back as .spacy/ewt-dev.import.conllu, for `deslag-exam score --import`. Nothing in
# ci reads or runs it, and the model takes minutes.
generate-spacy: preflight fetch-ewt fetch-spacy
	@mkdir -p .spacy
	cargo run $(CARGO_FLAGS) --quiet -p deslag-exam -- tokens --gold $(EWT_DEV) --out .spacy/ewt-dev.tokens.conllu
	@$(SPACY)/run.sh tag .spacy/ewt-dev.tokens.conllu .spacy/ewt-dev.import.conllu $(EWT_DEV)

publish-blobs:
	@$(BLOBSTORE)/blobs.sh publish
