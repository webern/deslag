//! The sample as the later stages read it: sentences of deslag tokens, and what the manifest says
//! about where each came from.
//!
//! `sample.conllu` is the token skeleton `deslag-exam tokens` writes for a gold file: a `sent_id`,
//! a `# text` and one line per token, `FORM` and `MISC` filled (`Kind=`, `SpaceAfter=No`) and every
//! other column `_`. It carries no tier, context or label, so it is what Harper and spaCy are run
//! over. `manifest.tsv` is the rest: split, tier, context and source file of each sentence.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;

use deslag::document::TokenKind;
use deslag_exam::conllu::{self, Id};
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::{Split, Tier, kind_from_name, kind_name};
use deslag_exam::tagger::Context;

use crate::problems::Problems;

/// One deslag token of a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    /// Its text, as deslag renders it.
    pub form: String,
    /// What kind of token it is.
    pub kind: TokenKind,
    /// Whether no space follows it in the source.
    pub joined: bool,
}

impl Tok {
    /// Whether it is a word, the only kind a tagger tags.
    pub fn is_word(&self) -> bool {
        self.kind == TokenKind::Word
    }
}

/// One sentence of the sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// Its `sent_id`.
    pub id: String,
    /// Its tokens, in order.
    pub toks: Vec<Tok>,
}

impl Sent {
    /// The text its tokens index into: the forms joined by one space, none after a joined token.
    /// It is what the exam makes of a `deslag` gold file's lines.
    pub fn text(&self) -> String {
        let mut text = String::new();
        for (index, tok) in self.toks.iter().enumerate() {
            text.push_str(&tok.form);
            if !tok.joined && index + 1 < self.toks.len() {
                text.push(' ');
            }
        }
        text
    }

    /// How many of its tokens are words.
    pub fn words(&self) -> usize {
        self.toks.iter().filter(|tok| tok.is_word()).count()
    }
}

/// The UPOS a token that is not a word gets, from its kind: `PUNCT`, `SYM` or `NUM`, and `X` for
/// code, URLs, HTML, images and footnotes. None of these is scored.
pub fn upos_of_kind(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Punctuation => "PUNCT",
        TokenKind::Symbol => "SYM",
        TokenKind::Number => "NUM",
        _ => "X",
    }
}

/// The MISC column of a token line: `Kind=`, then `Prov=` when `prov` is given, then
/// `SpaceAfter=No`. UD asks for keys in alphabetical order.
pub fn misc(tok: &Tok, prov: Option<&str>) -> String {
    let mut misc = format!("Kind={}", kind_name(tok.kind));
    if let Some(prov) = prov {
        let _ = write!(misc, "|Prov={prov}");
    }
    if tok.joined {
        misc.push_str("|SpaceAfter=No");
    }
    misc
}

/// One CoNLL-U line, with LEMMA, XPOS, HEAD, DEPREL and DEPS `_`.
pub fn line(index: usize, form: &str, upos: &str, feats: &str, misc: &str) -> String {
    format!(
        "{}\t{form}\t_\t{upos}\t_\t{feats}\t_\t_\t_\t{misc}\n",
        index + 1
    )
}

/// The skeleton of `sents`, byte for byte what `deslag-exam tokens` writes for the same tokens.
pub fn skeleton(sents: &[Sent]) -> String {
    let mut out = String::new();
    for sent in sents {
        let _ = writeln!(out, "# sent_id = {}", sent.id);
        let _ = writeln!(out, "# text = {}", sent.text());
        for (index, tok) in sent.toks.iter().enumerate() {
            out.push_str(&line(index, &tok.form, "_", "_", &misc(tok, None)));
        }
        out.push('\n');
    }
    out
}

/// What the manifest records about one sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// Dev or holdout.
    pub split: Split,
    /// Who wrote the file it came from.
    pub tier: Tier,
    /// The block it was in.
    pub context: Context,
    /// The corpus fixture it was quoted from, as the loader names it.
    pub file: String,
    /// That fixture's repository, `owner/name`.
    pub repo: String,
    /// The licence the fixture is quoted under.
    pub license: String,
    /// Where the sentence is in the fixture's bytes.
    pub range: Range<usize>,
}

/// The manifest: a header of `# key = value` lines and one row per sentence, in sample order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// The header's keys and values, in order.
    pub header: Vec<(String, String)>,
    /// The rows.
    pub rows: Vec<(String, Meta)>,
}

/// The columns of a manifest row.
const COLUMNS: [&str; 8] = [
    "sent_id", "split", "tier", "context", "file", "repo", "license", "bytes",
];

