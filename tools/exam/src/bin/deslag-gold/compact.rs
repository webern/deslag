//! The reader for compact tagger lines: `g0001: V.fi _ T V.in _`.
//!
//! The tagger writes one line per sentence, its id, a colon, and one code per token separated by
//! spaces (`_` for a token that is not a word). It never writes CoNLL-U; this module does, from
//! the sample's tokens, the way the guide says a script does. A line that is not what the guide
//! asks for is rejected with its sentence id and what is wrong, and nothing is written if any is.
//! Rejecting is the point: a wrong count shifts every later tag in a sentence, so it cannot be
//! patched. The batch is re-run alone.
//!
//! Blank lines and lines that open or close a code fence are skipped, since a model wraps its
//! answer in one now and then. Anything else that is not a line of the form is a problem.

use std::collections::BTreeMap;

use deslag_exam::error::{Error, Place};

use crate::code::Code;
use crate::data::{Sample, Sent, line, misc, upos_of_kind};
use crate::problems::Problems;

/// What a tagger said of one sentence: a code for each word token and `None` for each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tagged {
    /// The sentence.
    pub id: String,
    /// One entry per token.
    pub codes: Vec<Option<Code>>,
}

/// Why the first code of `codes` is wrong for `sent`, if one is: a word without a tag, a token
/// that is not a word with one, or a code the guide does not have.
fn first_fault(sent: &Sent, codes: &[&str]) -> Option<String> {
    for (index, (tok, code)) in sent.toks.iter().zip(codes).enumerate() {
        let at = index + 1;
        if tok.is_word() {
            if *code == "_" {
                return Some(format!(
                    "token {at} `{}` is a word and needs a tag, not `_`",
                    tok.form
                ));
            }
            if let Err(why) = Code::parse(code) {
                return Some(format!("token {at} `{}`: {why}", tok.form));
            }
        } else if *code != "_" {
            return Some(format!(
                "token {at} `{}` is not a word and takes `_`, not `{code}`",
                tok.form
            ));
        }
    }
    None
}

/// Reads the lines in `text`, which came from `path`, against `sample`. Returns what it could read
/// and a problem for each line it could not.
pub fn parse_lines(path: &str, text: &str, sample: &Sample) -> (Vec<Tagged>, Vec<Error>) {
    let index = sample.index_of();
    let mut read: BTreeMap<usize, Tagged> = BTreeMap::new();
    let mut problems = Vec::new();
    for (at, raw) in text.lines().enumerate() {
        let number = at + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        let Some((id, rest)) = line.split_once(':') else {
            problems.push(Error::at(
                path,
                number,
                "not a line of the form `id: codes`: there is no colon",
            ));
            continue;
        };
        let id = id.trim();
        let Some(&position) = index.get(id) else {
            problems.push(Error::at(
                path,
                number,
                format!("`{id}` is not the id of a sentence of the sample"),
            ));
            continue;
        };
        let sent = &sample.sents[position];
        let given: Vec<&str> = rest.split_whitespace().collect();
        let fault = if read.contains_key(&position) {
            Some("a second line for the sentence".to_string())
        } else if given.len() != sent.toks.len() {
            Some(format!(
                "{} codes for {} tokens",
                given.len(),
                sent.toks.len()
            ))
        } else {
            first_fault(sent, &given)
        };
        if let Some(fault) = fault {
            problems.push(Error::load(
                path,
                Place::Sentence(id.to_string()),
                format!("line {number}: {fault}"),
            ));
            continue;
        }
        let codes = given.iter().map(|code| Code::parse(code).ok()).collect();
        read.insert(
            position,
            Tagged {
                id: id.to_string(),
                codes,
            },
        );
    }
    (read.into_values().collect(), problems)
}

