//! The tic list: places the shipped `verbs_no_nouns` lint matches that a tagger must read as verbs.
//!
//! A lint acts on tags at `Likely` and above, and the exam measures a tagger over every word, so a
//! tagger can score well and still fail the one construction a lint needs. `tests/gold/ticlist.tsv`
//! holds the places: for each match of the shipped pattern in the English fixtures outside `core`,
//! the fixture's path, the byte offset of the `-s` word, the word, and its expected tag, `VERB`.
//! The offset is the token's `range.start` in the fixture read with `from_utf8_lossy`, which may
//! sit on formatting such as `**`; a lookup finds the token with that start and compares its text,
//! never `source[offset..]`. No sentence text is copied.
//!
//! [`cut`] makes the rows, [`List`] reads and writes them, [`score`] reports how a tagger reads
//! them: how many are right (`VERB` at `Likely` or above, the lint's own predicate), how many are
//! `VERB` below `Likely`, how many are another tag, and the reverse check, the places [`VARIANT`]
//! matches that are not rows. [`corpus_skeleton`] writes the token skeleton of every sentence of
//! the same fixtures, for a tagger outside deslag to fill.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;

use deslag::document::{Document, Token, TokenKind};
use deslag::lint::pattern::{Item, Pattern};
use deslag::lint::verbs_no_nouns::{CLAUSE_VERBS, PATTERN, STOP_WORDS};
use deslag_corpus::load;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::gold::kind_name;
use crate::import::Imported;
use crate::report::interval;
use crate::skeleton::{self, Filled};
use crate::stats::{Bootstrap, ratio};
use crate::tagger::BUILT_IN;
use crate::tags::{Confidence, Tag, TagSet};

/// What every row expects.
pub const EXPECTED: &str = "VERB";

/// The tags a row's word must be read as, and the confidence it must be read at or above: the
/// predicate of the lint's own `Item::Tag`.
const VERB: TagSet = TagSet::of(Tag::Verb);
const LEAST: Confidence = Confidence::Likely;

/// The variant of the lint that trusts the tagger: the shipped pattern with `Item::Tag` in place
/// of the closed set of function words. It is here only, never in the lint, and measures what
/// trusting a tagger would add.
pub const VARIANT: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Suffix("s"),
            Item::Without(&['\'']),
            Item::Tag(VERB, LEAST),
            Item::NotIn(&[CLAUSE_VERBS]),
        ]),
        Item::Literal("no"),
        Item::All(&[Item::Kind(TokenKind::Word), Item::NotIn(&[STOP_WORDS])]),
    ],
};

/// How many reverse-check examples a report prints.
pub const EXAMPLES: usize = 20;

/// One place of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The fixture's path under `tests/corpus/`.
    pub path: String,
    /// The byte offset of the `-s` token's start in the fixture read with `from_utf8_lossy`.
    pub offset: usize,
    /// The token's text.
    pub word: String,
}

/// The rows of a list file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// The rows, sorted by path and offset.
    pub rows: Vec<Row>,
    /// The SHA-256 of the file's bytes, in lowercase hex.
    pub sha256: String,
}

