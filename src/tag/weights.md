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
that read the rules' own reading. It is 3,526,736 bytes of UTF-8 text. Every number in it is an
integer, so the Rust reader sums with `i64` and uses no floating point. The format is described at
the top of `scripts/train/weights.py`.

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

Each sentence was first read by deslag's rules, version 11, and the model learned to correct that
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
  is `685bfa1c47182437393ca65fee77f6656a431ae0081e3bfb85db041264738f06`. The two readings files it
  trained on have sha256 `fe335d9cf7f906bd22d85127012968e5a0c689479af2e67683fd7283940444c7` (the
  treebank's train file) and `17644617e1d638fc7a7c3093eea0d92b9131617dea6a979f7362df4ca3ffb704`
  (silver's train split).
- To write the file again, run `make generate-shapes` and then `make generate-weights`, with
  Python 3.13. Nothing else makes the file, and a change to it is a change to the tagger, so raise
  `deslag::tag::VERSION` and run `make fix-golden`.

## How it scores

The Rust tagger gives the same answers as the Python import that the exam graded before this stage
shipped: no word of the dev set, the treebank's dev set, the owner's set or the corpus decides
differently (the tests keep a golden file of decisions to check it). The scores
below are the exam's counts on the dev gold, with the rules alone (version 11) against the shipped
tagger (version 12).

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
  differently from the Python import. The golden file of the stage holds none, and the tests check
  that Python 3.13 writes the same digit ranges again.
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
