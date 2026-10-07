//! The sample as the later stages read it: sentences of deslag tokens, and what the manifest says
//! about where each came from.
//!
//! `sample.conllu` is the token skeleton `deslag-exam tokens` writes for a gold file: `# exam.tokens
//! = deslag` first, then for each sentence a `sent_id`, its `# exam.context`, a `# text` and one
//! line per token, `FORM` and `MISC` filled (`Kind=`, `SpaceAfter=No`) and every other column `_`.
//! It carries no tier or label, so it is what Harper and spaCy are run
//! over. `manifest.tsv` is the rest: split, tier, context and source file of each sentence.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;

use deslag::document::{Token, TokenKind};
use deslag::tag::Origin;
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

/// The tokens of `sent` as deslag reads them from `joined`, which is `sent.text()`: each token has
/// its kind, its text and its place in `joined`, and nothing else is set. It is what `deslag-exam
/// tokens` reads back from the gold file of the same sentence.
pub fn tokens_of<'t>(joined: &'t str, sent: &Sent) -> Vec<Token<'t>> {
    let mut at = 0;
    let mut tokens = Vec::with_capacity(sent.toks.len());
    for (index, tok) in sent.toks.iter().enumerate() {
        let range = at..at + tok.form.len();
        tokens.push(Token {
            kind: tok.kind,
            text: Cow::Borrowed(&joined[range.clone()]),
            range,
            reading: None,
            origin: Origin::English,
        });
        at += tok.form.len();
        if !tok.joined && index + 1 < sent.toks.len() {
            at += 1;
        }
    }
    tokens
}

/// The skeleton of `sents`, byte for byte what `deslag-exam tokens` writes for the same tokens
/// when `with_origin` is given, and the same without its `Origin=` keys when it is not (a gold
/// draw does not carry them). `context` gives each sentence's context by its `sent_id`; a
/// sentence it names none for has no `# exam.context`, which a reader takes as prose.
pub fn skeleton(
    sents: &[Sent],
    context: impl Fn(&str) -> Option<Context>,
    with_origin: bool,
) -> String {
    let mut out = String::from(deslag_exam::skeleton::HEADER);
    for sent in sents {
        let _ = writeln!(out, "# sent_id = {}", sent.id);
        if let Some(context) = context(&sent.id) {
            let _ = writeln!(out, "# exam.context = {}", context.name());
        }
        let text = sent.text();
        let _ = writeln!(out, "# text = {text}");
        let tokens = tokens_of(&text, sent);
        let origins = if with_origin {
            deslag::tag::origins(&tokens)
        } else {
            vec![Origin::English; tokens.len()]
        };
        for (index, tok) in sent.toks.iter().enumerate() {
            // Kind, Origin, SpaceAfter: UD's alphabetical order of keys, as the exam writes them.
            let mut misc = format!("Kind={}", kind_name(tok.kind));
            misc.push_str(&deslag_exam::skeleton::origin_misc(
                &tokens[index],
                origins[index],
            ));
            if tok.joined {
                misc.push_str("|SpaceAfter=No");
            }
            out.push_str(&line(index, &tok.form, "_", "_", &misc));
        }
        out.push('\n');
    }
    out
}

/// What the manifest records about one sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// Dev or holdout; none for a sentence of a draw for labelling, written `unlabelled`. The exam's
    /// own `Split` has no such value, so no gold file can say it.
    pub split: Option<Split>,
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
    /// What a draw for labelling records about the fixture; none in a gold draw.
    pub provenance: Option<Provenance>,
}

/// What a draw for labelling records about a fixture, so a later datasheet can name each source
/// and the generator's licence without opening the corpus: a manifest carries it in five more
/// columns after the eight.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    /// The commit the fixture was quoted at, its sidecar's `source.commit`.
    pub commit: String,
    /// A permalink to the file at that commit, `source.url`.
    pub url: String,
    /// The sha256 of the fixture's bytes, `content.sha256`.
    pub sha256: String,
    /// The model a publisher names for the file, `declared.model`; empty when none is named.
    pub model: String,
    /// That model's licence, `declared.model_license`; empty when none is named.
    pub model_license: String,
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

/// What the split column says of a sentence no one has labelled.
const UNLABELLED: &str = "unlabelled";

/// The columns a draw for labelling adds after [`COLUMNS`].
const PROVENANCE_COLUMNS: [&str; 5] = [
    "source_commit",
    "source_url",
    "content_sha256",
    "model",
    "model_license",
];

