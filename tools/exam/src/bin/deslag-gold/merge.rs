//! The three-way merger: the blind tagger, Harper and spaCy over the same words.
//!
//! This merger is phase 1's and is frozen: the gold set was made with it, and its counts and text
//! must not change. The labelling flow's merger for any number of voters, which has its own
//! `Stats` and its own `Display`, is [`crate::voters`]; the two share the worklist, the answers
//! and the log below, and nothing else.
//!
//! Each of the three gives every word token a UPOS and, if it can, FEATS (an import file of the
//! exam, which is also what the compact reader writes). They are compared as the guide's codes
//! (`N.p`, `V.pp`):
//!
//! - The three agree on a word when they name the same base (`N`, `V`, ...) and no feature
//!   conflicts. The blind tagger is the reference for which features a word has, since it followed
//!   the guide: where its code has a number or a verb form, a tagger that gives one must give the
//!   same, and one that gives none abstains. Where the blind code has none (a pronoun with no
//!   number) nothing is asked of the others. An agreed word stands, `Prov=agree`.
//! - Harper's file holds a raw UPOS, `X` where Harper has no tag. An `X` from Harper is an
//!   abstention, not an answer: Harper neither agrees nor disagrees on that word, and the word is
//!   decided by the blind tagger and spaCy.
//! - Every other word goes to the adjudication worklist, with the sentence, the three answers and
//!   a place for one code and a reason of at most 15 words.
//!
//! `agreed.conllu` is every sentence with the agreed words filled in and the disputed words left
//! without a UPOS, so that it cannot be mistaken for a gold file until the adjudicated answers
//! (read by [`read_answers`]) are put into it by `assemble`. Words that are not words, code spans
//! and marks, are filled from their kind, `Prov=kind`, and never in dispute.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use deslag_exam::conllu::{self, Id};
use deslag_exam::error::{Error, Place};
use deslag_exam::tagger::Context;

use crate::batch;
use crate::code::{Base, Code};
use crate::data::{Sample, Sent, line, misc, upos_of_kind};
use crate::problems::Problems;

/// The three taggers, in the order the worklist and the report name them.
pub const NAMES: [&str; 3] = ["blind", "harper", "spacy"];

/// What one tagger said of every word token of the sample: a code for a word token, `None` for
/// any other, in the order of the sample's sentences and tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answers(pub Vec<Vec<Option<Code>>>);

impl Answers {
    /// Whether the tagger gave no answer for sentence `at`: a voter whose reply had no good line
    /// for it, which [`load_voter`] reads as an empty row.
    pub fn abstains(&self, at: usize) -> bool {
        self.0[at].is_empty()
    }
}

/// Reads the CoNLL-U answers of the tagger `name` from `text`, which came from `path`, against
/// `sample`. Every sentence of the sample must be there with the same forms, line for line, and
/// every word line must have a UPOS of UD's.
pub fn load_tagger(
    name: &str,
    path: &str,
    text: &str,
    sample: &Sample,
) -> Result<Answers, Problems> {
    read_answers_of(name, path, text, sample, false)
}

/// [`load_tagger`] for a voter of the labelling flow: a sentence the file has no block for is not
/// an error but an abstention, the voter's row for it left empty. `read-tags --check` keeps only
/// the sentences with a good line, so a sentence a model never got right is missing, and that must
/// not stop the merge. A sentence that is there must still be right.
pub fn load_voter(
    name: &str,
    path: &str,
    text: &str,
    sample: &Sample,
) -> Result<Answers, Problems> {
    read_answers_of(name, path, text, sample, true)
}

fn read_answers_of(
    name: &str,
    path: &str,
    text: &str,
    sample: &Sample,
    may_abstain: bool,
) -> Result<Answers, Problems> {
    let blocks = conllu::read(path, text)?;
    let mut by_id: BTreeMap<&str, &conllu::Block> = BTreeMap::new();
    let mut problems = Vec::new();
    for block in &blocks {
        match block.comment("sent_id") {
            Some(comment) if !comment.value.is_empty() => {
                if by_id.insert(comment.value.as_str(), block).is_some() {
                    problems.push(Error::at(
                        path,
                        comment.line,
                        format!("sent_id `{}` is used twice", comment.value),
                    ));
                }
            }
            _ => problems.push(Error::at(
                path,
                block.first_line,
                "no `# sent_id = ` comment",
            )),
        }
    }
    let known = sample.index_of();
    for id in by_id.keys() {
        if !known.contains_key(id) {
            problems.push(Error::load(
                path,
                Place::Sentence((*id).to_string()),
                format!("{name} tagged a sentence that is not in the sample"),
            ));
        }
    }
    let mut answers = Vec::with_capacity(sample.sents.len());
    for sent in &sample.sents {
        let Some(block) = by_id.get(sent.id.as_str()) else {
            if !may_abstain {
                problems.push(Problems::sentence(
                    path,
                    &sent.id,
                    format!("{name} has no answer for it"),
                ));
            }
            answers.push(Vec::new());
            continue;
        };
        match read_sentence(name, path, sent, block) {
            Ok(codes) => answers.push(codes),
            Err(error) => {
                problems.push(error);
                answers.push(Vec::new());
            }
        }
    }
    Problems::check(problems, Answers(answers))
}

/// The codes of one sentence's answer, or the first thing wrong with it.
fn read_sentence(
    name: &str,
    path: &str,
    sent: &Sent,
    block: &conllu::Block,
) -> Result<Vec<Option<Code>>, Error> {
    if block.lines.len() != sent.toks.len() {
        return Err(Problems::sentence(
            path,
            &sent.id,
            format!(
                "{name} has {} lines for {} tokens",
                block.lines.len(),
                sent.toks.len()
            ),
        ));
    }
    let mut codes = Vec::with_capacity(sent.toks.len());
    for (index, (tok, line)) in sent.toks.iter().zip(&block.lines).enumerate() {
        if !matches!(line.id, Id::Word(_)) || line.form != tok.form {
            return Err(Error::at(
                path,
                line.number,
                format!(
                    "sentence {}: token {} is `{}` in the sample and `{}` here",
                    sent.id,
                    index + 1,
                    tok.form,
                    line.form
                ),
            ));
        }
        if !tok.is_word() {
            codes.push(None);
            continue;
        }
        match Code::from_conllu(&line.upos, &line.feats) {
            Ok(code) => codes.push(Some(code)),
            Err(why) => {
                return Err(Error::at(
                    path,
                    line.number,
                    format!(
                        "sentence {}: token {} `{}`: {why}",
                        sent.id,
                        index + 1,
                        tok.form
                    ),
                ));
            }
        }
    }
    Ok(codes)
}

/// What the three said of one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// They agree, and this is the code that stands.
    Agreed(Code),
    /// They do not.
    Disputed {
        /// Whether the bases differ; if not, only a feature does.
        tags_differ: bool,
    },
}

/// Whether `other` conflicts with `blind` on one feature: it gives a value, the blind code has
/// one, and they differ.
fn conflicts<T: PartialEq>(blind: Option<T>, other: Option<T>) -> bool {
    matches!((blind, other), (Some(b), Some(o)) if b != o)
}

/// Whether Harper abstains on a word: its file holds `X` where it has no tag.
pub fn harper_abstains(harper: Code) -> bool {
    harper.base == Base::X
}

/// What the three said of a word: `blind`, `harper` and `spacy`. Harper's `X` is an abstention:
/// it is left out of the comparison.
pub fn judge(blind: Code, harper: Code, spacy: Code) -> Verdict {
    let others: &[Code] = if harper_abstains(harper) {
        &[spacy]
    } else {
        &[harper, spacy]
    };
    if others.iter().any(|other| blind.base != other.base) {
        return Verdict::Disputed { tags_differ: true };
    }
    let clash = others
        .iter()
        .any(|other| conflicts(blind.number, other.number) || conflicts(blind.form, other.form));
    if clash {
        Verdict::Disputed { tags_differ: false }
    } else {
        Verdict::Agreed(blind)
    }
}

/// A word the three did not agree on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its sentence's position in the sample.
    pub sent: usize,
    /// Its token's position in the sentence, from 0.
    pub tok: usize,
    /// What each tagger said, in the order of [`NAMES`] for the three-way merge and of the voters
    /// for [`crate::voters`]. `None` is a voter that abstained on the sentence.
    pub said: Vec<Option<Code>>,
    /// Whether the bases differ, or only a feature.
    pub tags_differ: bool,
    /// Whether fewer than two voters answered the sentence, so there was nothing to compare and
    /// the adjudicator gets the whole sentence.
    pub unvoted: bool,
}

impl Item {
    /// The id the adjudicator answers by: the sentence's id, a dot and the token's number.
    pub fn id(&self, sample: &Sample) -> String {
        format!("{}.{}", sample.sents[self.sent].id, self.tok + 1)
    }
}

