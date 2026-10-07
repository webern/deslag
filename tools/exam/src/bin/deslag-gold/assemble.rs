//! Final assembly: the agreed words and the adjudicated ones, put into the gold files.
//!
//! `agreed.conllu` has every sentence with the words the three taggers agreed on filled in
//! (`Prov=agree`) and the disputed words blank. The adjudication log (`adjudicated.tsv`) holds the
//! code decided for each blank word. Assembly puts them together and writes, beside each other and
//! apart:
//!
//! - `dev.conllu` and `holdout.conllu`, `exam.tokens = deslag` files whose first sentence names the
//!   split, `exam.trains`, and the seed and corpus the sample was drawn from, and whose every
//!   sentence names its tier, its context and the corpus file it was quoted from. The holdout
//!   header says in words that it is never training data, and the exam refuses a holdout file
//!   that does not say `exam.trains = no`. The dev file says `undecided`: whether labels a model
//!   made may train one is still open.
//! - `dev.disputes.tsv` and `holdout.disputes.tsv`, with no dispute open.
//! - `dev.adjudication.tsv` and `holdout.adjudication.tsv`, the log of each split, and
//!   `dev.manifest.tsv` and `holdout.manifest.tsv`, the sample's manifest of each, which hold the
//!   seed. Every artifact is made per split, so a holdout word, answer, reason, id or byte range
//!   is only in a holdout file.
//! - `accuracy.tsv`, how each tagger's answers stand against the result, on dev, holdout and
//!   both, in counts and rates only.
//!
//! A word with neither an agreed nor an adjudicated tag, or an adjudicated one that was already
//! agreed, is a problem and nothing is written. The files are read back with the exam's own loader
//! before assembly says it is done.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use deslag_exam::conllu::{self, Id};
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::{Split, Tier};

use crate::code::Code;
use crate::data::{Manifest, Meta, Sample, line, misc};
use crate::merge::{Answers, Logged, Verdict, judge};
use crate::problems::Problems;

/// Where a gold word's tag came from, as `Prov=` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prov {
    /// The three taggers agreed.
    Agree,
    /// No tagger decided it: a token that is not a word, tagged from its kind.
    Kind,
    /// A model decided.
    Adjudicated,
}

impl Prov {
    fn name(self) -> &'static str {
        match self {
            Prov::Agree => "agree",
            Prov::Kind => "kind",
            Prov::Adjudicated => "adjudicated",
        }
    }
}

/// One finished line of a gold sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filled {
    /// The UPOS.
    pub upos: String,
    /// The FEATS.
    pub feats: String,
    /// Where it came from.
    pub prov: Prov,
    /// For a word, its code as the guide writes it.
    pub code: Option<Code>,
    /// The runs that vouch for it, as `Runs=` names them: the agreeing voters' for an agreed word,
    /// the adjudicator's for an adjudicated one. `None` for a gold set built by hand.
    pub runs: Option<String>,
}

/// The gold sentences, in the order of the sample.
#[derive(Debug, Clone)]
pub struct Built {
    /// One row of lines per sentence of the sample.
    pub sentences: Vec<Vec<Filled>>,
}

