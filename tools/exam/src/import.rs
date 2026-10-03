//! Grading another program's tags: `score --import FILE`.
//!
//! `tokens` writes a skeleton, one CoNLL-U sentence per gold sentence and one line per deslag
//! token. A program fills it, on every `Word` line: `UPOS` (UD codes, through the one mapping of
//! [`crate::tags`]), and optionally `FEATS` (UD features) and the `MISC` keys `Conf=` (`Sure`,
//! `Likely`, `Unsure` or `Unknown`, default `Likely`, since an outside tagger makes no claim to
//! `Sure`'s meaning), `Score=` (0.0 to 1.0) and `Kept=` (deslag codes, comma-separated). Lines that
//! are not `Word` tokens are not read, whatever they hold.
//!
//! The file must have the gold's `sent_id`s in the gold's order and the skeleton's `FORM`s line for
//! line; otherwise the exam cannot run, and says where the first difference is. A `Word` line
//! tagged `PUNCT`, `SYM` or `X` becomes `Noun` at `Unknown`, and [`Imported::outside`] counts them.

use std::path::Path;

use deslag::document::TokenKind;

use crate::conllu::{self, Block, Id, Line};
use crate::error::Error;
use crate::gold::Gold;
use crate::tags::{Class, Confidence, Features, Reading, Tag, TagSet, map_upos};

/// An imported file, checked against its gold and read into one reading per word token.
#[derive(Debug, Clone)]
pub struct Imported {
    /// What the report calls the tagger.
    pub name: String,
    /// The readings of each sentence, one per token: `Some` on each `Word` token.
    pub readings: Vec<Vec<Option<Reading>>>,
    /// The `Word` lines tagged `PUNCT`, `SYM` or `X`, which the exam reads as `Noun` at `Unknown`.
    pub outside: usize,
}

impl Imported {
    /// Reads the file at `path`, which fills the skeleton of `gold`. When `quiet`, an error names
    /// the sentence's position and the line but never a word or a `sent_id`, since the gold may be
    /// holdout text.
    pub fn read(path: &Path, gold: &Gold, quiet: bool) -> Result<Imported, Error> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: shown.clone(),
            source,
        })?;
        let name = path.file_name().map_or_else(
            || shown.clone(),
            |name| format!("import:{}", name.to_string_lossy()),
        );
        let mut imported = Imported::parse(&shown, &text, gold, quiet)?;
        imported.name = name;
        Ok(imported)
    }

    /// Reads `text`, the contents of the file `path`.
    pub fn parse(path: &str, text: &str, gold: &Gold, quiet: bool) -> Result<Imported, Error> {
        let blocks = conllu::read(path, text)?;
        let fail = |line: usize, message: String| Err(Error::at(path, line, message));
        if blocks.len() < gold.sentences.len() {
            return fail(
                blocks.last().map_or(1, last_line),
                format!(
                    "the file ends after {} sentences, and the gold has {}",
                    blocks.len(),
                    gold.sentences.len()
                ),
            );
        }
        if blocks.len() > gold.sentences.len() {
            return fail(
                blocks[gold.sentences.len()].first_line,
                format!("more sentences than the gold's {}", gold.sentences.len()),
            );
        }
        let mut imported = Imported {
            name: String::new(),
            readings: Vec::with_capacity(blocks.len()),
            outside: 0,
        };
        for (index, (block, sentence)) in blocks.iter().zip(&gold.sentences).enumerate() {
            let at = block.first_line;
            let position = index + 1;
            let sent_id = block
                .comment("sent_id")
                .map(|comment| comment.value.as_str());
            if sent_id != Some(sentence.sent_id.as_str()) {
                return fail(
                    block.comment("sent_id").map_or(at, |comment| comment.line),
                    if quiet {
                        format!("sentence {position}: the sent_id differs from the gold's")
                    } else {
                        format!(
                            "sentence {position} is `{}` where the gold's is `{}`",
                            sent_id.unwrap_or(""),
                            sentence.sent_id
                        )
                    },
                );
            }
            let tokens = sentence.tokens();
            if block.lines.len() != tokens.len() {
                return fail(
                    at,
                    format!(
                        "sentence {position} has {} lines where the skeleton has {}",
                        block.lines.len(),
                        tokens.len()
                    ),
                );
            }
            let mut readings = Vec::with_capacity(tokens.len());
            for (line, token) in block.lines.iter().zip(&tokens) {
                if !matches!(line.id, Id::Word(_)) {
                    return fail(
                        line.number,
                        "a range line or an empty node, which the skeleton has none of".into(),
                    );
                }
                if line.form != token.text {
                    return fail(
                        line.number,
                        if quiet {
                            format!("sentence {position}: the FORM differs from the skeleton's")
                        } else {
                            format!(
                                "FORM `{}` where the skeleton has `{}`",
                                line.form, token.text
                            )
                        },
                    );
                }
                readings.push(if token.kind == TokenKind::Word {
                    Some(reading(path, line, &mut imported.outside)?)
                } else {
                    None
                });
            }
            imported.readings.push(readings);
        }
        Ok(imported)
    }
}

