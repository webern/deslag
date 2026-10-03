//! The three-way merger: the blind tagger, Harper and spaCy over the same words.
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
//! - Every other word goes to the adjudication worklist, with the sentence, the three answers and
//!   a place for one code and a reason of at most 15 words.
//!
//! `agreed.conllu` is every sentence with the agreed words filled in and the disputed words left
//! without a UPOS, so that it cannot be mistaken for a gold file until the adjudicated answers
//! (read by [`read_answers`]) are put into it by `assemble`. Words that are not words, code spans
//! and marks, are filled from their kind and never in dispute.

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

/// Reads the CoNLL-U answers of the tagger `name` from `text`, which came from `path`, against
/// `sample`. Every sentence of the sample must be there with the same forms, line for line, and
/// every word line must have a UPOS of UD's.
pub fn load_tagger(
    name: &str,
    path: &str,
    text: &str,
    sample: &Sample,
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
            problems.push(Problems::sentence(
                path,
                &sent.id,
                format!("{name} has no answer for it"),
            ));
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

/// What the three said of a word: `blind`, `harper` and `spacy`.
pub fn judge(blind: Code, harper: Code, spacy: Code) -> Verdict {
    if blind.base != harper.base || blind.base != spacy.base {
        return Verdict::Disputed { tags_differ: true };
    }
    let clash = [harper, spacy]
        .into_iter()
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
    /// What each of the three said, in the order of [`NAMES`].
    pub said: [Code; 3],
    /// Whether the bases differ, or only a feature.
    pub tags_differ: bool,
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
    /// How many words all three agree on the base of.
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
            stats.pairs[0] += usize::from(blind.base == harper.base);
            stats.pairs[1] += usize::from(blind.base == spacy.base);
            stats.pairs[2] += usize::from(harper.base == spacy.base);
            let verdict = judge(blind, harper, spacy);
            let all_tags = blind.base == harper.base && blind.base == spacy.base;
            stats.tag3 += usize::from(all_tags);
            let full = matches!(verdict, Verdict::Agreed(_));
            stats.full3 += usize::from(full);
            if let Some(meta) = meta {
                let tier = stats.by_tier.entry(meta.tier.name()).or_default();
                tier.0 += 1;
                tier.1 += usize::from(full);
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
                    said: [blind, harper, spacy],
                    tags_differ,
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
        row(f, "blind and harper", self.pairs[0])?;
        row(f, "blind and spacy", self.pairs[1])?;
        row(f, "harper and spacy", self.pairs[2])?;
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
/// words that are not words with theirs from their kind, and each disputed word with `_` for UPOS
/// and no `Prov=`.
pub fn agreed_conllu(sample: &Sample, merged: &Merged) -> String {
    let mut out = String::new();
    for (sent, row) in sample.sents.iter().zip(&merged.verdicts) {
        let _ = writeln!(out, "# sent_id = {}\n# text = {}", sent.id, sent.text());
        for (index, (tok, verdict)) in sent.toks.iter().zip(row).enumerate() {
            let text = match verdict {
                None => line(
                    index,
                    &tok.form,
                    upos_of_kind(tok.kind),
                    "_",
                    &misc(tok, Some("agree")),
                ),
                Some(Verdict::Agreed(code)) => line(
                    index,
                    &tok.form,
                    code.upos(&tok.form),
                    &code.feats(),
                    &misc(tok, Some("agree")),
                ),
                Some(Verdict::Disputed { .. }) => {
                    line(index, &tok.form, "_", "_", &misc(tok, None))
                }
            };
            out.push_str(&text);
        }
        out.push('\n');
    }
    out
}

/// What a tagger said, as the worklist shows it: the code, with `.?` where the tagger gave no
/// feature that the guide asks of the word, so that leaving one out cannot be read as a code.
fn said(code: Code) -> String {
    let missing = (matches!(code.base, Base::N | Base::Pn) && code.number.is_none())
        || (code.base.takes_form() && code.form.is_none());
    if missing {
        format!("{code}.?")
    } else {
        code.to_string()
    }
}

const WORKLIST_HEADER: &str = "\
Adjudicate the words below. Each sentence is shown with its tokens numbered; under it, each
word the three taggers disagree on, with what each said (blind, harper, spacy). A code that ends
in .? means that tagger gave the part of speech and no feature.

Decide each word from the annotation guide, for its use in this sentence. Answer with one line per
item, at the end, in the slots given: the item, a colon, one code of the guide with its feature,
a bar, and a reason of 15 words or fewer. Answer every item, change nothing else, and write
nothing else.
";

/// The worklist, in parts of about `per_part` items, never splitting a sentence. Each part is
/// text for the adjudicator: the numbered sentences, their words in dispute, and a slot to fill
/// for each item. The parts hold no tier, split or file.
pub fn worklist_parts(sample: &Sample, items: &[Item], per_part: usize) -> Vec<String> {
    let mut parts: Vec<Vec<&Item>> = Vec::new();
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
        parts.push(items[at..end].iter().collect());
        at = end;
    }
    parts
        .iter()
        .map(|part| {
            let mut out = String::from(WORKLIST_HEADER);
            let mut slots = String::new();
            let mut current = usize::MAX;
            for item in part {
                let sent = &sample.sents[item.sent];
                if item.sent != current {
                    current = item.sent;
                    let context = sample.meta(&sent.id).map_or(Context::Prose, |m| m.context);
                    let _ = write!(out, "\n{}\n", batch::render(sent, context));
                }
                let _ = writeln!(
                    out,
                    "  {} {}: blind {}, harper {}, spacy {}",
                    item.tok + 1,
                    sent.toks[item.tok].form,
                    said(item.said[0]),
                    said(item.said[1]),
                    said(item.said[2])
                );
                let _ = writeln!(slots, "{}: ", item.id(sample));
            }
            let _ = write!(out, "\nSlots:\n{slots}");
            out
        })
        .collect()
}

/// The columns of `worklist.tsv`.
const WORK_COLUMNS: [&str; 8] = [
    "item", "sent_id", "token", "form", "blind", "harper", "spacy", "differs",
];

/// `worklist.tsv`, the machine form of the worklist, which [`read_answers`] checks answers against.
pub fn worklist_tsv(sample: &Sample, items: &[Item]) -> String {
    let mut out = format!("{}\n", WORK_COLUMNS.join("\t"));
    for item in items {
        let sent = &sample.sents[item.sent];
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            item.id(sample),
            sent.id,
            item.tok + 1,
            sent.toks[item.tok].form,
            said(item.said[0]),
            said(item.said[1]),
            said(item.said[2]),
            if item.tags_differ { "tag" } else { "feature" }
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
    /// What the three said, as the worklist printed it.
    pub said: [String; 3],
}

/// Reads `worklist.tsv`, which came from `path`.
pub fn read_worklist(path: &str, text: &str) -> Result<Vec<WorkItem>, Error> {
    let mut items = Vec::new();
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, head)) if head.split('\t').eq(WORK_COLUMNS) => {}
        _ => {
            return Err(Error::at(
                path,
                1,
                format!("the columns should be {}", WORK_COLUMNS.join(", ")),
            ));
        }
    }
    for (at, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != WORK_COLUMNS.len() {
            return Err(Error::at(
                path,
                at + 1,
                format!(
                    "expected {} columns, found {}",
                    WORK_COLUMNS.len(),
                    cells.len()
                ),
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
            said: [cells[4], cells[5], cells[6]].map(str::to_string),
        });
    }
    Ok(items)
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

/// Reads the adjudicator's answers: each of `files` is a path and its text. Every line must be
/// `item: code | reason` for an item of `work`, once, with a code of the guide and a reason of
/// at most 15 words. With `all`, every item must be answered. Blank lines, code fences and the
/// lines of a `Slots:` heading are skipped. Returns the answers in the order of `work`.
pub fn read_answers(
    work: &[WorkItem],
    files: &[(String, String)],
    all: bool,
) -> Result<Vec<Answer>, Problems> {
    let wanted: BTreeMap<&str, &WorkItem> =
        work.iter().map(|item| (item.item.as_str(), item)).collect();
    let mut found: BTreeMap<&str, Answer> = BTreeMap::new();
    let mut problems = Vec::new();
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
            match parse_answer(line, &wanted) {
                Ok((item, code, reason)) => {
                    if found.contains_key(item.item.as_str()) {
                        problems.push(Problems::sentence(
                            path,
                            &item.sent_id,
                            format!("line {}: item {} is answered twice", at + 1, item.item),
                        ));
                    } else {
                        found.insert(
                            item.item.as_str(),
                            Answer {
                                item: item.clone(),
                                code,
                                reason,
                            },
                        );
                    }
                }
                Err(why) => {
                    let sent = line
                        .split_once(':')
                        .and_then(|(item, _)| wanted.get(item.trim()))
                        .map(|item| item.sent_id.as_str());
                    problems.push(match sent {
                        Some(sent) => {
                            Problems::sentence(path, sent, format!("line {}: {why}", at + 1))
                        }
                        None => Error::at(path, at + 1, why),
                    });
                }
            }
        }
    }
    if all {
        let missing: Vec<&str> = work
            .iter()
            .map(|item| item.item.as_str())
            .filter(|item| !found.contains_key(item))
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
    let answers = work
        .iter()
        .filter_map(|item| found.remove(item.item.as_str()))
        .collect();
    Problems::check(problems, answers)
}

/// The columns of `adjudicated.tsv`, the log of the adjudication.
const LOG_COLUMNS: [&str; 9] = [
    "item", "sent_id", "token", "form", "blind", "harper", "spacy", "final", "reason",
];

/// `adjudicated.tsv`: each answered item with what the three said, the code decided and why.
pub fn adjudicated_tsv(answers: &[Answer]) -> String {
    let mut out = format!("{}\n", LOG_COLUMNS.join("\t"));
    for answer in answers {
        let item = &answer.item;
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            item.item,
            item.sent_id,
            item.token,
            item.form,
            item.said[0],
            item.said[1],
            item.said[2],
            answer.code,
            answer.reason
        );
    }
    out
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
}

