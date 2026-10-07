//! The merger for any number of voters, two or more, which `deslag-gold merge --voter` runs: the
//! labelling flow's counterpart of [`crate::merge::merge`], which is hard-wired to the gold set's
//! blind tagger, Harper and spaCy.
//!
//! Each voter gives every word token a UPOS and, if it can, FEATS, as a CoNLL-U file over the same
//! tokens (what `read-tags` writes for a model, and what spaCy's runner writes). They are compared
//! as the guide's codes (`N.p`, `V.pp`). The voters agree on a word when:
//!
//! - they all name the same base (`N`, `V`, ...), and
//! - no feature conflicts. A voter that gives no feature abstains on it, so `N.s`, `N.s` and a bare
//!   `N` agree on `N.s`. A voter marked base-only, which is how spaCy votes, always abstains: its
//!   feature conventions differ from the guide's, so only its part of speech counts. The one
//!   exception to abstaining is a pronoun: the guide gives *you*, *who* and *which* no number, so
//!   a voter that follows the guide and writes a bare `PR` has said something, and `PR` against
//!   `PR.s` is a dispute and not an agreement on `PR.s`.
//! - a word the guide asks a feature of (a number on `N` and `PN`, a verb form on `V` and `AX`)
//!   has one: if every voter left it out, the word is disputed, since no one said it.
//!
//! Everything else goes to the adjudication worklist as phase 1's merger does, with the voters
//! shown as `A`, `B`, `C` in the order they were given, never by name. The agreed words stand,
//! `Prov=agree`, with `Runs=` naming the run of every voter that answered the sentence, since
//! every one of them agreed.
//!
//! A voter that has no good line for a sentence after its retries abstains on all of it, and the
//! merge goes on with the voters left, so one sentence a model cannot write never stops the
//! pilot. Agreement needs [`MIN_AGREEING_VOTERS`] model voters (three, unless `--min-voters` says
//! otherwise): a word stands as agreed only when at least that many model voters answered its
//! sentence and all agree. spaCy, which votes on the base alone, is a voter that never counts
//! toward the number. A sentence fewer model voters answered goes whole to the adjudicator, with
//! `-` for the voters that abstained. The report counts the abstentions of each voter and the
//! sentences that went whole, and says the minimum it used.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use crate::code::{Base, Code, Form, Number};
use crate::data::Sample;
use deslag_exam::error::Error;

use crate::merge::{Answers, FEWEST_TAGGERS, Item, Verdict};

/// How many model voters must answer a sentence for any of its words to count as agreed: the
/// default of `merge --min-voters`. Three voters is what the agreed-word accuracy of the pilot was
/// measured with; a word two voters agree on goes to the adjudicator. A voter that votes on the
/// base alone, as spaCy does, is not one of them.
pub const MIN_AGREEING_VOTERS: usize = 3;

/// One voter of a merge.
#[derive(Debug, Clone)]
pub struct Voter {
    /// What it is called in the files: letters, digits, `-` and `_`.
    pub name: String,
    /// What it said of every word token.
    pub answers: Answers,
    /// Whether it votes on the base alone.
    pub base_only: bool,
    /// The id of the run that made its answers, which `runs.tsv` describes: the one its file names
    /// in `Runs=`.
    pub run: Option<String>,
    /// The file its answers were read from, as the merge was given it.
    pub file: String,
}

/// What the voters said of a word, `said[n]` being the code of voter `n` and `base_only[n]` whether
/// it votes on the base alone.
pub fn judge_voters(said: &[Code], base_only: &[bool]) -> Verdict {
    let base = said[0].base;
    if said.iter().any(|code| code.base != base) {
        return Verdict::Disputed { tags_differ: true };
    }
    let giving: Vec<Code> = said
        .iter()
        .zip(base_only)
        .filter(|(_, only)| !**only)
        .map(|(code, _)| *code)
        .collect();
    // A voter that gives no number abstains on it, except on a pronoun: the guide gives *you* and
    // *who* none, so a bare `PR` is an answer.
    let numbers: Vec<Option<Number>> = giving
        .iter()
        .map(|code| code.number)
        .filter(|number| base == Base::Pr || number.is_some())
        .collect();
    let forms: Vec<Form> = giving.iter().filter_map(|code| code.form).collect();
    let clash = numbers.windows(2).any(|pair| pair[0] != pair[1])
        || forms.windows(2).any(|pair| pair[0] != pair[1]);
    let number = numbers.first().copied().flatten();
    let form = forms.first().copied();
    let missing = (matches!(base, Base::N | Base::Pn) && number.is_none())
        || (base.takes_form() && form.is_none());
    if clash || missing {
        Verdict::Disputed { tags_differ: false }
    } else {
        Verdict::Agreed(Code { base, number, form })
    }
}