/// `value` as one cell: runs of whitespace, tabs and line breaks among them, become one space.
fn cell(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
        // A manifest has the provenance columns when its rows carry it, which a draw for
        // labelling gives every row.
        let wide = self.rows.iter().any(|(_, meta)| meta.provenance.is_some());
        let mut columns = COLUMNS.to_vec();
        if wide {
            columns.extend(PROVENANCE_COLUMNS);
        }
        let _ = writeln!(out, "{}", columns.join("\t"));
        for (id, meta) in &self.rows {
            let _ = write!(
                out,
                "{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}-{}",
                meta.split.map_or(UNLABELLED, Split::name),
                meta.tier.name(),
                meta.context.name(),
                meta.file,
                meta.repo,
                meta.license,
                meta.range.start,
                meta.range.end
            );
            if wide {
                let none = Provenance::default();
                let from = meta.provenance.as_ref().unwrap_or(&none);
                for value in [
                    &from.commit,
                    &from.url,
                    &from.sha256,
                    &from.model,
                    &from.model_license,
                ] {
                    let _ = write!(out, "\t{}", cell(value));
                }
            }
            out.push('\n');
        }
        out
    }

    /// Reads a manifest, `path` being where `text` came from.
    pub fn parse(path: &str, text: &str) -> Result<Manifest, Error> {
        let mut manifest = Manifest::default();
        // How many columns the column line has: the eight, or those and the provenance columns.
        let mut width = 0;
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
            if width == 0 {
                let wide: Vec<&str> = COLUMNS.iter().chain(&PROVENANCE_COLUMNS).copied().collect();
                if cells == COLUMNS {
                    width = COLUMNS.len();
                } else if cells == wide {
                    width = wide.len();
                } else {
                    return Err(Error::at(
                        path,
                        number,
                        format!(
                            "the columns should be {}, and may go on with {}",
                            COLUMNS.join(", "),
                            PROVENANCE_COLUMNS.join(", ")
                        ),
                    ));
                }
                continue;
            }
            if cells.len() != width {
                return Err(Error::at(
                    path,
                    number,
                    format!("expected {width} columns, found {}", cells.len()),
                ));
            }
            let bad =
                |what: &str, value: &str| Error::at(path, number, format!("{what} `{value}`"));
            let range = cells[7]
                .split_once('-')
                .and_then(|(a, b)| Some(a.parse::<usize>().ok()?..b.parse::<usize>().ok()?))
                .ok_or_else(|| bad("bad byte range", cells[7]))?;
            let meta = Meta {
                split: if cells[1] == UNLABELLED {
                    None
                } else {
                    Some(Split::from_name(cells[1]).ok_or_else(|| bad("bad split", cells[1]))?)
                },
                tier: Tier::from_name(cells[2]).ok_or_else(|| bad("bad tier", cells[2]))?,
                context: Context::from_name(cells[3])
                    .ok_or_else(|| bad("bad context", cells[3]))?,
                file: cells[4].to_string(),
                repo: cells[5].to_string(),
                license: cells[6].to_string(),
                range,
                provenance: (width > COLUMNS.len()).then(|| Provenance {
                    commit: cells[8].to_string(),
                    url: cells[9].to_string(),
                    sha256: cells[10].to_string(),
                    model: cells[11].to_string(),
                    model_license: cells[12].to_string(),
                }),
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
        if width == 0 {
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
        let text = skeleton(&sents, |_| None, false);
        assert!(text.contains("\tKind=Word|SpaceAfter=No\n"));
        assert_eq!(parse_skeleton("f", &text).unwrap(), sents);
    }

    #[test]
    fn a_skeleton_with_a_duplicate_or_a_missing_kind_is_rejected() {
        let text = skeleton(&[run_now(), run_now()], |_| None, false);
        let error = parse_skeleton("f", &text).unwrap_err().to_string();
        assert!(error.contains("sent_id `g0001` is used twice"), "{error}");
        let text = "# sent_id = a\n1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let error = parse_skeleton("f", text).unwrap_err().to_string();
        assert!(error.contains("f:2: no Kind="), "{error}");
    }

    #[test]
    fn a_manifest_with_provenance_reads_back_and_one_without_keeps_its_eight_columns() {
        let meta = |provenance| Meta {
            split: None,
            tier: Tier::Llm,
            context: Context::Heading,
            file: "f.md".to_string(),
            repo: "o/n".to_string(),
            license: "MIT".to_string(),
            range: 1..9,
            provenance,
        };
        let provenance = Provenance {
            commit: "c0ffee".to_string(),
            url: "https://example.test/o/n/blob/c0ffee/f.md".to_string(),
            sha256: "ab".repeat(32),
            model: "a model\twith a tab".to_string(),
            model_license: String::new(),
        };
        let manifest = Manifest {
            header: Vec::new(),
            rows: vec![("p0001".to_string(), meta(Some(provenance.clone())))],
        };
        let text = manifest.render();
        assert!(text.contains("\tmodel\tmodel_license\n"), "{text}");
        // A tab in a cell would shift the columns: it is written as a space.
        let back = Manifest::parse("m", &text).unwrap();
        let read = back.rows[0].1.provenance.clone().unwrap();
        assert_eq!(read.model, "a model with a tab");
        assert_eq!(
            Provenance {
                model: provenance.model.clone(),
                ..read
            },
            provenance
        );
        let plain = Manifest {
            header: Vec::new(),
            rows: vec![("g0001".to_string(), meta(None))],
        };
        assert!(plain.render().contains("\tbytes\n"));
        assert_eq!(Manifest::parse("m", &plain.render()).unwrap(), plain);
        // Neither width may be mixed with the other's rows.
        let narrow = text.replacen(
            "\tsource_commit\tsource_url\tcontent_sha256\tmodel\tmodel_license",
            "",
            1,
        );
        assert!(Manifest::parse("m", &narrow).is_err());
    }

    #[test]
    fn a_manifest_reads_back() {
        let manifest = Manifest {
            header: vec![("seed".to_string(), "7".to_string())],
            rows: vec![(
                "g0001".to_string(),
                Meta {
                    split: Some(Split::Holdout),
                    tier: Tier::Llm,
                    context: Context::ListItem,
                    file: "batches/b/llm/o/n/f.md".to_string(),
                    repo: "o/n".to_string(),
                    license: "MIT".to_string(),
                    range: 10..42,
                    provenance: None,
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