impl Manifest {
    /// The header value of `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.header
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.as_str())
    }

    /// The manifest as a file.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.header {
            let _ = writeln!(out, "# {key} = {value}");
        }
        let _ = writeln!(out, "{}", COLUMNS.join("\t"));
        for (id, meta) in &self.rows {
            let _ = writeln!(
                out,
                "{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}-{}",
                meta.split.name(),
                meta.tier.name(),
                meta.context.name(),
                meta.file,
                meta.repo,
                meta.license,
                meta.range.start,
                meta.range.end
            );
        }
        out
    }

    /// Reads a manifest, `path` being where `text` came from.
    pub fn parse(path: &str, text: &str) -> Result<Manifest, Error> {
        let mut manifest = Manifest::default();
        let mut seen_columns = false;
        let mut ids = BTreeSet::new();
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                if let Some((key, value)) = comment.split_once('=') {
                    manifest
                        .header
                        .push((key.trim().to_string(), value.trim().to_string()));
                }
                continue;
            }
            let cells: Vec<&str> = line.split('\t').collect();
            if !seen_columns {
                if cells != COLUMNS {
                    return Err(Error::at(
                        path,
                        number,
                        format!("the columns should be {}", COLUMNS.join(", ")),
                    ));
                }
                seen_columns = true;
                continue;
            }
            if cells.len() != COLUMNS.len() {
                return Err(Error::at(
                    path,
                    number,
                    format!("expected {} columns, found {}", COLUMNS.len(), cells.len()),
                ));
            }
            let bad =
                |what: &str, value: &str| Error::at(path, number, format!("{what} `{value}`"));
            let range = cells[7]
                .split_once('-')
                .and_then(|(a, b)| Some(a.parse::<usize>().ok()?..b.parse::<usize>().ok()?))
                .ok_or_else(|| bad("bad byte range", cells[7]))?;
            let meta = Meta {
                split: Split::from_name(cells[1]).ok_or_else(|| bad("bad split", cells[1]))?,
                tier: Tier::from_name(cells[2]).ok_or_else(|| bad("bad tier", cells[2]))?,
                context: Context::from_name(cells[3])
                    .ok_or_else(|| bad("bad context", cells[3]))?,
                file: cells[4].to_string(),
                repo: cells[5].to_string(),
                license: cells[6].to_string(),
                range,
            };
            if !ids.insert(cells[0].to_string()) {
                return Err(Error::at(
                    path,
                    number,
                    format!("sent_id `{}` is used twice", cells[0]),
                ));
            }
            manifest.rows.push((cells[0].to_string(), meta));
        }
        if !seen_columns {
            return Err(Error::load(path, Place::File, "no column line"));
        }
        Ok(manifest)
    }
}

/// The sample: its sentences, and the manifest that says where each came from.
#[derive(Debug, Clone)]
pub struct Sample {
    /// The sentences, in the order of the file.
    pub sents: Vec<Sent>,
    /// The manifest.
    pub manifest: Manifest,
}

impl Sample {
    /// What the manifest says of sentence `id`.
    pub fn meta(&self, id: &str) -> Option<&Meta> {
        self.manifest
            .rows
            .iter()
            .find(|(row, _)| row == id)
            .map(|(_, meta)| meta)
    }

    /// The position of sentence `id`.
    pub fn index_of(&self) -> BTreeMap<&str, usize> {
        self.sents
            .iter()
            .enumerate()
            .map(|(index, sent)| (sent.id.as_str(), index))
            .collect()
    }

    /// Reads the skeleton at `sample` and the manifest at `manifest`, and checks that they name
    /// the same sentences.
    pub fn read(sample: &Path, manifest: &Path) -> Result<Sample, Problems> {
        let shown = sample.display().to_string();
        let text = read_text(sample)?;
        let sents = parse_skeleton(&shown, &text)?;
        let shown_manifest = manifest.display().to_string();
        let manifest = Manifest::parse(&shown_manifest, &read_text(manifest)?)?;
        let mut problems = Vec::new();
        let ids: BTreeSet<&str> = sents.iter().map(|sent| sent.id.as_str()).collect();
        let named: BTreeSet<&str> = manifest.rows.iter().map(|(id, _)| id.as_str()).collect();
        for id in ids.difference(&named) {
            problems.push(Error::load(
                &shown_manifest,
                Place::Sentence((*id).to_string()),
                "the manifest has no row for it",
            ));
        }
        for id in named.difference(&ids) {
            problems.push(Error::load(
                &shown,
                Place::Sentence((*id).to_string()),
                "the manifest names it and the sample does not hold it",
            ));
        }
        if !problems.is_empty() {
            return Err(Problems(problems));
        }
        Ok(Sample { sents, manifest })
    }
}

/// The text of `path`, which must be UTF-8.
pub fn read_text(path: &Path) -> Result<String, Error> {
    let shown = path.display().to_string();
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: shown.clone(),
        source,
    })?;
    String::from_utf8(bytes).map_err(|_| Error::load(&shown, Place::File, "the file is not UTF-8"))
}

/// Writes `text` to `path`, making its directory first.
pub fn write_text(path: &Path, text: &str) -> Result<(), Error> {
    let io = |source| Error::Io {
        path: path.display().to_string(),
        source,
    };
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(path, text).map_err(io)
}