/// Counts of agreement, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteStats {
    /// How many model voters had to answer a sentence for its words to count as agreed.
    pub min_voters: usize,
    /// The voters' names, in order.
    pub names: Vec<String>,
    /// Word tokens compared.
    pub words: usize,
    /// For each pair of voters in the order (0,1), (0,2), ..., (1,2), ...: the pair, how many
    /// words they agree on the base of, and how many words both answered.
    pub pairs: Vec<((usize, usize), usize, usize)>,
    /// Sentences each voter abstained on, in the order of `names`.
    pub abstained: Vec<usize>,
    /// Sentences fewer than `min_voters` model voters answered, whose words all went to the
    /// adjudicator.
    pub unvoted_sentences: usize,
    /// The words of those sentences.
    pub unvoted_words: usize,
    /// How many words every voter that answered agrees on the base of.
    pub tag_all: usize,
    /// How many words every voter agrees on completely: these stand.
    pub agreed: usize,
    /// Of the words in dispute, how many differ in a base. Words of an unvoted sentence are in
    /// neither this nor the next.
    pub tag_disputes: usize,
    /// Of the words in dispute, how many differ only in a feature.
    pub feature_disputes: usize,
    /// Words and complete agreements by tier, where the sample says the tier.
    pub by_tier: BTreeMap<&'static str, (usize, usize)>,
    /// Words and complete agreements by context.
    pub by_context: BTreeMap<&'static str, (usize, usize)>,
}

/// The merge of any number of voters.
#[derive(Debug, Clone)]
pub struct Voted {
    /// The verdict on each token, `None` for one that is not a word.
    pub verdicts: Vec<Vec<Option<Verdict>>>,
    /// The words to adjudicate, in the order of the sample.
    pub items: Vec<Item>,
    /// The counts.
    pub stats: VoteStats,
}

/// The letter the adjudicator knows voter `index` by.
pub fn letter(index: usize) -> String {
    let at = u8::try_from(index % 26).expect("a letter of 26");
    let repeat = index / 26 + 1;
    char::from(b'A' + at).to_string().repeat(repeat)
}

