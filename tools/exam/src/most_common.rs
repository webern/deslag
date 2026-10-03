//! `mct`, the most-common-tag baseline: each word gets the tag it most often has in EWT train, with
//! no context at all. Everything a real tagger scores above it is what context buys.
//!
//! It is built when it runs, from the treebank `make fetch-ewt` leaves in `.ewt/`. Nothing learned
//! from EWT is written down anywhere, and nothing here enters the `deslag` binary: EWT is CC BY-SA,
//! so this is studied, never shipped.
//!
//! - **Folding.** Words are matched by their text in Unicode lower case (`str::to_lowercase`),
//!   in train and in the sentence graded, and by nothing else: no stemming, no stripping of
//!   punctuation or digits.
//! - **Counting.** Train goes through the same alignment as the gold it is graded on, and every
//!   scored token counts once, for its folded text and its gold tag. So it meets `don't` as deslag
//!   splits it, one token, AUX by its first word, and never sees `do` and `n't`. `PUNCT`, `SYM` and
//!   `X` words, words that cannot be aligned and words on no word token are not counted, and
//!   neither are empty nodes or range lines.
//! - **Known word.** Its best guess is the tag it has most often; a tie goes to the tag that comes
//!   first in the report's order. `Sure` when train gave it one tag, else `Unsure`. `kept` is every
//!   tag train gave it.
//! - **Unknown word** (never a scored token of train): the most common tag over all of train, at
//!   `Unknown`, with `kept` holding only that tag.
//! - No features and no score: the baseline has neither to give, so the feature metrics read 0%
//!   and calibration is not printed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use deslag::document::TokenKind;

use crate::align::align_all;
use crate::error::Error;
use crate::gold::Gold;
use crate::tagger::{Sentence, Tagger};
use crate::tags::{Confidence, Features, Reading, Tag, TagSet};

/// The name `--tagger` and the report use.
pub const NAME: &str = "mct";

/// Where `make fetch-ewt` puts the train file, from the root of the repository. The release is
/// the one `scripts/ewt/ewt.lock` pins.
const TRAIN: &str = ".ewt/r2.18/en_ewt-ud-train.conllu";

/// How often each tag was seen, in the order of [`Tag::ALL`].
type Counts = [u32; Tag::ALL.len()];

/// The most-common-tag tagger.
pub struct MostCommonTag {
    /// Counts by folded word. A map in order, so that nothing depends on hashing.
    words: BTreeMap<String, Counts>,
    /// The tag seen most often over every word.
    overall: Tag,
}

impl MostCommonTag {
    /// Built from the EWT train file that `make fetch-ewt` left in `.ewt/` at the repository root.
    pub fn from_cache() -> Result<MostCommonTag, Error> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        MostCommonTag::from_file(&root.join(TRAIN))
    }

    /// Built from the gold file at `path`, which must be there.
    pub fn from_file(path: &Path) -> Result<MostCommonTag, Error> {
        if !path.is_file() {
            return Err(Error::Cannot(format!(
                "{NAME} learns from the EWT train file, which is not at {}; `make fetch-ewt` fetches it",
                clean(path).display()
            )));
        }
        MostCommonTag::from_gold(&Gold::read(path)?)
    }

    /// Built from the scored tokens of `train`, aligned as any gold is.
    pub fn from_gold(train: &Gold) -> Result<MostCommonTag, Error> {
        let mut words: BTreeMap<String, Counts> = BTreeMap::new();
        let mut total = Counts::default();
        for aligned in align_all(train) {
            for scored in &aligned.alignment.scored {
                let text = aligned.tokens[scored.token].text.to_lowercase();
                words.entry(text).or_default()[scored.tag.index()] += 1;
                total[scored.tag.index()] += 1;
            }
        }
        match most_common(&total) {
            Some(overall) => Ok(MostCommonTag { words, overall }),
            None => Err(Error::Cannot(format!(
                "{NAME} needs a train file with at least one scored token, and {} has none",
                train.path
            ))),
        }
    }

    /// What it says of the word `text`.
    fn read(&self, text: &str) -> Reading {
        let Some(counts) = self.words.get(&text.to_lowercase()) else {
            return Reading {
                tag: self.overall,
                features: Features::NONE,
                confidence: Confidence::Unknown,
                kept: TagSet::of(self.overall),
                score: None,
            };
        };
        let kept: TagSet = Tag::ALL
            .into_iter()
            .filter(|tag| counts[tag.index()] > 0)
            .collect();
        Reading {
            tag: most_common(counts).expect("a word in the table was seen at least once"),
            features: Features::NONE,
            confidence: if kept.len() == 1 {
                Confidence::Sure
            } else {
                Confidence::Unsure
            },
            kept,
            score: None,
        }
    }
}

