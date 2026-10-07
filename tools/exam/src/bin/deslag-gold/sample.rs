//! The sampler: 450 sentences from the corpus, 150 per tier, by a fixed seed.
//!
//! A sentence is what deslag's own reader finds: `Document::markdown` splits a file into blocks,
//! tokens and sentences, so a list item, a heading and a table cell each yield sentences, and each
//! sentence's tokens are the ones a gold file's lines name. The sentence is not re-split anywhere
//! else, and a sentence whose tokens the exam would not read back as they are (a form that
//! `Token::split` no longer splits as its kind) is not sampled.
//!
//! How a draw is made, per tier, so a rerun with the same seed and corpus gives the same sample:
//!
//! 1. The tier's files are sorted by path and shuffled with the seed.
//! 2. Files are read in that order. From each file at most `per_file` eligible sentences are
//!    taken, and from each repository at most `per_repo`, so no one document or project fills the
//!    tier. A sentence is taken only if its context still has room in the quota of its split and
//!    no earlier sentence of the whole sample has the same text.
//! 3. A file is dev or holdout, never both: the first time it gives a sentence it goes to one
//!    split, chosen by how much each still needs, and all it gives goes there. The two splits
//!    share no source file.
//! 4. The draw stops when every quota of both splits is full. A corpus that cannot fill one is an
//!    error naming the tier, context and split.
//!
//! Each tier's quota is split by context (prose, list item, heading, table cell) so the exam's
//! context strata are not left to chance. The holdout takes the same share of every cell, so it
//! has the same mix as the dev set. The 450 are shuffled and numbered `g0001` to `g0450`: the id
//! and the order say nothing of tier, split or file.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use deslag::Document;
use deslag::document::{Body, Token, TokenKind};
use deslag_corpus::stats::Rng;
use deslag_exam::error::Error;
use deslag_exam::gold::{Gold, Split, Tier};
use deslag_exam::tagger::Context;

use crate::data::{Manifest, Meta, Provenance, Sample, Sent, Tok};
use crate::exclude::Texts;
use crate::problems::Problems;

/// The seed of the real sample: the ASCII bytes of `deslag`.
pub const SEED: u64 = 0x6465_736C_6167;

/// What the draw is made with.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The seed. Equal seeds over an equal corpus give equal samples.
    pub seed: u64,
    /// How many sentences each tier gives of each context, in the order of [`Context::ALL`].
    pub quotas: [usize; 4],
    /// Quotas of each tier in the order of [`Tier::ALL`], when they are not all `quotas`; a draw
    /// for labelling, whose mixed tier is far smaller than the other two.
    pub tiers: Option<[[usize; 4]; 3]>,
    /// How many of each tier's sentences are holdout; the rest are dev.
    pub holdout: usize,
    /// The most sentences taken from one file.
    pub per_file: usize,
    /// The most sentences taken from one repository, in one tier.
    pub per_repo: usize,
    /// The fewest word tokens a sentence may have.
    pub min_words: usize,
    /// The most tokens a sentence may have.
    pub max_tokens: usize,
}

impl Settings {
    /// The quotas of the tier at `tier_at` in [`Tier::ALL`].
    pub fn quotas_of(&self, tier_at: usize) -> [usize; 4] {
        self.tiers.map_or(self.quotas, |tiers| tiers[tier_at])
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            seed: SEED,
            quotas: [90, 30, 15, 15],
            tiers: None,
            holdout: 50,
            per_file: 2,
            per_repo: 4,
            min_words: 2,
            max_tokens: 60,
        }
    }
}

/// A corpus file to draw from.
#[derive(Debug, Clone)]
pub struct File<'a> {
    /// Its path, as the loader names it.
    pub path: String,
    /// Who wrote it.
    pub tier: Tier,
    /// Its repository, `owner/name`.
    pub repo: String,
    /// The licence it is quoted under.
    pub license: String,
    /// The sha256 of its bytes, in lowercase hex, which its sidecar records.
    pub sha256: String,
    /// What a draw for labelling records about it.
    pub provenance: Provenance,
    /// Its text.
    pub text: &'a str,
}

/// A sentence that may be sampled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Where it is in the file.
    pub range: Range<usize>,
    /// The block it is in.
    pub context: Context,
    /// Its tokens.
    pub toks: Vec<Tok>,
}

/// Why sentences were left out, counted over the files read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Skipped {
    /// Fewer words than the settings ask for.
    pub few_words: usize,
    /// More tokens than the settings allow.
    pub long: usize,
    /// Not English by the cheap check.
    pub not_english: usize,
    /// A token the exam would not read back as it is.
    pub unreadable: usize,
    /// Its text is the text of a sentence already taken.
    pub duplicate: usize,
    /// Its normalised text is that of a dev, holdout, owner or queue sentence; a draw for
    /// labelling only.
    pub gold_text: usize,
    /// Its normalised text is that of a sentence of an earlier draw; a draw for labelling only.
    pub earlier: usize,
}

/// What the draw is for.
#[derive(Clone, Copy)]
pub enum Mode<'a> {
    /// The gold set: dev and holdout, with no check against other texts.
    Gold,
    /// Sentences to label.
    Labelling(Labelling<'a>),
}

/// What a draw for labelling is made under: every row is `unlabelled` and none is holdout; ids
/// are `prefix` and four digits; a sentence with the text of one of `gold` or `earlier` is left
/// out.
#[derive(Clone, Copy)]
pub struct Labelling<'a> {
    /// The ids' prefix.
    pub prefix: &'a str,
    /// The texts of dev, holdout, owner and the queues.
    pub gold: &'a Texts,
    /// The texts of earlier draws.
    pub earlier: &'a Texts,
}

pub use deslag_exam::skeleton::context_of;