/// Puts the adjudicated words of `log` into the blanks of `agreed`, which came from `agreed_path`.
pub fn build(
    sample: &Sample,
    agreed_path: &str,
    agreed: &str,
    log: &[Logged],
) -> Result<Built, Problems> {
    let blocks = conllu::read(agreed_path, agreed)?;
    let mut by_id: BTreeMap<&str, &conllu::Block> = BTreeMap::new();
    for block in &blocks {
        if let Some(comment) = block.comment("sent_id") {
            by_id.insert(comment.value.as_str(), block);
        }
    }
    let mut decided: BTreeMap<(&str, usize), (&Logged, bool)> = BTreeMap::new();
    let mut problems = Vec::new();
    for row in log {
        let key = (row.sent_id.as_str(), row.token);
        if decided.insert(key, (row, false)).is_some() {
            problems.push(Problems::sentence(
                "the adjudication log",
                &row.sent_id,
                format!("token {} is in the log twice", row.token),
            ));
        }
    }
    let mut sentences = Vec::with_capacity(sample.sents.len());
    for sent in &sample.sents {
        let Some(block) = by_id.get(sent.id.as_str()) else {
            problems.push(Problems::sentence(
                agreed_path,
                &sent.id,
                "it is not in the agreed file",
            ));
            continue;
        };
        if block.lines.len() != sent.toks.len() {
            problems.push(Problems::sentence(
                agreed_path,
                &sent.id,
                format!("{} lines for {} tokens", block.lines.len(), sent.toks.len()),
            ));
            continue;
        }
        let mut lines = Vec::with_capacity(sent.toks.len());
        for (index, (tok, read)) in sent.toks.iter().zip(&block.lines).enumerate() {
            let at = index + 1;
            if !matches!(read.id, Id::Word(_)) || read.form != tok.form {
                problems.push(Problems::sentence(
                    agreed_path,
                    &sent.id,
                    format!("token {at} is not `{}`", tok.form),
                ));
                continue;
            }
            let said_prov = conllu::pairs(&read.misc)
                .iter()
                .find(|(key, _)| *key == "Prov")
                .map(|(_, value)| *value);
            // A word is agreed by the taggers; a token that is not a word is tagged from its kind.
            let agreed_here = matches!(
                (said_prov, tok.is_word()),
                (Some("agree"), true) | (Some("kind"), false)
            );
            let logged = decided.get_mut(&(sent.id.as_str(), at));
            // An agreed word may be in the log as an override: the log row says what the three
            // agreed on, and that is the word's agreed code, which it now changes.
            let agreed_code = if agreed_here && tok.is_word() {
                Code::from_conllu(&read.upos, &read.feats).ok()
            } else {
                None
            };
            match (agreed_here, logged) {
                (true, Some((row, used)))
                    if agreed_code.is_some()
                        && row.agreed == agreed_code
                        && Some(row.code) != agreed_code
                        && row.form == tok.form =>
                {
                    *used = true;
                    lines.push(Filled {
                        upos: row.code.upos(&tok.form).to_string(),
                        feats: row.code.feats(),
                        prov: Prov::Adjudicated,
                        code: Some(row.code),
                        runs: row.run.clone(),
                    });
                }
                (true, Some(_)) => problems.push(Problems::sentence(
                    "the adjudication log",
                    &sent.id,
                    format!("token {at} `{}` was agreed, and is in the log", tok.form),
                )),
                (true, None) => {
                    let code = if tok.is_word() {
                        match Code::from_conllu(&read.upos, &read.feats) {
                            Ok(code) => Some(code),
                            Err(why) => {
                                problems.push(Problems::sentence(
                                    agreed_path,
                                    &sent.id,
                                    format!("token {at} `{}`: {why}", tok.form),
                                ));
                                continue;
                            }
                        }
                    } else {
                        None
                    };
                    let runs = conllu::pairs(&read.misc)
                        .iter()
                        .find(|(key, _)| *key == "Runs")
                        .map(|(_, value)| (*value).to_string());
                    lines.push(Filled {
                        upos: read.upos.clone(),
                        feats: read.feats.clone(),
                        prov: if tok.is_word() {
                            Prov::Agree
                        } else {
                            Prov::Kind
                        },
                        code,
                        runs: if tok.is_word() { runs } else { None },
                    });
                }
                (false, Some((row, used))) => {
                    *used = true;
                    if row.form != tok.form || !tok.is_word() {
                        problems.push(Problems::sentence(
                            "the adjudication log",
                            &sent.id,
                            format!("token {at} is `{}`, not `{}`", tok.form, row.form),
                        ));
                        continue;
                    }
                    lines.push(Filled {
                        upos: row.code.upos(&tok.form).to_string(),
                        feats: row.code.feats(),
                        prov: Prov::Adjudicated,
                        code: Some(row.code),
                        runs: row.run.clone(),
                    });
                }
                (false, None) => problems.push(Problems::sentence(
                    agreed_path,
                    &sent.id,
                    format!(
                        "token {at} `{}` has neither an agreed tag nor an adjudicated one",
                        tok.form
                    ),
                )),
            }
        }
        sentences.push(lines);
    }
    for ((sent_id, token), (_, used)) in &decided {
        if !used {
            problems.push(Problems::sentence(
                "the adjudication log",
                sent_id,
                format!("token {token} is not a token of the sample"),
            ));
        }
    }
    Problems::check(problems, Built { sentences })
}