/// Counts of agreement, for the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Word tokens compared.
    pub words: usize,
    /// How many words each pair agrees on the base of: blind and Harper, blind and spaCy, Harper
    /// and spaCy.
    pub pairs: [usize; 3],
    /// The words each pair was compared on: every word, less those Harper abstained on for the
    /// two pairs it is in.
    pub pair_words: [usize; 3],
    /// Words Harper abstained on, with `X`.
    pub abstained: usize,
    /// How many words all three agree on the base of, Harper's abstentions aside.
    pub tag3: usize,
    /// How many words all three agree on completely.
    pub full3: usize,
    /// Of the words in dispute, how many differ in a base.
    pub tag_disputes: usize,
    /// Of the words in dispute, how many differ only in a feature.
    pub feature_disputes: usize,
    /// Words and complete agreements by tier.
    pub by_tier: BTreeMap<&'static str, (usize, usize)>,
    /// Words and complete agreements by context.
    pub by_context: BTreeMap<&'static str, (usize, usize)>,
}

/// The merge of the three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// The verdict on each token, `None` for one that is not a word.
    pub verdicts: Vec<Vec<Option<Verdict>>>,
    /// The words to adjudicate, in the order of the sample.
    pub items: Vec<Item>,
    /// The counts.
    pub stats: Stats,
}

/// Compares the three taggers' answers on every word of `sample`.
pub fn merge(sample: &Sample, taggers: &[Answers; 3]) -> Merged {
    let mut verdicts = Vec::with_capacity(sample.sents.len());
    let mut items = Vec::new();
    let mut stats = Stats::default();
    for (at, sent) in sample.sents.iter().enumerate() {
        let meta = sample.meta(&sent.id);
        let mut row = Vec::with_capacity(sent.toks.len());
        for tok in 0..sent.toks.len() {
            let said = taggers.each_ref().map(|answers| answers.0[at][tok]);
            let [Some(blind), Some(harper), Some(spacy)] = said else {
                row.push(None);
                continue;
            };
            stats.words += 1;
            let abstains = harper_abstains(harper);
            stats.abstained += usize::from(abstains);
            stats.pair_words[0] += usize::from(!abstains);
            stats.pair_words[1] += 1;
            stats.pair_words[2] += usize::from(!abstains);
            stats.pairs[0] += usize::from(!abstains && blind.base == harper.base);
            stats.pairs[1] += usize::from(blind.base == spacy.base);
            stats.pairs[2] += usize::from(!abstains && harper.base == spacy.base);
            let verdict = judge(blind, harper, spacy);
            let all_tags = blind.base == spacy.base && (abstains || blind.base == harper.base);
            stats.tag3 += usize::from(all_tags);
            let full = matches!(verdict, Verdict::Agreed(_));
            stats.full3 += usize::from(full);
            if let Some(meta) = meta {
                if let Some(tier) = meta.tier {
                    let tier = stats.by_tier.entry(tier.name()).or_default();
                    tier.0 += 1;
                    tier.1 += usize::from(full);
                }
                let context = stats.by_context.entry(meta.context.name()).or_default();
                context.0 += 1;
                context.1 += usize::from(full);
            }
            if let Verdict::Disputed { tags_differ } = verdict {
                if tags_differ {
                    stats.tag_disputes += 1;
                } else {
                    stats.feature_disputes += 1;
                }
                items.push(Item {
                    sent: at,
                    tok,
                    said: vec![Some(blind), Some(harper), Some(spacy)],
                    tags_differ,
                    unvoted: false,
                });
            }
            row.push(Some(verdict));
        }
        verdicts.push(row);
    }
    Merged {
        verdicts,
        items,
        stats,
    }
}

fn percent(part: usize, whole: usize) -> String {
    if whole == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", 100.0 * part as f64 / whole as f64)
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let row = |f: &mut fmt::Formatter<'_>, label: &str, count: usize| {
            writeln!(
                f,
                "  {label:<44}{count:>6}  {:>6}",
                percent(count, self.words)
            )
        };
        writeln!(f, "Agreement over {} word tokens", self.words)?;
        writeln!(f, "pairs, on the part of speech")?;
        for (label, at) in [
            ("blind and harper", 0),
            ("blind and spacy", 1),
            ("harper and spacy", 2),
        ] {
            writeln!(
                f,
                "  {label:<44}{:>6}  {:>6}  of {}",
                self.pairs[at],
                percent(self.pairs[at], self.pair_words[at]),
                self.pair_words[at]
            )?;
        }
        row(f, "words harper abstained on (X), left out", self.abstained)?;
        writeln!(f, "all three")?;
        row(f, "agree on the part of speech", self.tag3)?;
        row(f, "agree on it and every feature (these stand)", self.full3)?;
        row(f, "to adjudicate", self.words - self.full3)?;
        row(
            f,
            "  of which the part of speech differs",
            self.tag_disputes,
        )?;
        row(
            f,
            "  of which only a feature differs",
            self.feature_disputes,
        )?;
        for (title, table) in [("by tier", &self.by_tier), ("by context", &self.by_context)] {
            writeln!(f, "{title}, all three agree on every word")?;
            for (name, (words, full)) in table {
                writeln!(
                    f,
                    "  {name:<14}{full:>6} of {words:<6}  {:>6}",
                    percent(*full, *words)
                )?;
            }
        }
        Ok(())
    }
}

/// `agreed.conllu`: every sentence, the agreed words with their UPOS, FEATS and `Prov=agree`, the
/// words that are not words with theirs from their kind (`Prov=kind`), and each disputed word with `_` for UPOS
/// and no `Prov=`.
///
/// `runs`, when the voters have runs, is for each sentence the `Runs=` of its agreed words: the
/// runs of the voters that answered it, all of which agreed. A gold flow merge passes none.
pub fn agreed_conllu(
    sample: &Sample,
    verdicts: &[Vec<Option<Verdict>>],
    runs: &[Option<String>],
) -> String {
    let mut out = String::new();
    for (at, (sent, row)) in sample.sents.iter().zip(verdicts).enumerate() {
        let runs = runs.get(at).and_then(Option::as_deref);
        let _ = writeln!(out, "# sent_id = {}\n# text = {}", sent.id, sent.text());
        for (index, (tok, verdict)) in sent.toks.iter().zip(row).enumerate() {
            let text = match verdict {
                None => line(
                    index,
                    &tok.form,
                    upos_of_kind(tok.kind),
                    "_",
                    &misc(tok, Some("kind"), None),
                ),
                Some(Verdict::Agreed(code)) => line(
                    index,
                    &tok.form,
                    code.upos(&tok.form),
                    &code.feats(),
                    &misc(tok, Some("agree"), runs),
                ),
                Some(Verdict::Disputed { .. }) => {
                    line(index, &tok.form, "_", "_", &misc(tok, None, None))
                }
            };
            out.push_str(&text);
        }
        out.push('\n');
    }
    out
}

/// What a tagger said, as the worklist shows it: the code, with `.?` where the tagger gave no
/// feature that the guide asks of the word, so that leaving one out cannot be read as a code, and
/// `-` for a voter that gave no answer for the sentence.
fn said(code: Option<Code>) -> String {
    let Some(code) = code else {
        return ABSTAINED.to_string();
    };
    let missing = (matches!(code.base, Base::N | Base::Pn) && code.number.is_none())
        || (code.base.takes_form() && code.form.is_none());
    if missing {
        format!("{code}.?")
    } else {
        code.to_string()
    }
}

/// What the worklist shows for a voter that abstained on the sentence.
const ABSTAINED: &str = "-";

/// How many taggers the worklist header says there are.
fn count_word(count: usize) -> String {
    match count {
        2 => "two".to_string(),
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        other => other.to_string(),
    }
}

/// The text a worklist part opens with, for the taggers `names`, in the order of an item's `said`.
fn worklist_header(names: &[&str]) -> String {
    format!(
        "\
Adjudicate the words below. Each sentence is shown with its tokens numbered; under it, each
word the {} taggers disagree on, with what each said ({}). A code that ends
in .? means that tagger gave the part of speech and no feature, and - means that tagger gave
no answer for the sentence.

Decide each word from the annotation guide, for its use in this sentence. Answer with one line per
item, at the end, in the slots given: the item, a colon, one code of the guide with its feature,
a bar, and a reason of 15 words or fewer. Answer every item, change nothing else, and write
nothing else.
",
        count_word(names.len()),
        names.join(", ")
    )
}

/// One item of a worklist part: its place in the sample, and what each tagger said as the part
/// prints it.
struct Row {
    sent: usize,
    tok: usize,
    id: String,
    said: Vec<String>,
}

