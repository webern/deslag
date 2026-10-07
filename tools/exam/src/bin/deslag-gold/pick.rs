//! Sentences for the owner to review: rank the corpus by how unsure deslag is, build a queue from
//! picks, and move reviewed sentences into `tests/gold/owner.conllu`.
//!
//! The ranking reads each sentence as the review will show it: tagged alone, in its context, by
//! `deslag::tag::sentence`. A word counts against a sentence by how little deslag commits to:
//!
//! ```text
//! points = (words below Likely) + (Unknown words)
//! score  = points / (words + 4)
//! ```
//!
//! A word below Likely is `Unsure` or `Unknown`, so an Unknown word is worth 2. The 4 keeps a
//! two-word fragment from beating a sentence with real content. The `non_english` column (words
//! whose origin is a symbol, command, path or flag) is shown and not scored: origin is
//! step 0's work, and a word deslag commits to at Likely costs nothing however it is spelt. Ties
//! go by id. The score only orders the list; the picking is an agent's, by the skill.
//!
//! Rows that would only fill the head of the list are dropped before ranking, and `rank` prints
//! how many of each kind (see [`Junk`]): sentences mostly not English (over half of the
//! English-origin words Unknown in a sentence of five or more, 45% in ten or more), lorem ipsum, version lists
//! (more numbers than words), and command lines (half the words or more of a code origin). What
//! is left is a judgement for the agent: personal data, near-duplicates, spread.
//!
//! `hesitates` lists the pairs of a word's best guess with each other tag deslag has not ruled
//! out, as `N/V:2`, most common first. A reading's `kept` set is unordered: it says which tags
//! are still possible, not which is likelier. So the column says what deslag cannot tell apart
//! and never which alternative to prefer; a word with three kept tags gives two pairs.
//!
//! A sentence's id is `r` and sixteen hex digits of the sha256 of its file's sha256 and its byte
//! range, so it names the same sentence in any copy of the corpus holding that file. Two
//! sentences with one id are an error in `rank` and in `queue`, never a silent choice.
//!
//! A queue is a skeleton the review opens, with `# tier`, `# source = <file> bytes a-b` (as
//! `dev.conllu` has it), `# repo = owner/name`, `# pick_id` (the id the pick named) and
//! `# license` per sentence. In the review, `x` twice marks a pick `# owner_rejected = <date>`:
//! a bad pick (personal data, not English) that needs no tags. [`own`] turns a queue the owner
//! has finished into the text of `owner.conllu`: it leaves the rejected sentences out, gives the
//! others ids `o0001` on, `exam.tier` for `tier`, and the file's header on the first sentence,
//! keeping `source`, `repo` and `pick_id`. The owner's sentences are a stratum of hard
//! sentences, never pooled with dev, never training data: silver excludes their repositories,
//! which [`crate::exclude::Repos::reserved`] reads back from the `repo` comments.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;

use deslag::document::{Token, TokenKind};
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
const COLUMNS: [&str; 13] = [
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
    "origins",
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
    /// The kinds of those origins, as `Command:2 Path:1`.
    pub origins: String,
    /// The pairs of a word's best guess and each other tag still possible, over the words below
    /// Likely, most common first, as `N/V:2`. Unordered: it names what deslag cannot tell apart,
    /// not which alternative is likelier.
    pub hesitates: String,
    /// Where the sentence is in its file, in bytes.
    pub range: Range<usize>,
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

/// What decides whether a sentence is junk, counted over its tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    /// Tokens that are words.
    pub words: usize,
    /// Tokens that are numbers.
    pub numbers: usize,
    /// Words of an origin other than English.
    pub code_words: usize,
    /// Words of English origin, and how many of them are Unknown.
    pub english: usize,
    /// See `english`.
    pub unknown_english: usize,
    /// Words of the lorem ipsum vocabulary.
    pub lorem: usize,
}

/// Why a sentence is left out of the ranking. The list needs the sentences deslag is unsure of
/// as English, not ones it cannot read or ones that are no sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Junk {
    /// Lorem ipsum: two or more of its words, and at least half the sentence.
    Lorem,
    /// A list of versions: three or more numbers, as many as the words.
    Versions,
    /// A command line: half the words or more of a code origin (symbol, command, path, flag).
    Command,
    /// Mostly another language: of the English-origin words, over half Unknown in a sentence of
    /// five or more, or 45% in a sentence of ten or more. Another language's words are mostly
    /// ones deslag has never seen; English with jargon is rarely so far over a third.
    NotEnglish,
}