impl List {
    /// Reads the list file at `path`.
    pub fn read(path: &Path) -> Result<List, Error> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: shown.clone(),
            source,
        })?;
        List::parse(&shown, &text)
    }

    /// Reads `text`, the contents of the list file `path`: `#` lines and blank lines are skipped,
    /// and a row is a path, an offset, a word and `VERB`, tab-separated.
    pub fn parse(path: &str, text: &str) -> Result<List, Error> {
        let mut rows = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let at = index + 1;
            let fields: Vec<&str> = line.split('\t').collect();
            let [file, offset, word, expected] = fields[..] else {
                return Err(Error::at(
                    path,
                    at,
                    "a row is four tab-separated fields: path, byte offset, word, VERB",
                ));
            };
            let offset = offset
                .parse()
                .map_err(|_| Error::at(path, at, format!("offset `{offset}` is not a number")))?;
            if expected != EXPECTED || file.is_empty() || word.is_empty() {
                return Err(Error::at(
                    path,
                    at,
                    format!("a row has a path, a word and the tag {EXPECTED}"),
                ));
            }
            let row = Row {
                path: file.to_string(),
                offset,
                word: word.to_string(),
            };
            if rows.last().is_some_and(|last: &Row| {
                (last.path.as_str(), last.offset) >= (row.path.as_str(), row.offset)
            }) {
                return Err(Error::at(
                    path,
                    at,
                    "rows are sorted by path and offset, with no row twice",
                ));
            }
            rows.push(row);
        }
        Ok(List {
            rows,
            sha256: hex(&Sha256::digest(text.as_bytes())),
        })
    }

    /// The file `commit` and these `rows` make: a header comment, then a row per line.
    pub fn render(rows: &[Row], commit: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# The places the shipped verbs_no_nouns pattern matches in the English fixtures outside core, cut by\n\
             # `deslag-exam ticlist cut` at commit {commit}. Every row's -s word must be read as a verb:\n\
             # a tagger is right on it at Likely or above with VERB, which is strict, so possessive \"has\" counts.\n\
             # Columns, tab-separated: fixture path under tests/corpus, byte offset of the -s token's start in the\n\
             # fixture read with from_utf8_lossy (it may sit on formatting such as **), the word, VERB. The file is\n\
             # generated, reviewed by the owner and then frozen; a lint change that moves a match is the owner's call."
        );
        for row in rows {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{EXPECTED}",
                row.path, row.offset, row.word
            );
        }
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// One fixture of the English, non-`core` corpus.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Its path under `tests/corpus/`.
    pub path: String,
    /// Where the harness puts it in the real layout, unique across the corpus.
    pub layout_path: String,
    /// Its bytes read with `from_utf8_lossy`.
    pub text: String,
}

