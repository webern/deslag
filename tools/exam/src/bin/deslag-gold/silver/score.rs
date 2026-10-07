//! The owner's audit of silver, scored: how often silver's labels agree with the person who
//! reviewed them blind.
//!
//! `audit --blind` draws sentences of `silver.conllu` into a queue with no labels, which the
//! owner reviews with deslag's readings pre-filled at Likely and above, as for `owner.conllu`.
//! [`score`] compares the reviewed queue with silver's own labels of the same sentences
//! (`labels.conllu`) on the part of speech and on the whole code, with the exam's sentence
//! bootstrap ([`deslag_exam::stats`]), so no interval is computed anywhere else. It splits the
//! words by how silver labelled them (agreed or adjudicated), by context, and by whether the
//! owner left deslag's pre-fill as it was (`Was=prefill`), which anchors him and flatters
//! silver where he agrees with it. Sentences the owner rejected (personal data, not English) are
//! counted and not scored.
//!
//! The result is `score.tsv`. A batch holds it in `audit/`, and `silver check` scores the
//! batch's stored queue and labels again and compares the text byte for byte. The bar is a drift
//! alarm and not proof of quality: the score is met when the part-of-speech point estimate is at
//! or above it, and the interval is printed beside the verdict.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use deslag_exam::conllu::{self, Block};
use deslag_exam::error::{Error, Place};
use deslag_exam::stats::{Bootstrap, Estimate, ratio};

use crate::code::Code;
use crate::pilot::SILVER_KEY;
use crate::problems::Problems;

/// The columns of `score.tsv` after the group's name.
pub const COLUMNS: [&str; 8] = [
    "group",
    "words",
    "pos",
    "pos_low",
    "pos_high",
    "code",
    "code_low",
    "code_high",
];

/// The tally of a sentence for one group: words, part of speech right, whole code right.
const WORDS: usize = 0;
const POS: usize = 1;
const CODE: usize = 2;

/// What the score says of a batch's audit, as the files hold it.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// Sentences the owner reviewed and scored.
    pub sentences: usize,
    /// Sentences the owner rejected, which are not scored.
    pub rejected: usize,
    /// The bar on the part of speech, in percent, if one was given.
    pub bar: Option<f64>,
    /// The groups, the first being all words.
    pub groups: Vec<Group>,
    /// Words the owner left at deslag's pre-fill, of all the words scored.
    pub prefilled: usize,
}

/// One group of words and how often silver is right about them.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Its name: `all`, `prov=agree`, `context=prose`, `prefilled=yes`.
    pub name: String,
    /// Its words.
    pub words: usize,
    /// Part of speech right.
    pub pos: Estimate,
    /// The whole code right.
    pub code: Estimate,
}

impl Score {
    /// The part-of-speech estimate over all words.
    pub fn pos(&self) -> &Estimate {
        &self.groups[0].pos
    }

    /// Whether the bar is met: the part of speech at or above it. `None` with no bar, or when no
    /// word was scored.
    pub fn met(&self) -> Option<bool> {
        let bar = self.bar?;
        self.pos().point.map(|point| 100.0 * point >= bar)
    }

    /// `score.tsv`: a header of `# key = value` lines, the columns, and a row per group.
    pub fn tsv(&self) -> String {
        let mut out = String::from("# silver audit score, owner against silver\n");
        let _ = writeln!(out, "# sentences = {}", self.sentences);
        let _ = writeln!(out, "# rejected = {}", self.rejected);
        let _ = writeln!(out, "# prefilled = {}", self.prefilled);
        match self.bar {
            Some(bar) => {
                let _ = writeln!(out, "# bar = {bar:.1}");
                let _ = writeln!(
                    out,
                    "# met = {}",
                    match self.met() {
                        Some(true) => "yes",
                        Some(false) => "no",
                        None => "-",
                    }
                );
            }
            None => {
                out.push_str("# bar = -\n# met = -\n");
            }
        }
        let _ = writeln!(out, "{}", COLUMNS.join("\t"));
        for group in &self.groups {
            let (pos, code) = (cells(&group.pos), cells(&group.code));
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}",
                group.name,
                group.words,
                pos.join("\t"),
                code.join("\t")
            );
        }
        out
    }

    /// What the command prints: the verdict, then the file.
    pub fn report(&self) -> String {
        let mut out = String::new();
        let pos = self.groups[0].pos;
        let interval = match (pos.point, pos.interval) {
            (Some(point), Some([low, high])) => format!(
                "{:.1} [{:.1}, {:.1}]",
                100.0 * point,
                100.0 * low,
                100.0 * high
            ),
            _ => "-".to_string(),
        };
        let _ = writeln!(
            out,
            "{} sentences scored, {} rejected; the part of speech is right in {interval} percent",
            self.sentences, self.rejected
        );
        match (self.bar, self.met()) {
            (Some(bar), Some(met)) => {
                let _ = writeln!(
                    out,
                    "{} the bar of {bar:.1} on the part of speech ({interval})",
                    if met { "met" } else { "not met:" }
                );
            }
            _ => out.push_str("no bar given\n"),
        }
        out.push_str(&self.tsv());
        out
    }
}