/// The CoNLL-U of one tagged sentence: UPOS and FEATS from the codes, and from token kind for a
/// token that is not a word, with `prov` as each line's `Prov=`.
pub fn conllu(sent: &Sent, tagged: &Tagged, prov: &str) -> String {
    let mut out = format!("# sent_id = {}\n# text = {}\n", sent.id, sent.text());
    for (index, (tok, code)) in sent.toks.iter().zip(&tagged.codes).enumerate() {
        let (upos, feats) = match code {
            Some(code) => (code.upos(&tok.form), code.feats()),
            None => (upos_of_kind(tok.kind), "_".to_string()),
        };
        out.push_str(&line(
            index,
            &tok.form,
            upos,
            &feats,
            &misc(tok, Some(prov)),
        ));
    }
    out.push('\n');
    out
}

/// Reads every file of `files`, each a path and its text, as answers about `sample` and writes
/// them as CoNLL-U with `prov`, in the order of the sample. With `all`, a sentence of the sample
/// with no line is a problem; without it, the missing are counted and left out. Returns the
/// CoNLL-U and how many sentences it holds.
pub fn read_tags(
    sample: &Sample,
    files: &[(String, String)],
    prov: &str,
    all: bool,
) -> Result<(String, usize), Problems> {
    let mut problems = Vec::new();
    let mut by_id: BTreeMap<String, (Tagged, &str)> = BTreeMap::new();
    for (path, text) in files {
        let (read, found) = parse_lines(path, text, sample);
        problems.extend(found);
        for tagged in read {
            if let Some((_, first)) = by_id.get(&tagged.id) {
                problems.push(Problems::sentence(
                    path,
                    &tagged.id,
                    format!("also answered in {first}"),
                ));
            } else {
                by_id.insert(tagged.id.clone(), (tagged, path));
            }
        }
    }
    let missing: Vec<&str> = sample
        .sents
        .iter()
        .filter(|sent| !by_id.contains_key(&sent.id))
        .map(|sent| sent.id.as_str())
        .collect();
    if all && !missing.is_empty() {
        let shown: Vec<&str> = missing.iter().copied().take(8).collect();
        problems.push(Error::load(
            "the answers",
            Place::File,
            format!(
                "{} sentences have no line, among them {}",
                missing.len(),
                shown.join(", ")
            ),
        ));
    }
    let mut out = String::new();
    let mut count = 0;
    for sent in &sample.sents {
        if let Some((tagged, _)) = by_id.get(&sent.id) {
            out.push_str(&conllu(sent, tagged, prov));
            count += 1;
        }
    }
    Problems::check(problems, (out, count))
}

#[cfg(test)]
pub mod tests {
    use deslag::document::TokenKind;

    use super::*;
    use crate::data::tests::tok;
    use crate::data::{Manifest, Meta};
    use deslag_exam::gold::{Split, Tier};
    use deslag_exam::tagger::Context;

    /// `s1`: Run [make ci] to compile [,] and `s2`: Why [:] The user's files.
    pub fn sample() -> Sample {
        let s1 = Sent {
            id: "s1".to_string(),
            toks: vec![
                tok("Run", TokenKind::Word, false),
                tok("make ci", TokenKind::Code, false),
                tok("to", TokenKind::Word, false),
                tok("compile", TokenKind::Word, true),
                tok(",", TokenKind::Punctuation, false),
            ],
        };
        let s2 = Sent {
            id: "s2".to_string(),
            toks: vec![
                tok("Why", TokenKind::Word, true),
                tok(":", TokenKind::Punctuation, false),
                tok("The", TokenKind::Word, false),
                tok("user's", TokenKind::Word, false),
                tok("files", TokenKind::Word, false),
            ],
        };
        let meta = |context| Meta {
            split: Split::Dev,
            tier: Tier::Human,
            context,
            file: "f.md".to_string(),
            repo: "o/r".to_string(),
            license: "MIT".to_string(),
            range: 0..1,
        };
        Sample {
            sents: vec![s1, s2],
            manifest: Manifest {
                header: Vec::new(),
                rows: vec![
                    ("s1".to_string(), meta(Context::Prose)),
                    ("s2".to_string(), meta(Context::ListItem)),
                ],
            },
        }
    }

    const GOOD: &str = "s1: V.fi _ T V.in _\ns2: R _ D N.s N.p\n";