/// Reads `adjudicated.tsv`, which came from `path`.
pub fn read_log(path: &str, text: &str) -> Result<Vec<Logged>, Error> {
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, head)) if head.split('\t').eq(LOG_COLUMNS) => {}
        _ => {
            return Err(Error::at(
                path,
                1,
                format!("the columns should be {}", LOG_COLUMNS.join(", ")),
            ));
        }
    }
    let mut rows = Vec::new();
    for (at, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != LOG_COLUMNS.len() {
            return Err(Error::at(
                path,
                at + 1,
                format!(
                    "expected {} columns, found {}",
                    LOG_COLUMNS.len(),
                    cells.len()
                ),
            ));
        }
        let bad = |what: &str, value: &str| Error::at(path, at + 1, format!("{what} `{value}`"));
        rows.push(Logged {
            sent_id: cells[1].to_string(),
            token: cells[2]
                .parse()
                .map_err(|_| bad("bad token number", cells[2]))?,
            form: cells[3].to_string(),
            code: Code::parse(cells[7]).map_err(|why| bad(&why, cells[7]))?,
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
                out.push_str(&line(index, &tok.form, upos, feats, &misc(tok, None)));
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
        let out = agreed_conllu(&sample, &merged);
        assert!(
            !out.contains("\t_\t_\t_\t_\t_\t_\tKind=Word\n"),
            "no pending word"
        );
        assert!(out.contains("4\tuser's\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree"));
        assert!(out.contains("2\tmake ci\t_\tX\t_\t_\t_\t_\t_\tKind=Code|Prov=agree"));
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

        let out = agreed_conllu(&sample, &merged);
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
        let parts = worklist_parts(&sample, &merged.items, 60);
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
        assert_eq!(said(bare("NOUN")), "N.?");
        assert_eq!(said(bare("PROPN")), "PN.?");
        assert_eq!(said(bare("VERB")), "V.?");
        assert_eq!(said(bare("AUX")), "AX.?");
        assert_eq!(said(bare("PRON")), "PR", "a pronoun may have no number");
        assert_eq!(said(bare("ADJ")), "J");
        assert_eq!(said(code("N.s")), "N.s");
        assert_eq!(said(code("V.pp")), "V.pp");
    }

    #[test]
    fn the_worklist_splits_into_parts_by_whole_sentences() {
        let sample = sample();
        let item = |sent, tok| Item {
            sent,
            tok,
            said: [code("N.s"), code("N.s"), code("N.s")],
            tags_differ: true,
        };
        let items = [item(0, 0), item(0, 2), item(1, 0), item(1, 2), item(1, 3)];
        assert_eq!(worklist_parts(&sample, &items, 100).len(), 1);
        let parts = worklist_parts(&sample, &items, 2);
        assert_eq!(parts.len(), 2, "a sentence is never split");
        assert!(
            parts[0].contains("s1.1") && parts[0].contains("s1.3") && !parts[0].contains("s2.1")
        );
        assert!(parts[1].contains("s2.1") && parts[1].contains("s2.4"));
        assert!(worklist_parts(&sample, &[], 10).is_empty());
        let one = worklist_parts(&sample, &items, 1);
        assert_eq!(
            one.len(),
            2,
            "one item per part still keeps a sentence whole"
        );
    }

    fn work() -> Vec<WorkItem> {
        let mut harper = SAME;
        harper[6] = ("ADJ", "_");
        let mut spacy = SAME;
        spacy[2] = ("NOUN", "Number=Sing");
        let (sample, merged) = merged(&blind(), &harper, &spacy);
        read_worklist("w.tsv", &worklist_tsv(&sample, &merged.items)).unwrap()
    }

    #[test]
    fn the_machine_worklist_reads_back() {
        let work = work();
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
        read_answers(&work(), &[("a.txt".to_string(), text.to_string())], all)
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
        let log = adjudicated_tsv(&got);
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
}
