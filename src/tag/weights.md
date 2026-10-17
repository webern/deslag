# The learned stage's weights

`src/tag/weights.txt` holds the weights of deslag's learned tagging stage, which `src/tag/learned.rs`
reads after the rules. This is its datasheet: what it was trained on, how, what it scores, and
under what licence it is shared.

## What it is

The file is a hybrid averaged perceptron, written as integers. It chooses among the tags the
rules left open for a word. A word the rules call `Sure` keeps its reading, and a word with one
candidate keeps it. The perceptron scores every other word over the tags the rules kept for it, and
sets its confidence from the margin between its best score and the second. It never adds a tag the
rules did not keep.

The file has a header, the normal forms the model was trained on (20,214 words), and 87,179 rows of
weights for 25 feature templates: 21 of the word, its neighbours and the tags before it, and four
that read the rules' own reading. It is 3,526,814 bytes of UTF-8 text. Every number in it is an
integer, so the Rust reader sums with `i64` and uses no floating point. The format is described at
the top of `scripts/train/weights.py`.

The file is a deliberate exception to the build doctrine's rule of thumb that text data over 1 MB
is an External Asset fetched outside git: it is three and a half times that. The build makes the
binary's tables from it, so the build needs it offline, and it is the model as it was trained and
graded. Pruning it to fit would ship a different model from the one that was measured. Each
retraining adds about 1.3 MB, packed, to git's history.

## What it costs

Measured on macOS x86-64 (an i9-9980HK) with the machine idle, release builds unless named:

- Binary: `deslag` is 18,185,048 bytes. The tables built from this file are in it, and not the
  file's text: the weights are 3,891,851 bytes, the word table of both lexicons 4,194,312 and the
  index of the lexicon's lines 524,288.
- Build: `build.rs` parses the file and builds the tables, so no run does. A clean debug build of
  `deslag` takes 32.4 s and a clean release build 61.2 s.
- First use: none. The reader finds the tables where they lie in the binary, and a run touches only
  the pages it reads.
- Per run: `deslag check` of one three-line Markdown file takes 24 ms in a debug build and 11 ms in
  a release build (medians of 21 runs).
- Tagging's share of reading the corpus's big tier, which `cargo run -p deslag-corpus -- time
  --check` holds under 75%, is 71.5% in debug. That check keeps each file's fastest pass, so it
  leaves out any setup.

## Training data

- The English Web Treebank, release r2.18 of Universal Dependencies, commit
  `b7711cce01cdd4f5fcc0a8199b8a50d951b16c0c`, train file only: 12,544 sentences. The exam aligned its
  words to deslag's tokens and its tags to deslag's 13 codes. The dev and test files never trained
  anything.
- The silver batch `2026-10-08-silver`, train split only: 4,395 sentences of its 4,794. The
  batch's own datasheet gives its sources, its licences and how right its labels are. The labels
  are a model's, voted by three open-weight models and settled by a stronger one, and were checked
  by the owner on a sample.
- The dev gold, the holdout and the owner's set trained nothing. The silver batch's tune split (399
  sentences) chose the cutoffs below and the mapping of margins to scores, and nothing else.

Each sentence was first read by deslag's rules, version 12, and the model learned to correct that
reading. That is why the weights say which rules version they belong to (`rules_version` in the
header) and why the tagger runs them only after the rules of that version.

The 663 repositories of the silver batch carry licences from MIT, Apache-2.0, BSD, ISC, CC0-1.0,
Unlicense, MIT-0 and Zlib, and some CC-BY-4.0. None is a non-commercial, no-derivatives or copyleft
code licence. The weights hold words and numbers, not sentences.

## How it was trained

- Mode: hybrid, with the rules' reading as features.
- Seed: 20261004. Passes: 10 over 16,939 sentences, each pass in an order shuffled by the seed.
- Steps: 1,394,830. A weight is stored as its total over all steps, not its average, and the file
  says the step count, so a score is a total and a margin is compared as a whole number.
- The three cutoffs are whole numbers over the steps, each a margin the tuning set showed. A margin
  of at least 75,460,458 is `Sure`. Below 49,944,573 the word is `Unsure`, or stays `Likely` when
  it is the rules' own tag and the rules were `Likely`. Tags within 49,944,573 of the best score
  stay in the word's kept set when it is `Unsure`.
- The exporter, `scripts/train/weights.py`, writes the model as it is. It retrains nothing and
  prunes nothing, and it refuses a model whose cutoffs are not whole numbers over the steps.