/// The worklist, in parts of about `per_part` items, never splitting a sentence. Each part is
/// text for the adjudicator: the numbered sentences, their words in dispute, and a slot to fill
/// for each item. The parts hold no tier, split or file. `names` is what each tagger is called in
/// them, in the order of an item's `said`: `blind`, `harper` and `spacy`, or `A`, `B` and `C` so
/// that the adjudicator never learns which model said what.
pub fn worklist_parts(
    sample: &Sample,
    items: &[Item],
    per_part: usize,
    names: &[&str],
) -> Vec<String> {
    let rows: Vec<Row> = items
        .iter()
        .map(|item| Row {
            sent: item.sent,
            tok: item.tok,
            id: item.id(sample),
            said: item.said.iter().map(|code| said(*code)).collect(),
        })
        .collect();
    parts_of(sample, &rows, per_part, names)
}

/// The parts of the worklist for the items of `work` named in `open`, laid out as
/// [`worklist_parts`] lays them out, with the taggers shown by letter: what is left to ask the
/// adjudicator after some answers were rejected.
pub fn retry_parts(
    sample: &Sample,
    work: &Worklist,
    open: &[String],
    per_part: usize,
) -> Vec<String> {
    let index = sample.index_of();
    let rows: Vec<Row> = work
        .items
        .iter()
        .filter(|item| open.contains(&item.item))
        .filter_map(|item| {
            Some(Row {
                sent: *index.get(item.sent_id.as_str())?,
                tok: item.token - 1,
                id: item.item.clone(),
                said: item.said.clone(),
            })
        })
        .collect();
    let letters: Vec<String> = (0..work.names.len()).map(crate::voters::letter).collect();
    let names: Vec<&str> = letters.iter().map(String::as_str).collect();
    parts_of(sample, &rows, per_part, &names)
}

fn parts_of(sample: &Sample, items: &[Row], per_part: usize, names: &[&str]) -> Vec<String> {
    let mut parts: Vec<&[Row]> = Vec::new();
    let mut at = 0;
    while at < items.len() {
        let mut end = at;
        // Whole sentences, until the part holds `per_part` items.
        while end < items.len() && (end == at || end - at < per_part) {
            let sent = items[end].sent;
            while end < items.len() && items[end].sent == sent {
                end += 1;
            }
        }
        parts.push(&items[at..end]);
        at = end;
    }
    parts
        .iter()
        .map(|part| {
            let mut out = worklist_header(names);
            let mut slots = String::new();
            let mut current = usize::MAX;
            for item in *part {
                let sent = &sample.sents[item.sent];
                if item.sent != current {
                    current = item.sent;
                    let context = sample.meta(&sent.id).map_or(Context::Prose, |m| m.context);
                    let _ = write!(out, "\n{}\n", batch::render(sent, context));
                }
                let answers: Vec<String> = names
                    .iter()
                    .zip(&item.said)
                    .map(|(name, code)| format!("{name} {code}"))
                    .collect();
                let _ = writeln!(
                    out,
                    "  {} {}: {}",
                    item.tok + 1,
                    sent.toks[item.tok].form,
                    answers.join(", ")
                );
                let _ = writeln!(slots, "{}: ", item.id);
            }
            let _ = write!(out, "\nSlots:\n{slots}");
            out
        })
        .collect()
}

/// The columns of `worklist.tsv` before the taggers' names, which come next, and the one after.
const WORK_FIRST: [&str; 4] = ["item", "sent_id", "token", "form"];
const WORK_LAST: &str = "differs";

/// The fewest taggers a merge, and so a worklist, has.
pub const FEWEST_TAGGERS: usize = 2;

/// `worklist.tsv`, the machine form of the worklist, which [`read_answers`] checks answers against.
/// Its columns are the item, the sentence, the token, the form, the name of each tagger in the
/// order of an item's `said`, and `differs`.
pub fn worklist_tsv(sample: &Sample, items: &[Item], names: &[&str]) -> String {
    let columns: Vec<&str> = WORK_FIRST
        .iter()
        .chain(names)
        .chain(std::iter::once(&WORK_LAST))
        .copied()
        .collect();
    let mut out = format!("{}\n", columns.join("\t"));
    for item in items {
        let sent = &sample.sents[item.sent];
        let said: Vec<String> = item.said.iter().map(|code| said(*code)).collect();
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}",
            item.id(sample),
            sent.id,
            item.tok + 1,
            sent.toks[item.tok].form,
            said.join("\t"),
            if item.unvoted {
                "none"
            } else if item.tags_differ {
                "tag"
            } else {
                "feature"
            }
        );
    }
    out
}

/// A row of `worklist.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    /// The item id, `g0007.5`.
    pub item: String,
    /// The sentence.
    pub sent_id: String,
    /// The token's number in the sentence, from 1.
    pub token: usize,
    /// The word.
    pub form: String,
    /// What each tagger said, as the worklist printed it, in the order of [`Worklist::names`].
    pub said: Vec<String>,
}

/// `worklist.tsv` read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worklist {
    /// The taggers' names, as the columns give them.
    pub names: Vec<String>,
    /// The rows.
    pub items: Vec<WorkItem>,
}

/// The taggers' names in a header of `columns` that opens with `first` and closes with `last`,
/// or `None` when the header is not that.
fn names_between<'h>(columns: &[&'h str], first: &[&str], last: &[&str]) -> Option<Vec<&'h str>> {
    let middle = columns.len().checked_sub(first.len() + last.len())?;
    let fits = columns.starts_with(first) && columns.ends_with(last);
    (fits && middle >= FEWEST_TAGGERS).then(|| columns[first.len()..first.len() + middle].to_vec())
}

/// Reads `worklist.tsv`, which came from `path`.
pub fn read_worklist(path: &str, text: &str) -> Result<Worklist, Error> {
    let mut items = Vec::new();
    let mut lines = text.lines().enumerate();
    let head: Vec<&str> = lines
        .next()
        .map(|(_, head)| head.split('\t').collect())
        .unwrap_or_default();
    let Some(names) = names_between(&head, &WORK_FIRST, &[WORK_LAST]) else {
        return Err(Error::at(
            path,
            1,
            format!(
                "the columns should be {}, the names of two or more taggers, and {WORK_LAST}",
                WORK_FIRST.join(", ")
            ),
        ));
    };
    let width = head.len();
    for (at, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != width {
            return Err(Error::at(
                path,
                at + 1,
                format!("expected {width} columns, found {}", cells.len()),
            ));
        }
        let token = cells[2]
            .parse()
            .map_err(|_| Error::at(path, at + 1, format!("bad token number `{}`", cells[2])))?;
        items.push(WorkItem {
            item: cells[0].to_string(),
            sent_id: cells[1].to_string(),
            token,
            form: cells[3].to_string(),
            said: cells[WORK_FIRST.len()..width - 1]
                .iter()
                .map(|cell| cell.to_string())
                .collect(),
        });
    }
    Ok(Worklist {
        names: names.into_iter().map(str::to_string).collect(),
        items,
    })
}

/// The most words a reason may have.
pub const REASON_WORDS: usize = 15;

/// One adjudicated word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// What it was asked about.
    pub item: WorkItem,
    /// The code decided.
    pub code: Code,
    /// Why, in 15 words or fewer.
    pub reason: String,
    /// The run of the adjudicator that made it, when it is not the run the log is written for: an
    /// answer settled by an earlier merge keeps the run that gave it.
    pub run: Option<String>,
}

/// The line of an answer, or why it is no answer: `g0007.5: N.p | reason`.
fn parse_answer<'w>(
    line: &str,
    wanted: &'w BTreeMap<&str, &WorkItem>,
) -> Result<(&'w WorkItem, Code, String), String> {
    let (item, rest) = line
        .split_once(':')
        .ok_or("not a line of the form `item: code | reason`: there is no colon")?;
    let item = item.trim();
    let work = wanted
        .get(item)
        .ok_or_else(|| format!("`{item}` is not an item of the worklist"))?;
    let (code, reason) = rest
        .split_once('|')
        .ok_or("there is no `|` between the code and the reason")?;
    let code = Code::parse(code.trim()).map_err(|why| format!("item {item}: {why}"))?;
    let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if reason.is_empty() {
        return Err(format!("item {item}: no reason"));
    }
    let words = reason.split(' ').count();
    if words > REASON_WORDS {
        return Err(format!(
            "item {item}: the reason has {words} words, and {REASON_WORDS} is the most"
        ));
    }
    Ok((work, code, reason))
}

/// A line of the adjudicator's answers that is not an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerFault {
    /// The file it is in.
    pub path: String,
    /// Its line, from 1.
    pub line: usize,
    /// The worklist item it answers, when it names one.
    pub item: Option<String>,
    /// The sentence of that item.
    pub sent_id: Option<String>,
    /// What is wrong, without the line's number.
    pub message: String,
}