/// Compares the voters' answers on every word of `sample`. There must be at least
/// [`FEWEST_TAGGERS`] voters, whose answers cover the sample as [`crate::merge::load_voter`] reads
/// them: a sentence a voter has no answer for is an abstention. A word is agreed only when at
/// least `min_voters` voters that are not base-only answered its sentence and all agree; for a
/// sentence fewer answered, every word goes to the adjudicator.
pub fn merge_voters(sample: &Sample, voters: &[Voter], min_voters: usize) -> Voted {
    assert!(
        voters.len() >= FEWEST_TAGGERS,
        "a merge needs {FEWEST_TAGGERS} voters or more"
    );
    let mut stats = VoteStats {
        min_voters,
        names: voters.iter().map(|voter| voter.name.clone()).collect(),
        words: 0,
        pairs: (0..voters.len())
            .flat_map(|a| (a + 1..voters.len()).map(move |b| ((a, b), 0, 0)))
            .collect(),
        abstained: vec![0; voters.len()],
        unvoted_sentences: 0,
        unvoted_words: 0,
        tag_all: 0,
        agreed: 0,
        tag_disputes: 0,
        feature_disputes: 0,
        by_tier: BTreeMap::new(),
        by_context: BTreeMap::new(),
    };
    let mut verdicts = Vec::with_capacity(sample.sents.len());
    let mut items = Vec::new();
    for (at, sent) in sample.sents.iter().enumerate() {
        let meta = sample.meta(&sent.id);
        let answering: Vec<usize> = (0..voters.len())
            .filter(|&voter| !voters[voter].answers.abstains(at))
            .collect();
        for (count, voter) in stats.abstained.iter_mut().zip(voters) {
            *count += usize::from(voter.answers.abstains(at));
        }
        let model_answering = answering
            .iter()
            .filter(|&&voter| !voters[voter].base_only)
            .count();
        let voted = answering.len() >= FEWEST_TAGGERS && model_answering >= min_voters;
        stats.unvoted_sentences += usize::from(!voted);
        let base_only: Vec<bool> = answering
            .iter()
            .map(|&voter| voters[voter].base_only)
            .collect();
        let mut row = Vec::with_capacity(sent.toks.len());
        for tok in 0..sent.toks.len() {
            if !sent.toks[tok].is_word() {
                row.push(None);
                continue;
            }
            // What every voter said, `None` for one that abstained on the sentence.
            let said: Vec<Option<Code>> = voters
                .iter()
                .map(|voter| voter.answers.0[at].get(tok).copied().flatten())
                .collect();
            stats.words += 1;
            if !voted {
                stats.unvoted_words += 1;
                items.push(Item {
                    sent: at,
                    tok,
                    said,
                    tags_differ: false,
                    unvoted: true,
                });
                row.push(Some(Verdict::Disputed { tags_differ: false }));
                continue;
            }
            let heard: Vec<Code> = answering.iter().filter_map(|&voter| said[voter]).collect();
            for ((a, b), agree, both) in &mut stats.pairs {
                if let (Some(a), Some(b)) = (said[*a], said[*b]) {
                    *both += 1;
                    *agree += usize::from(a.base == b.base);
                }
            }
            let verdict = judge_voters(&heard, &base_only);
            stats.tag_all += usize::from(heard.iter().all(|code| code.base == heard[0].base));
            let full = matches!(verdict, Verdict::Agreed(_));
            stats.agreed += usize::from(full);
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
                    said,
                    tags_differ,
                    unvoted: false,
                });
            }
            row.push(Some(verdict));
        }
        verdicts.push(row);
    }
    Voted {
        verdicts,
        items,
        stats,
    }
}

/// The `Runs=` value of the agreed words of each sentence: the run of every voter that answered
/// it, in order, as `r3,r4,r5`. A sentence's entry is `None` when a voter that answered it has no
/// run, since a list with a gap would name fewer voters than agreed; `finish` then refuses the
/// words, so provenance cannot go missing without an error.
pub fn runs_by_sentence(voters: &[Voter], sentences: usize) -> Vec<Option<String>> {
    (0..sentences)
        .map(|at| {
            let runs: Option<Vec<&str>> = voters
                .iter()
                .filter(|voter| !voter.answers.abstains(at))
                .map(|voter| voter.run.as_deref())
                .collect();
            runs.map(|runs| runs.join(","))
        })
        .collect()
}

fn percent(part: usize, whole: usize) -> String {
    if whole == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", 100.0 * part as f64 / whole as f64)
    }
}

impl fmt::Display for VoteStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let row = |f: &mut fmt::Formatter<'_>, label: &str, count: usize| {
            writeln!(
                f,
                "  {label:<44}{count:>6}  {:>6}",
                percent(count, self.words)
            )
        };
        writeln!(
            f,
            "Agreement of {} voters over {} word tokens",
            self.names.len(),
            self.words
        )?;
        writeln!(
            f,
            "a word is agreed only if at least {} model voters answered and all agree (spaCy, which votes the base tag alone, does not count toward them)",
            self.min_voters
        )?;
        writeln!(f, "voters, as the adjudicator sees them")?;
        for (index, name) in self.names.iter().enumerate() {
            writeln!(f, "  {:<4}{name}", letter(index))?;
        }
        writeln!(f, "sentences each voter gave no answer for (abstained)")?;
        for (name, count) in self.names.iter().zip(&self.abstained) {
            writeln!(f, "  {name:<44}{count:>6}")?;
        }
        writeln!(
            f,
            "  {:<44}{:>6}  ({} words, all to the adjudicator)",
            format!(
                "sentences fewer than {} model voters answered",
                self.min_voters
            ),
            self.unvoted_sentences,
            self.unvoted_words
        )?;
        writeln!(f, "pairs, on the part of speech")?;
        for ((a, b), count, both) in &self.pairs {
            let label = format!("{} and {}", self.names[*a], self.names[*b]);
            writeln!(
                f,
                "  {label:<44}{count:>6}  {:>6}  of {both}",
                percent(*count, *both)
            )?;
        }
        writeln!(f, "all that answered")?;
        row(f, "agree on the part of speech", self.tag_all)?;
        row(
            f,
            "agree on it and every feature (these stand)",
            self.agreed,
        )?;
        row(f, "to adjudicate", self.words - self.agreed)?;
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
        row(f, "  of which no vote was possible", self.unvoted_words)?;
        for (title, table) in [("by tier", &self.by_tier), ("by context", &self.by_context)] {
            if table.is_empty() {
                continue;
            }
            writeln!(f, "{title}, all voters agree on every word")?;
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

/// The run a voter's file names: the first `Runs=` on a word line of `text`, CoNLL-U.
pub fn run_in(text: &str) -> Option<String> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(9))
        .flat_map(deslag_exam::conllu::pairs)
        .find(|(key, _)| *key == "Runs")
        .map(|(_, value)| value.to_string())
}