/// Whitespace runs as one space, the ends trimmed. A form with a line break in it would break
/// the CoNLL-U line it is written on.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Very common English words: a sentence of six words or more that has too few of them is not
/// taken for English.
const COMMON: &[&str] = &[
    "the", "a", "an", "to", "of", "and", "or", "is", "are", "in", "for", "it", "this", "that",
    "you", "with", "be", "on", "as", "if", "not", "can", "will", "we", "i", "your", "by", "from",
    "at", "was", "have", "has", "do", "when", "use", "so", "but", "which", "all", "these", "those",
    "there", "they", "their", "then", "than", "into", "more", "also", "any", "each", "only", "its",
    "our", "what", "how", "should", "would", "could", "may", "must", "no", "one", "such", "other",
    "some", "using", "used", "been", "were", "does", "did", "get", "make", "while", "my", "me",
    "he", "she", "his", "her", "who", "after", "before", "about", "up", "out", "over",
];

/// A cheap check that a sentence is English: no more than a tenth of its words have a non-ASCII
/// letter, and a sentence of six words or more has very common English words in at least an
/// eighth of them. Short ones, headings and labels, pass on the first test alone.
fn looks_english(toks: &[Tok]) -> bool {
    let words: Vec<&Tok> = toks.iter().filter(|tok| tok.is_word()).collect();
    let foreign = words
        .iter()
        .filter(|tok| tok.form.chars().any(|c| c.is_alphabetic() && !c.is_ascii()))
        .count();
    if foreign * 10 > words.len() {
        return false;
    }
    let common = words
        .iter()
        .filter(|tok| COMMON.contains(&tok.form.to_lowercase().as_str()))
        .count();
    words.len() < 6 || (common > 0 && common * 8 >= words.len())
}

/// Whether the exam, reading these tokens back from gold lines, finds each one as it is: a word,
/// number, mark, symbol or URL must split into exactly one token of its kind with its text.
fn readable(toks: &[Tok]) -> bool {
    toks.iter().all(|tok| {
        if !matches!(
            tok.kind,
            TokenKind::Word
                | TokenKind::Number
                | TokenKind::Punctuation
                | TokenKind::Symbol
                | TokenKind::Url
        ) {
            return true;
        }
        let split = Token::split(&tok.form);
        matches!(split.as_slice(), [one] if one.kind == tok.kind && one.text == tok.form)
    })
}

/// The sentences of `text`, read as Markdown, that may be sampled, in the order of the file.
/// Those left out are counted in `skipped`.
pub fn candidates(text: &str, settings: &Settings, skipped: &mut Skipped) -> Vec<Candidate> {
    let document = Document::markdown(text);
    let mut found = Vec::new();
    for (block, ancestors) in document.walk() {
        if !matches!(block.body, Body::Text { .. }) {
            continue;
        }
        let context = context_of(block, &ancestors);
        for sentence in document.sentences_of(block) {
            let tokens = &document.tokens[sentence.tokens.clone()];
            let toks: Vec<Tok> = tokens
                .iter()
                .enumerate()
                .map(|(index, token)| Tok {
                    form: tidy(&token.text),
                    kind: token.kind,
                    joined: tokens
                        .get(index + 1)
                        .is_some_and(|next| next.range.start == token.range.end),
                })
                .collect();
            if toks.len() > settings.max_tokens {
                skipped.long += 1;
            } else if toks.iter().filter(|tok| tok.is_word()).count() < settings.min_words {
                skipped.few_words += 1;
            } else if toks.iter().any(|tok| tok.form.is_empty()) || !readable(&toks) {
                skipped.unreadable += 1;
            } else if !looks_english(&toks) {
                skipped.not_english += 1;
            } else {
                found.push(Candidate {
                    range: sentence.range.clone(),
                    context,
                    toks,
                });
            }
        }
    }
    found
}

/// What a sentence's text is for telling two apart: lower case, one space between words.
fn key(toks: &[Tok]) -> String {
    toks.iter()
        .map(|tok| tok.form.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A generator for `salt`, scrambled from `seed` so that small seeds do not give correlated draws.
fn rng(seed: u64, salt: u64) -> Rng {
    let mut z = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    Rng::new(z ^ (z >> 31))
}

/// Puts `items` in an order the generator picks.
fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for at in (1..items.len()).rev() {
        items.swap(at, rng.below(at + 1));
    }
}

/// Shares `total` among `weights` in proportion, by the largest remainder, no share past its
/// weight. The shares add up to `total` when the weights do.
pub fn apportion(total: usize, weights: &[usize]) -> Vec<usize> {
    let sum: usize = weights.iter().sum();
    if sum == 0 {
        return vec![0; weights.len()];
    }
    let mut shares: Vec<usize> = weights
        .iter()
        .map(|weight| (total * weight / sum).min(*weight))
        .collect();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by_key(|&at| std::cmp::Reverse((total * weights[at]) % sum));
    let mut left = total.saturating_sub(shares.iter().sum());
    for at in order {
        if left == 0 {
            break;
        }
        if shares[at] < weights[at] {
            shares[at] += 1;
            left -= 1;
        }
    }
    shares
}

/// A draw that could not fill a quota.
#[derive(Debug)]
pub struct Short {
    /// The tier.
    pub tier: Tier,
    /// The context.
    pub context: Context,
    /// The split; none for a draw for labelling.
    pub split: Option<Split>,
    /// How many were asked for.
    pub wanted: usize,
    /// How many there were.
    pub got: usize,
}

impl fmt::Display for Short {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the {} tier has {} eligible {} sentences for {} under the limits per file and per repository, and {} were asked for",
            self.tier.name(),
            self.got,
            self.context.name(),
            self.split.map_or("unlabelled", Split::name),
            self.wanted
        )
    }
}

/// What a draw made.
#[derive(Debug)]
pub struct Outcome {
    /// The sample.
    pub sample: Sample,
    /// How many files each tier read to fill its quotas.
    pub files_read: [usize; 3],
    /// What was left out of the files read.
    pub skipped: Skipped,
}

/// A sentence taken, before it is numbered.
struct Pick {
    tier: Tier,
    file: usize,
    cand: Candidate,
    split: Option<Split>,
}