impl AnswerFault {
    fn error(&self) -> Error {
        match &self.sent_id {
            Some(sent) => Problems::sentence(
                &self.path,
                sent,
                format!("line {}: {}", self.line, self.message),
            ),
            None => Error::at(&self.path, self.line, self.message.clone()),
        }
    }
}

/// Every answer in `files` that is one, in the order of `work`, and a fault for each line that is
/// not. An item answered twice keeps its first answer.
fn scan_answers(work: &[WorkItem], files: &[(String, String)]) -> (Vec<Answer>, Vec<AnswerFault>) {
    let wanted: BTreeMap<&str, &WorkItem> =
        work.iter().map(|item| (item.item.as_str(), item)).collect();
    let mut found: BTreeMap<&str, Answer> = BTreeMap::new();
    let mut faults = Vec::new();
    for (path, text) in files {
        for (at, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("```") || line.eq_ignore_ascii_case("slots:") {
                continue;
            }
            // A slot left empty is not an answer, but it is not a malformed line either.
            let empty_slot = line.split_once(':').is_some_and(|(item, rest)| {
                rest.trim().is_empty() && wanted.contains_key(item.trim())
            });
            if empty_slot {
                continue;
            }
            let fault = |item: Option<&WorkItem>, message: String| AnswerFault {
                path: path.clone(),
                line: at + 1,
                item: item.map(|item| item.item.clone()),
                sent_id: item.map(|item| item.sent_id.clone()),
                message,
            };
            match parse_answer(line, &wanted) {
                Ok((item, code, reason)) => {
                    if found.contains_key(item.item.as_str()) {
                        faults.push(fault(
                            Some(item),
                            format!("item {} is answered twice", item.item),
                        ));
                    } else {
                        found.insert(
                            item.item.as_str(),
                            Answer {
                                item: item.clone(),
                                code,
                                reason,
                                run: None,
                            },
                        );
                    }
                }
                Err(why) => {
                    let item = line
                        .split_once(':')
                        .and_then(|(item, _)| wanted.get(item.trim()))
                        .copied();
                    faults.push(fault(item, why));
                }
            }
        }
    }
    let answers = work
        .iter()
        .filter_map(|item| found.remove(item.item.as_str()))
        .collect();
    (answers, faults)
}

/// Reads the adjudicator's answers: each of `files` is a path and its text. Every line must be
/// `item: code | reason` for an item of `work`, once, with a code of the guide and a reason of
/// at most 15 words. With `all`, every item must be answered. Blank lines, code fences and the
/// lines of a `Slots:` heading are skipped. Returns the answers in the order of `work`.
pub fn read_answers(
    work: &[WorkItem],
    files: &[(String, String)],
    all: bool,
) -> Result<Vec<Answer>, Problems> {
    let (answers, faults) = scan_answers(work, files);
    let mut problems: Vec<Error> = faults.iter().map(AnswerFault::error).collect();
    if all {
        let answered: std::collections::BTreeSet<&str> = answers
            .iter()
            .map(|answer| answer.item.item.as_str())
            .collect();
        let missing: Vec<&str> = work
            .iter()
            .map(|item| item.item.as_str())
            .filter(|item| !answered.contains(item))
            .collect();
        if !missing.is_empty() {
            let shown: Vec<&str> = missing.iter().copied().take(8).collect();
            problems.push(Error::load(
                "the answers",
                Place::File,
                format!(
                    "{} items have no answer, among them {}",
                    missing.len(),
                    shown.join(", ")
                ),
            ));
        }
    }
    Problems::check(problems, answers)
}

/// What `read-answers --check` found: the good answers, and each item without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedAnswers {
    /// The answers that are answers, in the order of the worklist.
    pub answers: Vec<Answer>,
    /// The items with no answer, in the order of the worklist, each with what is wrong with the
    /// first line that tried to answer it, or `no answer` when none did.
    pub open: Vec<(String, String)>,
    /// Lines that name no item of the worklist, as `line N: message`.
    pub stray: Vec<String>,
}

/// Reads the answers as [`read_answers`] does, but keeps the good ones whatever else is wrong and
/// returns what is left open instead of failing. An item with a good answer and a bad line too is
/// answered.
pub fn check_answers(work: &[WorkItem], files: &[(String, String)]) -> CheckedAnswers {
    let (answers, faults) = scan_answers(work, files);
    let answered: std::collections::BTreeSet<&str> = answers
        .iter()
        .map(|answer| answer.item.item.as_str())
        .collect();
    let mut why: BTreeMap<&str, &str> = BTreeMap::new();
    for fault in &faults {
        if let Some(item) = &fault.item {
            why.entry(item).or_insert(&fault.message);
        }
    }
    let open = work
        .iter()
        .filter(|item| !answered.contains(item.item.as_str()))
        .map(|item| {
            let message = why.get(item.item.as_str()).copied().unwrap_or("no answer");
            (
                item.item.clone(),
                message.split_whitespace().collect::<Vec<_>>().join(" "),
            )
        })
        .collect();
    let stray = faults
        .iter()
        .filter(|fault| fault.item.is_none())
        .map(|fault| format!("line {}: {}", fault.line, fault.message))
        .collect();
    CheckedAnswers {
        answers,
        open,
        stray,
    }
}

/// The columns of `<name>.problems.tsv` of the adjudication.
pub fn open_tsv(open: &[(String, String)]) -> String {
    let mut out = String::from("item\tproblem\n");
    for (item, message) in open {
        let _ = writeln!(out, "{item}\t{message}");
    }
    out
}

/// The columns of `adjudicated.tsv`, the log of the adjudication, before the taggers' names, which
/// come next, and after them. A log written for an adjudicator's run adds `run` last.
const LOG_FIRST: [&str; 4] = ["item", "sent_id", "token", "form"];
const LOG_LAST: [&str; 2] = ["final", "reason"];
const LOG_RUN: &str = "run";

/// An agreed word that the revised guide changes: it is adjudicated after all, with a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    /// The sentence.
    pub sent_id: String,
    /// The token's number in the sentence, from 1.
    pub token: usize,
    /// The word.
    pub form: String,
    /// The code the three taggers agreed on.
    pub old: Code,
    /// The code decided now.
    pub new: Code,
    /// Why, in 15 words or fewer.
    pub reason: String,
}

const OVERRIDE_COLUMNS: [&str; 6] = [
    "sentence_id",
    "token_index",
    "form",
    "old_code",
    "new_code",
    "reason",
];

/// Reads `overrides.tsv`, which came from `path`, against `agreed`, which came from
/// `agreed_path`: each row must name a word of a sentence of the agreed file that the taggers
/// agreed on (`Prov=agree`), with the code they agreed on as its `old_code`, once, and a new code
/// of the guide that differs from it, with a reason of at most 15 words.
pub fn read_overrides(
    path: &str,
    text: &str,
    agreed_path: &str,
    agreed: &str,
) -> Result<Vec<Override>, Problems> {
    let blocks = conllu::read(agreed_path, agreed)?;
    let mut by_id: BTreeMap<&str, &conllu::Block> = BTreeMap::new();
    for block in &blocks {
        if let Some(comment) = block.comment("sent_id") {
            by_id.insert(comment.value.as_str(), block);
        }
    }
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, head)) if head.split('\t').eq(OVERRIDE_COLUMNS) => {}
        _ => {
            return Err(Error::at(
                path,
                1,
                format!("the columns should be {}", OVERRIDE_COLUMNS.join(", ")),
            )
            .into());
        }
    }
    let mut rows: Vec<Override> = Vec::new();
    let mut problems = Vec::new();
    for (at, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != OVERRIDE_COLUMNS.len() {
            problems.push(Error::at(
                path,
                at + 1,
                format!(
                    "expected {} columns, found {}",
                    OVERRIDE_COLUMNS.len(),
                    cells.len()
                ),
            ));
            continue;
        }
        let (sent_id, form) = (cells[0], cells[2]);
        let mut say = |why: String| {
            problems.push(Problems::sentence(
                path,
                sent_id,
                format!("line {}: {why}", at + 1),
            ));
        };
        let Ok(token) = cells[1].parse::<usize>() else {
            say(format!("bad token number `{}`", cells[1]));
            continue;
        };
        let (old, new) = match (Code::parse(cells[3]), Code::parse(cells[4])) {
            (Ok(old), Ok(new)) => (old, new),
            (Err(why), _) | (_, Err(why)) => {
                say(why);
                continue;
            }
        };
        let reason = cells[5].split_whitespace().collect::<Vec<_>>().join(" ");
        let words = reason.split(' ').filter(|word| !word.is_empty()).count();
        if words == 0 {
            say(format!("token {token}: no reason"));
            continue;
        }
        if words > REASON_WORDS {
            say(format!(
                "token {token}: the reason has {words} words, and {REASON_WORDS} is the most"
            ));
            continue;
        }
        let Some(block) = by_id.get(sent_id) else {
            say("it is not in the agreed file".to_string());
            continue;
        };
        let Some(read) = token
            .checked_sub(1)
            .and_then(|index| block.lines.get(index))
        else {
            say(format!("token {token} is not in the sentence"));
            continue;
        };
        let prov = conllu::pairs(&read.misc)
            .iter()
            .find(|(key, _)| *key == "Prov")
            .map(|(_, value)| *value);
        if read.form != form {
            say(format!("token {token} is `{}`, not `{form}`", read.form));
            continue;
        }
        if prov != Some("agree") {
            say(format!(
                "token {token} `{form}` was not agreed by the taggers"
            ));
            continue;
        }
        match Code::from_conllu(&read.upos, &read.feats) {
            Ok(agreed) if agreed == old => {}
            Ok(agreed) => {
                say(format!(
                    "token {token} `{form}` was agreed as {agreed}, not {old}"
                ));
                continue;
            }
            Err(why) => {
                say(format!("token {token} `{form}`: {why}"));
                continue;
            }
        }
        if new == old {
            say(format!(
                "token {token} `{form}`: the new code is the old one"
            ));
            continue;
        }
        if rows
            .iter()
            .any(|row| row.sent_id == sent_id && row.token == token)
        {
            say(format!("token {token} is overridden twice"));
            continue;
        }
        rows.push(Override {
            sent_id: sent_id.to_string(),
            token,
            form: form.to_string(),
            old,
            new,
            reason,
        });
    }
    Problems::check(problems, rows)
}