/// Reads a skeleton, `path` being where `text` came from.
pub fn parse_skeleton(path: &str, text: &str) -> Result<Vec<Sent>, Error> {
    let blocks = conllu::read(path, text)?;
    let mut sents = Vec::with_capacity(blocks.len());
    let mut seen = BTreeSet::new();
    for block in &blocks {
        let id = block
            .comment("sent_id")
            .map(|comment| comment.value.clone())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::at(path, block.first_line, "no `# sent_id = ` comment"))?;
        if !seen.insert(id.clone()) {
            return Err(Error::at(
                path,
                block.first_line,
                format!("sent_id `{id}` is used twice"),
            ));
        }
        let mut toks = Vec::with_capacity(block.lines.len());
        for line in &block.lines {
            if !matches!(line.id, Id::Word(_)) {
                return Err(Error::at(
                    path,
                    line.number,
                    "a range line or an empty node in a skeleton",
                ));
            }
            let mut kind = None;
            let mut joined = false;
            for (key, value) in conllu::pairs(&line.misc) {
                match key {
                    "Kind" => {
                        kind = Some(kind_from_name(value).ok_or_else(|| {
                            Error::at(path, line.number, format!("unknown Kind `{value}`"))
                        })?)
                    }
                    "SpaceAfter" => joined = value == "No",
                    _ => {}
                }
            }
            let kind = kind.ok_or_else(|| {
                Error::at(
                    path,
                    line.number,
                    "no Kind= in MISC, which a skeleton needs",
                )
            })?;
            toks.push(Tok {
                form: line.form.clone(),
                kind,
                joined,
            });
        }
        if toks.is_empty() {
            return Err(Error::load(path, Place::Sentence(id), "no tokens"));
        }
        sents.push(Sent { id, toks });
    }
    Ok(sents)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A token of kind `kind` and text `form`.
    pub fn tok(form: &str, kind: TokenKind, joined: bool) -> Tok {
        Tok {
            form: form.to_string(),
            kind,
            joined,
        }
    }

    /// `Run [make ci] now.`
    pub fn run_now() -> Sent {
        Sent {
            id: "g0001".to_string(),
            toks: vec![
                tok("Run", TokenKind::Word, false),
                tok("make ci", TokenKind::Code, false),
                tok("now", TokenKind::Word, true),
                tok(".", TokenKind::Punctuation, false),
            ],
        }
    }

    #[test]
    fn a_sentence_s_text_joins_forms_by_the_exam_s_rule() {
        let sent = run_now();
        assert_eq!(sent.text(), "Run make ci now. ".trim_end());
        assert_eq!(sent.words(), 2);
    }

    #[test]
    fn a_skeleton_reads_back_as_the_sentences_it_came_from() {
        let sents = vec![run_now(), {
            let mut other = run_now();
            other.id = "g0002".to_string();
            other.toks.pop();
            other
        }];
        let text = skeleton(&sents);
        assert!(text.contains("\tKind=Word|SpaceAfter=No\n"));
        assert_eq!(parse_skeleton("f", &text).unwrap(), sents);
    }

    #[test]
    fn a_skeleton_with_a_duplicate_or_a_missing_kind_is_rejected() {
        let text = skeleton(&[run_now(), run_now()]);
        let error = parse_skeleton("f", &text).unwrap_err().to_string();
        assert!(error.contains("sent_id `g0001` is used twice"), "{error}");
        let text = "# sent_id = a\n1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let error = parse_skeleton("f", text).unwrap_err().to_string();
        assert!(error.contains("f:2: no Kind="), "{error}");
    }

    #[test]
    fn a_manifest_reads_back() {
        let manifest = Manifest {
            header: vec![("seed".to_string(), "7".to_string())],
            rows: vec![(
                "g0001".to_string(),
                Meta {
                    split: Split::Holdout,
                    tier: Tier::Llm,
                    context: Context::ListItem,
                    file: "batches/b/llm/o/n/f.md".to_string(),
                    repo: "o/n".to_string(),
                    license: "MIT".to_string(),
                    range: 10..42,
                },
            )],
        };
        let text = manifest.render();
        assert_eq!(Manifest::parse("m", &text).unwrap(), manifest);
        assert_eq!(manifest.get("seed"), Some("7"));
    }

    #[test]
    fn a_broken_manifest_names_its_line() {
        let head = COLUMNS.join("\t");
        for (text, says) in [
            ("a\tb\n".to_string(), "the columns should be"),
            (format!("{head}\ng\tdev\n"), "expected 8 columns, found 2"),
            (
                format!("{head}\ng\tdevv\thuman\tprose\tf\tr\tl\t1-2\n"),
                "bad split `devv`",
            ),
            (
                format!("{head}\ng\tdev\thuman\tprose\tf\tr\tl\t12\n"),
                "bad byte range",
            ),
            (
                format!(
                    "{head}\ng\tdev\thuman\tprose\tf\tr\tl\t1-2\ng\tdev\thuman\tprose\tf\tr\tl\t1-2\n"
                ),
                "used twice",
            ),
            (String::new(), "no column line"),
        ] {
            let error = Manifest::parse("m", &text).unwrap_err().to_string();
            assert!(error.contains(says), "{error} should say {says}");
        }
    }
}
