//! `ngrams` and `candidates`: the phrases that set the focus side apart.
//!
//! An n-gram is a run of n prose tokens in one block, as `banned_phrases` matches a phrase: it
//! starts and ends with a word or a number and holds no mark that ends a sentence. Split by
//! `Token::split`, a phrase named here matches in exactly the files counted, which a test proves.
//!
//! Counting takes two passes. The first counts, length by length, the focus repositories each run
//! of tokens is in, and counts a run of n only where both its runs of n - 1 were in enough: a run
//! in fewer than `min_repos` repositories is part of none in more. The second finds every n-gram
//! the first kept in every file of both sides, which gives every other number.

use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasherDefault, Hash, Hasher};

use clap::builder::TypedValueParser as _;
use serde::Serialize;

use crate::chars::{COMPARED, UNITS, compared_cells};
use crate::compare::{Compared, Comparison, Hit, Sides};
use crate::load::Problem;
use crate::measure::{BLOCK, Corpus, Doc, Filters, Header, SEP, SPACED, TEXT, Vocab};
use crate::table::{Table, fixed};
use crate::work::in_chunks;

/// The longest n-gram counted, in tokens.
pub const MAX_N: usize = 6;

/// No gram, or the end of a row of tokens.
pub(crate) const NONE: u32 = u32::MAX;

/// A run of tokens, as vocabulary indexes, padded with [`NONE`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key([u32; MAX_N]);

impl Key {
    pub(crate) fn of(tokens: &[u32]) -> Key {
        let mut key = [NONE; MAX_N];
        key[..tokens.len()].copy_from_slice(tokens);
        Key(key)
    }

    pub(crate) fn tokens(&self) -> &[u32] {
        let n = self
            .0
            .iter()
            .position(|token| *token == NONE)
            .unwrap_or(MAX_N);
        &self.0[..n]
    }
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for pair in self.0.chunks(2) {
            state.write_u64(u64::from(pair[0]) << 32 | u64::from(pair[1]));
        }
    }
}

/// A fast hash for keys no one chooses, as rustc's own: multiply and rotate.
#[derive(Default)]
struct Fx(u64);

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    fn write_u64(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

/// How n-grams are counted.
#[derive(Debug, Clone, Serialize, clap::Args)]
pub struct Counting {
    /// The shortest n-gram listed, in tokens; a mark such as the hyphen of load-bearing is a token.
    #[arg(long, default_value_t = 1)]
    pub min_n: usize,
    /// The longest n-gram counted, in tokens; at most 6.
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u8).range(1..=6).map(usize::from))]
    pub max_n: usize,
    /// The fewest focus repositories an n-gram must be in to be counted.
    #[arg(long, default_value_t = 10)]
    pub min_repos: u64,
    /// How many to list.
    #[arg(long, default_value_t = 50)]
    pub top: usize,
}

impl Default for Counting {
    fn default() -> Counting {
        Counting {
            min_n: 1,
            max_n: 4,
            min_repos: 10,
            top: 50,
        }
    }
}

/// Every run of tokens the first pass kept, of every length.
pub(crate) struct Grams {
    pub(crate) keys: Vec<Key>,
    /// Whether each starts and ends with a word or a number: an n-gram, which is reported. The
    /// others are kept only as parts of longer runs.
    pub(crate) whole: Vec<bool>,
    /// For each vocabulary index, its gram, or [`NONE`].
    pub(crate) unigrams: Vec<u32>,
    /// For each length from 2, its grams.
    levels: Vec<FxMap<Key, u32>>,
}

impl Grams {
    fn new(vocab: &Vocab) -> Grams {
        Grams {
            keys: Vec::new(),
            whole: Vec::new(),
            unigrams: vec![NONE; vocab.texts.len()],
            levels: Vec::new(),
        }
    }

    /// Adds `keys`, all of one length, sorted.
    fn add(&mut self, vocab: &Vocab, keys: Vec<Key>) {
        let mut level = FxMap::default();
        for key in keys {
            let gram = self.keys.len() as u32;
            let tokens = key.tokens();
            let wordish = |token: &u32| vocab.wordish[*token as usize];
            self.whole
                .push(tokens.first().is_some_and(wordish) && tokens.last().is_some_and(wordish));
            if let [token] = tokens {
                self.unigrams[*token as usize] = gram;
            } else {
                level.insert(key, gram);
            }
            self.keys.push(key);
        }
        if !level.is_empty() {
            self.levels.push(level);
        }
    }