/// What the files of one split say about it: its `exam.trains` and the sentence beside the
/// header that says what the file is.
fn split_header(split: Split) -> (&'static str, &'static str) {
    match split {
        Split::Holdout => (
            "no",
            "# Holdout. Never training data, now or later, and never shown to anyone writing a tagger. \
             The exam prints aggregate numbers only for it.",
        ),
        _ => (
            "undecided",
            "# Dev. Labels to tune a tagger on. Whether labels that models helped make may train a model is not decided.",
        ),
    }
}

/// The file of one split: the sentences of `sample` that are in `split`, with their lines.
pub fn gold_file(sample: &Sample, built: &Built, split: Split) -> String {
    let (trains, note) = split_header(split);
    let seed = sample.manifest.get("seed").unwrap_or("unknown");
    let corpus = sample.manifest.get("corpus").unwrap_or("unknown");
    let mut out = format!(
        "# Deslag gold set: part of speech tags on sentences quoted from the test corpus.\n\
         {note}\n\
         # exam.tokens = deslag\n\
         # exam.split = {}\n\
         # exam.trains = {trains}\n\
         # exam.source = deslag gold set, seed {seed}, corpus {corpus}\n",
        split.name()
    );
    for (sent, lines) in sample.sents.iter().zip(&built.sentences) {
        let Some(meta) = sample
            .meta(&sent.id)
            .filter(|meta| meta.split == Some(split))
        else {
            continue;
        };
        let Meta {
            tier,
            context,
            file,
            range,
            license,
            ..
        } = meta;
        let _ = write!(
            out,
            "# sent_id = {}\n# exam.tier = {}\n# exam.context = {}\n# source = {file} bytes {}-{}\n# license = {license}\n# text = {}\n",
            sent.id,
            tier.map_or("unknown", Tier::name),
            context.name(),
            range.start,
            range.end,
            sent.text()
        );
        for (index, (tok, filled)) in sent.toks.iter().zip(lines).enumerate() {
            out.push_str(&line(
                index,
                &tok.form,
                &filled.upos,
                &filled.feats,
                &misc(tok, Some(filled.prov.name()), None),
            ));
        }
        out.push('\n');
    }
    out
}

/// The labelled sentences of `sample`, which is not the gold sample: `labelled.conllu`, every
/// sentence of the sample with its final tags, `Prov=` and the `Runs=` of the runs that vouch for
/// each word. `trains` is its `exam.trains`: `no` for a pilot run over gold sentences, which must
/// never train anything. The exam's own loader reads it, and so does `score --import`.
pub fn labelled_file(sample: &Sample, built: &Built, trains: &str) -> String {
    let source = sample.manifest.get("source").unwrap_or("a labelling draw");
    let mut out = format!(
        "# Sentences labelled by the labelling pipeline: voters' agreement, then an adjudicator.\n\
         # exam.tokens = deslag\n\
         # exam.trains = {trains}\n\
         # exam.source = deslag labelling pipeline, {source}\n"
    );
    for (sent, lines) in sample.sents.iter().zip(&built.sentences) {
        let _ = writeln!(out, "# sent_id = {}", sent.id);
        if let Some(meta) = sample.meta(&sent.id) {
            let _ = writeln!(out, "# exam.context = {}", meta.context.name());
            if let Some(tier) = meta.tier {
                let _ = writeln!(out, "# exam.tier = {}", tier.name());
            }
            if !meta.file.is_empty() {
                let _ = writeln!(
                    out,
                    "# source = {} bytes {}-{}\n# repo = {}\n# license = {}",
                    meta.file, meta.range.start, meta.range.end, meta.repo, meta.license
                );
            }
        }
        let _ = writeln!(out, "# text = {}", sent.text());
        for (index, (tok, filled)) in sent.toks.iter().zip(lines).enumerate() {
            out.push_str(&line(
                index,
                &tok.form,
                &filled.upos,
                &filled.feats,
                &misc(tok, Some(filled.prov.name()), filled.runs.as_deref()),
            ));
        }
        out.push('\n');
    }
    out
}

