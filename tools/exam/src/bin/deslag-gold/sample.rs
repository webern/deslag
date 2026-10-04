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
use deslag::document::{Block, BlockKind, Body, Token, TokenKind};
use deslag_corpus::stats::Rng;
use deslag_exam::error::Error;
use deslag_exam::gold::{Gold, Split, Tier};
use deslag_exam::tagger::Context;

use crate::data::{Manifest, Meta, Sample, Sent, Tok};
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

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            seed: SEED,
            quotas: [90, 30, 15, 15],
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
}

/// The context of a block of prose with `ancestors` around it: `heading` if it is a heading, else
/// `table-cell` if it is a table cell, else `list-item` if any block around it is a list item,
/// else `prose`. This is the rule the exam's gold files follow.
pub fn context_of(block: &Block<'_>, ancestors: &[&Block<'_>]) -> Context {
    match block.kind {
        BlockKind::Heading { .. } => Context::Heading,
        BlockKind::TableCell => Context::TableCell,
        _ if ancestors
            .iter()
            .any(|ancestor| matches!(ancestor.kind, BlockKind::Item { .. })) =>
        {
            Context::ListItem
        }
        _ => Context::Prose,
    }
}

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
    /// The split.
    pub split: Split,
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
            self.split.name(),
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
    split: Split,
}

/// Draws the sample from `files`. `corpus` says in the manifest what they are.
pub fn draw(files: &[File<'_>], corpus: &str, settings: &Settings) -> Result<Outcome, Short> {
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
        let held = apportion(settings.holdout, &settings.quotas);
        let mut want = [[0usize; 4]; 2];
        for cell in 0..4 {
            want[1][cell] = held[cell];
            want[0][cell] = settings.quotas[cell] - held[cell].min(settings.quotas[cell]);
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
                    split: if split == 0 {
                        Split::Dev
                    } else {
                        Split::Holdout
                    },
                });
            }
        }
        for split in 0..2 {
            for (cell, context) in Context::ALL.iter().enumerate() {
                if have[split][cell] < want[split][cell] {
                    return Err(Short {
                        tier,
                        context: *context,
                        split: if split == 0 {
                            Split::Dev
                        } else {
                            Split::Holdout
                        },
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
        let id = format!("g{:04}", index + 1);
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
            },
        ));
        sents.push(Sent { id, toks });
    }
    let quotas: Vec<String> = Context::ALL
        .iter()
        .zip(settings.quotas)
        .map(|(context, quota)| format!("{}:{quota}", context.name()))
        .collect();
    let header = [
        ("seed", format!("{:#x}", settings.seed)),
        ("corpus", corpus.to_string()),
        ("per tier quotas", quotas.join(" ")),
        ("holdout per tier", settings.holdout.to_string()),
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
            let split = usize::from(meta.split == Split::Holdout);
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
        let outcome = draw(&files(&owned), "hand-made", &small()).unwrap();
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
                .filter(|(_, m)| m.tier == tier && m.split == Split::Holdout)
                .count();
            assert_eq!(held, 4, "{}", tier.name());
        }
        // A holdout of 4 of 10 takes one of every cell, by the largest remainder, in every tier.
        let held_prose = rows
            .iter()
            .filter(|(_, m)| m.split == Split::Holdout && m.context == Context::Prose)
            .count();
        assert_eq!(held_prose, 3);
    }

    #[test]
    fn the_same_seed_gives_the_same_sample_and_another_seed_gives_another() {
        let owned = corpus(40);
        let first = draw(&files(&owned), "hand-made", &small()).unwrap();
        let again = draw(&files(&owned), "hand-made", &small()).unwrap();
        assert_eq!(first.sample.sents, again.sample.sents);
        assert_eq!(first.sample.manifest, again.sample.manifest);
        let other = Settings { seed: 7, ..small() };
        let other = draw(&files(&owned), "hand-made", &other).unwrap();
        assert_ne!(first.sample.manifest.rows, other.sample.manifest.rows);
        // The order of the files given does not matter, since they are sorted by path first.
        let mut reversed = owned.clone();
        reversed.reverse();
        let reversed = draw(&files(&reversed), "hand-made", &small()).unwrap();
        assert_eq!(first.sample.sents, reversed.sample.sents);
    }

    #[test]
    fn a_draw_with_files_excluded_takes_nothing_from_them() {
        use crate::exclude::Exclusion;
        let owned = corpus(40);
        let first = draw(&files(&owned), "hand-made", &small()).unwrap();
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
        let second = draw(&kept, "hand-made", &small()).unwrap();
        assert_eq!(second.sample.manifest.rows.len(), 30);
        for (_, meta) in &second.sample.manifest.rows {
            assert!(!used.contains(meta.file.as_str()), "{}", meta.file);
        }
        // The draw over what is kept is itself repeatable.
        let again = draw(&kept, "hand-made", &small()).unwrap();
        assert_eq!(second.sample.manifest, again.sample.manifest);
    }

    #[test]
    fn ids_say_nothing_of_the_tier() {
        let owned = corpus(40);
        let outcome = draw(&files(&owned), "hand-made", &small()).unwrap();
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
        let outcome = draw(&files(&owned), "hand-made", &settings).unwrap();
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
            let outcome = draw(&files(&owned), "hand-made", &settings).unwrap();
            let mut split_of: BTreeMap<&str, Split> = BTreeMap::new();
            for (_, meta) in &outcome.sample.manifest.rows {
                let first = *split_of.entry(meta.file.as_str()).or_insert(meta.split);
                assert_eq!(first, meta.split, "{} is in both splits", meta.file);
            }
            let held = outcome
                .sample
                .manifest
                .rows
                .iter()
                .filter(|(_, m)| m.split == Split::Holdout)
                .count();
            assert_eq!(held, 12, "four of each of the three tiers");
        }
    }

    #[test]
    fn a_corpus_with_too_few_files_for_two_splits_names_the_split_that_is_short() {
        // One file per tier holds everything, but a file goes to one split only.
        let owned = corpus(1);
        let error = draw(&files(&owned), "hand-made", &small()).unwrap_err();
        assert!(error.to_string().contains("for "), "{error}");
        assert!(["dev", "holdout"].contains(&error.split.name()));
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
        let error = draw(&files(&owned), "hand-made", &settings).unwrap_err();
        // One tier took the sentence, and the next could not take it again.
        assert_eq!(error.tier, Tier::Llm);
        assert_eq!(error.got, 0);
    }

    #[test]
    fn a_corpus_that_cannot_fill_a_quota_is_an_error_naming_it() {
        let owned = corpus(1);
        let error = draw(&files(&owned), "hand-made", &small()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("tier has"), "{message}");
        assert!(message.contains("were asked for"), "{message}");
    }

    #[test]
    fn the_exam_reads_the_sample_back_as_it_is() {
        let owned = corpus(40);
        let outcome = draw(&files(&owned), "hand-made", &small()).unwrap();
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
        let outcome = draw(&files(&owned), "hand-made", &small()).unwrap();
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