    /// The gram of the run `tokens`, or [`NONE`].
    fn find(&self, tokens: &[u32]) -> u32 {
        match tokens {
            [token] => self.unigrams[*token as usize],
            _ => self
                .levels
                .get(tokens.len() - 2)
                .and_then(|level| level.get(&Key::of(tokens)))
                .copied()
                .unwrap_or(NONE),
        }
    }
}

/// Appends the prose tokens of `doc` to `out` as rows: each token's vocabulary index, with
/// [`NONE`] after each row, where a block starts, a token that is not prose stands, or a mark
/// ends a sentence.
fn rows(doc: &Doc, vocab: &Vocab, out: &mut Vec<u32>) {
    let end_row = |out: &mut Vec<u32>| {
        if out.last().is_some_and(|last| *last != NONE) {
            out.push(NONE);
        }
    };
    for id in &doc.ids {
        if *id == SEP || vocab.terminal[(*id & TEXT) as usize] {
            end_row(out);
            continue;
        }
        if *id & BLOCK != 0 {
            end_row(out);
        }
        out.push(*id & TEXT);
    }
    end_row(out);
}

/// The first pass: the runs of up to `max_n` tokens in at least `min_repos` repositories of
/// `side`.
fn frequent(side: &crate::compare::Side<'_>, vocab: &Vocab, counting: &Counting) -> Grams {
    // Each file's rows, the files of a repository together, so that a repository is counted once
    // by remembering the last one that counted each run.
    let mut order: Vec<usize> = (0..side.docs.len()).collect();
    order.sort_by_key(|at| side.repo_of[*at]);
    let mut stream = Vec::new();
    let mut repo_at = Vec::new();
    for at in order {
        rows(side.docs[at], vocab, &mut stream);
        repo_at.resize(stream.len(), side.repo_of[at] as u32);
    }
    let min = counting.min_repos.min(u64::from(u32::MAX)) as u32;

    let mut grams = Grams::new(vocab);
    let mut repos = vec![0u32; vocab.texts.len()];
    let mut last = vec![NONE; vocab.texts.len()];
    for (token, repo) in stream.iter().zip(&repo_at) {
        if *token != NONE && last[*token as usize] != *repo {
            last[*token as usize] = *repo;
            repos[*token as usize] += 1;
        }
    }
    let keys = (0..vocab.texts.len() as u32)
        .filter(|token| repos[*token as usize] >= min)
        .map(|token| Key::of(&[token]))
        .collect();
    grams.add(vocab, keys);
    // Whether the run of the current length that starts at each place was kept.
    let mut kept: Vec<bool> = stream
        .iter()
        .map(|token| *token != NONE && grams.unigrams[*token as usize] != NONE)
        .collect();

    for n in 2..=counting.max_n {
        let starts =
            || (0..stream.len().saturating_sub(n - 1)).filter(|at| kept[*at] && kept[at + 1]);
        let mut counts: FxMap<Key, (u32, u32)> = FxMap::default();
        for at in starts() {
            let count = counts
                .entry(Key::of(&stream[at..at + n]))
                .or_insert((0, NONE));
            if count.1 != repo_at[at] {
                *count = (count.0 + 1, repo_at[at]);
            }
        }
        let mut keys: Vec<Key> = counts
            .iter()
            .filter(|(_, (repos, _))| *repos >= min)
            .map(|(key, _)| *key)
            .collect();
        drop(counts);
        if keys.is_empty() {
            break;
        }
        keys.sort_unstable();
        grams.add(vocab, keys);
        let mut next = vec![false; stream.len()];
        for at in starts() {
            next[at] = grams.find(&stream[at..at + n]) != NONE;
        }
        kept = next;
    }
    grams
}