/// The disputes file of `stem`, with none open.
pub fn disputes_file(stem: &str) -> String {
    format!(
        "# Open disputes against {stem}.conllu, one per line, tab separated: sent_id, word ID, proposed UPOS, reason.\n\
         # File a dispute by adding a line. Close it by removing the line in the change that accepts or rejects it.\n"
    )
}

/// How a tagger's answers stand against the final gold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Accuracy {
    /// Words compared.
    pub words: usize,
    /// Words whose part of speech the tagger got right.
    pub tag: usize,
    /// Words it got right in every feature it gave.
    pub full: usize,
}

/// How `answers` stand against the final codes of `built`, over the sentences in `split` or, with
/// `None`, over all: the part of speech, and the part of speech with no feature the tagger gave in
/// conflict. A feature a tagger leaves out is not held against it.
pub fn accuracy(
    sample: &Sample,
    built: &Built,
    answers: &Answers,
    split: Option<Split>,
) -> Accuracy {
    let mut total = Accuracy::default();
    for ((sent, lines), said) in sample.sents.iter().zip(&built.sentences).zip(&answers.0) {
        let wanted = match split {
            Some(split) => sample
                .meta(&sent.id)
                .is_some_and(|meta| meta.split == Some(split)),
            None => true,
        };
        if !wanted {
            continue;
        }
        for (filled, said) in lines.iter().zip(said) {
            let (Some(gold), Some(said)) = (filled.code, said) else {
                continue;
            };
            total.words += 1;
            total.tag += usize::from(gold.base == said.base);
            total.full += usize::from(matches!(judge(gold, *said, *said), Verdict::Agreed(_)));
        }
    }
    total
}

fn percent(part: usize, whole: usize) -> String {
    format!("{:.1}", 100.0 * part as f64 / whole.max(1) as f64)
}

/// `accuracy.tsv`: for each of `taggers` (a name and its answers), its accuracy on dev, on
/// holdout and on both. The numbers are counts and rates only, so the file names no holdout word.
pub fn accuracy_file(sample: &Sample, built: &Built, taggers: &[(&str, &Answers)]) -> String {
    let mut out = String::from(
        "tagger\tsplit\twords\tpart_of_speech\tpart_of_speech_pct\twith_features\twith_features_pct\n",
    );
    for (name, answers) in taggers {
        for (label, split) in [
            ("dev", Some(Split::Dev)),
            ("holdout", Some(Split::Holdout)),
            ("all", None),
        ] {
            let a = accuracy(sample, built, answers, split);
            let _ = writeln!(
                out,
                "{name}\t{label}\t{}\t{}\t{}\t{}\t{}",
                a.words,
                a.tag,
                percent(a.tag, a.words),
                a.full,
                percent(a.full, a.words)
            );
        }
    }
    out
}