/// The distinctive words of lorem ipsum, which are not English words.
const LOREM: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "eiusmod",
    "tempor",
    "incididunt",
    "labore",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "proident",
    "culpa",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "laborum",
    "pellentesque",
    "odio",
    "habitant",
    "morbi",
    "tristique",
    "senectus",
    "netus",
    "malesuada",
    "fames",
    "turpis",
    "egestas",
    "vestibulum",
    "tortor",
    "quam",
    "feugiat",
    "vitae",
    "ultricies",
    "lectus",
    "maecenas",
    "mauris",
    "nunc",
    "tellus",
    "risus",
    "sapien",
    "ligula",
    "urna",
    "augue",
    "dignissim",
    "sodales",
    "suscipit",
    "nibh",
    "cursus",
    "euismod",
    "lacinia",
    "pretium",
    "vulputate",
    "aenean",
    "praesent",
    "cras",
    "fusce",
    "curabitur",
    "proin",
    "elementum",
    "facilisis",
    "volutpat",
    "blandit",
    "congue",
    "eleifend",
    "tincidunt",
    "vivamus",
    "justo",
    "felis",
    "metus",
    "ornare",
    "rhoncus",
    "etiam",
    "gravida",
    "interdum",
    "phasellus",
    "lobortis",
    "faucibus",
    "luctus",
    "posuere",
    "cubilia",
    "curae",
    "donec",
    "orci",
    "primis",
];

/// Whether `word` is of the lorem ipsum vocabulary, whatever its case.
fn is_lorem(word: &str) -> bool {
    let word = word.to_lowercase();
    LOREM.contains(&word.trim_matches(|c: char| !c.is_alphabetic()))
}

/// Why `facts` are junk, if they are. The first that applies.
pub fn junk(facts: &Facts) -> Option<Junk> {
    if facts.lorem >= 2 && facts.lorem * 2 >= facts.words {
        Some(Junk::Lorem)
    } else if facts.numbers >= 3 && facts.numbers >= facts.words {
        Some(Junk::Versions)
    } else if facts.words > 0 && facts.code_words * 2 >= facts.words {
        Some(Junk::Command)
    } else if (facts.english >= 5 && facts.unknown_english * 2 > facts.english)
        || (facts.english >= 10 && facts.unknown_english * 100 >= 45 * facts.english)
    {
        Some(Junk::NotEnglish)
    } else {
        None
    }
}

/// How many sentences were left out of the ranking, by [`Junk`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dropped {
    /// See [`Junk::NotEnglish`].
    pub not_english: usize,
    /// See [`Junk::Lorem`].
    pub lorem: usize,
    /// See [`Junk::Versions`].
    pub versions: usize,
    /// See [`Junk::Command`].
    pub command: usize,
}

impl Dropped {
    fn count(&mut self, why: Junk) {
        match why {
            Junk::NotEnglish => self.not_english += 1,
            Junk::Lorem => self.lorem += 1,
            Junk::Versions => self.versions += 1,
            Junk::Command => self.command += 1,
        }
    }

    /// All of them.
    pub fn total(&self) -> usize {
        self.not_english + self.lorem + self.versions + self.command
    }
}

impl std::fmt::Display for Dropped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dropped {} junk sentences: {} not English, {} lorem ipsum, {} version lists, {} command lines",
            self.total(),
            self.not_english,
            self.lorem,
            self.versions,
            self.command
        )
    }
}

/// The ranking: the sentences that may be offered, best first, and what was dropped.
#[derive(Debug, Clone)]
pub struct Offer {
    /// Highest score first, ties by id.
    pub rows: Vec<Ranked>,
    /// What was left out as junk.
    pub dropped: Dropped,
}