/// The second pass: for each gram that is an n-gram, the files of `docs` that hold it, as indexes
/// into `docs`, with how often.
pub(crate) fn hits(grams: &Grams, vocab: &Vocab, docs: &[&Doc]) -> Vec<Vec<Hit>> {
    let indexed: Vec<(u32, &Doc)> = docs
        .iter()
        .enumerate()
        .map(|(at, doc)| (at as u32, *doc))
        .collect();
    let found = in_chunks(&indexed, 64, |chunk| {
        let mut found: Vec<(u32, u32, u32)> = Vec::new();
        let mut stream = Vec::new();
        let mut in_doc = Vec::new();
        for (at, doc) in chunk {
            stream.clear();
            rows(doc, vocab, &mut stream);
            in_doc.clear();
            let mut kept: Vec<u32> = stream
                .iter()
                .map(|token| match *token {
                    NONE => NONE,
                    token => grams.unigrams[token as usize],
                })
                .collect();
            let mut n = 1;
            loop {
                in_doc.extend(
                    kept.iter()
                        .filter(|gram| **gram != NONE && grams.whole[**gram as usize]),
                );
                n += 1;
                let Some(level) = grams.levels.get(n - 2) else {
                    break;
                };
                let mut next = vec![NONE; stream.len()];
                let mut any = false;
                for start in 0..stream.len().saturating_sub(n - 1) {
                    if kept[start] != NONE && kept[start + 1] != NONE {
                        if let Some(gram) = level.get(&Key::of(&stream[start..start + n])) {
                            next[start] = *gram;
                            any = true;
                        }
                    }
                }
                if !any {
                    break;
                }
                kept = next;
            }
            in_doc.sort_unstable();
            for run in in_doc.chunk_by(|a, b| a == b) {
                found.push((run[0], *at, run.len() as u32));
            }
        }
        found
    });
    let mut hits: Vec<Vec<Hit>> = vec![Vec::new(); grams.keys.len()];
    for (gram, doc, count) in found.into_iter().flatten() {
        hits[gram as usize].push((doc, count));
    }
    hits
}

/// One n-gram, measured.
#[derive(Debug, Clone, Serialize)]
pub struct Gram {
    /// The phrase, as `banned_phrases` takes it: folded, spaced as its first focus file spaces it.
    pub phrase: String,
    /// Its tokens.
    pub n: usize,
    /// Both sides' measures.
    #[serde(flatten)]
    pub compared: Compared,
    /// The largest share of its focus files that one repository holds.
    pub top_repo_share: f64,
    /// Every file of both sides that holds it, which the round trip checks.
    #[serde(skip)]
    pub files: Vec<String>,
}

/// Every n-gram both passes found, measured.
pub(crate) struct Counted<'c> {
    pub(crate) comparison: Comparison<'c>,
    pub(crate) grams: Grams,
    /// For each side, each gram's hits.
    pub(crate) hits: [Vec<Vec<Hit>>; 2],
    /// Each n-gram of at least `min_n` tokens, with its measures, best first: by the lower bound
    /// of its interval, then its ratio.
    pub(crate) measured: Vec<(u32, Compared)>,
}

impl<'c> Counted<'c> {
    pub(crate) fn new(
        corpus: &'c Corpus,
        filters: &Filters,
        sides: &Sides,
        counting: &Counting,
    ) -> Result<Counted<'c>, Problem> {
        let comparison = Comparison::new(corpus, filters, sides)?;
        let grams = frequent(&comparison.sides[0], &corpus.vocab, counting);
        let hits = [0, 1].map(|at| hits(&grams, &corpus.vocab, &comparison.sides[at].docs));
        let listed: Vec<u32> = (0..grams.keys.len() as u32)
            .filter(|gram| {
                grams.whole[*gram as usize]
                    && grams.keys[*gram as usize].tokens().len() >= counting.min_n
            })
            .collect();
        let mut measured: Vec<(u32, Compared)> = in_chunks(&listed, 256, |chunk| {
            chunk
                .iter()
                .map(|gram| {
                    let at = *gram as usize;
                    (*gram, comparison.compare([&hits[0][at], &hits[1][at]]))
                })
                .collect::<Vec<_>>()
        })
        .into_iter()
        .flatten()
        .collect();
        measured.sort_by(|(a, x), (b, y)| {
            y.interval[0]
                .total_cmp(&x.interval[0])
                .then(y.ratio.total_cmp(&x.ratio))
                .then(grams.keys[*a as usize].cmp(&grams.keys[*b as usize]))
        });
        Ok(Counted {
            comparison,
            grams,
            hits,
            measured,
        })
    }

    /// The tokens of `gram`.
    pub(crate) fn tokens(&self, gram: u32) -> &[u32] {
        self.grams.keys[gram as usize].tokens()
    }

    /// The first focus file that holds `gram`.
    pub(crate) fn first_doc(&self, gram: u32) -> &'c Doc {
        let (doc, _) = self.hits[0][gram as usize][0];
        self.comparison.sides[0].docs[doc as usize]
    }

    /// `gram`, measured, with its phrase.
    pub(crate) fn gram(&self, corpus: &Corpus, gram: u32, compared: &Compared) -> Gram {
        let focus = &self.comparison.sides[0];
        let mut per_repo: BTreeMap<usize, u64> = BTreeMap::new();
        for (doc, _) in &self.hits[0][gram as usize] {
            *per_repo.entry(focus.repo_of[*doc as usize]).or_default() += 1;
        }
        let top = per_repo.values().max().copied().unwrap_or(0);
        let mut files = Vec::new();
        for (side, hits) in self.comparison.sides.iter().zip(&self.hits) {
            for (doc, _) in &hits[gram as usize] {
                files.push(side.docs[*doc as usize].path.clone());
            }
        }
        let tokens = self.tokens(gram);
        Gram {
            phrase: phrase(&corpus.vocab, self.first_doc(gram), tokens),
            n: tokens.len(),
            compared: compared.clone(),
            top_repo_share: top as f64 / compared.focus.files.max(1) as f64,
            files,
        }
    }
}