/// The rows of `log`, the text of `adjudicated.tsv`, whose sentence is in `split`, under its
/// header line. Holdout words, answers and reasons go only into the holdout file.
pub fn log_file(sample: &Sample, log: &str, split: Split) -> String {
    let mut lines = log.lines();
    let mut out = format!("{}\n", lines.next().unwrap_or_default());
    for line in lines {
        let sent_id = line.split('\t').nth(1).unwrap_or_default();
        if sample
            .meta(sent_id)
            .is_some_and(|meta| meta.split == Some(split))
        {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The manifest of the sentences in `split` alone: the header, and their rows.
pub fn manifest_file(sample: &Sample, split: Split) -> String {
    let rows = sample
        .manifest
        .rows
        .iter()
        .filter(|(_, meta)| meta.split == Some(split))
        .cloned()
        .collect();
    Manifest {
        header: sample.manifest.header.clone(),
        rows,
    }
    .render()
}

/// A problem reading back what was written: the file, and why the exam would not take it.
pub fn reread(path: &str, text: &str) -> Result<deslag_exam::gold::Gold, Error> {
    deslag_exam::gold::Gold::parse(path, path, text).map_err(|error| {
        Error::load(
            path,
            Place::File,
            format!("the exam does not read what was written: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use deslag_exam::align::align_all;
    use deslag_exam::disputes::Disputes;
    use deslag_exam::gold::{Split, Tier, TokenMode, Trains};
    use deslag_exam::tagger::Context;
    use deslag_exam::words::Words;

    use super::*;
    use crate::compact::{read_tags, tests::sample};
    use crate::merge::{NAMES, agreed_conllu, worklist_tsv};
    use crate::merge::{
        adjudicated_tsv, load_tagger, merge, read_answers, read_log, read_overrides, read_worklist,
    };

    /// The sample of the compact tests, with `s2` moved into the holdout.
    fn split_sample() -> Sample {
        let mut sample = sample();
        sample.manifest.header = vec![
            ("seed".to_string(), "0x6465736c6167".to_string()),
            ("corpus".to_string(), "hand-made".to_string()),
        ];
        sample.manifest.rows[1].1.split = Some(Split::Holdout);
        sample.manifest.rows[1].1.tier = Some(Tier::Llm);
        sample.manifest.rows[1].1.context = Context::ListItem;
        sample
    }

    const BLIND: &str = "s1: V.fi _ T V.in _\ns2: R _ D N.s N.p\n";

    /// A tagger's CoNLL-U with one UPOS and FEATS per word.
    fn tagger(sample: &Sample, words: &[(&str, &str)]) -> String {
        let mut words = words.iter();
        let mut out = String::new();
        for sent in &sample.sents {
            let _ = writeln!(out, "# sent_id = {}", sent.id);
            for (index, tok) in sent.toks.iter().enumerate() {
                let (upos, feats) = if tok.is_word() {
                    *words.next().unwrap()
                } else {
                    ("X", "_")
                };
                out.push_str(&line(index, &tok.form, upos, feats, &misc(tok, None, None)));
            }
            out.push('\n');
        }
        out
    }

    /// Everything from the three taggers to the built gold, with `files` tagged ADJ by Harper.
    fn pipeline() -> (Sample, Built, String, String, Answers) {
        let sample = split_sample();
        let blind = read_tags(
            &sample,
            &[("b".to_string(), BLIND.to_string())],
            "blind",
            None,
            true,
        )
        .unwrap()
        .0;
        let same = [
            ("VERB", "VerbForm=Fin"),
            ("PART", "_"),
            ("VERB", "VerbForm=Inf"),
            ("ADV", "_"),
            ("DET", "_"),
            ("NOUN", "Number=Sing"),
            ("NOUN", "Number=Plur"),
        ];
        let mut harper = same;
        harper[6] = ("ADJ", "_");
        let answers = [
            load_tagger("blind", "b", &blind, &sample).unwrap(),
            load_tagger("harper", "h", &tagger(&sample, &harper), &sample).unwrap(),
            load_tagger("spacy", "s", &tagger(&sample, &same), &sample).unwrap(),
        ];
        let merged = merge(&sample, &answers);
        let agreed = agreed_conllu(&sample, &merged.verdicts, None);
        let work = read_worklist("w", &worklist_tsv(&sample, &merged.items, &NAMES)).unwrap();
        let decided = read_answers(
            &work.items,
            &[(
                "a".to_string(),
                "s2.5: N.p | plural noun, agrees with are\n".to_string(),
            )],
            true,
        )
        .unwrap();
        let log = adjudicated_tsv(&NAMES.map(String::from), &decided, &[], None);
        let rows = read_log("log", &log).unwrap();
        let built = build(&sample, "agreed.conllu", &agreed, &rows).unwrap();
        let [blind_answers, ..] = answers;
        (sample, built, agreed, log, blind_answers)
    }

    #[test]
    fn the_adjudicated_word_fills_the_blank_and_the_rest_keep_their_agreed_tags() {
        let (_, built, agreed, _, _) = pipeline();
        assert!(agreed.contains("5\tfiles\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n"));
        let files = &built.sentences[1][4];
        assert_eq!(files.upos, "NOUN");
        assert_eq!(files.feats, "Number=Plur");
        assert_eq!(files.prov, Prov::Adjudicated);
        assert_eq!(built.sentences[1][3].prov, Prov::Agree);
        assert_eq!(
            built.sentences[0][1].prov,
            Prov::Kind,
            "no tagger decided a code span"
        );
        assert_eq!(built.sentences[0][4].prov, Prov::Kind);
        assert_eq!(built.sentences[0][1].upos, "X", "code span");
        assert_eq!(built.sentences[0][1].code, None);
        assert_eq!(built.sentences[0][4].upos, "PUNCT");
    }

    #[test]
    fn the_files_load_in_the_exam_apart_with_the_conventions() {
        let (sample, built, ..) = pipeline();
        let dev = gold_file(&sample, &built, Split::Dev);
        let holdout = gold_file(&sample, &built, Split::Holdout);

        let dev_gold = reread("dev.conllu", &dev).unwrap();
        assert_eq!(dev_gold.split, Some(Split::Dev));
        assert_eq!(dev_gold.trains, Trains::Undecided);
        assert_eq!(dev_gold.tokens, TokenMode::Deslag);
        assert_eq!(dev_gold.sentences.len(), 1);
        assert_eq!(dev_gold.sentences[0].sent_id, "s1");
        assert_eq!(dev_gold.sentences[0].tier, Some(Tier::Human));
        assert!(dev_gold.source.contains("seed 0x6465736c6167"));
        assert!(dev_gold.source.contains("corpus hand-made"));
        assert!(!dev.contains("sent_id = s2"));

        let holdout_gold = reread("holdout.conllu", &holdout).unwrap();
        assert_eq!(holdout_gold.split, Some(Split::Holdout));
        assert_eq!(holdout_gold.trains, Trains::No);
        assert!(holdout_gold.holdout());
        assert_eq!(holdout_gold.sentences.len(), 1);
        assert_eq!(holdout_gold.sentences[0].tier, Some(Tier::Llm));
        assert_eq!(holdout_gold.sentences[0].context, Context::ListItem);
        assert!(holdout.contains("# Holdout. Never training data"));
        assert!(!holdout.contains("sent_id = s1"));
    }

    #[test]
    fn every_line_names_its_provenance_and_the_exam_aligns_every_word() {
        let (sample, built, ..) = pipeline();
        let holdout = gold_file(&sample, &built, Split::Holdout);
        let gold = reread("holdout.conllu", &holdout).unwrap();
        let words = Words::of(&gold, &align_all(&gold), &Disputes::default());
        assert_eq!(words.words, 5);
        assert_eq!(
            words.provenance,
            [3, 1, 0, 0, 1],
            "agree, adjudicated, corrected, owner, kind"
        );
        assert_eq!(words.unmarked, 0);
        assert_eq!(words.unalignable_total(), 0);
        assert_eq!(words.scored_words, 4);
        let dev = reread("dev.conllu", &gold_file(&sample, &built, Split::Dev)).unwrap();
        let words = Words::of(&dev, &align_all(&dev), &Disputes::default());
        assert_eq!(words.provenance, [3, 0, 0, 0, 2]);
        assert_eq!(words.x, 1, "the code span");
    }

    #[test]
    fn the_source_of_each_sentence_is_recorded_beside_it() {
        let (sample, built, ..) = pipeline();
        let dev = gold_file(&sample, &built, Split::Dev);
        assert!(dev.contains("# source = f.md bytes 0-1\n# license = MIT\n"));
        assert!(dev.contains("# text = Run make ci to compile,\n"));
    }

    #[test]
    fn the_disputes_files_hold_no_dispute_and_load() {
        let text = disputes_file("holdout");
        let disputes = Disputes::parse("holdout.disputes.tsv", &text).unwrap();
        assert!(disputes.open.is_empty());
        assert!(text.contains("holdout.conllu"));
    }

    #[test]
    fn a_blank_word_with_no_adjudicated_answer_stops_assembly() {
        let (sample, _, agreed, ..) = pipeline();
        let error = build(&sample, "agreed.conllu", &agreed, &[])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "sentence s2: token 5 `files` has neither an agreed tag nor an adjudicated one"
            ),
            "{error}"
        );
    }

    #[test]
    fn an_adjudicated_word_that_was_agreed_or_is_unknown_stops_assembly() {
        let (sample, _, agreed, ..) = pipeline();
        let row = |sent: &str, token, form: &str| Logged {
            sent_id: sent.to_string(),
            token,
            form: form.to_string(),
            code: Code::parse("N.p").unwrap(),
            agreed: None,
            run: None,
        };
        let files = row("s2", 5, "files");
        let says = |rows: &[Logged], expect: &str| {
            let error = build(&sample, "agreed.conllu", &agreed, rows)
                .unwrap_err()
                .to_string();
            assert!(error.contains(expect), "{error} should say {expect}");
        };
        says(
            &[files.clone(), row("s1", 1, "Run")],
            "token 1 `Run` was agreed, and is in the log",
        );
        says(
            &[files.clone(), files.clone()],
            "token 5 is in the log twice",
        );
        says(
            &[files.clone(), row("s9", 1, "x")],
            "token 1 is not a token of the sample",
        );
        says(&[row("s2", 5, "file")], "token 5 is `files`, not `file`");
    }

    const OVERRIDE_HEAD: &str = "sentence_id\ttoken_index\tform\told_code\tnew_code\treason\n";

    #[test]
    fn an_override_turns_an_agreed_word_into_an_adjudicated_one_with_its_reason() {
        let (sample, _, agreed, log, _) = pipeline();
        let text = format!("{OVERRIDE_HEAD}s1\t3\tto\tT\tP\tthe guide now says to here is P\n");
        let overrides = read_overrides("o.tsv", &text, "agreed.conllu", &agreed).unwrap();
        assert_eq!(overrides.len(), 1);
        // The log of the answers, then the override.
        let mut text = log.clone();
        for line in adjudicated_tsv(&NAMES.map(String::from), &[], &overrides, None)
            .lines()
            .skip(1)
        {
            text.push_str(line);
            text.push('\n');
        }
        assert!(
            text.contains("s1.3\ts1\t3\tto\tT\tT\tT\tP\tthe guide now says to here is P\n"),
            "{text}"
        );
        let rows = read_log("log", &text).unwrap();
        let built = build(&sample, "agreed.conllu", &agreed, &rows).unwrap();
        let to = &built.sentences[0][2];
        assert_eq!(to.prov, Prov::Adjudicated);
        assert_eq!(to.code, Some(Code::parse("P").unwrap()));
        assert_eq!(to.upos, "ADP");
        assert_eq!(built.sentences[0][0].prov, Prov::Agree);
        let dev = gold_file(&sample, &built, Split::Dev);
        assert!(dev.contains("Prov=adjudicated"), "{dev}");
        let gold = reread("dev.conllu", &dev).unwrap();
        let words = Words::of(&gold, &align_all(&gold), &Disputes::default());
        assert_eq!(words.provenance[1], 1, "one adjudicated word in dev");
    }

    #[test]
    fn a_bad_override_is_rejected_and_an_answer_for_an_agreed_word_still_is() {
        let (sample, _, agreed, log, _) = pipeline();
        let says = |row: &str, expect: &str| {
            let error = read_overrides(
                "o.tsv",
                &format!("{OVERRIDE_HEAD}{row}\n"),
                "agreed.conllu",
                &agreed,
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains(expect), "{error} should say {expect}");
        };
        says("s1\t3\tto\tT\tP\t", "no reason");
        says("s1\t3\tto\tT\tT\tsame", "the new code is the old one");
        says("s1\t3\tto\tP\tT\tx", "was agreed as T, not P");
        says("s1\t3\tat\tT\tP\tx", "token 3 is `to`, not `at`");
        says("s2\t5\tfiles\tN.p\tN.s\tx", "was not agreed by the taggers");
        says("s1\t9\tto\tT\tP\tx", "token 9 is not in the sentence");
        says("s7\t1\tto\tT\tP\tx", "it is not in the agreed file");
        says("s1\t3\tto\tT\tQ.s\tx", "`Q.s` is not a code of the guide");
        let long = vec!["word"; 16].join(" ");
        says(
            &format!("s1\t3\tto\tT\tP\t{long}"),
            "the reason has 16 words",
        );
        assert!(read_overrides("o.tsv", "sentence_id\tform\n", "agreed.conllu", &agreed).is_err());
        // A row whose three answers differ is an answer, not an override, and an agreed word
        // may not have one.
        let rows = read_log("log", &format!("{log}s1.3\ts1\t3\tto\tT\tP\tT\tP\tx\n")).unwrap();
        let error = build(&sample, "agreed.conllu", &agreed, &rows)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("token 3 `to` was agreed, and is in the log"),
            "{error}"
        );
        // An override whose old code is not the agreed one is refused too.
        let rows = read_log("log", &format!("{log}s1.3\ts1\t3\tto\tP\tP\tP\tP\tx\n")).unwrap();
        assert!(build(&sample, "agreed.conllu", &agreed, &rows).is_err());
    }

    #[test]
    fn an_agreed_file_that_does_not_match_the_sample_is_rejected() {
        let (sample, _, agreed, ..) = pipeline();
        let error = build(
            &sample,
            "agreed.conllu",
            &agreed.replace("compile", "build"),
            &[],
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("sentence s1: token 4 is not `compile`"),
            "{error}"
        );
        let error = build(&sample, "agreed.conllu", "# sent_id = s1\n", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(error.contains("agreed.conllu"), "{error}");
    }

    #[test]
    fn a_tagger_is_scored_against_the_final_gold() {
        let (sample, built, _, _, blind) = pipeline();
        let accuracy_of = accuracy(&sample, &built, &blind, None);
        assert_eq!(
            accuracy_of,
            Accuracy {
                words: 7,
                tag: 7,
                full: 7
            }
        );
        let mut off = blind.clone();
        // Say `to` is a preposition and `files` is singular.
        off.0[0][2] = Some(Code::parse("P").unwrap());
        off.0[1][4] = Some(Code::parse("N.s").unwrap());
        assert_eq!(
            accuracy(&sample, &built, &off, None),
            Accuracy {
                words: 7,
                tag: 6,
                full: 5
            }
        );
    }

    #[test]
    fn accuracy_is_counted_by_split_and_written_as_a_table() {
        let (sample, built, _, _, blind) = pipeline();
        let dev = accuracy(&sample, &built, &blind, Some(Split::Dev));
        let holdout = accuracy(&sample, &built, &blind, Some(Split::Holdout));
        assert_eq!((dev.words, holdout.words), (3, 4));
        let table = accuracy_file(&sample, &built, &[("blind", &blind)]);
        let mut lines = table.lines();
        assert_eq!(
            lines.next().unwrap(),
            "tagger\tsplit\twords\tpart_of_speech\tpart_of_speech_pct\twith_features\twith_features_pct"
        );
        assert_eq!(lines.next().unwrap(), "blind\tdev\t3\t3\t100.0\t3\t100.0");
        assert_eq!(
            lines.next().unwrap(),
            "blind\tholdout\t4\t4\t100.0\t4\t100.0"
        );
        assert_eq!(lines.next().unwrap(), "blind\tall\t7\t7\t100.0\t7\t100.0");
    }

    #[test]
    fn the_log_and_the_manifest_are_split_so_holdout_rows_are_only_in_holdout_files() {
        let (sample, _, _, log, _) = pipeline();
        // The one adjudicated word, s2.5, is in the holdout.
        let dev_log = log_file(&sample, &log, Split::Dev);
        let holdout_log = log_file(&sample, &log, Split::Holdout);
        assert_eq!(dev_log.lines().count(), 1, "the header alone: {dev_log}");
        assert!(dev_log.starts_with("item\tsent_id\t"));
        assert!(!dev_log.contains("files"));
        assert_eq!(holdout_log.lines().count(), 2);
        assert!(holdout_log.contains("s2.5\ts2\t5\tfiles\t"));

        let dev_manifest = manifest_file(&sample, Split::Dev);
        let holdout_manifest = manifest_file(&sample, Split::Holdout);
        let rows = |text: &str| -> Vec<String> {
            Manifest::parse("m", text)
                .unwrap()
                .rows
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        assert_eq!(rows(&dev_manifest), ["s1"]);
        assert_eq!(rows(&holdout_manifest), ["s2"]);
        assert!(dev_manifest.starts_with("# seed = 0x6465736c6167\n"));
        assert!(!dev_manifest.contains("s2") && !holdout_manifest.contains("s1\t"));
    }
}