fn last_line(block: &Block) -> usize {
    block.lines.last().map_or(block.first_line, |l| l.number)
}

/// The reading a filled `Word` line says; a tag outside the 13 counts in `outside`.
fn reading(path: &str, line: &Line, outside: &mut usize) -> Result<Reading, Error> {
    let bad = |message: String| Error::at(path, line.number, message);
    let tag = match map_upos(&line.upos) {
        Some(Class::Tagged(tag)) => tag,
        Some(Class::Punctuation | Class::X) => {
            *outside += 1;
            return Ok(Reading {
                tag: Tag::Noun,
                features: Features::NONE,
                confidence: Confidence::Unknown,
                kept: TagSet::EMPTY,
                score: None,
            });
        }
        None if line.upos == "_" => return Err(bad("a Word line with no UPOS".into())),
        None => {
            return Err(bad(format!(
                "UPOS `{}` is not one of the 17 UD tags",
                line.upos
            )));
        }
    };
    let features = Features::from_ud(&line.feats).map_err(bad)?;
    let mut confidence = Confidence::Likely;
    let mut score = None;
    let mut kept = TagSet::EMPTY;
    for (key, value) in conllu::pairs(&line.misc) {
        match key {
            "Conf" => {
                confidence = Confidence::from_name(value).ok_or_else(|| {
                    bad(format!(
                        "Conf `{value}` is not Sure, Likely, Unsure or Unknown"
                    ))
                })?;
            }
            "Score" => {
                let number: f32 = value
                    .parse()
                    .map_err(|_| bad(format!("Score `{value}` is not a number")))?;
                if !(0.0..=1.0).contains(&number) {
                    return Err(bad(format!("Score {value} is outside 0.0 to 1.0")));
                }
                score = Some(number);
            }
            "Kept" => {
                kept = value
                    .split(',')
                    .filter(|code| !code.is_empty())
                    .map(|code| {
                        Tag::from_code(code).ok_or_else(|| {
                            bad(format!("Kept names `{code}`, which is no tag code"))
                        })
                    })
                    .collect::<Result<TagSet, Error>>()?;
            }
            _ => {}
        }
    }
    Ok(Reading {
        tag,
        features,
        confidence,
        kept,
        score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::skeleton;

    const GOLD: &str = "# sent_id = a\n# text = Go now, 2 times\n\
        1\tGo\t_\tVERB\t_\t_\t_\t_\t_\t_\n2\tnow\t_\tADV\t_\t_\t_\t_\t_\tSpaceAfter=No\n\
        3\t,\t_\tPUNCT\t_\t_\t_\t_\t_\t_\n4\t2\t_\tNUM\t_\t_\t_\t_\t_\t_\n5\ttimes\t_\tNOUN\t_\t_\t_\t_\t_\t_\n";

    fn gold() -> Gold {
        Gold::parse("g.conllu", "g.conllu", GOLD).unwrap()
    }

    /// The skeleton with `UPOS`, `FEATS` and `MISC` set on line `n` (counted from 1).
    fn fill(skeleton: &str, edits: &[(usize, &str, &str, &str)]) -> String {
        let mut out = String::new();
        let mut word = 0;
        for line in skeleton.lines() {
            if line.starts_with('#') || line.is_empty() {
                out.push_str(line);
            } else {
                word += 1;
                let mut columns: Vec<String> = line.split('\t').map(str::to_string).collect();
                if let Some((_, upos, feats, misc)) = edits.iter().find(|e| e.0 == word) {
                    columns[3] = upos.to_string();
                    columns[5] = feats.to_string();
                    if !misc.is_empty() {
                        columns[9] = format!("{}|{misc}", columns[9]);
                    }
                }
                out.push_str(&columns.join("\t"));
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn a_filled_skeleton_reads_into_readings_on_word_tokens() {
        let gold = gold();
        let text = fill(
            &skeleton(&gold),
            &[
                (
                    1,
                    "VERB",
                    "VerbForm=Inf",
                    "Conf=Sure|Score=0.9|Kept=VERB,NOUN",
                ),
                (2, "ADV", "_", ""),
                (5, "NOUN", "Number=Plur", "Conf=Unsure"),
            ],
        );
        let imported = Imported::parse("f", &text, &gold, false).unwrap();
        let readings = &imported.readings[0];
        assert_eq!(readings.len(), 5);
        let go = readings[0].unwrap();
        assert_eq!(go.tag, Tag::Verb);
        assert_eq!(go.features, Features::INFINITIVE);
        assert_eq!(go.confidence, Confidence::Sure);
        assert_eq!(go.score, Some(0.9));
        assert_eq!(go.kept, TagSet::of(Tag::Verb).with(Tag::Noun));
        let now = readings[1].unwrap();
        assert_eq!(now.confidence, Confidence::Likely, "the default");
        assert_eq!((now.score, now.kept), (None, TagSet::EMPTY));
        assert!(readings[2].is_none(), "the comma is no word");
        assert!(
            readings[3].is_none(),
            "nor is the number, whatever it is tagged"
        );
        assert_eq!(readings[4].unwrap().confidence, Confidence::Unsure);
        assert_eq!(imported.outside, 0);
    }

    #[test]
    fn a_word_tagged_punct_sym_or_x_is_a_noun_at_unknown_and_counted() {
        let gold = gold();
        let text = fill(
            &skeleton(&gold),
            &[
                (1, "VERB", "_", ""),
                (2, "PUNCT", "_", "Conf=Sure|Score=0.5"),
                (5, "X", "_", ""),
            ],
        );
        let imported = Imported::parse("f", &text, &gold, false).unwrap();
        for index in [1, 4] {
            let reading = imported.readings[0][index].unwrap();
            assert_eq!(reading.tag, Tag::Noun);
            assert_eq!(reading.confidence, Confidence::Unknown);
            assert_eq!((reading.score, reading.kept), (None, TagSet::EMPTY));
        }
        assert_eq!(imported.outside, 2);
    }

    fn error_of(text: &str, quiet: bool) -> String {
        Imported::parse("f", text, &gold(), quiet)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn a_changed_form_names_its_line() {
        let text = fill(
            &skeleton(&gold()),
            &[
                (1, "VERB", "_", ""),
                (2, "ADV", "_", ""),
                (5, "NOUN", "_", ""),
            ],
        )
        .replace("\tGo\t", "\tGone\t");
        let error = error_of(&text, false);
        assert!(error.starts_with("f:3:"), "{error}");
        assert!(
            error.contains("FORM `Gone` where the skeleton has `Go`"),
            "{error}"
        );
        let quiet = error_of(&text, true);
        assert!(
            quiet.contains("sentence 1") && !quiet.contains("Gone") && !quiet.contains("Go`"),
            "{quiet}"
        );
    }

    #[test]
    fn the_other_ways_a_file_can_differ_are_named() {
        let filled = fill(
            &skeleton(&gold()),
            &[
                (1, "VERB", "_", ""),
                (2, "ADV", "_", ""),
                (5, "NOUN", "_", ""),
            ],
        );
        let cases = [
            (
                filled.replace("sent_id = a", "sent_id = b"),
                "sentence 1 is `b` where the gold's is `a`",
            ),
            (String::new(), "ends after 0 sentences"),
            (
                format!("{filled}\n{filled}"),
                "more sentences than the gold's 1",
            ),
            (
                filled.replace("5\ttimes", "5\ttimes\t_\t_\t_\t_\t_\t_\t_\n6\textra"),
                "expected 10 tab-separated columns",
            ),
            (
                filled
                    .lines()
                    .filter(|l| !l.starts_with("5\t"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                "has 4 lines where the skeleton has 5",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "_", "_", "")]),
                "a Word line with no UPOS",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "NN", "_", "")]),
                "UPOS `NN` is not one of the 17 UD tags",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "VERB", "_", "Conf=Maybe")]),
                "Conf `Maybe`",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "VERB", "_", "Score=1.5")]),
                "Score 1.5 is outside",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "VERB", "_", "Score=high")]),
                "Score `high` is not a number",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "VERB", "_", "Kept=VERB,NN")]),
                "Kept names `NN`",
            ),
            (
                fill(&skeleton(&gold()), &[(1, "VERB", "Number", "")]),
                "FEATS entry `Number` has no value",
            ),
        ];
        for (text, expect) in cases {
            let error = error_of(&text, false);
            assert!(error.starts_with("f:"), "{error}");
            assert!(error.contains(expect), "{error} should say {expect}");
        }
    }

    #[test]
    fn a_missing_file_is_an_io_error_and_the_name_is_the_file_s() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Imported::read(&dir.path().join("none.conllu"), &gold(), false),
            Err(Error::Io { .. })
        ));
        let path = dir.path().join("spacy.conllu");
        let gold = gold();
        std::fs::write(
            &path,
            fill(
                &skeleton(&gold),
                &[
                    (1, "VERB", "_", ""),
                    (2, "ADV", "_", ""),
                    (5, "NOUN", "_", ""),
                ],
            ),
        )
        .unwrap();
        assert_eq!(
            Imported::read(&path, &gold, false).unwrap().name,
            "import:spacy.conllu"
        );
    }
}