- The model's file is `.train/shapes/p-hyb-s.model.json` after `make generate-shapes`. Its sha256
  is `4ade3302e849ca3255135d553ef02b36df093e8c6d4857b142634c756b43e0f9`. The two readings files it
  trained on have sha256 `2e7f239c16a77d1a21ba02a801047ea38a688c1b2a22ad6f53cc19a6dee91dab` (the
  treebank's train file) and `68e2c9353571606d328cd0c57d66f6ec91c43bdff6f6342ec60f21c35628bcca`
  (silver's train split).
- The file's header names the model's sha256 (`model_sha256`). `weights.py check --model` refuses
  a model file with another hash, and `make generate-weights` runs it after each export.

## Training it again

The weights belong to one version of the rules, `deslag::tag::RULES_VERSION`, and name it in their
header. `tests/golden/rules.txt` holds a hash of what the rules read of `tests/corpus/core/`, and
fails a change to it until `RULES_VERSION` is raised and `make fix-golden` run. Then:

1. Copy `.train/shapes` to a directory of its own first (`cp -a .train/shapes .train/shapes-r11`):
   `make generate-shapes` writes over the models of the last training, and a retraining on the
   same features should be compared with them. Then `make generate-shapes`, under Python 3.13,
   trains the models from the rules of the new version. The tools build while the weights are
   behind the rules.
2. `make generate-weights`, with Python 3.13, writes this file from the hybrid model and checks it
   against the model. Nothing else makes the file. When the features did not change, only
   `rules_version` and `model_sha256` differ from the last file, and `python3
   scripts/train/weights.py same --old .train/shapes-r11/p-hyb-s.model.json --new
   .train/shapes/p-hyb-s.model.json` says whether the new model's weights, vocabulary, steps and
   tuning are the old one's.
3. A change to the file is a change to the tagger: raise `deslag::tag::VERSION`, run
   `make fix-golden`, and update the counts and costs here.
4. Read the diff of `tests/golden/verbs_no_nouns.txt` after `make fix-golden`. The lint reads the
   tagger's readings, so a retrain can move its findings, the unit tests in
   `src/lint/verbs_no_nouns.rs`, `tests/cases/verbs_no_nouns/forms.stderr`, the tree test in
   `tests/corpus.rs` and the big tier's ceiling in `tests/blobs.rs`. If findings moved, add a
   changelog entry under `src/changelog/releases/next/`. If a ceiling or ratio test fails, stop
   and ask a human: no limit moves on a retrain's say-so.

While the weights lag a `RULES_VERSION` bump the learned stage is off, so a `make fix-golden` run
then, such as the one before step 1, records what the rules alone find in
`tests/golden/verbs_no_nouns.txt`. Restore that file with `git restore` and fix it only after
step 3.

A build whose weights name another rules version than its own does not run the learned stage: its
tagger is the rules alone. A test fails on such a build, so CI never passes one, and none ships.
Two gates of the dev gold have no slack at this model's counts (`clean_sentences` and
`likely_accuracy`), so a retrained model that loses one sentence or one Likely word fails them.

## How it scores

The Rust tagger gives the same answers as the Python import that the exam graded before this stage
shipped. CI checks it on the `core` fixtures of the corpus and on synthetic Unicode cases: a golden
file of decisions, `tools/exam/tests/golden/learned.txt`, which `cargo test` holds the Rust to and
`make test-python` holds the Python to. A comparison run once, when the file was written, found no
word that decides differently in the dev set (3,138 words), the treebank's dev set (21,315), a set
the maintainer labelled (583), or the corpus (840,484). The scores
below are the exam's counts on the dev gold, with the rules alone (version 12) against the shipped
tagger (version 13).

| measure | rules alone | with the learned stage |
| --- | --- | --- |
| best guess right, dev | 2702 of 3119 | 2890 of 3119 |
| gold tag kept, dev | 2993 of 3119 | 2991 of 3119 |
| committed (Sure or Likely), dev | 1494 | 2094 |
| committed and right, dev | 1482 | 2052 |
| `Sure` and right, dev | 999 of 1008 | 1539 of 1559 |
| `Likely` and right, dev | 483 of 486 | 513 of 535 |
| `Unknown`, dev | 203 | 83 |
| clean sentences, dev | 286 of 296 | 263 of 296 |
| best guess right, treebank dev | 18,466 of 21,149 | 19,728 of 21,149 |
| gold tag kept, treebank dev | 20,394 of 21,149 | 20,346 of 21,149 |

The model's own `Score` is not shipped: the file holds no calibration, so a word's `score` is
`None` and any calibration differs by construction from the Python import's.

## Known limits

- The model learned from English prose, headings and lists in Markdown. It is weaker on text that is
  not, and on text with many words it has not seen: a word outside the 20,214 the model knows, and
  unknown to the rules, stays `Unknown`.
- The perceptron lowercases and reads the shape of a word with Rust's Unicode tables (version 17),
  and Python's `isdigit` and `isdecimal` tables (version 15.1) are written into the file. About 55
  code points case differently between the two versions. A word made of such a character can decide
  differently from the Python import. No test checks that the golden file of the stage holds none
  of them; the tests check that Python 3.13 writes the same digit ranges again.
- The `Unsure` width and the cutoffs were tuned on 399 sentences, so they are good to a few words.
- The labels of the silver batch are a model's. A label that is 96 or 97 percent right bounds what
  the stage can learn from it.

## Licence

The weights were learned in part from the annotations of the English Web Treebank, which are
licensed under CC BY-SA 4.0. The file `src/tag/weights.txt` is a work adapted from them and is
shared under CC BY-SA 4.0, with the treebank's attribution in `LICENSES/CC-BY-SA-4.0.txt`. The
rest of the crate is under MIT, so the crate's licence is `MIT AND CC-BY-SA-4.0`. The silver
batch's annotations are published under MIT, and its texts are quoted under their own licences,
which its datasheet lists.

The weights hold words and pairs of words from the silver batch's texts. Eleven of its source
repositories offer their Markdown under CC BY 4.0, alone or beside another licence, and CC BY 4.0
asks for attribution on an adapted work. `LICENSES/CC-BY-4.0.txt` names them, with the licence text.
CC BY 4.0 lets an adapter share an adapted work under its own licence where that does not stop
anyone from meeting CC BY's terms; CC BY-SA 4.0 does not, so the file stays under CC BY-SA 4.0
and the crate's licence expression is unchanged.
