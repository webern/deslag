//! Sentences for the owner to review: rank the corpus by how unsure deslag is, build a queue from
//! picks, and move reviewed sentences into `tests/gold/owner.conllu`.
//!
//! The ranking reads each sentence as the review will show it: tagged alone, in its context, by
//! `deslag::tag::sentence`. A word counts against a sentence by how little deslag commits to:
//!
//! ```text
//! points = (words below Likely) + (Unknown words) + (words of an origin other than English)
//! score  = points / (words + 4)
//! ```
//!
//! A word below Likely is `Unsure` or `Unknown`, so an Unknown word is worth 2 and a non-English
//! one 1 more. The 4 keeps a two-word fragment from beating a sentence with real content. Ties
//! go by id. The score only orders the list; the picking is an agent's, by the skill.
//!
//! A sentence's id is `r` and ten hex digits of the sha256 of its file's sha256 and its byte
//! range, so it names the same sentence in any copy of the corpus holding that file.
//!
//! A queue is a skeleton the review opens, with `# tier`, `# source = <repo> <file>` and
//! `# license` per sentence. [`own`] turns a queue the owner has finished into the text of
//! `owner.conllu`: ids `o0001` on, `exam.tier` for `tier`, and the file's header on the first
//! sentence. The owner's sentences are a stratum of hard sentences, never pooled with dev, never
//! training data: silver excludes their repositories, which [`crate::exclude::Repos`] reads back
//! from the `source` comments.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use deslag::document::Token;
use deslag::tag::{Origin, Reading};
use deslag_exam::conllu;
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::{Gold, Tier};
use deslag_exam::skeleton;
use deslag_exam::tagger::Context;

use crate::code::Base;
use crate::data::{Sent, Tok};
use crate::exclude::sha256_hex;
use crate::problems::Problems;
use crate::sample::{File, Settings, Skipped, candidates};

/// The header of the ranking's TSV.
const COLUMNS: [&str; 12] = [
    "id",
    "score",
    "file",
    "repo",
    "tier",
    "context",
    "words",
    "below_likely",
    "unknown",
    "non_english",
    "hesitates",
    "text",
];

/// One sentence with what deslag makes of it.
#[derive(Debug, Clone)]
pub struct Ranked {
    /// Its stable id.
    pub id: String,
    /// The score, to three places.
    pub score: f64,
    /// Its file, as the loader names it.
    pub file: String,
    /// The file's repository, `owner/name`.
    pub repo: String,
    /// The file's licence.
    pub license: String,
    /// Who wrote the file.
    pub tier: Tier,
    /// The block it is in.
    pub context: Context,
    /// How many of its tokens are words.
    pub words: usize,
    /// Words deslag reads below Likely.
    pub below_likely: usize,
    /// Words in neither of deslag's tables.
    pub unknown: usize,
    /// Words of an origin other than English.
    pub non_english: usize,
    /// The tag pairs of the words below Likely, most common first, as `N/V:2`.
    pub hesitates: String,
    /// Its tokens.
    pub toks: Vec<Tok>,
}

impl Ranked {
    /// The sentence's text.
    pub fn text(&self) -> String {
        Sent {
            id: String::new(),
            toks: self.toks.clone(),
        }
        .text()
    }
}

/// The tokens of `toks` as deslag reads them in `context`, and their text.
fn read<'t>(joined: &'t str, toks: &[Tok], context: Context) -> Vec<Token<'t>> {
    let mut at = 0;
    let mut tokens: Vec<Token<'t>> = Vec::with_capacity(toks.len());
    for (index, tok) in toks.iter().enumerate() {
        let range = at..at + tok.form.len();
        tokens.push(Token {
            kind: tok.kind,
            text: Cow::Borrowed(&joined[range.clone()]),
            range,
            reading: None,
            origin: Origin::English,
        });
        at += tok.form.len();
        if !tok.joined && index + 1 < toks.len() {
            at += 1;
        }
    }
    deslag::tag::sentence(&mut tokens, context);
    tokens
}

/// The text the tokens index into: the forms with one space between them where none is joined.
fn joined(toks: &[Tok]) -> String {
    Sent {
        id: String::new(),
        toks: toks.to_vec(),
    }
    .text()
}

/// `score` rounded to three places.
fn round(score: f64) -> f64 {
    (score * 1000.0).round() / 1000.0
}