    #[test]
    fn good_lines_become_conllu_with_prov_and_kinds() {
        let (out, count) = read_tags(
            &sample(),
            &[("tags.txt".to_string(), GOOD.to_string())],
            "blind",
            true,
        )
        .unwrap();
        assert_eq!(count, 2);
        let expect = "\
# sent_id = s1
# text = Run make ci to compile,
1\tRun\t_\tVERB\t_\tVerbForm=Fin\t_\t_\t_\tKind=Word|Prov=blind
2\tmake ci\t_\tX\t_\t_\t_\t_\t_\tKind=Code|Prov=blind
3\tto\t_\tPART\t_\t_\t_\t_\t_\tKind=Word|Prov=blind
4\tcompile\t_\tVERB\t_\tVerbForm=Inf\t_\t_\t_\tKind=Word|Prov=blind|SpaceAfter=No
5\t,\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=blind

# sent_id = s2
# text = Why: The user's files
1\tWhy\t_\tADV\t_\t_\t_\t_\t_\tKind=Word|Prov=blind|SpaceAfter=No
2\t:\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=blind
3\tThe\t_\tDET\t_\t_\t_\t_\t_\tKind=Word|Prov=blind
4\tuser's\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=blind
5\tfiles\t_\tNOUN\t_\tNumber=Plur\t_\t_\t_\tKind=Word|Prov=blind

";
        assert_eq!(out, expect);
    }

    #[test]
    fn non_words_get_their_upos_from_their_kind() {
        let sent = Sent {
            id: "k".to_string(),
            toks: vec![
                tok(",", TokenKind::Punctuation, false),
                tok("+", TokenKind::Symbol, false),
                tok("3", TokenKind::Number, false),
                tok("a", TokenKind::Code, false),
                tok("http://x", TokenKind::Url, false),
                tok("<b>", TokenKind::Html, false),
                tok("alt", TokenKind::Image, false),
                tok("1", TokenKind::Footnote, false),
            ],
        };
        let tagged = Tagged {
            id: "k".to_string(),
            codes: vec![None; 8],
        };
        let upos: Vec<String> = conllu(&sent, &tagged, "blind")
            .lines()
            .skip(2)
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').nth(3).unwrap().to_string())
            .collect();
        assert_eq!(upos, ["PUNCT", "SYM", "NUM", "X", "X", "X", "X", "X"]);
    }

    #[test]
    fn a_coordinator_is_cconj_and_the_other_conjunctions_sconj() {
        let sent = Sent {
            id: "c".to_string(),
            toks: vec![
                tok("and", TokenKind::Word, false),
                tok("because", TokenKind::Word, false),
            ],
        };
        let tagged = Tagged {
            id: "c".to_string(),
            codes: vec![Some(Code::parse("C").unwrap()); 2],
        };
        let out = conllu(&sent, &tagged, "blind");
        assert!(out.contains("1\tand\t_\tCCONJ"));
        assert!(out.contains("2\tbecause\t_\tSCONJ"));
    }

    fn rejects(text: &str) -> Vec<String> {
        parse_lines("tags.txt", text, &sample())
            .1
            .iter()
            .map(|e| e.to_string())
            .collect()
    }

    #[test]
    fn a_wrong_count_is_rejected_with_the_sentence_id() {
        assert_eq!(
            rejects("s1: V.fi _ T V.in\n"),
            ["tags.txt: sentence s1: line 1: 4 codes for 5 tokens"]
        );
        assert_eq!(
            rejects("\ns2: R _ D N.s N.p N.p\n"),
            ["tags.txt: sentence s2: line 2: 6 codes for 5 tokens"]
        );
    }