/// The fixtures the list and the skeleton are of, sorted by path: those under `root/tests/corpus`
/// outside `core` whose sidecar says `natural_language` is `en`.
pub fn corpus(root: &Path) -> Result<Vec<Entry>, Error> {
    let tree = root.join("tests/corpus");
    let fixtures = load::tree(&tree).map_err(|problem| Error::Cannot(problem.0))?;
    let mut entries: Vec<Entry> = fixtures
        .into_iter()
        .filter(|fixture| {
            fixture.category != "core" && fixture.sidecar.content.natural_language == "en"
        })
        .map(|fixture| Entry {
            text: String::from_utf8_lossy(&fixture.bytes).into_owned(),
            path: fixture.path,
            layout_path: fixture.sidecar.layout_path,
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// The rows of the shipped pattern in `entries`, sorted by path and offset.
pub fn cut(entries: &[Entry]) -> Vec<Row> {
    let mut rows = Vec::new();
    for entry in entries {
        let document = Document::markdown(&entry.text);
        for found in PATTERN.find(&document) {
            let token = &document.tokens[found.start];
            rows.push(Row {
                path: entry.path.clone(),
                offset: token.range.start,
                word: token.text.to_string(),
            });
        }
    }
    rows.sort_by(|a, b| (&a.path, a.offset).cmp(&(&b.path, b.offset)));
    rows
}

/// The sentences of `document` that have a token, as the range of each one's tokens in the
/// document's row and the byte where it starts, in the order of the file.
fn sentences(document: &Document<'_>) -> Vec<(Range<usize>, usize)> {
    let mut found = Vec::new();
    for (block, _) in document.walk() {
        for sentence in document.sentences_of(block) {
            if !sentence.tokens.is_empty() {
                found.push((sentence.tokens.clone(), sentence.range.start));
            }
        }
    }
    found
}

/// The `sent_id` of a sentence of `entry` that starts at byte `start`.
fn sent_id(entry: &Entry, start: usize) -> String {
    format!("{}@{start}", entry.layout_path)
}

/// Whether no space stands between a sentence's token and the next, going by the source bytes
/// between them.
fn joined(source: &str, token: &Token<'_>, next: &Token<'_>) -> bool {
    source
        .get(token.range.end..next.range.start)
        .is_none_or(|gap| !gap.chars().any(char::is_whitespace))
}

/// The FORM a skeleton writes for `token`: its text, or `_`, which CoNLL-U writes for a blank form,
/// where an image has no description.
fn form(token: &Token<'_>) -> String {
    if token.text.is_empty() {
        "_".to_string()
    } else {
        token.text.to_string()
    }
}

/// The skeleton of every sentence of `entries`, in the format of [`crate::skeleton`]: `sent_id` is
/// `<layout_path>@<sentence start byte>`, `# text` is built from the FORMs and their spacing and
/// never sliced from the source, `SpaceAfter=No` is decided from the source bytes between tokens,
/// and `Start=<byte>` in each token's `MISC` is where the token starts. A token that holds a
/// newline or a tab cannot be a CoNLL-U FORM, and is an error; one with no text is written `_`.
///
/// With `tagged`, every `Word` line carries the reading deslag's tagger gave it as the document
/// was read, as `deslag-exam readings` writes it; there is no gold, so no `Gold=`.
pub fn corpus_skeleton(entries: &[Entry], tagged: bool) -> Result<(String, usize), Error> {
    let mut out = String::new();
    let mut count = 0;
    for entry in entries {
        let document = Document::markdown(&entry.text);
        for (range, start) in sentences(&document) {
            let tokens = &document.tokens[range];
            let mut text = String::new();
            let mut lines = String::new();
            for (index, token) in tokens.iter().enumerate() {
                if token.text.contains(['\n', '\r', '\t']) {
                    return Err(Error::Cannot(format!(
                        "{}: the token at byte {} holds a newline or a tab, which a skeleton cannot carry",
                        entry.path, token.range.start
                    )));
                }
                let next = tokens.get(index + 1);
                let space = next.is_some_and(|next| !joined(document.source, token, next));
                let form = form(token);
                text.push_str(&form);
                if space {
                    text.push(' ');
                }
                let misc = format!(
                    "Kind={}|Start={}{}",
                    kind_name(token.kind),
                    token.range.start,
                    if next.is_some() && !space {
                        "|SpaceAfter=No"
                    } else {
                        ""
                    }
                );
                let filled = token
                    .reading
                    .as_ref()
                    .filter(|_| tagged)
                    .map(|reading| Filled::new(reading, None));
                lines.push_str(&skeleton::line(index + 1, &form, &misc, filled.as_ref()));
            }
            let _ = writeln!(out, "# sent_id = {}", sent_id(entry, start));
            let _ = writeln!(out, "# text = {text}");
            out.push_str(&lines);
            out.push('\n');
            count += 1;
        }
    }
    Ok((out, count))
}

/// Where the readings come from.
pub enum Source<'a> {
    /// deslag's own tagger, which `Document::markdown` has already run.
    Deslag,
    /// A file filled from [`corpus_skeleton`]'s output.
    Import(&'a Path),
}

/// How a tagger read a row's word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// `VERB` at `Likely` or above.
    Right,
    /// `VERB`, below `Likely`.
    Below,
    /// Another tag, at any confidence.
    Other,
}

/// What a tagger said of one row.
#[derive(Debug, Clone, Serialize)]
pub struct Read {
    /// The row's fixture.
    pub path: String,
    /// The row's offset.
    pub offset: usize,
    /// The row's word.
    pub word: String,
    /// The tag code the tagger gave, or `None` where it gave no reading.
    pub tag: Option<&'static str>,
    /// The confidence it gave.
    pub confidence: Option<&'static str>,
    /// Which of the three it is.
    pub verdict: Verdict,
}

/// A place the variant matches that is no row.
#[derive(Debug, Clone, Serialize)]
pub struct Example {
    /// The fixture.
    pub path: String,
    /// The `-s` token's start.
    pub offset: usize,
    /// Its text.
    pub word: String,
}

/// A tagger's score on a list.
#[derive(Debug, Clone)]
pub struct Score {
    /// What the report calls the tagger.
    pub tagger: String,
    /// The list's SHA-256.
    pub list: String,
    /// One read per row, in the list's order.
    pub reads: Vec<Read>,
    /// Every reverse-check match, in the order of the files.
    pub reverse: Vec<Example>,
    /// The rows right and the 95% interval of the rate, from a bootstrap over the sentences that
    /// hold rows.
    pub interval: Option<[f64; 2]>,
}

impl Score {
    /// How many rows have `verdict`.
    pub fn count(&self, verdict: Verdict) -> usize {
        self.reads
            .iter()
            .filter(|read| read.verdict == verdict)
            .count()
    }

    /// The report, ending in a newline.
    pub fn render(&self, list_name: &str) -> String {
        let rows = self.reads.len();
        let right = self.count(Verdict::Right);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "ticlist: {list_name} ({}), tagger {}",
            &self.list[..12],
            self.tagger
        );
        let rate = if rows == 0 {
            "n/a".to_string()
        } else {
            format!("{:.1}%", right as f64 * 100.0 / rows as f64)
        };
        let _ = writeln!(
            out,
            "  rows right at Likely or above   {right} of {rows}  {rate}  {}",
            interval(self.interval)
        );
        let _ = writeln!(
            out,
            "  rows right below Likely         {}",
            self.count(Verdict::Below)
        );
        let _ = writeln!(
            out,
            "  rows read as another tag        {}",
            self.count(Verdict::Other)
        );
        let _ = writeln!(
            out,
            "  reverse check                   {} places the tag-trusting variant matches that are not rows",
            self.reverse.len()
        );
        for example in self.reverse.iter().take(EXAMPLES) {
            let _ = writeln!(
                out,
                "    {}\t{}\t{}",
                example.path, example.offset, example.word
            );
        }
        if self.reverse.len() > EXAMPLES {
            let _ = writeln!(out, "    and {} more", self.reverse.len() - EXAMPLES);
        }
        out
    }

    /// The saved run: JSON with every row's read and every reverse match.
    pub fn saved(&self) -> String {
        #[derive(Serialize)]
        struct Saved<'a> {
            format: u32,
            tagger: &'a str,
            list_sha256: &'a str,
            rows: usize,
            right: usize,
            below_likely: usize,
            other_tag: usize,
            reverse: usize,
            reads: &'a [Read],
            reverse_places: &'a [Example],
        }
        let saved = Saved {
            format: 1,
            tagger: &self.tagger,
            list_sha256: &self.list,
            rows: self.reads.len(),
            right: self.count(Verdict::Right),
            below_likely: self.count(Verdict::Below),
            other_tag: self.count(Verdict::Other),
            reverse: self.reverse.len(),
            reads: &self.reads,
            reverse_places: &self.reverse,
        };
        let mut text = serde_json::to_string_pretty(&saved).expect("a plain struct serializes");
        text.push('\n');
        text
    }
}