/// An estimate as three cells: the point, and the ends of the interval, in percent with two
/// places, or `-`.
fn cells(estimate: &Estimate) -> [String; 3] {
    let percent =
        |value: Option<f64>| value.map_or("-".to_string(), |v| format!("{:.2}", 100.0 * v));
    [
        percent(estimate.point),
        percent(estimate.interval.map(|[low, _]| low)),
        percent(estimate.interval.map(|[_, high]| high)),
    ]
}

/// The value of `key` in a MISC column.
pub fn misc_value<'a>(misc: &'a str, key: &str) -> Option<&'a str> {
    conllu::pairs(misc)
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

/// A word of the owner's queue set beside silver's.
struct Pair {
    prov: String,
    prefilled: bool,
    pos: bool,
    code: bool,
}

/// Whether a block says it is silver, as the queue of `audit --blind` does.
fn is_silver(blocks: &[Block]) -> bool {
    blocks.iter().any(|block| {
        block
            .comment(SILVER_KEY)
            .is_some_and(|comment| comment.value == "yes")
    })
}

/// The id of `block`.
fn id_of(block: &Block) -> String {
    block
        .comment("sent_id")
        .map_or("?".to_string(), |comment| comment.value.clone())
}

/// Scores the reviewed `queue` (from `queue_path`) against silver's `labels` (from `labels_path`).
/// `earlier_rejected` is how many sentences were rejected and taken out of both before, which a
/// batch holds in its drops. `bar` is the bar on the part of speech, in percent.
///
/// A queue that is not marked `exam.silver = yes`, a sentence neither reviewed nor rejected, a
/// reviewed word with no tag, tokens that differ from silver's, or a sentence on one side only,
/// are errors, and nothing is scored.
pub fn score(
    queue_path: &str,
    queue: &str,
    labels_path: &str,
    labels: &str,
    earlier_rejected: usize,
    bar: Option<f64>,
) -> Result<Score, Problems> {
    let queue_blocks = conllu::read(queue_path, queue)?;
    let label_blocks = conllu::read(labels_path, labels)?;
    let mut problems = Vec::new();
    if !is_silver(&queue_blocks) {
        problems.push(Error::load(
            queue_path,
            Place::File,
            format!(
                "it is not marked `{SILVER_KEY} = yes`; only the queue of `audit --blind` is scored"
            ),
        ));
    }
    let by_id: BTreeMap<String, &Block> = label_blocks
        .iter()
        .map(|block| (id_of(block), block))
        .collect();
    let queue_ids: Vec<String> = queue_blocks.iter().map(id_of).collect();
    for id in by_id.keys() {
        if !queue_ids.contains(id) {
            problems.push(Problems::sentence(
                labels_path,
                id,
                "silver's labels hold it and the queue does not",
            ));
        }
    }
    let mut rejected = earlier_rejected;
    // Per sentence: its context, and its words.
    let mut sentences: Vec<(String, Vec<Pair>)> = Vec::new();
    for block in &queue_blocks {
        let id = id_of(block);
        if block.comment("owner_rejected").is_some() {
            rejected += 1;
            continue;
        }
        if block.comment("owner_reviewed").is_none() {
            problems.push(Problems::sentence(
                queue_path,
                &id,
                "it is neither reviewed nor rejected",
            ));
            continue;
        }
        let Some(silver) = by_id.get(&id) else {
            problems.push(Problems::sentence(
                labels_path,
                &id,
                "the queue holds it and silver's labels do not",
            ));
            continue;
        };
        if silver.lines.len() != block.lines.len() {
            problems.push(Problems::sentence(
                queue_path,
                &id,
                format!(
                    "{} lines, and silver's labels have {}",
                    block.lines.len(),
                    silver.lines.len()
                ),
            ));
            continue;
        }
        let mut words = Vec::new();
        for (at, (owner, theirs)) in block.lines.iter().zip(&silver.lines).enumerate() {
            if owner.form != theirs.form {
                problems.push(Problems::sentence(
                    queue_path,
                    &id,
                    format!("token {} is not the token silver labelled", at + 1),
                ));
                break;
            }
            if misc_value(&owner.misc, "Kind") != Some("Word") {
                continue;
            }
            let read = |line: &conllu::Line, path: &str| {
                Code::from_conllu(&line.upos, &line.feats).map_err(|why| {
                    Problems::sentence(path, &id, format!("token {}: {why}", at + 1))
                })
            };
            match (read(owner, queue_path), read(theirs, labels_path)) {
                (Ok(owner_code), Ok(silver_code)) => words.push(Pair {
                    prov: misc_value(&theirs.misc, "Prov").unwrap_or("-").to_string(),
                    prefilled: misc_value(&owner.misc, "Was") == Some("prefill"),
                    pos: owner_code.base == silver_code.base,
                    code: owner_code == silver_code,
                }),
                (a, b) => {
                    problems.extend(a.err());
                    problems.extend(b.err());
                }
            }
        }
        let context = block
            .comment("exam.context")
            .map_or("prose".to_string(), |comment| comment.value.clone());
        sentences.push((context, words));
    }
    Problems::check(problems, ())?;
    if sentences.is_empty() {
        return Err(Error::load(
            queue_path,
            Place::File,
            "no sentence was reviewed, so there is nothing to score",
        )
        .into());
    }

    let tally = |words: &[Pair], keep: &dyn Fn(&Pair) -> bool| -> [u64; 3] {
        let mut counts = [0; 3];
        for word in words.iter().filter(|word| keep(word)) {
            counts[WORDS] += 1;
            counts[POS] += u64::from(word.pos);
            counts[CODE] += u64::from(word.code);
        }
        counts
    };
    let contexts: Vec<String> = {
        let mut names: Vec<String> = sentences
            .iter()
            .map(|(context, _)| context.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    };
    type Keep<'a> = Box<dyn Fn(&str, &Pair) -> bool + 'a>;
    let mut groups: Vec<(String, Keep<'_>)> = vec![
        ("all".to_string(), Box::new(|_, _| true)),
        ("prov=agree".to_string(), Box::new(|_, w| w.prov == "agree")),
        (
            "prov=adjudicated".to_string(),
            Box::new(|_, w| w.prov == "adjudicated"),
        ),
    ];
    for context in &contexts {
        let name = context.clone();
        groups.push((
            format!("context={context}"),
            Box::new(move |c, _| c == name),
        ));
    }
    groups.push(("prefilled=yes".to_string(), Box::new(|_, w| w.prefilled)));
    groups.push(("prefilled=no".to_string(), Box::new(|_, w| !w.prefilled)));

    let mut out = Vec::new();
    let mut total_words = 0;
    let mut prefilled = 0;
    for (name, keep) in &groups {
        let units: Vec<[u64; 3]> = sentences
            .iter()
            .map(|(context, words)| tally(words, &|word| keep(context, word)))
            .filter(|unit| unit[WORDS] > 0)
            .collect();
        let slices: Vec<&[u64]> = units.iter().map(|unit| unit.as_slice()).collect();
        let boot = Bootstrap::new(&format!("score {name}"), &slices, 3);
        let words = boot.total[WORDS] as usize;
        if name == "all" {
            total_words = words;
        }
        if name == "prefilled=yes" {
            prefilled = words;
        }
        // A group with no word is left out, but `all` is always the first row.
        if words == 0 && name != "all" {
            continue;
        }
        out.push(Group {
            name: name.clone(),
            words,
            pos: boot.estimate(&|sum| ratio(sum, POS, WORDS)),
            code: boot.estimate(&|sum| ratio(sum, CODE, WORDS)),
        });
    }
    let _ = total_words;
    Ok(Score {
        sentences: sentences.len(),
        rejected,
        bar,
        groups: out,
        prefilled,
    })
}

#[cfg(test)]
/// The `# key = value` header lines of a `score.tsv`: the value of `key`.
pub fn header_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("# "))
        .find_map(|line| {
            let (name, value) = line.split_once(" = ")?;
            (name == key).then_some(value)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A queue sentence `id` of three tokens, a noun, a verb and a full stop, with the owner's
    /// codes and the `Was=` and `Prov=` marks he leaves, reviewed unless `review` is empty.
    fn sentence(
        id: &str,
        owner: [&str; 2],
        silver: [&str; 2],
        review: &str,
        prefill: bool,
    ) -> (String, String) {
        let word = |at: usize, form: &str, code: &str, misc: &str| -> String {
            let code = Code::parse(code).unwrap();
            format!(
                "{at}\t{form}\t_\t{}\t_\t{}\t_\t_\t_\t{misc}\n",
                code.upos(form),
                code.feats()
            )
        };
        let head = format!(
            "# sent_id = {id}\n# exam.context = prose\n# pick_id = {id}\n# text = Cats run.\n"
        );
        let mark = if review.is_empty() {
            String::new()
        } else {
            format!("# {review}\n")
        };
        let owner_misc = |extra: &str| format!("Kind=Word|Prov=owner{extra}");
        let was = if prefill { "|Was=prefill" } else { "" };
        let queue = format!(
            "{head}{mark}{}{}3\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind\n\n",
            word(1, "Cats", owner[0], &owner_misc(was)),
            word(2, "run", owner[1], &owner_misc("")),
        );
        let labels = format!(
            "{head}{}{}3\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind\n\n",
            word(1, "Cats", silver[0], "Kind=Word|Prov=agree|Runs=r1"),
            word(2, "run", silver[1], "Kind=Word|Prov=adjudicated|Runs=r2"),
        );
        (queue, labels)
    }

    fn files(sentences: &[(String, String)]) -> (String, String) {
        let mut queue = String::from("# exam.tokens = deslag\n# exam.silver = yes\n");
        let mut labels = queue.clone();
        for (q, l) in sentences {
            queue.push_str(q);
            labels.push_str(l);
        }
        (queue, labels)
    }

    fn scored(
        sentences: &[(String, String)],
        earlier: usize,
        bar: Option<f64>,
    ) -> Result<Score, Problems> {
        let (queue, labels) = files(sentences);
        score("q.conllu", &queue, "l.conllu", &labels, earlier, bar)
    }

    const REVIEWED: &str = "owner_reviewed = 2026-10-20";

    #[test]
    fn silver_is_scored_against_the_owner_on_the_part_of_speech_and_the_whole_code() {
        let sentences = [
            // Right in both words.
            sentence("a1", ["N.p", "V.fi"], ["N.p", "V.fi"], REVIEWED, false),
            // The noun's number differs: the part of speech is right and the code is not.
            sentence("a2", ["N.s", "V.fi"], ["N.p", "V.fi"], REVIEWED, true),
            // The verb is a noun to silver: wrong in both.
            sentence("a3", ["N.p", "V.fi"], ["N.p", "N.s"], REVIEWED, false),
            // Rejected: counted and not scored.
            sentence(
                "a4",
                ["N.p", "V.fi"],
                ["N.p", "V.fi"],
                "owner_rejected = 2026-10-20",
                false,
            ),
        ];
        let score = scored(&sentences, 1, Some(80.0)).unwrap();
        assert_eq!((score.sentences, score.rejected), (3, 2));
        let all = &score.groups[0];
        assert_eq!((all.name.as_str(), all.words), ("all", 6));
        assert_eq!(all.pos.point, Some(5.0 / 6.0));
        assert_eq!(all.code.point, Some(4.0 / 6.0));
        // Agreed words are the nouns, adjudicated the verbs.
        let by_name: BTreeMap<&str, &Group> = score
            .groups
            .iter()
            .map(|group| (group.name.as_str(), group))
            .collect();
        assert_eq!(by_name["prov=agree"].words, 3);
        assert_eq!(by_name["prov=agree"].pos.point, Some(1.0));
        assert_eq!(by_name["prov=agree"].code.point, Some(2.0 / 3.0));
        assert_eq!(by_name["prov=adjudicated"].pos.point, Some(2.0 / 3.0));
        assert_eq!(by_name["context=prose"].words, 6);
        // One word was left at deslag's pre-fill.
        assert_eq!(score.prefilled, 1);
        assert_eq!(by_name["prefilled=yes"].words, 1);
        assert_eq!(by_name["prefilled=no"].words, 5);
        // The bar is on the part of speech: 83.3 against 80 is met.
        assert_eq!(score.met(), Some(true));
        let again = scored(&sentences, 1, Some(90.0)).unwrap();
        assert_eq!(again.met(), Some(false));
        let report = score.report();
        assert!(
            report.contains("met the bar of 80.0 on the part of speech (83.3"),
            "{report}"
        );
        assert!(
            again.report().contains("not met: the bar of 90.0"),
            "{}",
            again.report()
        );
        let tsv = score.tsv();
        assert!(tsv.contains("# met = yes\n") && tsv.contains("# rejected = 2\n"));
        assert_eq!(header_value(&tsv, "bar"), Some("80.0"));
        assert_eq!(header_value(&tsv, "met"), Some("yes"));
        assert!(tsv.contains("all\t6\t83.33\t"), "{tsv}");
        // The same inputs give the same bytes.
        assert_eq!(tsv, scored(&sentences, 1, Some(80.0)).unwrap().tsv());
        // No bar given: no verdict.
        let open = scored(&sentences, 0, None).unwrap();
        assert_eq!(open.met(), None);
        assert!(open.tsv().contains("# bar = -\n# met = -\n"));
    }

    #[test]
    fn a_queue_that_is_not_silver_or_not_finished_or_not_the_labels_sentences_is_refused() {
        let ok = sentence("a1", ["N.p", "V.fi"], ["N.p", "V.fi"], REVIEWED, false);
        let (queue, labels) = files(std::slice::from_ref(&ok));
        // Not marked silver.
        let plain = queue.replace("# exam.silver = yes\n", "");
        let error = score("q.conllu", &plain, "l.conllu", &labels, 0, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("exam.silver = yes"), "{error}");
        // Neither reviewed nor rejected.
        let open = sentence("a2", ["N.p", "V.fi"], ["N.p", "V.fi"], "", false);
        let error = scored(&[ok.clone(), open], 0, None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("a2") && error.contains("neither reviewed nor rejected"),
            "{error}"
        );
        // A reviewed word with no tag.
        let blank = (
            ok.0.replace("\tNOUN\t_\tNumber=Plur", "\t_\t_\t_"),
            ok.1.clone(),
        );
        let error = scored(&[blank], 0, None).unwrap_err().to_string();
        assert!(error.contains("a1"), "{error}");
        // A sentence on one side only.
        let other = sentence("a9", ["N.p", "V.fi"], ["N.p", "V.fi"], REVIEWED, false);
        let (queue, _) = files(std::slice::from_ref(&ok));
        let (_, labels) = files(&[ok.clone(), other]);
        let error = score("q.conllu", &queue, "l.conllu", &labels, 0, None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("a9") && error.contains("the queue does not"),
            "{error}"
        );
        // Other tokens than silver labelled.
        let moved = (ok.0.replace("run", "ran"), ok.1);
        let error = scored(&[moved], 0, None).unwrap_err().to_string();
        assert!(error.contains("not the token silver labelled"), "{error}");
        // Everything rejected: nothing to score.
        let gone = sentence(
            "a1",
            ["N.p", "V.fi"],
            ["N.p", "V.fi"],
            "owner_rejected = 2026-10-20",
            false,
        );
        let error = scored(&[gone], 0, None).unwrap_err().to_string();
        assert!(error.contains("nothing to score"), "{error}");
    }
}