    #[test]
    fn a_bad_code_names_the_token_and_the_rule() {
        let says = |text: &str, expect: &str| {
            let found = rejects(text);
            assert_eq!(found.len(), 1, "{found:?}");
            assert!(found[0].contains("sentence s2"), "{found:?}");
            assert!(
                found[0].contains(expect),
                "{} should say {expect}",
                found[0]
            );
        };
        says("s2: R _ D N N.p", "token 4 `user's`: `N` needs a number");
        says(
            "s2: R _ D N.s Q",
            "token 5 `files`: `Q` is not a code of the guide",
        );
        says(
            "s2: R _ D V.x N.p",
            "token 4 `user's`: `V.x`: a verb form is",
        );
        says(
            "s2: R _ D.s N.s N.p",
            "token 3 `The`: `D.s`: `D` takes no feature",
        );
        says(
            "s2: R _ _ N.s N.p",
            "token 3 `The` is a word and needs a tag",
        );
        says(
            "s2: R N.s D N.s N.p",
            "token 2 `:` is not a word and takes `_`, not `N.s`",
        );
    }

    #[test]
    fn a_line_that_is_not_a_line_or_not_a_sentence_is_rejected_by_line() {
        assert_eq!(
            rejects("here are the tags\n"),
            ["tags.txt:1: not a line of the form `id: codes`: there is no colon"]
        );
        assert_eq!(
            rejects("s9: R\n"),
            ["tags.txt:1: `s9` is not the id of a sentence of the sample"]
        );
        let twice = rejects("s1: V.fi _ T V.in _\ns1: V.fi _ T V.in _\n");
        assert_eq!(
            twice,
            ["tags.txt: sentence s1: line 2: a second line for the sentence"]
        );
    }

    #[test]
    fn code_fences_and_blank_lines_are_skipped() {
        let (read, problems) =
            parse_lines("tags.txt", "```\n\ns1: V.fi _ T V.in _\n```\n", &sample());
        assert!(problems.is_empty());
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].codes[1], None);
        assert_eq!(read[0].codes[0].unwrap().to_string(), "V.fi");
    }

    #[test]
    fn nothing_is_written_when_any_line_is_bad_and_every_bad_line_is_named() {
        let files = [(
            "tags.txt".to_string(),
            "s1: V.fi _ T V.in\ns2: R _ D N.s Q\n".to_string(),
        )];
        let error = read_tags(&sample(), &files, "blind", false).unwrap_err();
        assert_eq!(error.0.len(), 2);
        let shown = error.to_string();
        assert!(shown.contains("sentence s1") && shown.contains("sentence s2"));
    }

    #[test]
    fn missing_sentences_are_a_problem_only_when_all_are_required() {
        let files = [("a.txt".to_string(), "s1: V.fi _ T V.in _\n".to_string())];
        let (out, count) = read_tags(&sample(), &files, "blind", false).unwrap();
        assert_eq!(count, 1);
        assert!(out.contains("sent_id = s1") && !out.contains("sent_id = s2"));
        let error = read_tags(&sample(), &files, "blind", true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("1 sentences have no line, among them s2"),
            "{error}"
        );
    }

    #[test]
    fn answers_split_over_batch_files_are_joined_in_sample_order() {
        let files = [
            ("b2.txt".to_string(), "s2: R _ D N.s N.p\n".to_string()),
            ("b1.txt".to_string(), "s1: V.fi _ T V.in _\n".to_string()),
        ];
        let (out, count) = read_tags(&sample(), &files, "blind", true).unwrap();
        assert_eq!(count, 2);
        assert!(out.find("sent_id = s1").unwrap() < out.find("sent_id = s2").unwrap());
        let twice = [
            files[1].clone(),
            ("b3.txt".to_string(), "s1: V.fi _ T V.in _\n".to_string()),
        ];
        let error = read_tags(&sample(), &twice, "blind", false).unwrap_err();
        assert!(
            error.to_string().contains("also answered in b1.txt"),
            "{error}"
        );
    }

    #[test]
    fn the_output_loads_as_a_skeleton_filled_in() {
        // The reader's CoNLL-U names the same tokens as the sample, so it can be compared with
        // Harper's and spaCy's by line.
        let (out, _) = read_tags(
            &sample(),
            &[("t".to_string(), GOOD.to_string())],
            "blind",
            true,
        )
        .unwrap();
        let sents = crate::data::parse_skeleton("out", &out).unwrap();
        assert_eq!(sents, sample().sents);
    }
}