impl Tagger for MostCommonTag {
    fn name(&self) -> &str {
        NAME
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        sentence
            .tokens
            .iter()
            .map(|token| (token.kind == TokenKind::Word).then(|| self.read(&token.text)))
            .collect()
    }
}

/// The tag with the highest count, the first of them in report order on a tie; none if all are 0.
fn most_common(counts: &Counts) -> Option<Tag> {
    let mut best: Option<Tag> = None;
    for tag in Tag::ALL {
        let count = counts[tag.index()];
        if count > 0 && best.is_none_or(|b| count > counts[b.index()]) {
            best = Some(tag);
        }
    }
    best
}

/// `path` without the `..` that reaches the repository root from this crate, for a message.
fn clean(path: &Path) -> PathBuf {
    path.components().fold(PathBuf::new(), |mut clean, part| {
        if part.as_os_str() == ".." && clean.file_name().is_some() {
            clean.pop();
        } else {
            clean.push(part);
        }
        clean
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagger::{Context, run};
    use deslag::document::Token;

    /// A train file of three sentences. Folded, `run` is VERB once and NOUN twice, `the` is DET
    /// twice, `cats` is NOUN once and PROPN once, `don't` is one token and AUX, as its first word
    /// `do` is, and punctuation is not counted. Over all, NOUN is seen 3 times, DET 2, and the rest
    /// once.
    const TRAIN_TEXT: &str = "\
# sent_id = t1
# text = Run the Cats.
1\tRun\t_\tVERB\t_\t_\t_\t_\t_\t_
2\tthe\t_\tDET\t_\t_\t_\t_\t_\t_
3\tCats\t_\tNOUN\t_\t_\t_\t_\t_\tSpaceAfter=No
4\t.\t_\tPUNCT\t_\t_\t_\t_\t_\t_

# sent_id = t2
# text = The run, Cats
1\tThe\t_\tDET\t_\t_\t_\t_\t_\t_
2\trun\t_\tNOUN\t_\t_\t_\t_\t_\tSpaceAfter=No
3\t,\t_\tPUNCT\t_\t_\t_\t_\t_\t_
4\tCats\t_\tPROPN\t_\t_\t_\t_\t_\t_

# sent_id = t3
# text = don't run
1-2\tdon't\t_\t_\t_\t_\t_\t_\t_\t_
1\tdo\t_\tAUX\t_\t_\t_\t_\t_\t_
2\tn't\t_\tPART\t_\t_\t_\t_\t_\t_
3\trun\t_\tNOUN\t_\t_\t_\t_\t_\t_
";

    fn tagger() -> MostCommonTag {
        let gold = Gold::parse("train.conllu", "train.conllu", TRAIN_TEXT).unwrap();
        MostCommonTag::from_gold(&gold).unwrap()
    }

    fn read(tagger: &MostCommonTag, text: &str) -> Vec<Option<Reading>> {
        let tokens = Token::split(text);
        let sentence = Sentence {
            text,
            tokens: &tokens,
            context: Context::Prose,
        };
        run(tagger, "s1", &sentence).unwrap()
    }

    #[test]
    fn a_word_takes_its_most_common_tag_folded_to_lower_case() {
        let tagger = tagger();
        // VERB once and NOUN twice, so NOUN, `Unsure`.
        let run = tagger.read("RUN");
        assert_eq!(run.tag, Tag::Noun);
        assert_eq!(run.confidence, Confidence::Unsure);
        assert_eq!(run.kept, [Tag::Noun, Tag::Verb].into_iter().collect());
        assert_eq!(run.features, Features::NONE);
        assert_eq!(run.score, None);
    }

    #[test]
    fn a_word_with_one_tag_is_sure() {
        let the = tagger().read("The");
        assert_eq!(the.tag, Tag::Determiner);
        assert_eq!(the.confidence, Confidence::Sure);
        assert_eq!(the.kept, TagSet::of(Tag::Determiner));
    }

    #[test]
    fn a_tie_goes_to_the_tag_first_in_report_order() {
        // Cats: NOUN once, PROPN once. NOUN comes first.
        let cats = tagger().read("cats");
        assert_eq!(cats.tag, Tag::Noun);
        assert_eq!(cats.confidence, Confidence::Unsure);
    }

    #[test]
    fn an_unknown_word_gets_the_most_common_tag_overall_at_unknown() {
        // NOUN is seen 3 times over all of train, DET 2, the rest once.
        let tagger = tagger();
        assert_eq!(tagger.overall, Tag::Noun);
        let unknown = tagger.read("zebra");
        assert_eq!(unknown.tag, Tag::Noun);
        assert_eq!(unknown.confidence, Confidence::Unknown);
        assert_eq!(unknown.kept, TagSet::of(Tag::Noun));
    }

    #[test]
    fn a_contraction_counts_as_one_token_tagged_by_its_first_word() {
        let tagger = tagger();
        let contraction = tagger.read("don't");
        assert_eq!(contraction.tag, Tag::Auxiliary);
        assert_eq!(contraction.confidence, Confidence::Sure);
        assert_eq!(contraction.kept, TagSet::of(Tag::Auxiliary));
        // Its parts were never tokens, so deslag never meets them apart.
        assert_eq!(tagger.read("do").confidence, Confidence::Unknown);
        assert_eq!(tagger.read("n't").confidence, Confidence::Unknown);
    }

    #[test]
    fn punctuation_is_not_counted() {
        // PUNCT is never counted, so `,` would be an unknown word, if it were ever asked.
        assert_eq!(tagger().read(",").confidence, Confidence::Unknown);
    }

    #[test]
    fn a_word_that_cannot_be_aligned_is_not_counted() {
        // `e-mail` is one gold word over three tokens, so neither it nor its parts are learned.
        let text = "# sent_id = u1\n# text = Send e-mail\n\
            1\tSend\t_\tVERB\t_\t_\t_\t_\t_\t_\n\
            2\te-mail\t_\tNOUN\t_\t_\t_\t_\t_\t_\n";
        let gold = Gold::parse("u.conllu", "u.conllu", text).unwrap();
        let tagger = MostCommonTag::from_gold(&gold).unwrap();
        assert_eq!(tagger.read("send").confidence, Confidence::Sure);
        for part in ["e-mail", "e", "mail"] {
            assert_eq!(tagger.read(part).confidence, Confidence::Unknown, "{part}");
        }
    }

    #[test]
    fn it_reads_words_and_only_words() {
        let readings = read(&tagger(), "Run the 2 Cats, zebra");
        let confidence: Vec<Option<Confidence>> =
            readings.iter().map(|r| r.map(|r| r.confidence)).collect();
        assert_eq!(
            confidence,
            [
                Some(Confidence::Unsure),
                Some(Confidence::Sure),
                None,
                Some(Confidence::Unsure),
                None,
                Some(Confidence::Unknown),
            ]
        );
    }

    #[test]
    fn a_train_file_without_tagged_words_is_refused() {
        let text = "# sent_id = a\n# text = .\n1\t.\t_\tPUNCT\t_\t_\t_\t_\t_\t_\n";
        let gold = Gold::parse("p.conllu", "p.conllu", text).unwrap();
        let error = MostCommonTag::from_gold(&gold).err().unwrap().to_string();
        assert!(error.contains("scored token"), "{error}");
        assert!(error.contains("p.conllu has none"), "{error}");
    }

    #[test]
    fn a_missing_train_file_says_how_to_fetch_it() {
        let error = MostCommonTag::from_file(Path::new("/no/such/train.conllu"))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("/no/such/train.conllu"), "{error}");
        assert!(error.contains("make fetch-ewt"), "{error}");
    }

    #[test]
    fn the_message_names_the_cache_without_dots() {
        let path = Path::new("/repo/tools/exam/../../.ewt/r2.18/en_ewt-ud-train.conllu");
        assert_eq!(
            clean(path),
            Path::new("/repo/.ewt/r2.18/en_ewt-ud-train.conllu")
        );
    }
}