/// The columns of `voters.tsv`.
const VOTER_COLUMNS: [&str; 5] = ["letter", "voter", "base_only", "run", "file"];

/// The line before the columns of `voters.tsv` that says how many model voters a word needed to
/// count as agreed.
const MIN_LINE: &str = "# min_voters = ";

/// The `voters.tsv` of a merge: the `min_voters` it was made with, then a voter's letter, name,
/// whether it votes on the base alone, its run and its file, so a reader of a worklist can map a
/// letter back and `report` can find the answers. It is kept apart from the worklist, which the
/// adjudicator reads.
pub fn voters_tsv(voters: &[Voter], min_voters: usize) -> String {
    let mut out = format!("{MIN_LINE}{min_voters}\n{}\n", VOTER_COLUMNS.join("\t"));
    for (index, voter) in voters.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}",
            letter(index),
            voter.name,
            if voter.base_only { "yes" } else { "no" },
            voter.run.as_deref().unwrap_or("-"),
            voter.file
        );
    }
    out
}

/// The `min_voters` a `voters.tsv` records, or `None` for a merge made before it was recorded.
/// That merge compared any two voters, spaCy included, which is neither of the minimums the
/// current rule can take, so it is never read as one of them: a caller that needs a number says
/// "unknown" or refuses.
pub fn read_min_voters(text: &str) -> Option<usize> {
    text.lines()
        .next()
        .and_then(|line| line.strip_prefix(MIN_LINE))
        .and_then(|value| value.trim().parse().ok())
}

/// The `min_voters` that reproduces the rule before the minimum was recorded: any two voters
/// answering, spaCy included, since a model voter count of at least zero holds for every sentence.
pub const OLD_MERGE_MIN_VOTERS: usize = 0;