/// How `reading` stands against the rows' predicate.
fn verdict(reading: Option<(Tag, Confidence)>) -> Verdict {
    match reading {
        Some((tag, confidence)) if VERB.contains(tag) => {
            if confidence.at_least(LEAST) {
                Verdict::Right
            } else {
                Verdict::Below
            }
        }
        _ => Verdict::Other,
    }
}

/// Scores `source` on `list`, over `entries`, the fixtures the list was cut from.
pub fn score(list: &List, entries: &[Entry], source: &Source<'_>) -> Result<Score, Error> {
    let mut documents: Vec<Document<'_>> = entries
        .iter()
        .map(|entry| Document::markdown(&entry.text))
        .collect();
    let tagger = match source {
        Source::Deslag => "deslag".to_string(),
        Source::Import(path) => {
            let ids: Vec<(String, Vec<Token<'_>>)> = entries
                .iter()
                .zip(&documents)
                .flat_map(|(entry, document)| {
                    sentences(document).into_iter().map(move |(range, start)| {
                        let tokens = document.tokens[range]
                            .iter()
                            .map(|token| Token {
                                text: form(token).into(),
                                ..token.clone()
                            })
                            .collect();
                        (sent_id(entry, start), tokens)
                    })
                })
                .collect();
            let expected: Vec<(&str, Vec<Token<'_>>)> = ids
                .iter()
                .map(|(id, tokens)| (id.as_str(), tokens.clone()))
                .collect();
            let imported = Imported::read_skeleton(path, &expected)?;
            drop(expected);
            drop(ids);
            let mut readings = imported.readings.iter();
            for document in &mut documents {
                for (range, _) in sentences(document) {
                    let sentence = readings.next().expect("one reading row per sentence");
                    for (token, reading) in document.tokens[range].iter_mut().zip(sentence) {
                        token.reading = reading.map(|reading| reading.without_score());
                    }
                }
            }
            imported.name
        }
    };

    // Each row's token, by the fixture's document and the token's start.
    let by_path: BTreeMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.path.as_str(), index))
        .collect();
    let mut reads = Vec::with_capacity(list.rows.len());
    let mut units: BTreeMap<(usize, usize), [u64; 2]> = BTreeMap::new();
    let mut listed: BTreeSet<(&str, usize)> = BTreeSet::new();
    for row in &list.rows {
        let stale = |why: &str| {
            Error::Cannot(format!(
                "{}: the row at byte {} {why}; the list is stale",
                row.path, row.offset
            ))
        };
        let &fixture = by_path
            .get(row.path.as_str())
            .ok_or_else(|| stale("names a fixture the corpus lacks"))?;
        let document = &documents[fixture];
        let at = document
            .tokens
            .binary_search_by_key(&row.offset, |token| token.range.start)
            .map_err(|_| stale("starts no token"))?;
        let token = &document.tokens[at];
        if token.text != row.word {
            return Err(stale("names another word"));
        }
        let read = token
            .reading
            .map(|reading| (reading.tag, reading.confidence));
        let verdict = verdict(read);
        let sentence = sentences(document)
            .into_iter()
            .find(|(range, _)| range.contains(&at))
            .map_or(0, |(_, start)| start);
        let unit = units.entry((fixture, sentence)).or_insert([0, 0]);
        unit[0] += u64::from(verdict == Verdict::Right);
        unit[1] += 1;
        listed.insert((row.path.as_str(), row.offset));
        reads.push(Read {
            path: row.path.clone(),
            offset: row.offset,
            word: row.word.clone(),
            tag: read.map(|(tag, _)| tag.code()),
            confidence: read.map(|(_, confidence)| confidence.name()),
            verdict,
        });
    }

    let tallies: Vec<&[u64]> = units.values().map(|unit| unit.as_slice()).collect();
    let boot = Bootstrap::new("ticlist", &tallies, 2);
    let interval = boot.estimate(&|sum| ratio(sum, 0, 1)).interval;

    let mut reverse = Vec::new();
    for (entry, document) in entries.iter().zip(&documents) {
        for found in VARIANT.find(document) {
            let token = &document.tokens[found.start];
            if !listed.contains(&(entry.path.as_str(), token.range.start)) {
                reverse.push(Example {
                    path: entry.path.clone(),
                    offset: token.range.start,
                    word: token.text.to_string(),
                });
            }
        }
    }
    Ok(Score {
        tagger,
        list: list.sha256.clone(),
        reads,
        reverse,
        interval,
    })
}

/// The built-in taggers `ticlist score --tagger` reads, which is the one the lint reads.
pub fn source_of(name: &str) -> Result<Source<'static>, Error> {
    if name == "deslag" {
        Ok(Source::Deslag)
    } else {
        Err(Error::Cannot(format!(
            "ticlist score reads the built-in tagger `deslag`, not `{name}`; the built-in taggers are {}; \
             use --import for another",
            BUILT_IN.join(", ")
        )))
    }
}