/// `adjudicated.tsv`: each answered item with what each of the taggers `names` said, the code
/// decided and why, then each override. An override's item is `sentence.token`, and the columns of
/// what the taggers said hold the code they agreed on, which is how `assemble` knows it from an
/// answer. With `run`, the adjudicator's run, every row ends with it in a `run` column, which
/// `finish` copies into the `Runs=` of the word; an answer with a run of its own keeps that one.
pub fn adjudicated_tsv(
    names: &[String],
    answers: &[Answer],
    overrides: &[Override],
    run: Option<&str>,
) -> String {
    let with_run = run.is_some() || answers.iter().any(|answer| answer.run.is_some());
    let mut columns: Vec<&str> = LOG_FIRST.to_vec();
    columns.extend(names.iter().map(String::as_str));
    columns.extend(LOG_LAST);
    columns.extend(with_run.then_some(LOG_RUN));
    let mut out = format!("{}\n", columns.join("\t"));
    let tail_of = |own: Option<&str>| {
        if with_run {
            format!("\t{}", own.or(run).unwrap_or("-"))
        } else {
            String::new()
        }
    };
    for answer in answers {
        let item = &answer.item;
        let tail = tail_of(answer.run.as_deref());
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}{tail}",
            item.item,
            item.sent_id,
            item.token,
            item.form,
            item.said.join("\t"),
            answer.code,
            answer.reason
        );
    }
    let tail = tail_of(None);
    for row in overrides {
        let old = vec![row.old.to_string(); names.len()].join("\t");
        let _ = writeln!(
            out,
            "{id}.{token}\t{id}\t{token}\t{}\t{old}\t{}\t{}{tail}",
            row.form,
            row.new,
            row.reason,
            id = row.sent_id,
            token = row.token,
        );
    }
    out
}

/// The columns of `settled.tsv`: the answers a merge takes from an earlier one.
const SETTLED_COLUMNS: [&str; 4] = ["item", "final", "reason", "run"];

/// The answers of the earlier merge's log `text`, which came from `path`, for the items of `work`
/// that the log answered, each keeping the run that answered it. A merge of the same sample with a
/// voter more disputes mostly the same words; the adjudicator is asked about each only once, so
/// the two merges differ in their voting alone. An item the log has no run for is an error, since
/// the answer would lose its provenance.
pub fn settle_from_log(path: &str, text: &str, work: &[WorkItem]) -> Result<Vec<Answer>, Problems> {
    let mut lines = text.lines().enumerate();
    let head: Vec<&str> = lines
        .next()
        .map(|(_, head)| head.split('\t').collect())
        .unwrap_or_default();
    let column = |name: &str| head.iter().position(|cell| *cell == name);
    let (Some(item_at), Some(final_at), Some(reason_at), Some(run_at)) = (
        column("item"),
        column("final"),
        column("reason"),
        column(LOG_RUN),
    ) else {
        return Err(Error::at(
            path,
            1,
            "the log to settle from needs the columns item, final, reason and run",
        )
        .into());
    };
    let wanted: BTreeMap<&str, &WorkItem> =
        work.iter().map(|item| (item.item.as_str(), item)).collect();
    let mut answers = Vec::new();
    let mut problems = Vec::new();
    for (at, line) in lines.filter(|(_, line)| !line.trim().is_empty()) {
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != head.len() {
            problems.push(Error::at(
                path,
                at + 1,
                format!("expected {} columns, found {}", head.len(), cells.len()),
            ));
            continue;
        }
        let Some(item) = wanted.get(cells[item_at]) else {
            continue;
        };
        let run = cells[run_at];
        if run.is_empty() || run == "-" {
            problems.push(Error::at(path, at + 1, "the answer has no run"));
            continue;
        }
        match Code::parse(cells[final_at]) {
            Ok(code) => answers.push(Answer {
                item: (*item).clone(),
                code,
                reason: cells[reason_at].to_string(),
                run: Some(run.to_string()),
            }),
            Err(why) => problems.push(Error::at(path, at + 1, why)),
        }
    }
    Problems::check(problems, answers)
}

/// `settled.tsv`: the answers `settle_from_log` took, which `read-answers` adds to the ones the
/// adjudicator gives for the rest.
pub fn settled_tsv(answers: &[Answer]) -> String {
    let mut out = format!("{}\n", SETTLED_COLUMNS.join("\t"));
    for answer in answers {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}",
            answer.item.item,
            answer.code,
            answer.reason,
            answer.run.as_deref().unwrap_or("-")
        );
    }
    out
}

/// Reads `settled.tsv`, which came from `path`, for the items of `work`.
pub fn read_settled(path: &str, text: &str, work: &[WorkItem]) -> Result<Vec<Answer>, Problems> {
    let mut lines = text.lines().enumerate();
    if lines
        .next()
        .map(|(_, head)| head.split('\t').collect::<Vec<_>>())
        != Some(SETTLED_COLUMNS.to_vec())
    {
        return Err(Error::at(
            path,
            1,
            format!("the columns should be {}", SETTLED_COLUMNS.join(", ")),
        )
        .into());
    }
    let wanted: BTreeMap<&str, &WorkItem> =
        work.iter().map(|item| (item.item.as_str(), item)).collect();
    let mut answers = Vec::new();
    let mut problems = Vec::new();
    for (at, line) in lines.filter(|(_, line)| !line.trim().is_empty()) {
        let cells: Vec<&str> = line.split('\t').collect();
        let item = wanted.get(cells.first().copied().unwrap_or(""));
        match (cells.len() == SETTLED_COLUMNS.len(), item) {
            (true, Some(item)) => match Code::parse(cells[1]) {
                Ok(code) => answers.push(Answer {
                    item: (*item).clone(),
                    code,
                    reason: cells[2].to_string(),
                    run: Some(cells[3].to_string()),
                }),
                Err(why) => problems.push(Error::at(path, at + 1, why)),
            },
            _ => problems.push(Error::at(
                path,
                at + 1,
                "a row of four cells for an item of the worklist is expected",
            )),
        }
    }
    Problems::check(problems, answers)
}

/// A row of `adjudicated.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logged {
    /// The sentence.
    pub sent_id: String,
    /// The token's number in the sentence, from 1.
    pub token: usize,
    /// The word.
    pub form: String,
    /// The code decided.
    pub code: Code,
    /// For an override, the code the taggers had agreed on.
    pub agreed: Option<Code>,
    /// The adjudicator's run, when the log has a `run` column.
    pub run: Option<String>,
}