/// The split of the picks of `split`, 0 dev and 1 holdout; none in a draw for labelling.
fn split_of(split: usize, mode: Mode<'_>) -> Option<Split> {
    match (split, mode) {
        (_, Mode::Labelling(_)) => None,
        (0, Mode::Gold) => Some(Split::Dev),
        _ => Some(Split::Holdout),
    }
}

/// Draws the sample from `files`. `corpus` says in the manifest what they are. A draw for
/// labelling takes `settings.holdout` as 0 and leaves out the sentences the gold text has.
pub fn draw(
    files: &[File<'_>],
    corpus: &str,
    settings: &Settings,
    mode: Mode<'_>,
) -> Result<Outcome, Short> {
    let mut skipped = Skipped::default();
    let mut files_read = [0usize; 3];
    let mut picks: Vec<Pick> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for (tier_at, tier) in Tier::ALL.iter().copied().enumerate() {
        let mut order: Vec<usize> = (0..files.len())
            .filter(|&at| files[at].tier == tier)
            .collect();
        order.sort_by(|&a, &b| files[a].path.cmp(&files[b].path));
        let mut rng = rng(settings.seed, tier_at as u64 + 1);
        shuffle(&mut order, &mut rng);

        // What each split of this tier takes of each context: the same share of every cell is
        // holdout, so both splits have the tier's mix. Index 0 is dev and 1 is holdout.
        let quotas = settings.quotas_of(tier_at);
        let held = apportion(settings.holdout, &quotas);
        let mut want = [[0usize; 4]; 2];
        for cell in 0..4 {
            want[1][cell] = held[cell];
            want[0][cell] = quotas[cell] - held[cell].min(quotas[cell]);
        }
        let mut have = [[0usize; 4]; 2];
        let mut repos: BTreeMap<&str, usize> = BTreeMap::new();
        let open = |have: &[[usize; 4]; 2], split: usize, cell: usize| {
            have[split][cell] < want[split][cell]
        };
        for &at in &order {
            if (0..2).all(|split| (0..4).all(|cell| !open(&have, split, cell))) {
                break;
            }
            let file = &files[at];
            if repos.get(file.repo.as_str()).copied().unwrap_or(0) >= settings.per_repo {
                continue;
            }
            files_read[tier_at] += 1;
            let mut found = candidates(file.text, settings, &mut skipped);
            if let Mode::Labelling(labelling) = mode {
                let before = found.len();
                found.retain(|cand| !labelling.gold.has(&cand.toks));
                skipped.gold_text += before - found.len();
                let before = found.len();
                found.retain(|cand| !labelling.earlier.has(&cand.toks));
                skipped.earlier += before - found.len();
            }
            shuffle(&mut found, &mut rng);
            let cell_of = |cand: &Candidate| {
                Context::ALL
                    .iter()
                    .position(|context| *context == cand.context)
                    .unwrap_or(0)
            };
            // A file is dev or holdout, never both, so the two splits share no source. Of the
            // splits the file has something to give, it goes to one chosen by how much each
            // still needs.
            let usable = |split: usize| {
                found.iter().any(|cand| {
                    open(&have, split, cell_of(cand)) && !seen.contains(&key(&cand.toks))
                })
            };
            let split = match (usable(0), usable(1)) {
                (false, false) => continue,
                (true, false) => 0,
                (false, true) => 1,
                (true, true) => {
                    let need = |split: usize| {
                        (0..4)
                            .map(|cell| want[split][cell] - have[split][cell])
                            .sum::<usize>()
                    };
                    usize::from(rng.below(need(0) + need(1)) < need(1))
                }
            };
            let mut from_file = 0;
            for cand in found {
                let used = repos.entry(file.repo.as_str()).or_default();
                if from_file >= settings.per_file || *used >= settings.per_repo {
                    break;
                }
                let cell = cell_of(&cand);
                if !open(&have, split, cell) {
                    continue;
                }
                if !seen.insert(key(&cand.toks)) {
                    skipped.duplicate += 1;
                    continue;
                }
                *used += 1;
                from_file += 1;
                have[split][cell] += 1;
                picks.push(Pick {
                    tier,
                    file: at,
                    cand,
                    split: split_of(split, mode),
                });
            }
        }
        for split in 0..2 {
            for (cell, context) in Context::ALL.iter().enumerate() {
                if have[split][cell] < want[split][cell] {
                    return Err(Short {
                        tier,
                        context: *context,
                        split: split_of(split, mode),
                        wanted: want[split][cell],
                        got: have[split][cell],
                    });
                }
            }
        }
    }

    shuffle(&mut picks, &mut rng(settings.seed, 0));
    let mut sents = Vec::with_capacity(picks.len());
    let mut rows = Vec::with_capacity(picks.len());
    for (index, pick) in picks.into_iter().enumerate() {
        let prefix = match mode {
            Mode::Gold => "g",
            Mode::Labelling(labelling) => labelling.prefix,
        };
        let id = format!("{prefix}{:04}", index + 1);
        let file = &files[pick.file];
        let mut toks = pick.cand.toks;
        if let Some(last) = toks.last_mut() {
            // Nothing follows the last token inside its sentence, as the exam's skeleton has it.
            last.joined = false;
        }
        rows.push((
            id.clone(),
            Meta {
                split: pick.split,
                tier: pick.tier,
                context: pick.cand.context,
                file: file.path.clone(),
                repo: file.repo.clone(),
                license: file.license.clone(),
                range: pick.cand.range,
                provenance: matches!(mode, Mode::Labelling(_)).then(|| file.provenance.clone()),
            },
        ));
        sents.push(Sent { id, toks });
    }
    let quotas_text = |quotas: [usize; 4]| {
        Context::ALL
            .iter()
            .zip(quotas)
            .map(|(context, quota)| format!("{}:{quota}", context.name()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let quotas = match settings.tiers {
        None => quotas_text(settings.quotas),
        Some(tiers) => Tier::ALL
            .iter()
            .zip(tiers)
            .map(|(tier, quotas)| format!("{} {}", tier.name(), quotas_text(quotas)))
            .collect::<Vec<_>>()
            .join("; "),
    };
    let header = [
        ("seed", format!("{:#x}", settings.seed)),
        ("corpus", corpus.to_string()),
        ("per tier quotas", quotas),
        match mode {
            Mode::Gold => ("holdout per tier", settings.holdout.to_string()),
            Mode::Labelling(labelling) => (
                "draw",
                format!(
                    "for labelling, split unlabelled, ids {}0001 on",
                    labelling.prefix
                ),
            ),
        },
        (
            "limits",
            format!(
                "per file {}, per repo {}, words at least {}, tokens at most {}",
                settings.per_file, settings.per_repo, settings.min_words, settings.max_tokens
            ),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    Ok(Outcome {
        sample: Sample {
            sents,
            manifest: Manifest { header, rows },
        },
        files_read,
        skipped,
    })
}

/// What a tier could give at most under the caps: the sentences of each context, drawn on their
/// own, and of all contexts together. Per repository, a file gives at most `per_file` and the
/// repository `per_repo`; a text is counted once, for the first file in path order that has it.
/// It is an upper bound: a quota of several contexts at once may be met by fewer.
pub fn capacity(files: &[File<'_>], settings: &Settings, mode: Mode<'_>) -> [[usize; 5]; 3] {
    let mut out = [[0usize; 5]; 3];
    let mut skipped = Skipped::default();
    for (tier_at, tier) in Tier::ALL.iter().copied().enumerate() {
        let mut order: Vec<&File<'_>> = files.iter().filter(|file| file.tier == tier).collect();
        order.sort_by(|a, b| a.path.cmp(&b.path));
        let mut seen: BTreeSet<String> = BTreeSet::new();
        // Per repository and per column (the four contexts, then all), what its files give.
        let mut repos: BTreeMap<&str, [usize; 5]> = BTreeMap::new();
        for file in order {
            let mut found = candidates(file.text, settings, &mut skipped);
            if let Mode::Labelling(labelling) = mode {
                found.retain(|cand| {
                    !labelling.gold.has(&cand.toks) && !labelling.earlier.has(&cand.toks)
                });
            }
            found.retain(|cand| seen.insert(key(&cand.toks)));
            let mut per_context = [0usize; 5];
            for cand in &found {
                let cell = Context::ALL
                    .iter()
                    .position(|context| *context == cand.context)
                    .unwrap_or(0);
                per_context[cell] += 1;
                per_context[4] += 1;
            }
            let held = repos.entry(file.repo.as_str()).or_default();
            for column in 0..5 {
                held[column] += per_context[column].min(settings.per_file);
            }
        }
        for held in repos.values() {
            for column in 0..5 {
                out[tier_at][column] += held[column].min(settings.per_repo);
            }
        }
    }
    out
}

/// The gold file the exam would read for `sents` if every word were a noun: the sample's own
/// tokens, as lines. It exists to prove the exam reads them back.
fn as_gold(sents: &[Sent]) -> String {
    let mut text = String::from("# exam.tokens = deslag\n");
    for sent in sents {
        text.push_str(&format!("# sent_id = {}\n", sent.id));
        for (index, tok) in sent.toks.iter().enumerate() {
            let upos = match tok.kind {
                TokenKind::Word => "NOUN",
                kind => crate::data::upos_of_kind(kind),
            };
            text.push_str(&crate::data::line(
                index,
                &tok.form,
                upos,
                "_",
                &crate::data::misc(tok, Some("agree")),
            ));
        }
        text.push('\n');
    }
    text
}

/// Checks that the exam reads every sentence of `sents` back with the tokens it has, and aligns
/// each word to its own token. A sentence it does not read as it is gives a problem naming it.
pub fn check_with_exam(sents: &[Sent]) -> Result<(), Problems> {
    let gold = Gold::parse("the sample", "the sample", &as_gold(sents))?;
    let mut problems = Vec::new();
    for (sent, read) in sents.iter().zip(&gold.sentences) {
        let tokens = read.tokens();
        let same = tokens.len() == sent.toks.len()
            && tokens
                .iter()
                .zip(&sent.toks)
                .all(|(token, tok)| token.kind == tok.kind && token.text == tok.form);
        if !same {
            problems.push(Problems::sentence(
                "the sample",
                &sent.id,
                "the exam reads other tokens than the sample holds",
            ));
            continue;
        }
        let alignment = read.align(&tokens);
        if !alignment.unalignable.is_empty() {
            problems.push(Problems::sentence(
                "the sample",
                &sent.id,
                "a token the exam cannot align",
            ));
        }
        if read.text != sent.text() {
            problems.push(Problems::sentence(
                "the sample",
                &sent.id,
                "the exam builds other text than the sample's",
            ));
        }
    }
    Problems::check(problems, ())
}

/// Counts of the sample by tier, context and split, for the printed report.
pub struct Counts<'a>(pub &'a Outcome);

impl fmt::Display for Counts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let outcome = self.0;
        let mut table: BTreeMap<(usize, usize, usize), usize> = BTreeMap::new();
        for (_, meta) in &outcome.sample.manifest.rows {
            let tier = Tier::ALL.iter().position(|t| *t == meta.tier).unwrap_or(0);
            let context = Context::ALL
                .iter()
                .position(|c| *c == meta.context)
                .unwrap_or(0);
            let split = usize::from(meta.split == Some(Split::Holdout));
            *table.entry((tier, context, split)).or_default() += 1;
        }
        writeln!(
            f,
            "sampled {} sentences ({} words, {} tokens)",
            outcome.sample.sents.len(),
            outcome.sample.sents.iter().map(Sent::words).sum::<usize>(),
            outcome
                .sample
                .sents
                .iter()
                .map(|s| s.toks.len())
                .sum::<usize>(),
        )?;
        writeln!(
            f,
            "{:<8}{:<12}{:>6}{:>9}",
            "tier", "context", "dev", "holdout"
        )?;
        for (tier_at, tier) in Tier::ALL.iter().enumerate() {
            for (context_at, context) in Context::ALL.iter().enumerate() {
                let count = |split| {
                    table
                        .get(&(tier_at, context_at, split))
                        .copied()
                        .unwrap_or(0)
                };
                writeln!(
                    f,
                    "{:<8}{:<12}{:>6}{:>9}",
                    tier.name(),
                    context.name(),
                    count(0),
                    count(1)
                )?;
            }
        }
        for (tier, read) in Tier::ALL.iter().zip(outcome.files_read) {
            writeln!(f, "files read for {}: {read}", tier.name())?;
        }
        let s = &outcome.skipped;
        write!(
            f,
            "left out of those files: {} too short, {} too long, {} not English, {} unreadable by the exam, {} duplicate",
            s.few_words, s.long, s.not_english, s.unreadable, s.duplicate
        )?;
        Ok(())
    }
}

/// Turns a problem with the draw into the error the binary prints.
impl From<Short> for Error {
    fn from(short: Short) -> Error {
        Error::load(
            "the corpus",
            deslag_exam::error::Place::File,
            short.to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Markdown file with each context in it, and a few sentences that must be left out.
    const DOC: &str = "\
# Install the CLI

Run the build first. Then you can use the tool with your own files.

- Waits for the web server to start
- Yes

| Name | Meaning |
| --- | --- |
| Default | The value that the tool uses |

```sh
cargo build
```

> Quoted prose is prose, so tag it as such.

Ok.
";

    fn found(text: &str) -> (Vec<Candidate>, Skipped) {
        let mut skipped = Skipped::default();
        let found = candidates(text, &Settings::default(), &mut skipped);
        (found, skipped)
    }

    fn texts(found: &[Candidate]) -> Vec<(Context, String)> {
        found
            .iter()
            .map(|c| {
                let sent = Sent {
                    id: String::new(),
                    toks: c.toks.clone(),
                };
                (c.context, sent.text())
            })
            .collect()
    }

    #[test]
    fn each_context_yields_its_sentences_and_a_code_block_yields_none() {
        let (found, skipped) = found(DOC);
        let got = texts(&found);
        let has = |context: Context, text: &str| got.contains(&(context, text.to_string()));
        assert!(has(Context::Heading, "Install the CLI"), "{got:?}");
        assert!(has(Context::Prose, "Run the build first."), "{got:?}");
        assert!(
            has(
                Context::Prose,
                "Then you can use the tool with your own files."
            ),
            "{got:?}"
        );
        assert!(
            has(Context::ListItem, "Waits for the web server to start"),
            "{got:?}"
        );
        assert!(
            has(Context::TableCell, "The value that the tool uses"),
            "{got:?}"
        );
        assert!(
            has(Context::Prose, "Quoted prose is prose, so tag it as such."),
            "a quote is prose: {got:?}"
        );
        assert!(
            !got.iter().any(|(_, text)| text.contains("cargo build")),
            "no code block: {got:?}"
        );
        // `Yes`, `Name`, `Meaning`, `Default` and `Ok.` are one word each.
        assert!(skipped.few_words >= 4, "{skipped:?}");
        assert!(!got.iter().any(|(_, text)| text == "Yes"));
    }

    #[test]
    fn a_sentence_keeps_the_source_range_the_reader_gave_it() {
        let (found, _) = found("Intro text.\n\nRun the build first.\n");
        let run = found
            .iter()
            .find(|c| c.toks[0].form == "Run")
            .expect("the sentence");
        assert_eq!(
            &"Intro text.\n\nRun the build first.\n"[run.range.clone()],
            "Run the build first."
        );
    }

    #[test]
    fn a_code_span_is_a_token_that_is_not_a_word() {
        let (found, _) = found("Run `cargo build` now, then stop.\n");
        let toks = &found[0].toks;
        assert_eq!(toks[1].kind, TokenKind::Code);
        assert_eq!(toks[1].form, "cargo build");
        assert!(toks[2].joined, "now is followed at once by the comma");
        assert!(!toks[0].joined);
    }

    #[test]
    fn long_sentences_and_foreign_ones_are_left_out_and_counted() {
        let long = vec!["word"; 61].join(" ");
        let foreign = "Der schnelle braune Fuchs springt über den faulen Hund und läuft weg";
        let english = "The quick brown fox jumps over the lazy dog and runs away.";
        let text = format!("{long}.\n\n{foreign}.\n\n{english}\n");
        let (found, skipped) = found(&text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(skipped.long, 1);
        assert_eq!(skipped.not_english, 1);
    }

    #[test]
    fn a_short_label_is_not_judged_by_its_common_words() {
        let (found, skipped) = found("# Release notes\n");
        assert_eq!(found.len(), 1);
        assert_eq!(skipped.not_english, 0);
    }

    #[test]
    fn the_context_rule_is_heading_then_cell_then_list_then_prose() {
        let text = "\
- A list item with a [link to the docs](https://example.com/docs) in it
  - A nested item inside the item above it

1. An item of a numbered list
";
        let (found, _) = found(text);
        assert!(
            found.iter().all(|c| c.context == Context::ListItem),
            "{found:?}"
        );
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn apportion_shares_by_the_largest_remainder() {
        assert_eq!(apportion(50, &[90, 30, 15, 15]), [30, 10, 5, 5]);
        assert_eq!(apportion(7, &[5, 3, 2]), [4, 2, 1]);
        assert_eq!(apportion(1, &[1, 1]), [1, 0]);
        assert_eq!(apportion(10, &[1, 1]), [1, 1], "no share past its weight");
        assert_eq!(
            apportion(5, &[2, 0, 0]),
            [2, 0, 0],
            "no share past its weight"
        );
        assert_eq!(apportion(3, &[0, 0]), [0, 0]);
        for total in 0..30 {
            let shares = apportion(total, &[7, 5, 3, 1]);
            assert_eq!(shares.iter().sum::<usize>(), total.min(16));
        }
    }

    /// A corpus of `files` files per tier, each with a few sentences of each context.
    fn corpus(files: usize) -> Vec<(String, Tier, String, String)> {
        let mut out = Vec::new();
        for tier in Tier::ALL.iter().copied() {
            for n in 0..files {
                let text = format!(
                    "# Heading number {n} of the {} tier\n\n\
                     Paragraph one of file {n} says that the tool works well for you.\n\n\
                     - List item {n} says that the tool runs on every platform\n\n\
                     | Key | Value |\n| --- | --- |\n| Cell {n} | The value of cell {n} is set by you |\n",
                    tier.name()
                );
                out.push((
                    format!("{}/repo{}/file{n}.md", tier.name(), n % 5),
                    tier,
                    format!("owner/repo{}-{}", n % 5, tier.name()),
                    text,
                ));
            }
        }
        out
    }

    fn files(owned: &[(String, Tier, String, String)]) -> Vec<File<'_>> {
        owned
            .iter()
            .map(|(path, tier, repo, text)| File {
                path: path.clone(),
                tier: *tier,
                repo: repo.clone(),
                license: "MIT".to_string(),
                sha256: crate::exclude::sha256_hex(text.as_bytes()),
                provenance: Provenance::default(),
                text,
            })
            .collect()
    }

    /// Settings small enough for the hand-made corpus.
    fn small() -> Settings {
        Settings {
            quotas: [4, 2, 2, 2],
            holdout: 4,
            per_file: 2,
            per_repo: 5,
            ..Settings::default()
        }
    }

    #[test]
    fn a_draw_fills_every_quota_and_splits_each_tier_the_same_way() {
        let owned = corpus(40);
        let outcome = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        let rows = &outcome.sample.manifest.rows;
        assert_eq!(rows.len(), 30);
        assert_eq!(outcome.sample.sents.len(), 30);
        for tier in Tier::ALL.iter().copied() {
            for (context, quota) in Context::ALL.iter().zip(small().quotas) {
                let in_cell: Vec<_> = rows
                    .iter()
                    .filter(|(_, m)| m.tier == tier && m.context == *context)
                    .collect();
                assert_eq!(in_cell.len(), quota, "{} {}", tier.name(), context.name());
            }
            let held = rows
                .iter()
                .filter(|(_, m)| m.tier == tier && m.split == Some(Split::Holdout))
                .count();
            assert_eq!(held, 4, "{}", tier.name());
        }
        // A holdout of 4 of 10 takes one of every cell, by the largest remainder, in every tier.
        let held_prose = rows
            .iter()
            .filter(|(_, m)| m.split == Some(Split::Holdout) && m.context == Context::Prose)
            .count();
        assert_eq!(held_prose, 3);
    }

    #[test]
    fn the_same_seed_gives_the_same_sample_and_another_seed_gives_another() {
        let owned = corpus(40);
        let first = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        let again = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        assert_eq!(first.sample.sents, again.sample.sents);
        assert_eq!(first.sample.manifest, again.sample.manifest);
        let other = Settings { seed: 7, ..small() };
        let other = draw(&files(&owned), "hand-made", &other, Mode::Gold).unwrap();
        assert_ne!(first.sample.manifest.rows, other.sample.manifest.rows);
        // The order of the files given does not matter, since they are sorted by path first.
        let mut reversed = owned.clone();
        reversed.reverse();
        let reversed = draw(&files(&reversed), "hand-made", &small(), Mode::Gold).unwrap();
        assert_eq!(first.sample.sents, reversed.sample.sents);
    }

    #[test]
    fn a_draw_with_files_excluded_takes_nothing_from_them() {
        use crate::exclude::Exclusion;
        let owned = corpus(40);
        let first = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        let used: BTreeSet<&str> = first
            .sample
            .manifest
            .rows
            .iter()
            .map(|(_, meta)| meta.file.as_str())
            .collect();
        // Half the used files go by path and half by sha256, as a list may mix them.
        let list: String = used
            .iter()
            .enumerate()
            .map(|(at, path)| {
                if at % 2 == 0 {
                    format!("{path}\n")
                } else {
                    let text = &owned.iter().find(|row| row.0 == *path).unwrap().3;
                    format!("{}\t{path}\n", crate::exclude::sha256_hex(text.as_bytes()))
                }
            })
            .collect();
        let list = Exclusion::parse("list", &list).unwrap();
        let (kept, dropped) = list.apply("list", files(&owned)).unwrap();
        assert_eq!(dropped, used.len());
        let second = draw(&kept, "hand-made", &small(), Mode::Gold).unwrap();
        assert_eq!(second.sample.manifest.rows.len(), 30);
        for (_, meta) in &second.sample.manifest.rows {
            assert!(!used.contains(meta.file.as_str()), "{}", meta.file);
        }
        // The draw over what is kept is itself repeatable.
        let again = draw(&kept, "hand-made", &small(), Mode::Gold).unwrap();
        assert_eq!(second.sample.manifest, again.sample.manifest);
    }

    #[test]
    fn ids_say_nothing_of_the_tier() {
        let owned = corpus(40);
        let outcome = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        let ids: Vec<&str> = outcome.sample.sents.iter().map(|s| s.id.as_str()).collect();
        let expect: Vec<String> = (1..=30).map(|n| format!("g{n:04}")).collect();
        assert_eq!(ids, expect.iter().map(String::as_str).collect::<Vec<_>>());
        let tiers: Vec<Tier> = outcome
            .sample
            .manifest
            .rows
            .iter()
            .map(|(_, m)| m.tier)
            .collect();
        let runs = tiers.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(
            runs > 6,
            "the tiers are mixed through the ids: {runs} changes"
        );
    }

    #[test]
    fn no_file_and_no_repository_gives_more_than_its_share() {
        let owned = corpus(40);
        let settings = Settings {
            per_file: 1,
            per_repo: 3,
            ..small()
        };
        let outcome = draw(&files(&owned), "hand-made", &settings, Mode::Gold).unwrap();
        let mut by_file: BTreeMap<&str, usize> = BTreeMap::new();
        let mut by_repo: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, meta) in &outcome.sample.manifest.rows {
            *by_file.entry(meta.file.as_str()).or_default() += 1;
            *by_repo.entry(meta.repo.as_str()).or_default() += 1;
        }
        assert!(by_file.values().all(|n| *n <= 1), "{by_file:?}");
        assert!(by_repo.values().all(|n| *n <= 3), "{by_repo:?}");
    }

    #[test]
    fn no_file_gives_sentences_to_both_splits() {
        let owned = corpus(40);
        for seed in [1, 2, 3, 4, 5] {
            let settings = Settings { seed, ..small() };
            let outcome = draw(&files(&owned), "hand-made", &settings, Mode::Gold).unwrap();
            let mut split_of: BTreeMap<&str, Option<Split>> = BTreeMap::new();
            for (_, meta) in &outcome.sample.manifest.rows {
                let first = *split_of.entry(meta.file.as_str()).or_insert(meta.split);
                assert_eq!(first, meta.split, "{} is in both splits", meta.file);
            }
            let held = outcome
                .sample
                .manifest
                .rows
                .iter()
                .filter(|(_, m)| m.split == Some(Split::Holdout))
                .count();
            assert_eq!(held, 12, "four of each of the three tiers");
        }
    }

    /// `files(owned)` with each file's provenance made from its path.
    fn files_with_provenance(owned: &[(String, Tier, String, String)]) -> Vec<File<'_>> {
        let mut found = files(owned);
        for file in &mut found {
            file.provenance = Provenance {
                commit: format!("commit-of-{}", file.path),
                url: format!("https://example.test/{}", file.path),
                sha256: file.sha256.clone(),
                model: if file.tier == Tier::Llm {
                    "a-model".to_string()
                } else {
                    String::new()
                },
                model_license: if file.tier == Tier::Llm {
                    "Apache-2.0".to_string()
                } else {
                    String::new()
                },
            };
        }
        found
    }

    fn none() -> Texts {
        Texts::default()
    }

    /// A draw for labelling with ids `p0001` on that avoids the texts of `gold` and `earlier`.
    fn labelling_of<'a>(gold: &'a Texts, earlier: &'a Texts) -> Mode<'a> {
        Mode::Labelling(Labelling {
            prefix: "p",
            gold,
            earlier,
        })
    }

    #[test]
    fn a_draw_for_labelling_has_no_holdout_drops_gold_text_and_records_where_each_came_from() {
        let owned = corpus(40);
        // Gold has the heading of every even file of every tier, and a sentence in other case and
        // spacing is the same sentence.
        let gold = Texts::of(Tier::ALL.iter().flat_map(|tier| {
            (0..40)
                .step_by(2)
                .map(move |n| format!("HEADING number {n}   of the {} tier", tier.name()))
        }));
        let settings = Settings {
            holdout: 0,
            ..small()
        };
        let drawn = |settings: &Settings| {
            draw(
                &files_with_provenance(&owned),
                "hand-made",
                settings,
                labelling_of(&gold, &none()),
            )
            .unwrap()
        };
        let outcome = drawn(&settings);
        let rows = &outcome.sample.manifest.rows;
        assert_eq!(rows.len(), 3 * (4 + 2 + 2 + 2));
        assert!(rows.iter().all(|(_, m)| m.split.is_none()));
        assert!(outcome.skipped.gold_text > 0);
        for (sent, (_, meta)) in outcome.sample.sents.iter().zip(rows) {
            assert!(!gold.has(&sent.toks), "{}", sent.text());
            let from = meta.provenance.as_ref().unwrap();
            assert_eq!(from.commit, format!("commit-of-{}", meta.file));
            assert_eq!(from.model.is_empty(), meta.tier != Tier::Llm);
        }
        // The same seed gives the same bytes, manifest included.
        let again = drawn(&settings);
        assert_eq!(
            outcome.sample.manifest.render(),
            again.sample.manifest.render()
        );
        let manifest = outcome.sample.manifest.render();
        assert!(
            manifest
                .contains("\tsource_commit\tsource_url\tcontent_sha256\tmodel\tmodel_license\n")
        );
        assert!(manifest.contains("\tunlabelled\t"));
        // A gold draw has the eight columns alone.
        let gold_draw = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        assert!(gold_draw.sample.manifest.render().contains("\tbytes\n"));
        // A gold file the draw would have taken everything from leaves it short, never silent.
        let all = Texts::of(Tier::ALL.iter().flat_map(|tier| {
            (0..40).map(move |n| format!("Heading number {n} of the {} tier", tier.name()))
        }));
        let short = draw(
            &files(&owned),
            "hand-made",
            &settings,
            labelling_of(&all, &none()),
        )
        .unwrap_err();
        assert_eq!(short.context, Context::Heading);
        assert_eq!(short.split, None);
    }

    #[test]
    fn a_draw_has_its_own_ids_and_never_repeats_the_text_of_an_earlier_draw() {
        let owned = corpus(40);
        let settings = Settings {
            holdout: 0,
            ..small()
        };
        let first = draw(
            &files(&owned),
            "hand-made",
            &settings,
            labelling_of(&none(), &none()),
        )
        .unwrap();
        assert!(first.sample.sents.iter().all(|sent| {
            sent.id.starts_with('p') && sent.id[1..].bytes().all(|b| b.is_ascii_digit())
        }));
        assert_eq!(first.sample.sents[0].id, "p0001");
        assert_eq!(first.sample.manifest.rows[0].0, "p0001");
        let earlier = Texts::of(first.sample.sents.iter().map(Sent::text));
        let second = draw(
            &files(&owned),
            "hand-made",
            &settings,
            labelling_of(&none(), &earlier),
        )
        .unwrap();
        assert!(second.skipped.earlier > 0);
        for sent in &second.sample.sents {
            assert!(!earlier.has(&sent.toks), "{}", sent.text());
        }
        // A gold draw's ids are as they were.
        let gold_draw = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        assert_eq!(gold_draw.sample.sents[0].id, "g0001");
    }

    #[test]
    fn capacity_counts_what_the_caps_allow_a_context_at_a_time() {
        let owned = corpus(40);
        let settings = Settings {
            per_file: 1,
            per_repo: 3,
            ..small()
        };
        let got = capacity(&files(&owned), &settings, Mode::Gold);
        let heading = Context::ALL
            .iter()
            .position(|context| *context == Context::Heading)
            .unwrap();
        // Each file has one heading, so one a file; 40 files in 5 repositories of 3 at most is 15.
        for row in got {
            assert_eq!(row[heading], 15, "{got:?}");
            // All together, a file gives one and a repository three.
            assert_eq!(row[4], 15, "{got:?}");
            assert!(row[0] >= 1);
        }
        let more = capacity(
            &files(&owned),
            &Settings {
                per_file: 2,
                per_repo: 100,
                ..small()
            },
            Mode::Gold,
        );
        assert!(more[0][4] > got[0][4]);
    }

    #[test]
    fn a_tier_may_have_quotas_of_its_own() {
        let owned = corpus(40);
        let settings = Settings {
            holdout: 0,
            tiers: Some([[4, 2, 2, 2], [2, 1, 1, 1], [1, 0, 0, 0]]),
            ..small()
        };
        let gold = Texts::default();
        let outcome = draw(
            &files(&owned),
            "hand-made",
            &settings,
            labelling_of(&gold, &none()),
        )
        .unwrap();
        let count = |tier: Tier| {
            outcome
                .sample
                .manifest
                .rows
                .iter()
                .filter(|(_, meta)| meta.tier == tier)
                .count()
        };
        assert_eq!(
            (count(Tier::Human), count(Tier::Llm), count(Tier::Mixed)),
            (10, 5, 1)
        );
        let header = outcome.sample.manifest.get("per tier quotas").unwrap();
        assert!(header.contains("llm prose:2 list-item:1"), "{header}");
    }

    #[test]
    fn a_skeleton_with_origin_is_what_the_exam_writes_for_the_same_sentence() {
        let text = "Edit main.rs, then call foo_bar in `cargo build` and run grep on it.\n";
        let mut skipped = Skipped::default();
        let found = candidates(text, &Settings::default(), &mut skipped);
        assert_eq!(found.len(), 1);
        let sents: Vec<Sent> = found
            .into_iter()
            .map(|cand| Sent {
                id: "g0001".to_string(),
                toks: cand.toks,
            })
            .collect();
        let with = crate::data::skeleton(&sents, |_| Some(Context::Prose), true);
        let exam = deslag_exam::skeleton::skeleton(
            &Gold::parse("the sample", "the sample", &as_gold(&sents)).unwrap(),
        );
        assert_eq!(with, exam);
        // The bare mentions carry an origin; the code span is a kind of token and carries none.
        let line_of = |form: &str| {
            with.lines()
                .find(|line| line.split('\t').nth(1) == Some(form))
                .unwrap_or_else(|| panic!("no line for {form}"))
        };
        assert!(
            line_of("main.rs").contains("Kind=Word|Origin=Path"),
            "{with}"
        );
        assert!(
            line_of("foo_bar").contains("Kind=Word|Origin=Symbol"),
            "{with}"
        );
        assert!(
            line_of("grep").contains("Kind=Word|Origin=Command"),
            "{with}"
        );
        assert!(line_of("cargo build").ends_with("\tKind=Code"), "{with}");
        assert!(line_of("Edit").ends_with("\tKind=Word"), "{with}");
        // A gold draw's skeleton leaves origin out, as it always has.
        let without = crate::data::skeleton(&sents, |_| Some(Context::Prose), false);
        assert!(!without.contains("Origin="), "{without}");
    }

    #[test]
    fn a_corpus_with_too_few_files_for_two_splits_names_the_split_that_is_short() {
        // One file per tier holds everything, but a file goes to one split only.
        let owned = corpus(1);
        let error = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap_err();
        assert!(error.to_string().contains("for "), "{error}");
        assert!(["dev", "holdout"].contains(&error.split.unwrap().name()));
    }

    #[test]
    fn a_text_is_never_sampled_twice() {
        // Every file has the same sentences, so the quotas can only be met by distinct ones.
        let same =
            "# Same heading text here\n\nThe same sentence says that the tool works for you.\n";
        let owned: Vec<(String, Tier, String, String)> = Tier::ALL
            .iter()
            .flat_map(|tier| {
                (0..6).map(move |n| {
                    (
                        format!("{}/r{n}/f.md", tier.name()),
                        *tier,
                        format!("o/r{n}"),
                        same.to_string(),
                    )
                })
            })
            .collect();
        let settings = Settings {
            quotas: [1, 0, 0, 0],
            holdout: 0,
            ..small()
        };
        let error = draw(&files(&owned), "hand-made", &settings, Mode::Gold).unwrap_err();
        // One tier took the sentence, and the next could not take it again.
        assert_eq!(error.tier, Tier::Llm);
        assert_eq!(error.got, 0);
    }

    #[test]
    fn a_corpus_that_cannot_fill_a_quota_is_an_error_naming_it() {
        let owned = corpus(1);
        let error = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("tier has"), "{message}");
        assert!(message.contains("were asked for"), "{message}");
    }

    #[test]
    fn the_exam_reads_the_sample_back_as_it_is() {
        let owned = corpus(40);
        let outcome = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        check_with_exam(&outcome.sample.sents).unwrap();
    }

    #[test]
    fn a_form_the_tokenizer_would_split_differently_fails_the_exam_check() {
        let mut sents = vec![crate::data::tests::run_now()];
        sents[0].toks[0].form = "Run now".to_string();
        let error = check_with_exam(&sents).unwrap_err().to_string();
        assert!(error.contains("g0001"), "{error}");
    }

    #[test]
    fn the_counts_name_every_tier_and_context() {
        let owned = corpus(40);
        let outcome = draw(&files(&owned), "hand-made", &small(), Mode::Gold).unwrap();
        let shown = Counts(&outcome).to_string();
        assert!(shown.starts_with("sampled 30 sentences"), "{shown}");
        for tier in Tier::ALL.iter().copied() {
            for context in Context::ALL {
                assert!(
                    shown
                        .lines()
                        .any(|l| l.starts_with(tier.name()) && l.contains(context.name())),
                    "{shown}"
                );
            }
        }
    }
}