fn rank_one(file: &File<'_>, cand: crate::sample::Candidate) -> (Ranked, Option<Junk>) {
    let text = joined(&cand.toks);
    let tokens = read(&text, &cand.toks, cand.context);
    let (mut words, mut below, mut unknown) = (0, 0, 0);
    let mut facts = Facts::default();
    let mut foreign = 0;
    let mut pairs: BTreeMap<String, usize> = BTreeMap::new();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for token in &tokens {
        if token.kind == TokenKind::Number {
            facts.numbers += 1;
        }
        let Some(reading) = token.reading.as_ref() else {
            continue;
        };
        words += 1;
        let is_unknown = reading.confidence == deslag::tag::Confidence::Unknown;
        if is_lorem(&token.text) {
            facts.lorem += 1;
        }
        if token.origin == Origin::English {
            facts.english += 1;
            facts.unknown_english += usize::from(is_unknown);
        } else {
            foreign += 1;
            facts.code_words += 1;
            *kinds.entry(token.origin.name()).or_default() += 1;
        }
        if reading.confidence.committed() {
            continue;
        }
        below += 1;
        unknown += usize::from(is_unknown);
        for pair in pairs_of(reading) {
            *pairs.entry(pair).or_default() += 1;
        }
    }
    facts.words = words;
    let mut by_count: Vec<(String, usize)> = pairs.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let hesitates = by_count
        .iter()
        .take(4)
        .map(|(pair, count)| format!("{pair}:{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    let origins = kinds
        .iter()
        .map(|(kind, count)| format!("{kind}:{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    let points = below + unknown;
    let digest =
        sha256_hex(format!("{}:{}-{}", file.sha256, cand.range.start, cand.range.end).as_bytes());
    let row = Ranked {
        id: format!("r{}", &digest[..16]),
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
        origins,
        hesitates,
        range: cand.range,
        toks: cand.toks,
    };
    (row, junk(&facts))
}

/// The guide codes of a reading's best guess and each other tag still possible, each pair in
/// alphabetical order so `N/V` and `V/N` are one pair. The order of `kept` means nothing, so
/// every alternative gives a pair.
fn pairs_of(reading: &Reading) -> Vec<String> {
    let best = Base::from_tag(reading.tag).code();
    let others: BTreeSet<&str> = reading
        .kept
        .iter()
        .map(Base::from_tag)
        .map(Base::code)
        .filter(|code| *code != best)
        .collect();
    others
        .into_iter()
        .map(|other| {
            let (a, b) = if best <= other {
                (best, other)
            } else {
                (other, best)
            };
            format!("{a}/{b}")
        })
        .collect()
}

/// Every sentence of `files` that may be reviewed, scored and sorted: highest score first, ties by
/// id, and what was left out as junk. A sentence whose text an earlier file (in path order)
/// already gave is left out. Two sentences with one id are an error.
pub fn rank(files: &[File<'_>]) -> Result<Offer, Error> {
    let settings = Settings::default();
    let mut order: Vec<&File<'_>> = files.iter().collect();
    order.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = BTreeSet::new();
    let mut skipped = Skipped::default();
    let mut dropped = Dropped::default();
    let mut rows = Vec::new();
    for file in order {
        for cand in candidates(file.text, &settings, &mut skipped) {
            if !seen.insert(joined(&cand.toks).to_lowercase()) {
                continue;
            }
            let (row, why) = rank_one(file, cand);
            match why {
                Some(why) => dropped.count(why),
                None => rows.push(row),
            }
        }
    }
    unique_ids(&rows)?;
    rows.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    Ok(Offer { rows, dropped })
}

/// An error naming an id two sentences share. Said without the sentences, which may be anything.
fn unique_ids(rows: &[Ranked]) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for row in rows {
        if !seen.insert(row.id.as_str()) {
            return Err(Error::load(
                "rank",
                Place::Sentence(row.id.clone()),
                "two different sentences have this id, so a pick of it would be ambiguous",
            ));
        }
    }
    Ok(())
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
            "{}\t{:.3}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
            row.origins,
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
    unique_ids(ranked)?;
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
        let _ = writeln!(
            out,
            "# source = {} bytes {}-{}",
            row.file, row.range.start, row.range.end
        );
        let _ = writeln!(out, "# repo = {}", row.repo);
        let _ = writeln!(out, "# pick_id = {}", row.id);
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
# They are never training data. Silver must leave out every repository named in a repo line.
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
/// it: `owner` is its text now, or `None` when there is no file yet. A sentence marked
/// `# owner_rejected` is left out. Every other sentence of the queue must have `# owner_reviewed`
/// and a tag on every line, must carry `repo` and `pick_id`, and may not be in the file already.
/// Nothing is returned if any does not, or if every sentence is rejected. The numbers are those
/// moved and those rejected.
pub fn own(
    queue_path: &str,
    queue: &str,
    owner_path: &str,
    owner: Option<&str>,
) -> Result<(String, usize, usize), Problems> {
    let blocks = conllu::read(queue_path, queue)?;
    if blocks.is_empty() {
        return Err(Error::load(queue_path, Place::File, "the queue has no sentences").into());
    }
    let held = match owner {
        Some(text) => conllu::read(owner_path, text)?,
        None => Vec::new(),
    };
    // Silver is a model's labels. An audit queue is drawn from it and says so; a word that
    // names runs was labelled by one. Neither may become owner gold, however the owner reviews it.
    let silver = blocks.iter().any(|block| {
        block.comment(crate::pilot::SILVER_KEY).is_some()
            || block.lines.iter().any(|line| {
                conllu::pairs(&line.misc)
                    .iter()
                    .any(|(key, _)| *key == "Runs")
            })
    });
    if silver {
        return Err(Error::load(
            queue_path,
            Place::File,
            "this queue holds silver, labels a model made, which never becomes owner gold; \
             `deslag-gold queue` makes the queues `own` takes",
        )
        .into());
    }
    let rejected = |block: &conllu::Block| block.comment("owner_rejected").is_some();
    let mut problems = Vec::new();
    for block in blocks.iter().filter(|block| !rejected(block)) {
        let id = block
            .comment("sent_id")
            .map_or("?".to_string(), |c| c.value.clone());
        let mut fail = |message: &str| {
            problems.push(Problems::sentence(queue_path, &id, message));
        };
        if block.comment("owner_reviewed").is_none() {
            fail("not reviewed: it has no owner_reviewed (reject it with x, or tag it)");
        }
        let blank = block.lines.iter().filter(|line| line.upos == "_").count();
        if blank > 0 {
            fail(&format!("{blank} words have no tag"));
        }
        for key in [
            "tier",
            "source",
            "repo",
            "pick_id",
            "license",
            "exam.context",
        ] {
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
    let left_out = blocks.iter().filter(|block| rejected(block)).count();
    if left_out == blocks.len() {
        return Err(Error::load(
            queue_path,
            Place::File,
            "every sentence is rejected, so there is nothing to move",
        )
        .into());
    }

    let mut number = last_number(&held);
    let mut out = match owner {
        Some(text) => format!("{}\n\n", text.trim_end()),
        None => String::new(),
    };
    let mut first = owner.is_none();
    for (group, block) in groups(queue).into_iter().zip(&blocks) {
        if rejected(block) {
            continue;
        }
        number += 1;
        if first {
            out.push_str(OWNER_HEAD);
            first = false;
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
    crate::exclude::Repos::gold(owner_path, &out)?;
    Ok((out, blocks.len() - left_out, left_out))
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
            provenance: Default::default(),
            text,
        }
    }

    fn ranked() -> Vec<Ranked> {
        rank(&[
            file("human/a/one.md", "o/a", A),
            file("llm/b/two.md", "o/b", B),
        ])
        .unwrap()
        .rows
    }

    #[test]
    fn sentences_are_scored_by_what_deslag_does_not_commit_to_and_sorted_by_it() {
        let rows = ranked();
        assert!(rows.len() >= 4, "{rows:#?}");
        assert!(rows.windows(2).all(|pair| pair[0].score >= pair[1].score));
        for row in &rows {
            let points = row.below_likely + row.unknown;
            assert_eq!(row.score, round(points as f64 / (row.words + 4) as f64));
            assert!(row.unknown <= row.below_likely);
        }
        let ids: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), rows.len(), "ids are unique");
        // `r` and sixteen hex digits.
        assert!(
            rows.iter()
                .all(|r| r.id.len() == 17 && r.id[1..].bytes().all(|b| b.is_ascii_hexdigit()))
        );
        // The same file and range give the same id in another run.
        let again = ranked();
        assert_eq!(
            rows.iter().map(|r| &r.id).collect::<Vec<_>>(),
            again.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn two_sentences_with_one_id_fail_the_ranking_and_the_queue() {
        let mut rows = ranked();
        rows[1].id = rows[0].id.clone();
        let error = unique_ids(&rows).unwrap_err().to_string();
        assert!(error.contains(&rows[0].id), "{error}");
        assert!(!error.contains(&rows[0].text()), "{error}");
        let error = queue("picks", &[rows[0].id.clone()], &rows).unwrap_err();
        assert!(error.to_string().contains("ambiguous"), "{error}");
    }

    fn facts(words: usize) -> Facts {
        Facts {
            words,
            english: words,
            ..Facts::default()
        }
    }

    #[test]
    fn junk_is_lorem_version_lists_command_lines_and_other_languages() {
        // Real English with a few unknown words stays.
        let fine = Facts {
            unknown_english: 2,
            ..facts(10)
        };
        assert_eq!(junk(&fine), None);
        // Over half the English words unknown, in a sentence of five or more.
        let other = Facts {
            unknown_english: 6,
            ..facts(10)
        };
        assert_eq!(junk(&other), Some(Junk::NotEnglish));
        let long = Facts {
            unknown_english: 5,
            ..facts(11)
        };
        assert_eq!(
            junk(&long),
            Some(Junk::NotEnglish),
            "45% of a long sentence"
        );
        let medium = Facts {
            unknown_english: 4,
            ..facts(9)
        };
        assert_eq!(junk(&medium), None, "45% of a short one is not enough");
        let short = Facts {
            unknown_english: 3,
            ..facts(4)
        };
        assert_eq!(junk(&short), None, "a fragment is not judged by its words");
        // Code words are not counted against the language: 3 of 4 English words unknown is not
        // enough when most of the sentence is code, which is a command line anyway.
        let command = Facts {
            code_words: 5,
            english: 5,
            unknown_english: 1,
            ..facts(10)
        };
        assert_eq!(junk(&command), Some(Junk::Command));
        let some_code = Facts {
            code_words: 2,
            english: 8,
            ..facts(10)
        };
        assert_eq!(junk(&some_code), None);
        let versions = Facts {
            numbers: 4,
            ..facts(4)
        };
        assert_eq!(junk(&versions), Some(Junk::Versions));
        let dated = Facts {
            numbers: 3,
            ..facts(12)
        };
        assert_eq!(junk(&dated), None);
        let lorem = Facts {
            lorem: 3,
            ..facts(5)
        };
        assert_eq!(junk(&lorem), Some(Junk::Lorem));
        let one = Facts {
            lorem: 1,
            ..facts(1)
        };
        assert_eq!(junk(&one), None, "one word is not lorem ipsum");
    }

    #[test]
    fn rank_drops_junk_sentences_and_counts_them_by_kind() {
        const J: &str = "Duis aute irure dolor in reprehenderit in voluptate velit esse.\n\n\
            Pellentesque odio odio tristique senectus.\n\n\
            Install the package first, then restart the server.\n";
        let offer = rank(&[file("human/j/junk.md", "o/j", J)]).unwrap();
        assert!(offer.dropped.lorem >= 2, "{}", offer.dropped);
        assert_eq!(
            offer.dropped.total(),
            offer.dropped.lorem
                + offer.dropped.not_english
                + offer.dropped.versions
                + offer.dropped.command
        );
        assert!(
            offer.rows.iter().all(|r| !r.text().contains("Duis")),
            "{:?}",
            offer.rows.iter().map(Ranked::text).collect::<Vec<_>>()
        );
        assert!(offer.rows.iter().any(|r| r.text().starts_with("Install")));
        assert!(offer.dropped.to_string().starts_with("dropped "));
    }

    #[test]
    fn hesitates_gives_a_pair_for_every_other_tag_still_possible() {
        use deslag::tag::{Confidence, Features, Tag, TagSet};
        let reading = Reading {
            tag: Tag::Verb,
            features: Features::default(),
            confidence: Confidence::Unsure,
            kept: TagSet::of(Tag::Noun).with(Tag::Adjective).with(Tag::Verb),
        };
        let pairs = pairs_of(&reading);
        assert_eq!(pairs.len(), 2, "{pairs:?}");
        assert!(pairs.iter().all(|pair| pair.contains('V')), "{pairs:?}");
        // Either alternative can come first; neither is preferred by the tag order.
        assert_ne!(pairs[0], pairs[1]);
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
        let (text, moved, rejected) = own("q.conllu", &done, "owner.conllu", None).unwrap();
        assert_eq!((moved, rejected), (rows.len(), 0));
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
        assert!(text.contains("# source = human/a/one.md bytes "), "{text}");
        assert!(text.contains("# repo = o/a\n"), "{text}");
        assert_eq!(text.matches("# pick_id = r").count(), rows.len());
        assert_eq!(text.matches("exam.tokens").count(), 1);
        assert_eq!(text.matches("exam.trains").count(), 1);
        assert!(!text.contains("# tier ="));
        assert!(text.contains("Prov=owner"));
        // Its repositories are what a later draw leaves out.
        let repos = crate::exclude::Repos::gold("owner.conllu", &text).unwrap();
        let files = vec![file("x.md", " O/A ", "x"), file("y.md", "o/c", "y")];
        let (kept, dropped) = repos.drop(files);
        assert_eq!((dropped, kept[0].path.as_str()), (1, "y.md"));
    }

    #[test]
    fn a_rejected_pick_needs_no_tags_and_is_left_out_of_the_owner_file() {
        let rows = ranked();
        assert!(rows.len() >= 3);
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let raw = queue("picks", &ids, &rows).unwrap();
        let mut session = open(&raw);
        let mut store = Memory::default();
        // The first `x` only asks; any other key cancels it.
        session.press(Key::Char('x'), &mut store);
        assert!(session.sentence().rejected.is_none());
        assert!(store.saved.is_empty());
        session.press(Key::Char('j'), &mut store);
        session.press(Key::Char('x'), &mut store);
        assert!(
            session.sentence().rejected.is_none(),
            "asked again, not done"
        );
        session.press(Key::Char('x'), &mut store);
        // Rejected with no tag, saved, and the session is on the next sentence.
        assert_eq!(session.at, 1);
        let text = store.saved.last().cloned().unwrap();
        assert_eq!(text.matches("# owner_rejected = ").count(), 1);
        // The rest are tagged; the rejected one is skipped over and never needs its blanks done.
        for _ in 1..rows.len() {
            fill(&mut session, &mut store);
            session.press(Key::Char('n'), &mut store);
        }
        let done = store.saved.last().cloned().unwrap();
        let (text, moved, rejected) = own("q", &done, "owner.conllu", None).unwrap();
        assert_eq!((moved, rejected), (rows.len() - 1, 1));
        assert_eq!(text.matches("# sent_id = o").count(), rows.len() - 1);
        assert!(!text.contains("owner_rejected"));
        assert!(!text.contains(&format!("# pick_id = {}\n", rows[0].id)));
        assert!(text.contains(&format!("# pick_id = {}\n", rows[1].id)));
        // Everything rejected is nothing to move.
        let all = done.replace("# owner_reviewed", "# owner_rejected");
        let error = own("q", &all, "owner.conllu", None).unwrap_err();
        assert!(error.to_string().contains("nothing to move"), "{error}");
    }

    #[test]
    fn a_second_queue_is_added_after_the_first_with_the_next_ids() {
        let rows = ranked();
        let (first, ..) = own("q", &reviewed(&rows[..2]), "owner.conllu", None).unwrap();
        let (both, moved, _) =
            own("q", &reviewed(&rows[2..]), "owner.conllu", Some(&first)).unwrap();
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

    #[test]
    fn each_committed_queue_opens_in_the_review_and_has_a_reason_for_every_sentence() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/gold/queue");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.map(|entry| entry.unwrap().path()) {
            if entry.extension().is_none_or(|ext| ext != "conllu") {
                continue;
            }
            let shown = entry.display().to_string();
            let text = std::fs::read_to_string(&entry).unwrap();
            crate::review::Session::open(&shown, text.clone(), "2026-10-06").unwrap();
            let blocks = conllu::read(&shown, &text).unwrap();
            for block in &blocks {
                // A pick the owner rejected stays in the queue, marked, with its reason.
                for key in [
                    "source",
                    "repo",
                    "pick_id",
                    "license",
                    "tier",
                    "exam.context",
                ] {
                    assert!(block.comment(key).is_some(), "{shown}: no {key}");
                }
                let source = &block.comment("source").unwrap().value;
                assert!(
                    source.contains(" bytes "),
                    "{shown}: `source` is `<file> bytes a-b`"
                );
                assert_eq!(
                    block.comment("pick_id").unwrap().value,
                    block.comment("sent_id").unwrap().value
                );
            }
            crate::exclude::Repos::gold(&shown, &text).unwrap();
            let reasons = entry.with_extension("reasons.tsv");
            let picks = read_picks("reasons", &std::fs::read_to_string(&reasons).unwrap()).unwrap();
            let ids: Vec<&str> = blocks
                .iter()
                .map(|block| block.comment("sent_id").unwrap().value.as_str())
                .collect();
            assert_eq!(ids, picks, "{shown}");
        }
    }
}