/// Reads `adjudicated.tsv`, which came from `path`.
pub fn read_log(path: &str, text: &str) -> Result<Vec<Logged>, Error> {
    let mut lines = text.lines().enumerate();
    let head: Vec<&str> = lines
        .next()
        .map(|(_, head)| head.split('\t').collect())
        .unwrap_or_default();
    let with_run = head.last() == Some(&LOG_RUN);
    let closing: Vec<&str> = if with_run {
        LOG_LAST.iter().copied().chain([LOG_RUN]).collect()
    } else {
        LOG_LAST.to_vec()
    };
    let Some(names) = names_between(&head, &LOG_FIRST, &closing) else {
        return Err(Error::at(
            path,
            1,
            format!(
                "the columns should be {}, the names of two or more taggers, and {}",
                LOG_FIRST.join(", "),
                closing.join(", ")
            ),
        ));
    };
    let (width, voters) = (head.len(), names.len());
    let mut rows = Vec::new();
    for (at, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != width {
            return Err(Error::at(
                path,
                at + 1,
                format!("expected {width} columns, found {}", cells.len()),
            ));
        }
        let bad = |what: &str, value: &str| Error::at(path, at + 1, format!("{what} `{value}`"));
        let said = &cells[LOG_FIRST.len()..LOG_FIRST.len() + voters];
        let final_at = LOG_FIRST.len() + voters;
        rows.push(Logged {
            sent_id: cells[1].to_string(),
            token: cells[2]
                .parse()
                .map_err(|_| bad("bad token number", cells[2]))?,
            form: cells[3].to_string(),
            code: Code::parse(cells[final_at]).map_err(|why| bad(&why, cells[final_at]))?,
            // Equal answers all round are no dispute: the row is an override of an agreed word.
            agreed: if said.iter().all(|cell| *cell == said[0]) {
                Code::parse(said[0]).ok()
            } else {
                None
            },
            run: with_run.then(|| cells[width - 1].to_string()),
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::read_tags;
    use crate::compact::tests::sample;

    fn code(text: &str) -> Code {
        Code::parse(text).unwrap()
    }

    /// A tagger's CoNLL-U: for each word of the sample, `upos` and `feats`, in order, as
    /// `(upos, feats)`; the other tokens get a placeholder.
    fn tagger(words: &[(&str, &str)]) -> String {
        let sample = sample();
        let mut words = words.iter();
        let mut out = String::new();
        for sent in &sample.sents {
            let _ = writeln!(out, "# sent_id = {}", sent.id);
            for (index, tok) in sent.toks.iter().enumerate() {
                let (upos, feats) = if tok.is_word() {
                    *words.next().expect("a tag for each word")
                } else {
                    ("X", "_")
                };
                out.push_str(&line(index, &tok.form, upos, feats, &misc(tok, None, None)));
            }
            out.push('\n');
        }
        assert!(words.next().is_none(), "too many tags");
        out
    }

    /// The blind tagger's answer: `s1: V.fi _ T V.in _` and `s2: R _ D N.s N.p`.
    fn blind() -> String {
        read_tags(
            &sample(),
            &[(
                "b".to_string(),
                "s1: V.fi _ T V.in _\ns2: R _ D N.s N.p\n".to_string(),
            )],
            "blind",
            None,
            true,
        )
        .unwrap()
        .0
    }

    /// Run, to, compile, Why, The, user's, files.
    const SAME: [(&str, &str); 7] = [
        ("VERB", "Mood=Imp|VerbForm=Fin"),
        ("PART", "_"),
        ("VERB", "VerbForm=Inf"),
        ("ADV", "_"),
        ("DET", "_"),
        ("NOUN", "Number=Sing"),
        ("NOUN", "Number=Plur"),
    ];

    fn merged(blind: &str, harper: &[(&str, &str)], spacy: &[(&str, &str)]) -> (Sample, Merged) {
        let sample = sample();
        let load = |name: &str, text: &str| load_tagger(name, name, text, &sample).unwrap();
        let taggers = [
            load("blind", blind),
            load("harper", &tagger(harper)),
            load("spacy", &tagger(spacy)),
        ];
        let merged = merge(&sample, &taggers);
        (sample, merged)
    }

    #[test]
    fn judge_agrees_on_equal_codes_and_on_abstaining_features() {
        assert_eq!(
            judge(code("N.p"), code("N.p"), code("N.p")),
            Verdict::Agreed(code("N.p"))
        );
        let bare = Code::from_conllu("NOUN", "_").unwrap();
        assert_eq!(judge(code("N.p"), bare, bare), Verdict::Agreed(code("N.p")));
        let bare_verb = Code::from_conllu("VERB", "_").unwrap();
        assert_eq!(
            judge(code("V.pp"), code("V.pp"), bare_verb),
            Verdict::Agreed(code("V.pp"))
        );
    }

    #[test]
    fn judge_disputes_a_different_base_or_a_conflicting_feature() {
        assert_eq!(
            judge(code("N.p"), code("J"), code("N.p")),
            Verdict::Disputed { tags_differ: true }
        );
        assert_eq!(
            judge(code("N.p"), code("N.p"), code("PN.p")),
            Verdict::Disputed { tags_differ: true }
        );
        assert_eq!(
            judge(code("N.p"), code("N.s"), code("N.p")),
            Verdict::Disputed { tags_differ: false }
        );
        assert_eq!(
            judge(code("V.pp"), code("V.pp"), code("V.pa")),
            Verdict::Disputed { tags_differ: false }
        );
    }

    #[test]
    fn harper_x_abstains_and_the_other_two_decide() {
        // Harper's `X` agrees with whatever the blind tagger and spaCy agree on.
        assert_eq!(
            judge(code("N.p"), code("X"), code("N.p")),
            Verdict::Agreed(code("N.p"))
        );
        assert_eq!(
            judge(code("V.pp"), code("X"), code("V.pp")),
            Verdict::Agreed(code("V.pp"))
        );
        // They still dispute when blind and spaCy differ, in a base or in a feature.
        assert_eq!(
            judge(code("N.p"), code("X"), code("J")),
            Verdict::Disputed { tags_differ: true }
        );
        assert_eq!(
            judge(code("N.p"), code("X"), code("N.s")),
            Verdict::Disputed { tags_differ: false }
        );
        // Only Harper abstains: an `X` from spaCy against another base is a real answer.
        assert_eq!(
            judge(code("N.p"), code("N.p"), code("X")),
            Verdict::Disputed { tags_differ: true }
        );
    }

    #[test]
    fn harper_abstentions_are_left_out_of_its_pairs() {
        let mut harper = SAME;
        harper[6] = ("X", "_"); // files
        harper[3] = ("ADJ", "_"); // Why: a real disagreement
        let (sample, merged) = merged(&blind(), &harper, &SAME);
        let ids: Vec<String> = merged.items.iter().map(|i| i.id(&sample)).collect();
        assert_eq!(ids, ["s2.1"]);
        assert_eq!(merged.stats.words, 7);
        assert_eq!(merged.stats.abstained, 1);
        assert_eq!(merged.stats.pair_words, [6, 7, 6]);
        assert_eq!(merged.stats.pairs, [5, 7, 5]);
        assert_eq!(merged.stats.tag3, 6);
        assert_eq!(merged.stats.full3, 6);
        let report = merged.stats.to_string();
        assert!(report.contains("harper abstained on"), "{report}");
        assert!(report.contains("83.3%"), "{report}");
    }

    #[test]
    fn the_blind_code_decides_whether_a_pronoun_has_a_number() {
        // The guide gives `you` and `which` no number; another tagger's number is not a conflict.
        assert_eq!(
            judge(code("PR"), code("PR.s"), code("PR.p")),
            Verdict::Agreed(code("PR"))
        );
        assert_eq!(
            judge(code("PR.s"), code("PR.s"), code("PR.p")),
            Verdict::Disputed { tags_differ: false }
        );
    }

    #[test]
    fn three_agreeing_taggers_leave_nothing_to_adjudicate() {
        let (sample, merged) = merged(&blind(), &SAME, &SAME);
        assert!(merged.items.is_empty());
        assert_eq!(merged.stats.words, 7);
        assert_eq!(merged.stats.pairs, [7, 7, 7]);
        assert_eq!(merged.stats.tag3, 7);
        assert_eq!(merged.stats.full3, 7);
        let out = agreed_conllu(&sample, &merged.verdicts, &[]);
        assert!(
            !out.contains("\t_\t_\t_\t_\t_\t_\tKind=Word\n"),
            "no pending word"
        );
        assert!(out.contains("4\tuser's\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree"));
        assert!(out.contains("2\tmake ci\t_\tX\t_\t_\t_\t_\t_\tKind=Code|Prov=kind"));
    }

    #[test]
    fn a_disagreement_goes_to_the_worklist_and_leaves_its_word_blank() {
        let mut harper = SAME;
        harper[6] = ("ADJ", "_"); // files
        let mut spacy = SAME;
        spacy[2] = ("NOUN", "Number=Sing"); // compile
        spacy[5] = ("NOUN", "Number=Plur"); // user's: only a feature
        let (sample, merged) = merged(&blind(), &harper, &spacy);
        let ids: Vec<String> = merged.items.iter().map(|i| i.id(&sample)).collect();
        assert_eq!(ids, ["s1.4", "s2.4", "s2.5"]);
        assert!(merged.items[0].tags_differ);
        assert!(!merged.items[1].tags_differ);
        assert_eq!(merged.stats.pairs, [6, 6, 5]);
        assert_eq!(merged.stats.tag3, 5);
        assert_eq!(merged.stats.full3, 4);
        assert_eq!(merged.stats.tag_disputes, 2);
        assert_eq!(merged.stats.feature_disputes, 1);
        assert_eq!(merged.stats.by_tier["human"], (7, 4));
        assert_eq!(merged.stats.by_context["list-item"], (4, 2));
        assert_eq!(merged.stats.by_context["prose"], (3, 2));

        let out = agreed_conllu(&sample, &merged.verdicts, &[]);
        assert!(
            out.contains("4\tcompile\t_\t_\t_\t_\t_\t_\t_\tKind=Word|SpaceAfter=No\n"),
            "{out}"
        );
        assert!(
            out.contains("5\tfiles\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n"),
            "{out}"
        );
        assert!(out.contains("1\tRun\t_\tVERB\t_\tVerbForm=Fin\t_\t_\t_\tKind=Word|Prov=agree"));
    }

    #[test]
    fn the_report_names_pairs_the_three_and_the_strata() {
        let mut harper = SAME;
        harper[6] = ("ADJ", "_");
        let (_, merged) = merged(&blind(), &harper, &SAME);
        let report = merged.stats.to_string();
        assert!(report.contains("Agreement over 7 word tokens"));
        assert!(report.contains("blind and harper"));
        assert!(report.contains("harper and spacy"));
        assert!(report.contains("agree on it and every feature (these stand)"));
        assert!(report.contains("85.7%"), "{report}");
        assert!(report.contains("by tier"));
        assert!(report.contains("list-item"));
    }

    #[test]
    fn a_tagger_file_that_does_not_match_the_sample_is_rejected_by_sentence() {
        let sample = sample();
        let mut text = tagger(&SAME);
        text = text.replace("user's", "users");
        let error = load_tagger("harper", "h.conllu", &text, &sample)
            .unwrap_err()
            .to_string();
        assert!(error.contains("h.conllu:"), "{error}");
        assert!(
            error.contains("sentence s2: token 4 is `user's` in the sample and `users` here"),
            "{error}"
        );

        let missing = text_without(&tagger(&SAME), "s2");
        let error = load_tagger("harper", "h.conllu", &missing, &sample)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("sentence s2: harper has no answer for it"),
            "{error}"
        );

        let bad_tag = tagger(&SAME).replacen("ADV", "WORD", 1);
        let error = load_tagger("harper", "h.conllu", &bad_tag, &sample)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("UPOS `WORD` is not one of the 17 UD tags"),
            "{error}"
        );

        let unfilled = tagger(&SAME).replacen("ADV", "_", 1);
        let error = load_tagger("harper", "h.conllu", &unfilled, &sample)
            .unwrap_err()
            .to_string();
        assert!(error.contains("no UPOS"), "{error}");

        let extra = format!(
            "{}# sent_id = zz\n1\tx\t_\tNOUN\t_\t_\t_\t_\t_\t_\n",
            tagger(&SAME)
        );
        let error = load_tagger("harper", "h.conllu", &extra, &sample)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("sentence zz: harper tagged a sentence that is not in the sample"),
            "{error}"
        );
    }

    fn text_without(text: &str, sent_id: &str) -> String {
        text.split("\n\n")
            .filter(|block| !block.contains(&format!("# sent_id = {sent_id}\n")))
            .collect::<Vec<_>>()
            .join("\n\n")
            + "\n\n"
    }

    #[test]
    fn the_worklist_shows_the_sentence_the_three_answers_and_a_slot() {
        let mut harper = SAME;
        harper[6] = ("ADJ", "_");
        let mut spacy = SAME;
        spacy[2] = ("NOUN", "Number=Sing");
        let (sample, merged) = merged(&blind(), &harper, &spacy);
        let parts = worklist_parts(&sample, &merged.items, 60, &NAMES);
        assert_eq!(parts.len(), 1);
        let part = &parts[0];
        assert!(part.contains("15 words or fewer"));
        assert!(
            part.contains("\ns1: 1 Run 2 [code: make ci] 3 to 4 compile 5 [,]\n"),
            "{part}"
        );
        assert!(
            part.contains("\n  4 compile: blind V.in, harper V.in, spacy N.s\n"),
            "{part}"
        );
        assert!(
            part.contains("\ns2 (list item): 1 Why 2 [:] 3 The 4 user's 5 files\n"),
            "{part}"
        );
        assert!(
            part.contains("\n  5 files: blind N.p, harper J, spacy N.p\n"),
            "{part}"
        );
        assert!(part.ends_with("Slots:\ns1.4: \ns2.5: \n"), "{part}");
        assert!(
            !part.contains("human") && !part.contains("dev"),
            "no tier or split: {part}"
        );
    }

    #[test]
    fn a_feature_a_tagger_left_out_shows_as_a_question_mark() {
        let bare = |upos| Code::from_conllu(upos, "_").unwrap();
        assert_eq!(said(Some(bare("NOUN"))), "N.?");
        assert_eq!(said(Some(bare("PROPN"))), "PN.?");
        assert_eq!(said(Some(bare("VERB"))), "V.?");
        assert_eq!(said(Some(bare("AUX"))), "AX.?");
        assert_eq!(
            said(Some(bare("PRON"))),
            "PR",
            "a pronoun may have no number"
        );
        assert_eq!(said(Some(bare("ADJ"))), "J");
        assert_eq!(said(Some(code("N.s"))), "N.s");
        assert_eq!(said(Some(code("V.pp"))), "V.pp");
    }

    #[test]
    fn the_worklist_splits_into_parts_by_whole_sentences() {
        let sample = sample();
        let item = |sent, tok| Item {
            sent,
            tok,
            said: vec![Some(code("N.s")); 3],
            tags_differ: true,
            unvoted: false,
        };
        let items = [item(0, 0), item(0, 2), item(1, 0), item(1, 2), item(1, 3)];
        assert_eq!(worklist_parts(&sample, &items, 100, &NAMES).len(), 1);
        let parts = worklist_parts(&sample, &items, 2, &NAMES);
        assert_eq!(parts.len(), 2, "a sentence is never split");
        assert!(
            parts[0].contains("s1.1") && parts[0].contains("s1.3") && !parts[0].contains("s2.1")
        );
        assert!(parts[1].contains("s2.1") && parts[1].contains("s2.4"));
        assert!(worklist_parts(&sample, &[], 10, &NAMES).is_empty());
        let one = worklist_parts(&sample, &items, 1, &NAMES);
        assert_eq!(
            one.len(),
            2,
            "one item per part still keeps a sentence whole"
        );
    }

    fn names() -> Vec<String> {
        NAMES.map(String::from).to_vec()
    }

    fn work() -> Worklist {
        let mut harper = SAME;
        harper[6] = ("ADJ", "_");
        let mut spacy = SAME;
        spacy[2] = ("NOUN", "Number=Sing");
        let (sample, merged) = merged(&blind(), &harper, &spacy);
        read_worklist("w.tsv", &worklist_tsv(&sample, &merged.items, &NAMES)).unwrap()
    }

    #[test]
    fn the_machine_worklist_reads_back() {
        let work = work();
        assert_eq!(work.names, NAMES);
        let work = work.items;
        assert_eq!(work.len(), 2);
        assert_eq!(work[0].item, "s1.4");
        assert_eq!(work[0].sent_id, "s1");
        assert_eq!(work[0].token, 4);
        assert_eq!(work[0].form, "compile");
        assert_eq!(work[0].said, ["V.in", "V.in", "N.s"]);
        assert_eq!(work[1].said, ["N.p", "J", "N.p"]);
        assert!(read_worklist("w.tsv", "item\tsent\n").is_err());
    }

    fn answers(text: &str, all: bool) -> Result<Vec<Answer>, Problems> {
        read_answers(
            &work().items,
            &[("a.txt".to_string(), text.to_string())],
            all,
        )
    }

    #[test]
    fn good_answers_are_read_in_the_order_of_the_worklist() {
        let got = answers(
            "Slots:\ns2.5: N.p | plural noun, agrees with are\n\ns1.4: V.in | infinitive after to\n",
            true,
        )
        .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].item.item, "s1.4");
        assert_eq!(got[0].code, code("V.in"));
        assert_eq!(got[0].reason, "infinitive after to");
        assert_eq!(got[1].code, code("N.p"));
    }

    #[test]
    fn a_bad_answer_names_its_item_and_its_sentence() {
        let says = |text: &str, expect: &str| {
            let error = answers(text, false).unwrap_err().to_string();
            assert!(error.contains(expect), "{error} should say {expect}");
        };
        says(
            "s1.4: V | no feature",
            "sentence s1: line 1: item s1.4: `V` needs a verb form",
        );
        says("s1.4: Q.s | x", "`Q.s` is not a code of the guide");
        says(
            "s1.4: V.in",
            "there is no `|` between the code and the reason",
        );
        says("s1.4: V.in |  ", "item s1.4: no reason");
        says(
            "s9.1: N.s | x",
            "a.txt:1: `s9.1` is not an item of the worklist",
        );
        says("what is this", "a.txt:1: not a line of the form");
        let long = vec!["word"; 16].join(" ");
        says(
            &format!("s1.4: V.in | {long}"),
            "the reason has 16 words, and 15 is the most",
        );
        let fifteen = vec!["word"; 15].join(" ");
        assert!(answers(&format!("s1.4: V.in | {fifteen}"), false).is_ok());
        says(
            "s1.4: V.in | a\ns1.4: V.in | b",
            "item s1.4 is answered twice",
        );
    }

    #[test]
    fn every_item_must_be_answered_when_all_are_required() {
        let error = answers("s1.4: V.in | x\n", true).unwrap_err().to_string();
        assert!(
            error.contains("1 items have no answer, among them s2.5"),
            "{error}"
        );
        assert_eq!(answers("s1.4: V.in | x\n", false).unwrap().len(), 1);
        // Slots left empty are skipped, not malformed.
        assert_eq!(answers("s1.4: \ns2.5: \n", false).unwrap().len(), 0);
    }

    #[test]
    fn the_log_records_what_the_three_said_what_was_decided_and_why() {
        let got = answers("s1.4: V.in | x\ns2.5: N.p | y z\n", true).unwrap();
        let log = adjudicated_tsv(&names(), &got, &[], None);
        let mut lines = log.lines();
        assert_eq!(
            lines.next().unwrap(),
            "item\tsent_id\ttoken\tform\tblind\tharper\tspacy\tfinal\treason"
        );
        assert_eq!(
            lines.next().unwrap(),
            "s1.4\ts1\t4\tcompile\tV.in\tV.in\tN.s\tV.in\tx"
        );
        assert_eq!(
            lines.next().unwrap(),
            "s2.5\ts2\t5\tfiles\tN.p\tJ\tN.p\tN.p\ty z"
        );
        let rows = read_log("log", &log).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].sent_id, "s1");
        assert_eq!(rows[0].token, 4);
        assert_eq!(rows[1].code, code("N.p"));
    }

    #[test]
    fn check_mode_keeps_the_good_answers_and_names_what_is_open() {
        let work = work();
        let long = "a b c d e f g h i j k l m n o p";
        let text = format!("s1.4: V.in | fine\ns2.5: N.p | {long}\nnot an answer\ns9.1: N.s | x\n");
        let got = check_answers(&work.items, &[("a.txt".to_string(), text)]);
        assert_eq!(got.answers.len(), 1);
        assert_eq!(got.answers[0].item.item, "s1.4");
        assert_eq!(got.open.len(), 1);
        assert_eq!(got.open[0].0, "s2.5");
        assert!(got.open[0].1.contains("16 words"), "{:?}", got.open);
        assert_eq!(got.stray.len(), 2, "{:?}", got.stray);
        let tsv = open_tsv(&got.open);
        assert!(tsv.starts_with("item\tproblem\ns2.5\t"), "{tsv}");
        // An item with no line at all is open too.
        let none = check_answers(&work.items, &[("a.txt".to_string(), String::new())]);
        assert_eq!(
            none.open,
            [
                ("s1.4".to_string(), "no answer".to_string()),
                ("s2.5".to_string(), "no answer".to_string())
            ]
        );
        // A good answer wins over a bad line for the same item.
        let both = check_answers(
            &work.items,
            &[(
                "a.txt".to_string(),
                "s1.4: V.in | x\ns1.4: ZZ | y\n".to_string(),
            )],
        );
        assert_eq!(both.answers.len(), 1);
        assert_eq!(both.open.len(), 1);
    }

    #[test]
    fn the_retry_asks_only_the_open_items_and_shows_the_voters_by_letter() {
        let sample = sample();
        let work = work();
        let parts = retry_parts(&sample, &work, &["s2.5".to_string()], 60);
        assert_eq!(parts.len(), 1);
        let part = &parts[0];
        assert!(part.contains("(A, B, C)"), "{part}");
        assert!(part.contains("\n  5 files: A N.p, B J, C N.p\n"), "{part}");
        assert!(
            !part.contains("compile") && !part.contains("blind"),
            "{part}"
        );
        assert!(part.ends_with("Slots:\ns2.5: \n"), "{part}");
        assert!(retry_parts(&sample, &work, &[], 60).is_empty());
    }

    #[test]
    fn an_answer_settled_by_an_earlier_merge_keeps_its_run_and_is_not_asked_again() {
        let got = answers("s1.4: V.in | x\ns2.5: N.p | y z\n", true).unwrap();
        // The earlier merge's log: both answered, by run r4.
        let earlier = adjudicated_tsv(&names(), &got, &[], Some("r4"));
        let work = work();
        let settled = settle_from_log("earlier", &earlier, &work.items[..1]).unwrap();
        assert_eq!(settled.len(), 1, "only the items of the new worklist");
        assert_eq!(settled[0].item.item, "s1.4");
        assert_eq!(settled[0].run.as_deref(), Some("r4"));
        // settled.tsv reads back, and the log of the new merge names the run of each answer: r4
        // for the settled, the new adjudicator's r9 for the rest.
        let tsv = settled_tsv(&settled);
        let back = read_settled("settled.tsv", &tsv, &work.items).unwrap();
        assert_eq!(back, settled);
        let mut all = back;
        all.push(got[1].clone());
        let log = adjudicated_tsv(&names(), &all, &[], Some("r9"));
        let rows = read_log("log", &log).unwrap();
        assert_eq!(rows[0].run.as_deref(), Some("r4"));
        assert_eq!(rows[1].run.as_deref(), Some("r9"));
        // An answer with no run to keep is refused: its provenance would be lost.
        let bare = adjudicated_tsv(&names(), &got, &[], None);
        assert!(settle_from_log("bare", &bare, &work.items).is_err());
    }

    #[test]
    fn the_log_carries_the_adjudicator_s_run_and_reads_back_with_or_without_it() {
        let got = answers("s1.4: V.in | x\ns2.5: N.p | y z\n", true).unwrap();
        let log = adjudicated_tsv(&names(), &got, &[], Some("r9"));
        assert!(
            log.starts_with(
                "item\tsent_id\ttoken\tform\tblind\tharper\tspacy\tfinal\treason\trun\n"
            ),
            "{log}"
        );
        assert!(log.contains("\tx\tr9\n"));
        let rows = read_log("log", &log).unwrap();
        assert_eq!(rows[0].run.as_deref(), Some("r9"));
        assert_eq!(rows[1].code, code("N.p"));
        let plain = read_log("log", &adjudicated_tsv(&names(), &got, &[], None)).unwrap();
        assert_eq!(plain[0].run, None);
        // Any number of voters, two or more, has its columns.
        let two = vec!["a".to_string(), "b".to_string()];
        let mut item = got[0].clone();
        item.item.said = vec!["V.in".to_string(), "N.s".to_string()];
        let log = adjudicated_tsv(&two, &[item], &[], None);
        assert!(
            log.starts_with("item\tsent_id\ttoken\tform\ta\tb\tfinal\treason\n"),
            "{log}"
        );
        assert_eq!(read_log("log", &log).unwrap().len(), 1);
        assert!(read_log("log", "item\tsent_id\ttoken\tform\ta\tfinal\treason\n").is_err());
    }

    #[test]
    fn the_machine_worklist_has_a_column_for_each_voter() {
        let sample = sample();
        let item = Item {
            sent: 0,
            tok: 0,
            said: vec![Some(code("V.fi")), Some(code("V.pp"))],
            tags_differ: false,
            unvoted: false,
        };
        let tsv = worklist_tsv(&sample, &[item], &["x", "y"]);
        assert_eq!(
            tsv,
            "item\tsent_id\ttoken\tform\tx\ty\tdiffers\ns1.1\ts1\t1\tRun\tV.fi\tV.pp\tfeature\n"
        );
        let back = read_worklist("w", &tsv).unwrap();
        assert_eq!(back.names, ["x", "y"]);
        assert_eq!(back.items[0].said, ["V.fi", "V.pp"]);
        assert!(read_worklist("w", "item\tsent_id\ttoken\tform\tx\tdiffers\n").is_err());
    }
}