fn rank_one(file: &File<'_>, cand: crate::sample::Candidate) -> Ranked {
    let text = joined(&cand.toks);
    let tokens = read(&text, &cand.toks, cand.context);
    let (mut words, mut below, mut unknown, mut foreign) = (0, 0, 0, 0);
    let mut pairs: BTreeMap<String, usize> = BTreeMap::new();
    for token in &tokens {
        let Some(reading) = token.reading.as_ref() else {
            continue;
        };
        words += 1;
        foreign += usize::from(token.origin != Origin::English);
        if reading.confidence.committed() {
            continue;
        }
        below += 1;
        unknown += usize::from(reading.confidence == deslag::tag::Confidence::Unknown);
        if let Some(pair) = pair_of(reading) {
            *pairs.entry(pair).or_default() += 1;
        }
    }
    let mut by_count: Vec<(String, usize)> = pairs.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let hesitates = by_count
        .iter()
        .take(3)
        .map(|(pair, count)| format!("{pair}:{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    let points = below + unknown + foreign;
    let digest =
        sha256_hex(format!("{}:{}-{}", file.sha256, cand.range.start, cand.range.end).as_bytes());
    Ranked {
        id: format!("r{}", &digest[..10]),
        score: round(points as f64 / (words + 4) as f64),
        file: file.path.clone(),
        repo: file.repo.clone(),
        license: file.license.clone(),
        tier: file.tier,
        context: cand.context,
        words,
        below_likely: below,
        unknown,
        non_english: foreign,
        hesitates,
        toks: cand.toks,
    }
}

/// The guide codes of a reading's best guess and its first other kept tag, in alphabetical order
/// so `N/V` and `V/N` are one pair.
fn pair_of(reading: &Reading) -> Option<String> {
    let best = Base::from_tag(reading.tag).code();
    let other = reading
        .kept
        .iter()
        .map(Base::from_tag)
        .map(Base::code)
        .find(|code| *code != best)?;
    let (a, b) = if best <= other {
        (best, other)
    } else {
        (other, best)
    };
    Some(format!("{a}/{b}"))
}

/// Every sentence of `files` that may be reviewed, scored and sorted: highest score first, ties by
/// id. A sentence whose text an earlier file (in path order) already gave is left out.
pub fn rank(files: &[File<'_>]) -> Vec<Ranked> {
    let settings = Settings::default();
    let mut order: Vec<&File<'_>> = files.iter().collect();
    order.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = std::collections::BTreeSet::new();
    let mut skipped = Skipped::default();
    let mut ranked = Vec::new();
    for file in order {
        for cand in candidates(file.text, &settings, &mut skipped) {
            if seen.insert(joined(&cand.toks).to_lowercase()) {
                ranked.push(rank_one(file, cand));
            }
        }
    }
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    ranked
}

/// The first `top` of `ranked` once no repository has more than `per_repo`, the best first. The
/// top of a plain sort is full of one repository's jargon, so a list to read from is cut so.
pub fn spread(ranked: Vec<Ranked>, per_repo: usize, top: usize) -> Vec<Ranked> {
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    ranked
        .into_iter()
        .filter(|row| {
            let n = taken.entry(row.repo.to_lowercase()).or_default();
            *n += 1;
            *n <= per_repo
        })
        .take(top)
        .collect()
}

/// The ranking as a TSV, one line per sentence under a header line.
pub fn tsv(ranked: &[Ranked]) -> String {
    let mut out = format!("{}\n", COLUMNS.join("\t"));
    for row in ranked {
        let _ = writeln!(
            out,
            "{}\t{:.3}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.id,
            row.score,
            row.file,
            row.repo,
            row.tier.name(),
            row.context.name(),
            row.words,
            row.below_likely,
            row.unknown,
            row.non_english,
            row.hesitates,
            row.text()
        );
    }
    out
}

/// The ids of a picks file, in order: one per line, then a tab and the reason. Blank lines and
/// lines starting with `#` are skipped.
pub fn read_picks(path: &str, text: &str) -> Result<Vec<String>, Problems> {
    let mut ids: Vec<String> = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let id = line.split('\t').next().unwrap_or_default().trim();
        if ids.iter().any(|seen| seen == id) {
            problems.push(Error::at(
                path,
                index + 1,
                format!("`{id}` is picked twice"),
            ));
        } else {
            ids.push(id.to_string());
        }
    }
    if ids.is_empty() && problems.is_empty() {
        problems.push(Error::load(path, Place::File, "no pick"));
    }
    Problems::check(problems, ids)
}

/// The queue of the sentences `ids` names among `ranked`, in the order of `ids`: a skeleton the
/// review opens. An id nothing in `ranked` has is a problem naming it.
pub fn queue(path: &str, ids: &[String], ranked: &[Ranked]) -> Result<String, Problems> {
    let by_id: BTreeMap<&str, &Ranked> = ranked.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut out = String::from(skeleton::HEADER);
    let mut problems = Vec::new();
    for id in ids {
        let Some(row) = by_id.get(id.as_str()) else {
            problems.push(Error::load(
                path,
                Place::Sentence(id.clone()),
                "no such sentence among those the ranking may offer",
            ));
            continue;
        };
        let text = joined(&row.toks);
        let tokens = read(&text, &row.toks, row.context);
        let origins = deslag::tag::origins(&tokens);
        let _ = writeln!(out, "# sent_id = {}", row.id);
        let _ = writeln!(out, "# exam.context = {}", row.context.name());
        let _ = writeln!(out, "# tier = {}", row.tier.name());
        let _ = writeln!(out, "# source = {} {}", row.repo, row.file);
        let _ = writeln!(out, "# license = {}", row.license);
        let _ = writeln!(out, "# text = {text}");
        for (index, token) in tokens.iter().enumerate() {
            let misc = format!(
                "Kind={}{}{}",
                deslag_exam::gold::kind_name(token.kind),
                skeleton::origin_misc(token, origins[index]),
                if row.toks[index].joined {
                    "|SpaceAfter=No"
                } else {
                    ""
                }
            );
            out.push_str(&skeleton::line(index + 1, &token.text, &misc, None));
        }
        out.push('\n');
    }
    Problems::check(problems, out)
}

/// What `owner.conllu` says before its first sentence.
const OWNER_HEAD: &str = "\
# exam.tokens = deslag
# exam.trains = no
# exam.source = owner review
# Sentences the owner chose and reviewed: a stratum of hard sentences, never pooled with dev.
# They are never training data. Silver must leave out every repository named in a source line.
";

/// The numbers of the `o0001` ids `owner` holds, the highest.
fn last_number(owner: &[conllu::Block]) -> usize {
    owner
        .iter()
        .filter_map(|block| block.comment("sent_id"))
        .filter_map(|id| id.value.strip_prefix('o')?.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
}

/// The text of `owner.conllu` once the sentences of `queue`, which the owner has finished, are in
/// it: `owner` is its text now, or `None` when there is no file yet. Every sentence of the queue
/// must have `# owner_reviewed` and a tag on every line, and may not be in the file already.
/// Nothing is returned if any does not.
pub fn own(
    queue_path: &str,
    queue: &str,
    owner_path: &str,
    owner: Option<&str>,
) -> Result<(String, usize), Problems> {
    let blocks = conllu::read(queue_path, queue)?;
    if blocks.is_empty() {
        return Err(Error::load(queue_path, Place::File, "the queue has no sentences").into());
    }
    let held = match owner {
        Some(text) => conllu::read(owner_path, text)?,
        None => Vec::new(),
    };
    let mut problems = Vec::new();
    for block in &blocks {
        let id = block
            .comment("sent_id")
            .map_or("?".to_string(), |c| c.value.clone());
        let mut fail = |message: &str| {
            problems.push(Problems::sentence(queue_path, &id, message));
        };
        if block.comment("owner_reviewed").is_none() {
            fail("not reviewed: it has no owner_reviewed");
        }
        let blank = block.lines.iter().filter(|line| line.upos == "_").count();
        if blank > 0 {
            fail(&format!("{blank} words have no tag"));
        }
        for key in ["tier", "source", "license", "exam.context"] {
            if block.comment(key).is_none() {
                fail(&format!("no `{key}`"));
            }
        }
        if block
            .comment("tier")
            .is_some_and(|c| Tier::from_name(&c.value).is_none())
        {
            fail("`tier` is not human, llm or mixed");
        }
        if let (Some(source), Some(text)) = (block.comment("source"), block.comment("text")) {
            let twin = held.iter().any(|other| {
                other.comment("source").map(|c| &c.value) == Some(&source.value)
                    && other.comment("text").map(|c| &c.value) == Some(&text.value)
            });
            if twin {
                fail("it is in the owner file already");
            }
        }
    }
    Problems::check(std::mem::take(&mut problems), ())?;

    let mut number = last_number(&held);
    let mut out = match owner {
        Some(text) => format!("{}\n\n", text.trim_end()),
        None => String::new(),
    };
    for (index, group) in groups(queue).into_iter().enumerate() {
        number += 1;
        if owner.is_none() && index == 0 {
            out.push_str(OWNER_HEAD);
        }
        for line in group {
            if line.starts_with("# exam.tokens") || line.starts_with("# exam.trains") {
                continue;
            }
            if line.starts_with("# sent_id") {
                let _ = writeln!(out, "# sent_id = o{number:04}");
            } else if let Some(tier) = line.strip_prefix("# tier = ") {
                let _ = writeln!(out, "# exam.tier = {tier}");
            } else {
                let _ = writeln!(out, "{line}");
            }
        }
        out.push('\n');
    }
    let out = format!("{}\n", out.trim_end());
    let name = std::path::Path::new(owner_path).file_name().map_or_else(
        || owner_path.to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Gold::parse(owner_path, &name, &out)?;
    Ok((out, blocks.len()))
}

/// The lines of `text` grouped by blank lines, the way `conllu::read` makes blocks.
fn groups(text: &str) -> Vec<Vec<&str>> {
    let mut groups: Vec<Vec<&str>> = Vec::new();
    let mut open = false;
    for line in text.trim_start_matches('\u{feff}').lines() {
        if line.trim().is_empty() {
            open = false;
        } else if open {
            groups.last_mut().expect("a group is open").push(line);
        } else {
            groups.push(vec![line]);
            open = true;
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::Key;
    use crate::review::tests::{Memory, fill, open};

    const A: &str =
        "# Setup\n\nRun the frobnicator now. See zorblax for details.\n\n- Check the quux twice.\n";
    const B: &str = "Install the package first, then restart the server.\n";

    fn file<'a>(path: &str, repo: &str, text: &'a str) -> File<'a> {
        File {
            path: path.to_string(),
            tier: if path.starts_with("llm") {
                Tier::Llm
            } else {
                Tier::Human
            },
            repo: repo.to_string(),
            license: "MIT".to_string(),
            sha256: sha256_hex(text.as_bytes()),
            text,
        }
    }

    fn ranked() -> Vec<Ranked> {
        rank(&[
            file("human/a/one.md", "o/a", A),
            file("llm/b/two.md", "o/b", B),
        ])
    }

    #[test]
    fn sentences_are_scored_by_what_deslag_does_not_commit_to_and_sorted_by_it() {
        let rows = ranked();
        assert!(rows.len() >= 4, "{rows:#?}");
        assert!(rows.windows(2).all(|pair| pair[0].score >= pair[1].score));
        for row in &rows {
            let points = row.below_likely + row.unknown + row.non_english;
            assert_eq!(row.score, round(points as f64 / (row.words + 4) as f64));
            assert!(row.unknown <= row.below_likely);
        }
        let ids: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), rows.len(), "ids are unique");
        // The same file and range give the same id in another run.
        let again = ranked();
        assert_eq!(
            rows.iter().map(|r| &r.id).collect::<Vec<_>>(),
            again.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_list_keeps_a_few_of_a_repository_and_the_tsv_has_one_line_each() {
        let rows = ranked();
        let kept = spread(rows.clone(), 1, 100);
        assert_eq!(kept.len(), 2);
        assert_ne!(kept[0].repo, kept[1].repo);
        let text = tsv(&kept);
        assert_eq!(text.lines().count(), 3);
        assert!(
            text.lines()
                .all(|line| line.split('\t').count() == COLUMNS.len())
        );
        assert!(text.starts_with("id\tscore\tfile\trepo\ttier\tcontext\t"));
    }

    #[test]
    fn a_pick_of_an_unknown_id_or_the_same_id_twice_is_a_problem() {
        let rows = ranked();
        let error = queue("picks.tsv", &["rnothing".into()], &rows).unwrap_err();
        assert!(error.to_string().contains("rnothing"), "{error}");
        let error = read_picks("picks.tsv", "r1\tone\nr1\tagain\n").unwrap_err();
        assert!(error.to_string().starts_with("picks.tsv:2"), "{error}");
        let ids = read_picks("p", "# note\n\nr1\tbecause\nr2\n").unwrap();
        assert_eq!(ids, ["r1", "r2"]);
    }

    /// The queue of every sentence, reviewed through the session with every blank word `N.s`.
    fn reviewed(rows: &[Ranked]) -> String {
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let queue = queue("picks", &ids, rows).unwrap();
        let mut session = open(&queue);
        let mut store = Memory::default();
        for _ in rows {
            fill(&mut session, &mut store);
            session.press(Key::Char('n'), &mut store);
        }
        store.saved.last().cloned().expect("the review saved")
    }

    #[test]
    fn a_reviewed_queue_moves_into_the_owner_file_and_loads_as_gold() {
        let rows = ranked();
        let done = reviewed(&rows);
        let (text, moved) = own("q.conllu", &done, "owner.conllu", None).unwrap();
        assert_eq!(moved, rows.len());
        let gold = Gold::parse("owner.conllu", "owner.conllu", &text).unwrap();
        assert_eq!(gold.sentences.len(), rows.len());
        assert_eq!(gold.sentences[0].sent_id, "o0001");
        assert_eq!(
            gold.sentences[rows.len() - 1].sent_id,
            format!("o{:04}", rows.len())
        );
        assert_eq!(gold.trains, deslag_exam::gold::Trains::No);
        assert_eq!(gold.split, None);
        assert_eq!(gold.source, "owner review");
        assert!(gold.sentences.iter().any(|s| s.tier == Some(Tier::Llm)));
        assert!(text.contains("# source = o/a human/a/one.md"), "{text}");
        assert_eq!(text.matches("exam.tokens").count(), 1);
        assert_eq!(text.matches("exam.trains").count(), 1);
        assert!(!text.contains("# tier ="));
        assert!(text.contains("Prov=owner"));
        // Its repositories are what a later draw leaves out.
        let repos = crate::exclude::Repos::gold("owner.conllu", &text).unwrap();
        let files = vec![file("x.md", "O/A", "x"), file("y.md", "o/c", "y")];
        let (kept, dropped) = repos.drop(files);
        assert_eq!((dropped, kept[0].path.as_str()), (1, "y.md"));
    }

    #[test]
    fn a_second_queue_is_added_after_the_first_with_the_next_ids() {
        let rows = ranked();
        let (first, _) = own("q", &reviewed(&rows[..2]), "owner.conllu", None).unwrap();
        let (both, moved) = own("q", &reviewed(&rows[2..]), "owner.conllu", Some(&first)).unwrap();
        assert_eq!(moved, rows.len() - 2);
        assert!(both.starts_with(first.trim_end()));
        assert_eq!(both.matches("exam.tokens").count(), 1);
        let gold = Gold::parse("owner.conllu", "owner.conllu", &both).unwrap();
        assert_eq!(gold.sentences.len(), rows.len());
        assert_eq!(gold.sentences[2].sent_id, "o0003");
        // The same sentences again are refused.
        let error = own("q", &reviewed(&rows[..2]), "owner.conllu", Some(&both)).unwrap_err();
        assert!(error.to_string().contains("already"), "{error}");
    }

    #[test]
    fn a_queue_with_an_unreviewed_sentence_or_a_blank_word_moves_nothing() {
        let rows = ranked();
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        // Never opened: no owner_reviewed and blank words, both named.
        let raw = queue("picks", &ids, &rows).unwrap();
        let error = own("q", &raw, "owner.conllu", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not reviewed"), "{error}");
        assert!(error.contains("have no tag"), "{error}");
        // Reviewed, then one word blanked by hand.
        let done = reviewed(&rows);
        let blanked = done.replacen("\tNOUN\t", "\t_\t", 1);
        let error = own("q", &blanked, "owner.conllu", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("have no tag"), "{error}");
        // Reviewed, then the date removed from one sentence.
        let undated: String = done
            .lines()
            .filter(|line| !line.starts_with("# owner_reviewed"))
            .map(|line| format!("{line}\n"))
            .collect();
        let error = own("q", &undated, "owner.conllu", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not reviewed"), "{error}");
    }
}