/// Reads `voters.tsv`, which came from `path`. The voters' answers are empty: the caller reads
/// them from each voter's file.
pub fn read_voters_tsv(path: &str, text: &str) -> Result<Vec<Voter>, Error> {
    let mut lines = text
        .lines()
        .enumerate()
        .skip_while(|(_, line)| line.starts_with('#'));
    let head: Vec<&str> = lines
        .next()
        .map(|(_, head)| head.split('\t').collect())
        .unwrap_or_default();
    if head != VOTER_COLUMNS {
        return Err(Error::at(
            path,
            1,
            format!("the columns should be {}", VOTER_COLUMNS.join(", ")),
        ));
    }
    let mut voters = Vec::new();
    for (at, line) in lines.filter(|(_, line)| !line.trim().is_empty()) {
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() != VOTER_COLUMNS.len() {
            return Err(Error::at(
                path,
                at + 1,
                format!(
                    "expected {} columns, found {}",
                    VOTER_COLUMNS.len(),
                    cells.len()
                ),
            ));
        }
        voters.push(Voter {
            name: cells[1].to_string(),
            answers: Answers(Vec::new()),
            base_only: cells[2] == "yes",
            run: (cells[3] != "-").then(|| cells[3].to_string()),
            file: cells[4].to_string(),
        });
    }
    Ok(voters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::tests::sample;
    use crate::merge::{agreed_conllu, load_tagger, load_voter, worklist_parts};

    fn code(text: &str) -> Code {
        Code::parse(text).unwrap()
    }

    fn judged(said: &[&str], base_only: &[bool]) -> Verdict {
        let said: Vec<Code> = said.iter().map(|text| code(text)).collect();
        judge_voters(&said, base_only)
    }

    #[test]
    fn two_voters_that_say_the_same_agree_and_any_number_may_vote() {
        assert_eq!(
            judged(&["N.p", "N.p"], &[false, false]),
            Verdict::Agreed(code("N.p"))
        );
        assert_eq!(
            judged(&["V.pp"; 5], &[false; 5]),
            Verdict::Agreed(code("V.pp"))
        );
    }

    #[test]
    fn a_different_base_or_a_clashing_feature_is_a_dispute_of_that_kind() {
        let free = [false; 3];
        assert_eq!(
            judged(&["N.p", "N.p", "J"], &free),
            Verdict::Disputed { tags_differ: true }
        );
        assert_eq!(
            judged(&["N.p", "N.s", "N.p"], &free),
            Verdict::Disputed { tags_differ: false }
        );
        assert_eq!(
            judged(&["V.pp", "V.pp", "V.pa"], &free),
            Verdict::Disputed { tags_differ: false }
        );
    }

    #[test]
    fn a_voter_that_gives_no_feature_abstains_on_it() {
        let bare_noun = Code::from_conllu("NOUN", "_").unwrap();
        let bare_verb = Code::from_conllu("VERB", "_").unwrap();
        assert_eq!(
            judge_voters(&[code("N.s"), code("N.s"), bare_noun], &[false; 3]),
            Verdict::Agreed(code("N.s"))
        );
        assert_eq!(
            judge_voters(&[bare_verb, code("V.pp"), code("V.pp")], &[false; 3]),
            Verdict::Agreed(code("V.pp"))
        );
        // Nobody gave the feature the guide asks of a noun: no one said it.
        assert_eq!(
            judge_voters(&[bare_noun, bare_noun], &[false; 2]),
            Verdict::Disputed { tags_differ: false }
        );
    }

    #[test]
    fn a_base_only_voter_counts_for_the_base_and_never_for_a_feature() {
        // spaCy's `Number=Plur` is no vote against two voters' `N.s`.
        let spacy = Code::from_conllu("NOUN", "Number=Plur").unwrap();
        assert_eq!(
            judge_voters(&[code("N.s"), code("N.s"), spacy], &[false, false, true]),
            Verdict::Agreed(code("N.s"))
        );
        // But its base is a vote.
        let adjective = Code::from_conllu("ADJ", "_").unwrap();
        assert_eq!(
            judge_voters(
                &[code("N.s"), code("N.s"), adjective],
                &[false, false, true]
            ),
            Verdict::Disputed { tags_differ: true }
        );
        // And it supplies no feature when the others give none either.
        let bare = Code::from_conllu("NOUN", "_").unwrap();
        assert_eq!(
            judge_voters(&[bare, spacy], &[false, true]),
            Verdict::Disputed { tags_differ: false }
        );
    }

    #[test]
    fn a_bare_pronoun_is_an_answer_and_not_an_abstention() {
        let free = [false; 3];
        assert_eq!(
            judged(&["PR", "PR", "PR"], &free),
            Verdict::Agreed(code("PR"))
        );
        assert_eq!(
            judged(&["PR.s", "PR.s", "PR.s"], &free),
            Verdict::Agreed(code("PR.s"))
        );
        assert_eq!(
            judged(&["PR", "PR", "PR.s"], &free),
            Verdict::Disputed { tags_differ: false }
        );
        assert_eq!(
            judged(&["PR.s", "PR", "PR"], &free),
            Verdict::Disputed { tags_differ: false }
        );
        assert_eq!(
            judged(&["PR.s", "PR.p", "PR.s"], &free),
            Verdict::Disputed { tags_differ: false }
        );
        // A base-only voter says nothing of number, bare or not.
        assert_eq!(
            judge_voters(
                &[code("PR"), code("PR"), code("PR.s")],
                &[false, false, true]
            ),
            Verdict::Agreed(code("PR"))
        );
    }

    /// A voter's CoNLL-U over the sample's tokens: `codes` is one code of the guide for each word,
    /// in order, written as a tagger that gives features would, and `run` goes in `Runs=`.
    fn conllu_of(codes: &[&str], run: Option<&str>) -> String {
        let sample = sample();
        let mut codes = codes.iter();
        let mut out = String::new();
        for sent in &sample.sents {
            let _ = writeln!(out, "# sent_id = {}", sent.id);
            for (index, tok) in sent.toks.iter().enumerate() {
                let (upos, feats) = if tok.is_word() {
                    let code = code(codes.next().expect("a code for each word"));
                    (code.upos(&tok.form), code.feats())
                } else {
                    ("X", "_".to_string())
                };
                out.push_str(&crate::data::line(
                    index,
                    &tok.form,
                    upos,
                    &feats,
                    &crate::data::misc(tok, None, run),
                ));
            }
            out.push('\n');
        }
        out
    }

    /// Run, to, compile, Why, The, user's, files, as the sample's words go.
    const SAME: [&str; 7] = ["V.fi", "T", "V.in", "R", "D", "N.s", "N.p"];

    fn voter(name: &str, codes: &[&str], run: Option<&str>, base_only: bool) -> Voter {
        let text = conllu_of(codes, run);
        Voter {
            name: name.to_string(),
            answers: load_tagger(name, name, &text, &sample()).unwrap(),
            base_only,
            run: run_in(&text),
            file: format!("tags/{name}.conllu"),
        }
    }

    #[test]
    fn four_voters_that_agree_leave_nothing_to_adjudicate_and_the_words_carry_their_runs() {
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter("b", &SAME, Some("r2"), false),
            voter("c", &SAME, Some("r3"), false),
            voter(
                "spacy",
                &["V.pp", "T", "V.pp", "R", "D", "N.p", "N.s"],
                Some("r4"),
                true,
            ),
        ];
        let sample = sample();
        let voted = merge_voters(&sample, &voters, MIN_AGREEING_VOTERS);
        assert!(voted.items.is_empty());
        assert_eq!(voted.stats.words, 7);
        assert_eq!(voted.stats.agreed, 7);
        // Six pairs of four voters.
        assert_eq!(voted.stats.pairs.len(), 6);
        assert!(
            voted
                .stats
                .pairs
                .iter()
                .all(|(_, count, both)| *count == 7 && *both == 7)
        );
        let runs = runs_by_sentence(&voters, sample.sents.len());
        assert_eq!(runs[0].as_deref(), Some("r1,r2,r3,r4"));
        let out = agreed_conllu(&sample, &voted.verdicts, &runs);
        assert!(
            out.contains(
                "4\tuser's\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2,r3,r4\n"
            ),
            "{out}"
        );
        // A token that is not a word is tagged by its kind, and no run vouches for it.
        assert!(
            out.contains("2\tmake ci\t_\tX\t_\t_\t_\t_\t_\tKind=Code|Prov=kind\n"),
            "{out}"
        );
    }

    #[test]
    fn a_voter_without_a_run_leaves_runs_off_the_words() {
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter("harper", &SAME, None, false),
        ];
        assert_eq!(runs_by_sentence(&voters, 2), [None, None]);
    }

    #[test]
    fn two_voters_are_enough_and_the_dispute_goes_to_the_worklist_by_letter() {
        let mut other = SAME;
        other[2] = "N.s";
        let voters = vec![
            voter("alpha", &SAME, Some("r1"), false),
            voter("beta", &other, Some("r2"), false),
        ];
        let sample = sample();
        let voted = merge_voters(&sample, &voters, 2);
        let ids: Vec<String> = voted.items.iter().map(|item| item.id(&sample)).collect();
        assert_eq!(ids, ["s1.4"]);
        assert!(voted.items[0].tags_differ);
        assert_eq!(voted.stats.tag_disputes, 1);
        assert_eq!(voted.stats.agreed, 6);
        assert_eq!(voted.stats.by_context["list-item"], (4, 4));
        assert_eq!(voted.stats.by_context["prose"], (3, 2));
        assert_eq!(voted.stats.by_tier["human"], (7, 6));
        let parts = worklist_parts(&sample, &voted.items, 60, &["A", "B"]);
        assert_eq!(parts.len(), 1);
        assert!(
            parts[0].contains("the two taggers disagree"),
            "{}",
            parts[0]
        );
        assert!(parts[0].contains("(A, B)"), "{}", parts[0]);
        assert!(
            parts[0].contains("\n  4 compile: A V.in, B N.s\n"),
            "{}",
            parts[0]
        );
        // The adjudicator is never told a model's name.
        assert!(!parts[0].contains("alpha") && !parts[0].contains("beta"));
        let report = voted.stats.to_string();
        assert!(report.contains("alpha and beta"), "{report}");
        assert!(report.contains("A   alpha"), "{report}");
    }

    /// A voter whose reply had no good line for the sentences in `drop`: its file has no block
    /// for them, as `read-tags --check` leaves it.
    fn voter_without(name: &str, run: &str, drop: &[&str]) -> Voter {
        let kept: Vec<String> = conllu_of(&SAME, Some(run))
            .split("\n\n")
            .filter(|block| !block.is_empty())
            .filter(|block| {
                !drop
                    .iter()
                    .any(|id| block.contains(&format!("sent_id = {id}\n")))
            })
            .map(|block| format!("{block}\n\n"))
            .collect();
        let text = kept.concat();
        Voter {
            name: name.to_string(),
            answers: load_voter(name, name, &text, &sample()).unwrap(),
            base_only: false,
            run: run_in(&text),
            file: format!("tags/{name}.conllu"),
        }
    }

    #[test]
    fn a_voter_with_no_good_line_for_a_sentence_abstains_on_it_and_the_merge_goes_on() {
        let text = conllu_of(&SAME, Some("r1"));
        let head = &text[..text.find("\n\n").unwrap() + 2];
        // As a tagger, a missing sentence is an error; as a voter, an abstention.
        assert!(load_tagger("a", "a", head, &sample()).is_err());
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter("b", &SAME, Some("r2"), false),
            voter_without("c", "r3", &["s2"]),
        ];
        let sample = sample();
        assert!(voters[2].answers.abstains(1) && !voters[2].answers.abstains(0));
        let voted = merge_voters(&sample, &voters, 2);
        assert!(voted.items.is_empty(), "the two that answered agree");
        assert_eq!(voted.stats.abstained, [0, 0, 1]);
        assert_eq!(voted.stats.unvoted_sentences, 0);
        // The pair that includes the abstainer is compared on the words it answered.
        let both: Vec<usize> = voted.stats.pairs.iter().map(|(_, _, both)| *both).collect();
        assert_eq!(both, [7, 3, 3]);
        // Runs= names the voters that answered the sentence, and no others.
        let runs = runs_by_sentence(&voters, sample.sents.len());
        assert_eq!(runs[0].as_deref(), Some("r1,r2,r3"));
        assert_eq!(runs[1].as_deref(), Some("r1,r2"));
        let out = agreed_conllu(&sample, &voted.verdicts, &runs);
        assert!(
            out.contains("Prov=agree|Runs=r1,r2|SpaceAfter=No\n"),
            "{out}"
        );
        let report = voted.stats.to_string();
        assert!(
            report.contains("gave no answer for (abstained)"),
            "{report}"
        );
    }

    #[test]
    fn a_sentence_fewer_than_two_voters_answered_goes_to_the_adjudicator_whole() {
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter_without("b", "r2", &["s1"]),
            voter_without("c", "r3", &["s1", "s2"]),
        ];
        let sample = sample();
        let voted = merge_voters(&sample, &voters, 2);
        // s1 had one answer, so all three of its words go; s2 had two, which agree.
        let ids: Vec<String> = voted.items.iter().map(|item| item.id(&sample)).collect();
        assert_eq!(ids, ["s1.1", "s1.3", "s1.4"]);
        assert!(
            voted
                .items
                .iter()
                .all(|item| item.unvoted && !item.tags_differ)
        );
        assert_eq!(voted.stats.unvoted_sentences, 1);
        assert_eq!(voted.stats.unvoted_words, 3);
        assert_eq!(voted.stats.agreed, 4);
        assert_eq!(voted.stats.tag_disputes + voted.stats.feature_disputes, 0);
        assert_eq!(voted.stats.abstained, [0, 1, 2]);
        let names = ["a", "b", "c"];
        let tsv = crate::merge::worklist_tsv(&sample, &voted.items, &names);
        assert!(
            tsv.contains("s1.1\ts1\t1\tRun\tV.fi\t-\t-\tnone\n"),
            "{tsv}"
        );
        let parts = worklist_parts(&sample, &voted.items, 60, &["A", "B", "C"]);
        assert!(parts[0].contains("1 Run: A V.fi, B -, C -"), "{}", parts[0]);
        // The words an unvoted sentence has carry no voter's run.
        let runs = runs_by_sentence(&voters, 2);
        assert_eq!(runs, [Some("r1".to_string()), Some("r1,r2".to_string())]);
    }

    #[test]
    fn two_model_voters_that_agree_are_not_enough_and_spacy_is_never_the_third() {
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter("b", &SAME, Some("r2"), false),
            voter("spacy", &SAME, Some("r3"), true),
        ];
        let sample = sample();
        // Three voters in all, but only two vote on more than the base: nothing is agreed.
        let voted = merge_voters(&sample, &voters, MIN_AGREEING_VOTERS);
        assert_eq!(voted.stats.agreed, 0);
        assert_eq!(voted.items.len(), 7);
        assert!(voted.items.iter().all(|item| item.unvoted));
        assert_eq!(voted.stats.unvoted_sentences, 2);
        let report = voted.stats.to_string();
        assert!(
            report.contains("sentences fewer than 3 model voters answered"),
            "{report}"
        );
        assert!(report.contains("at least 3 model voters"), "{report}");
        // The same voters stand when the minimum is asked to be two.
        let voted = merge_voters(&sample, &voters, 2);
        assert_eq!(voted.stats.agreed, 7);
        assert!(voted.items.is_empty());
    }

    #[test]
    fn a_sentence_one_of_three_model_voters_missed_goes_whole_to_the_adjudicator() {
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter("b", &SAME, Some("r2"), false),
            voter_without("c", "r3", &["s2"]),
        ];
        let sample = sample();
        let voted = merge_voters(&sample, &voters, MIN_AGREEING_VOTERS);
        // s1 has three answers, which agree; s2 has two, and two do not make an agreement.
        let ids: Vec<String> = voted.items.iter().map(|item| item.id(&sample)).collect();
        assert_eq!(ids, ["s2.1", "s2.3", "s2.4", "s2.5"]);
        assert_eq!(voted.stats.agreed, 3);
        assert_eq!(voted.stats.unvoted_sentences, 1);
    }

    #[test]
    fn voters_file_records_the_minimum_and_an_old_one_has_none() {
        let voters = vec![voter("a", &SAME, Some("r1"), false)];
        assert_eq!(read_min_voters(&voters_tsv(&voters, 3)), Some(3));
        assert_eq!(read_min_voters(&voters_tsv(&voters, 2)), Some(2));
        let old = "letter\tvoter\tbase_only\trun\tfile\nA\ta\tno\tr1\ttags/a.conllu\n";
        assert_eq!(read_min_voters(old), None, "never read as two");
        assert_eq!(read_voters_tsv("voters.tsv", old).unwrap().len(), 1);
    }

    #[test]
    fn letters_run_past_z() {
        assert_eq!(letter(0), "A");
        assert_eq!(letter(25), "Z");
        assert_eq!(letter(26), "AA");
    }

    #[test]
    fn the_voters_file_maps_letters_to_names_runs_and_files_and_reads_back() {
        let voters = vec![
            voter("a", &SAME, Some("r1"), false),
            voter("spacy", &SAME, None, true),
        ];
        let text = voters_tsv(&voters, 3);
        assert_eq!(
            text,
            "# min_voters = 3\nletter\tvoter\tbase_only\trun\tfile\nA\ta\tno\tr1\ttags/a.conllu\nB\tspacy\tyes\t-\ttags/spacy.conllu\n"
        );
        let back = read_voters_tsv("voters.tsv", &text).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].run.as_deref(), Some("r1"));
        assert!(back[1].base_only && back[1].run.is_none());
        assert_eq!(back[1].file, "tags/spacy.conllu");
    }

    #[test]
    fn a_run_is_the_first_runs_on_a_word_line() {
        let text = conllu_of(&SAME, Some("r9"));
        assert_eq!(run_in(&text).as_deref(), Some("r9"));
        assert_eq!(run_in(&conllu_of(&SAME, None)), None);
    }
}