/// Where `tokens` first starts in `doc`, as an index into its tokens.
pub(crate) fn find(doc: &Doc, tokens: &[u32]) -> Option<usize> {
    (0..doc.ids.len().saturating_sub(tokens.len() - 1)).find(|at| {
        tokens.iter().enumerate().all(|(offset, token)| {
            let id = doc.ids[at + offset];
            id != SEP && id & TEXT == *token && (offset == 0 || id & BLOCK == 0)
        })
    })
}

/// The phrase `tokens` spells, spaced as it is where `doc` first holds it.
pub(crate) fn phrase(vocab: &Vocab, doc: &Doc, tokens: &[u32]) -> String {
    let start = find(doc, tokens).expect("a file that holds a gram holds it");
    let mut phrase = String::new();
    for (offset, token) in tokens.iter().enumerate() {
        if offset > 0 && doc.ids[start + offset] & SPACED != 0 {
            phrase.push(' ');
        }
        phrase.push_str(&vocab.texts[*token as usize]);
    }
    phrase
}

/// What `ngrams` finds.
#[derive(Debug, Serialize)]
pub struct Ngrams {
    /// What was measured.
    pub header: Header,
    /// What is compared with what.
    pub comparison: String,
    /// How n-grams were counted.
    pub counting: Counting,
    /// How many n-grams were counted.
    pub counted: u64,
    /// The best `top`, by the lower bound of their interval.
    pub grams: Vec<Gram>,
}

/// Runs `ngrams`.
pub fn ngrams(
    corpus: &Corpus,
    filters: &Filters,
    sides: &Sides,
    counting: &Counting,
) -> Result<Ngrams, Problem> {
    let counted = Counted::new(corpus, filters, sides, counting)?;
    let grams = counted
        .measured
        .iter()
        .take(counting.top)
        .map(|(gram, compared)| counted.gram(corpus, *gram, compared))
        .collect();
    Ok(Ngrams {
        header: Header::new("ngrams", corpus, filters),
        comparison: counted.comparison.title.clone(),
        counting: counting.clone(),
        counted: counted.measured.len() as u64,
        grams,
    })
}

impl Ngrams {
    /// The n-grams as a table.
    pub fn table(&self) -> Table {
        let mut header = vec!["phrase"];
        header.extend(COMPARED);
        header.push("top repo");
        let mut table = Table::new("n-grams", &header);
        for gram in &self.grams {
            let mut cells = vec![gram.phrase.clone()];
            cells.extend(compared_cells(&gram.compared));
            cells.push(fixed(gram.top_repo_share * 100.0, 0) + "%");
            table.row(cells);
        }
        table
    }

    /// The n-grams as text.
    pub fn render(&self) -> String {
        let mut out = self.header.render();
        out.push_str(&format!(
            "{}; {UNITS}\n{} n-grams of {} to {} tokens in at least {} focus repositories; the \
             best {} by the lower bound of the 95% interval\n",
            self.comparison,
            self.counted,
            self.counting.min_n,
            self.counting.max_n,
            self.counting.min_repos,
            self.grams.len(),
        ));
        out.push_str(&self.table().render());
        out
    }
}
