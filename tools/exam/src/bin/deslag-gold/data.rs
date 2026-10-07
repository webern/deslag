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
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use deslag::document::{Token, TokenKind};
use deslag::tag::Origin;
use deslag_exam::conllu::{self, Id};
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::{Split, Tier, kind_from_name, kind_name};
use deslag_exam::skeleton as exam_skeleton;
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
    /// Where the word comes from, as the skeleton's `Origin=` says; English when it says none, and
    /// for every token that is not a word. A draw for labelling writes it, so the batches can show
    /// a labeller that `foo_bar` is a name from code; the gold sample has none.
    pub origin: Origin,
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

/// The MISC column of a token line: `Kind=`, then `Origin=` when the token is a word of a
/// non-English origin, then `Prov=` when `prov` is given, then `Runs=` when `runs` is given, then
/// `SpaceAfter=No`. UD asks for keys in alphabetical order.
pub fn misc(tok: &Tok, prov: Option<&str>, runs: Option<&str>) -> String {
    let mut misc = format!("Kind={}", kind_name(tok.kind));
    if tok.is_word() && tok.origin != Origin::English {
        let _ = write!(misc, "|Origin={}", tok.origin.name());
    }
    if let Some(prov) = prov {
        let _ = write!(misc, "|Prov={prov}");
    }
    if let Some(runs) = runs {
        let _ = write!(misc, "|Runs={runs}");
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
    /// Who wrote the file it came from; none for a sentence of a bare skeleton, which has no
    /// manifest to say.
    pub tier: Option<Tier>,
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

/// What the tier column says of a sentence whose tier is not known.
const UNKNOWN_TIER: &str = "unknown";

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
                meta.tier.map_or(UNKNOWN_TIER, Tier::name),
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
                tier: if cells[2] == UNKNOWN_TIER {
                    None
                } else {
                    Some(Tier::from_name(cells[2]).ok_or_else(|| bad("bad tier", cells[2]))?)
                },
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

    /// The sample in `dir`: `sample.conllu` and the manifest beside it, or, where `dir` has no
    /// `manifest.tsv`, the skeleton alone, which is how `deslag-exam tokens --gold` writes a gold
    /// file's sentences. A bare sample has each sentence's context from its `# exam.context`, and
    /// no split, tier, file or repository.
    ///
    /// Nothing the labelling flow reads or writes may hold a holdout sentence, so a directory
    /// under `.label`, by its path as written or by its real path, is read by an allow-list and
    /// not a deny-list. Its real path is taken first, and checked before any file is opened:
    /// symlinks and `..` are followed, no file may be a link out of the directory, and no part of
    /// a real path may name holdout. Then it is opened only if it is
    ///
    /// - a skeleton of the dev or the owner gold, with no manifest, which is the very text
    ///   `deslag-exam tokens --gold tests/gold/<from>.conllu` writes now: `exam.from` only says
    ///   which gold to write it from, and the text is compared whole, so a header on some other
    ///   text, a hand-made file or a hard link to one proves nothing; or
    /// - a draw for labelling: a manifest that says `draw = for labelling ...`, whose every row is
    ///   `unlabelled`.
    ///
    /// A copy of the gold flow's `.gold/sample.conllu`, which mixes holdout in and keeps its split
    /// only in its manifest, says neither and is refused. A skeleton that says
    /// `exam.split = holdout`, as `deslag-exam tokens` writes for a holdout gold, is refused
    /// wherever it is. The gold flow's own `.gold` is not under `.label` and is read as before.
    pub fn open(dir: &Path) -> Result<Sample, Problems> {
        Sample::open_in(dir, &gold_dir())
    }

    /// [Sample::open], with the golds a skeleton is compared with in `golds`.
    pub fn open_in(dir: &Path, golds: &Path) -> Result<Sample, Problems> {
        let real = real_path(dir)?;
        let labelling = in_label_place(dir) || in_label_place(&real);
        let sample_path = dir.join("sample.conllu");
        let manifest_path = dir.join("manifest.tsv");
        if labelling {
            refuse_holdout(&real)?;
            for file in [&sample_path, &manifest_path] {
                if file.exists() {
                    let linked = real_path(file)?;
                    refuse_holdout(&linked)?;
                    // The Make targets write these files afresh, so a hard link is not theirs.
                    if std::fs::metadata(&linked).is_ok_and(|meta| meta.nlink() > 1) {
                        return Err(Error::load(
                            &file.display().to_string(),
                            Place::File,
                            "it is a hard link, and the labelling flow reads only files its targets wrote",
                        )
                        .into());
                    }
                    if linked.parent() != Some(real.as_path()) {
                        return Err(Error::load(
                            &file.display().to_string(),
                            Place::File,
                            "it is a link to a file outside its directory, which the labelling flow does not follow",
                        )
                        .into());
                    }
                }
            }
        }
        let shown = sample_path.display().to_string();
        let text = read_text(&sample_path)?;
        let blocks = conllu::read(&shown, &text)?;
        let first = |key: &str| {
            blocks
                .first()
                .and_then(|block| block.comment(key))
                .map(|comment| comment.value.as_str())
        };
        if first("exam.split") == Some(Split::Holdout.name()) {
            return Err(Error::load(
                &shown,
                Place::File,
                "this skeleton came from a holdout gold, which no labelling stage may read",
            )
            .into());
        }
        let sample = if manifest_path.exists() {
            Sample::read(&sample_path, &manifest_path)?
        } else {
            let sents = parse_skeleton(&shown, &text)?;
            let rows = sents
                .iter()
                .zip(&blocks)
                .map(|(sent, block)| {
                    let context = block
                        .comment("exam.context")
                        .and_then(|comment| Context::from_name(&comment.value))
                        .unwrap_or(Context::Prose);
                    (
                        sent.id.clone(),
                        Meta {
                            split: None,
                            tier: None,
                            context,
                            file: String::new(),
                            repo: String::new(),
                            license: String::new(),
                            range: 0..0,
                            provenance: None,
                        },
                    )
                })
                .collect();
            Sample {
                sents,
                manifest: Manifest {
                    header: vec![("source".to_string(), "a bare skeleton".to_string())],
                    rows,
                },
            }
        };
        if labelling {
            let held = sample
                .manifest
                .rows
                .iter()
                .filter(|(_, meta)| meta.split == Some(Split::Holdout))
                .count();
            let headed = sample
                .manifest
                .header
                .iter()
                .any(|(key, value)| key == "split" && value == Split::Holdout.name());
            if held > 0 || headed {
                return Err(Error::load(
                    &manifest_path.display().to_string(),
                    Place::File,
                    format!(
                        "the manifest has {held} holdout rows, and nothing under `{LABEL_DIR}` may hold holdout"
                    ),
                )
                .into());
            }
            let allowed = if manifest_path.exists() {
                sample.is_labelling_draw()
            } else {
                match first(exam_skeleton::FROM).filter(|from| LABELLED_FROM.contains(from)) {
                    Some(from) => {
                        let gold = deslag_exam::gold::Gold::read(&graded_gold(golds, from)?)?;
                        if exam_skeleton::skeleton(&gold) != text {
                            return Err(Error::load(
                                &shown,
                                Place::File,
                                format!(
                                    "it is not what `deslag-exam tokens --gold tests/gold/{from}.conllu` writes now, \
                                     so it is not a sample the labelling flow made; run the generate-label target again"
                                ),
                            )
                            .into());
                        }
                        true
                    }
                    None => false,
                }
            };
            if !allowed {
                return Err(Error::load(
                    &shown,
                    Place::File,
                    format!(
                        "under `{LABEL_DIR}` only a skeleton of tests/gold/dev.conllu or owner.conllu (`{}` says which) \
                         or a labelling draw (a manifest that says `draw = for labelling`, every row `unlabelled`) is read",
                        exam_skeleton::FROM
                    ),
                )
                .into());
            }
        }
        Ok(sample)
    }

    /// Whether this is a draw for labelling: its manifest says so, and every row is unlabelled.
    pub fn is_labelling_draw(&self) -> bool {
        self.manifest
            .get("draw")
            .is_some_and(|draw| draw.starts_with("for labelling"))
            && self
                .manifest
                .rows
                .iter()
                .all(|(_, meta)| meta.split.is_none())
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

/// Whether `dir` is, or is inside, a directory named `.label`: where the labelling flow keeps
/// everything, and where no holdout sentence may be.
pub fn in_label_place(dir: &Path) -> bool {
    dir.components()
        .any(|part| part.as_os_str() == std::ffi::OsStr::new(LABEL_DIR))
}

/// The golds a labelling skeleton may be made from, as `exam.from` says them.
const LABELLED_FROM: [&str; 2] = ["dev", "owner"];

/// The variable that names the directory of the dev and owner golds, for the tests.
pub const GOLD_DIR_VARIABLE: &str = "DESLAG_GOLD_DIR";

/// The directory of the dev and owner golds: `tests/gold` of the checkout this was built in, or
/// the one `DESLAG_GOLD_DIR` names, which the tests use for golds of their own.
pub fn gold_dir() -> PathBuf {
    std::env::var_os(GOLD_DIR_VARIABLE).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/gold"),
        PathBuf::from,
    )
}

/// The real path of the gold `name` (`dev` or `owner`) in `golds`.
pub fn graded_gold(golds: &Path, name: &str) -> Result<PathBuf, Error> {
    real_path(&golds.join(format!("{name}.conllu")))
}

/// `path` with every symlink and `..` resolved, as the file system has it.
pub fn real_path(path: &Path) -> Result<PathBuf, Error> {
    std::fs::canonicalize(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Refuses a path that names holdout or an English Web Treebank file: a component that starts with
/// `holdout`, or that holds `en_ewt` or `en-ewt`, or is `.ewt`. The labelling flow grades against dev and owner
/// only, and checks the path before it opens anything.
pub fn refuse_holdout(path: &Path) -> Result<(), Error> {
    let barred = path.components().any(|part| {
        let name = part.as_os_str().to_string_lossy().to_lowercase();
        name.starts_with("holdout")
            || name.contains("en_ewt")
            || name.contains("en-ewt")
            || name == ".ewt"
    });
    if barred {
        return Err(Error::load(
            &path.display().to_string(),
            Place::File,
            "holdout and EWT files are never read by the labelling flow",
        ));
    }
    Ok(())
}

/// The directory the labelling flow keeps its files in, which git ignores.
pub const LABEL_DIR: &str = ".label";

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
            let mut origin = Origin::English;
            for (key, value) in conllu::pairs(&line.misc) {
                match key {
                    "Origin" => {
                        origin = Origin::from_name(value).ok_or_else(|| {
                            Error::at(path, line.number, format!("unknown Origin `{value}`"))
                        })?
                    }
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
            // Only a word has an origin; a skeleton that gives one to another token is read as
            // English there, as the exam's own skeleton never writes it.
            let origin = if kind == TokenKind::Word {
                origin
            } else {
                Origin::English
            };
            toks.push(Tok {
                form: line.form.clone(),
                kind,
                joined,
                origin,
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
            origin: Origin::English,
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
            tier: Some(Tier::Llm),
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
                    tier: Some(Tier::Llm),
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

    /// A gold of one sentence, as the dev or owner gold stands in for the real ones.
    const GOLD: &str = "# exam.tokens = deslag\n# exam.split = dev\n# exam.trains = undecided\n\
        # exam.source = hand-made\n# sent_id = g0001\n# exam.context = list-item\n\
        # text = Run it now.\n\
        1\tRun\t_\tVERB\t_\tVerbForm=Fin\t_\t_\t_\tKind=Word|Prov=agree\n\
        2\tit\t_\tPRON\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n\
        3\tnow\t_\tADV\t_\t_\t_\t_\t_\tKind=Word|Prov=agree|SpaceAfter=No\n\
        4\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind\n\n";

    /// A directory of golds `dev.conllu` and `owner.conllu` in `root`, and its path.
    fn golds(root: &Path) -> PathBuf {
        let dir = root.join("golds");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["dev", "owner"] {
            std::fs::write(dir.join(format!("{name}.conllu")), GOLD).unwrap();
        }
        dir
    }

    /// What `deslag-exam tokens --gold <root>/golds/dev.conllu` writes.
    fn made_text(root: &Path) -> String {
        let gold = deslag_exam::gold::Gold::read(&golds(root).join("dev.conllu")).unwrap();
        exam_skeleton::skeleton(&gold)
    }

    /// The sample `deslag-exam tokens` writes for the dev gold under `.label/<name>` in `root`,
    /// with `extra` as comments after the line that says what it was made from.
    fn label_dir(root: &Path, name: &str, extra: &str) -> PathBuf {
        label_dir_from(root, name, &format!("# exam.from = dev\n{extra}"))
    }

    /// The same with `extra` in place of the line that says what it was made from.
    fn label_dir_from(root: &Path, name: &str, extra: &str) -> PathBuf {
        let dir = root.join(LABEL_DIR).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let text = made_text(root).replacen("# exam.from = dev\n", extra, 1);
        std::fs::write(dir.join("sample.conllu"), text).unwrap();
        dir
    }

    /// [Sample::open] with the golds of [golds] in `root`.
    fn open(root: &Path, dir: &Path) -> Result<Sample, Problems> {
        Sample::open_in(dir, &golds(root))
    }

    #[test]
    fn a_bare_skeleton_opens_with_its_contexts_and_no_tier_or_split() {
        let root = tempfile::tempdir().unwrap();
        let dir = label_dir(root.path(), "dev", "");
        let sample = open(root.path(), &dir).unwrap();
        assert_eq!(sample.sents.len(), 1);
        let meta = sample.meta("g0001").unwrap();
        assert_eq!(meta.context, Context::ListItem);
        assert_eq!((meta.tier, meta.split), (None, None));
        assert!(in_label_place(&dir));
    }

    #[test]
    fn under_label_only_a_skeleton_of_dev_or_owner_or_a_labelling_draw_opens() {
        let root = tempfile::tempdir().unwrap();
        // No header: a copy of `.gold/sample.conllu`, which mixes holdout in, or any skeleton whose
        // header was stripped.
        let bare = label_dir_from(root.path(), "bare", "");
        let error = open(root.path(), &bare).unwrap_err().to_string();
        assert!(error.contains("exam.from"), "{error}");
        // Any gold's stem other than dev or owner, holdout's among them.
        for stem in ["holdout", "train", "x"] {
            let dir = label_dir_from(root.path(), stem, &format!("# exam.from = {stem}\n"));
            assert!(open(root.path(), &dir).is_err(), "{stem}");
        }
        for stem in ["dev", "owner"] {
            let dir = label_dir_from(root.path(), stem, &format!("# exam.from = {stem}\n"));
            assert!(open(root.path(), &dir).is_ok(), "{stem}");
        }
        // A manifest with the gold flow's rows is no labelling draw, whatever its sample says.
        let mixed = label_dir(root.path(), "mixed", "");
        let row = |split| Manifest {
            header: Vec::new(),
            rows: vec![("g0001".to_string(), meta_of(split))],
        };
        std::fs::write(mixed.join("manifest.tsv"), row(Some(Split::Dev)).render()).unwrap();
        assert!(open(root.path(), &mixed).is_err());
        // A draw for labelling is: its header says so and no row has a split.
        let mut draw = row(None);
        draw.header.push((
            "draw".to_string(),
            "for labelling, split unlabelled, ids x0001 on".to_string(),
        ));
        let drawn = label_dir_from(root.path(), "draw", "");
        std::fs::write(drawn.join("manifest.tsv"), draw.render()).unwrap();
        assert!(open(root.path(), &drawn).is_ok());
        assert!(open(root.path(), &drawn).unwrap().is_labelling_draw());
        // And a draw header over rows that are dev is not.
        draw.rows[0].1.split = Some(Split::Dev);
        std::fs::write(drawn.join("manifest.tsv"), draw.render()).unwrap();
        assert!(open(root.path(), &drawn).is_err());
    }

    #[test]
    fn a_header_on_other_text_or_a_hard_link_proves_nothing() {
        let root = tempfile::tempdir().unwrap();
        let dir = label_dir(root.path(), "forged", "");
        assert!(open(root.path(), &dir).is_ok());
        // The header says dev and the sentences are somebody else's.
        let text = std::fs::read_to_string(dir.join("sample.conllu")).unwrap();
        std::fs::write(
            dir.join("sample.conllu"),
            text.replace("Run it now", "Held it back"),
        )
        .unwrap();
        let error = open(root.path(), &dir).unwrap_err().to_string();
        assert!(
            error.contains("not what `deslag-exam tokens --gold"),
            "{error}"
        );
        assert!(!error.contains("Held"), "no text in the error: {error}");
        // The split moved off the first block, as a holdout gold's text could have it.
        std::fs::write(
            dir.join("sample.conllu"),
            text.replace(
                "# sent_id = g0001",
                "# sent_id = g0001\n# exam.split = holdout",
            ),
        )
        .unwrap();
        assert!(open(root.path(), &dir).is_err());
        // Extra text after the sentences.
        std::fs::write(dir.join("sample.conllu"), format!("{text}# sent_id = h1\n")).unwrap();
        assert!(open(root.path(), &dir).is_err());
        // A hard link, even to text that is right.
        let linked = root.path().join(LABEL_DIR).join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::write(root.path().join("elsewhere.conllu"), &text).unwrap();
        std::fs::hard_link(
            root.path().join("elsewhere.conllu"),
            linked.join("sample.conllu"),
        )
        .unwrap();
        let error = open(root.path(), &linked).unwrap_err().to_string();
        assert!(error.contains("hard link"), "{error}");
        // The same text a target wrote is read.
        std::fs::write(dir.join("sample.conllu"), &text).unwrap();
        assert!(open(root.path(), &dir).is_ok());
    }

    #[test]
    fn a_link_or_a_dot_dot_cannot_take_a_holdout_sample_into_label() {
        let root = tempfile::tempdir().unwrap();
        // A directory named for holdout, holding a sample that says nothing of it.
        let hold = root.path().join("holdout-copy");
        std::fs::create_dir_all(&hold).unwrap();
        let ok = label_dir(root.path(), "ok", "");
        std::fs::copy(ok.join("sample.conllu"), hold.join("sample.conllu")).unwrap();
        // `.label/decoy/sample.conllu` is a link into it.
        let decoy = root.path().join(LABEL_DIR).join("decoy");
        std::fs::create_dir_all(&decoy).unwrap();
        std::os::unix::fs::symlink(hold.join("sample.conllu"), decoy.join("sample.conllu"))
            .unwrap();
        let error = open(root.path(), &decoy).unwrap_err().to_string();
        assert!(error.contains("holdout"), "{error}");
        // `.label/hop` is a link to the holdout directory itself.
        let hop = root.path().join(LABEL_DIR).join("hop");
        std::os::unix::fs::symlink(&hold, &hop).unwrap();
        assert!(open(root.path(), &hop).is_err());
        // A sample linked from a harmless place outside its directory is refused all the same.
        let elsewhere = root.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::copy(ok.join("sample.conllu"), elsewhere.join("sample.conllu")).unwrap();
        let linked = root.path().join(LABEL_DIR).join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(
            elsewhere.join("sample.conllu"),
            linked.join("sample.conllu"),
        )
        .unwrap();
        assert!(open(root.path(), &linked).is_err());
        // `..` is resolved: this reaches the holdout directory from a `.label` path.
        let dotted = ok.join("..").join("..").join("holdout-copy");
        assert!(open(root.path(), &dotted).is_err());
        // A link to the gold flow's directory is read as label, so its mixed manifest is refused.
        let gold = root.path().join(".gold");
        std::fs::create_dir_all(&gold).unwrap();
        std::fs::copy(ok.join("sample.conllu"), gold.join("sample.conllu")).unwrap();
        std::fs::write(
            gold.join("manifest.tsv"),
            Manifest {
                header: Vec::new(),
                rows: vec![("g0001".to_string(), meta_of(Some(Split::Holdout)))],
            }
            .render(),
        )
        .unwrap();
        let via = root.path().join(LABEL_DIR).join("via");
        std::os::unix::fs::symlink(&gold, &via).unwrap();
        assert!(open(root.path(), &via).is_err());
    }

    /// A manifest row's meta, with the split `split`.
    fn meta_of(split: Option<Split>) -> Meta {
        Meta {
            split,
            tier: Some(Tier::Human),
            context: Context::Prose,
            file: "f.md".to_string(),
            repo: "o/r".to_string(),
            license: "MIT".to_string(),
            range: 0..1,
            provenance: None,
        }
    }

    #[test]
    fn a_skeleton_of_a_holdout_gold_is_refused_wherever_it_is() {
        let root = tempfile::tempdir().unwrap();
        let dir = label_dir(root.path(), "hold", "# exam.split = holdout\n");
        let error = open(root.path(), &dir).unwrap_err().to_string();
        assert!(error.contains("holdout"), "{error}");
        assert!(!error.contains("Run"), "no text in the error: {error}");
        // Outside `.label` too.
        let elsewhere = root.path().join("anywhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::copy(dir.join("sample.conllu"), elsewhere.join("sample.conllu")).unwrap();
        assert!(open(root.path(), &elsewhere).is_err());
    }

    #[test]
    fn a_manifest_row_or_header_that_says_holdout_is_refused_under_label_only() {
        let root = tempfile::tempdir().unwrap();
        let dir = label_dir(root.path(), "x", "");
        let rows = |split: Option<Split>| Manifest {
            header: Vec::new(),
            rows: vec![(
                "g0001".to_string(),
                Meta {
                    split,
                    tier: Some(Tier::Human),
                    context: Context::Prose,
                    file: "f.md".to_string(),
                    repo: "o/r".to_string(),
                    license: "MIT".to_string(),
                    range: 0..1,
                    provenance: None,
                },
            )],
        };
        std::fs::write(
            dir.join("manifest.tsv"),
            rows(Some(Split::Holdout)).render(),
        )
        .unwrap();
        let error = open(root.path(), &dir).unwrap_err().to_string();
        assert!(error.contains("1 holdout rows"), "{error}");
        // Dev rows are no draw for labelling either: only a draw's manifest is read here.
        std::fs::write(dir.join("manifest.tsv"), rows(Some(Split::Dev)).render()).unwrap();
        assert!(open(root.path(), &dir).is_err());
        // A header `split = holdout` is as bad.
        let mut headed = rows(Some(Split::Dev));
        headed
            .header
            .push(("split".to_string(), "holdout".to_string()));
        std::fs::write(dir.join("manifest.tsv"), headed.render()).unwrap();
        assert!(open(root.path(), &dir).is_err());
        // The gold flow's directory is not under `.label`, and mixes holdout in.
        let gold_flow = root.path().join(".gold");
        std::fs::create_dir_all(&gold_flow).unwrap();
        std::fs::copy(dir.join("sample.conllu"), gold_flow.join("sample.conllu")).unwrap();
        std::fs::write(
            gold_flow.join("manifest.tsv"),
            rows(Some(Split::Holdout)).render(),
        )
        .unwrap();
        assert!(open(root.path(), &gold_flow).is_ok());
    }

    #[test]
    fn holdout_and_ewt_paths_are_refused_before_anything_is_read() {
        for barred in [
            "tests/gold/holdout.conllu",
            "holdout-copy.conllu",
            "/tmp/Holdout/dev.conllu",
            "en_ewt-ud-test.conllu",
            "en-ewt-ud-test.conllu",
            ".ewt/train.conllu",
        ] {
            assert!(refuse_holdout(Path::new(barred)).is_err(), "{barred}");
        }
        for allowed in [
            "tests/gold/dev.conllu",
            "tests/gold/owner.conllu",
            ".label/dev",
        ] {
            assert!(refuse_holdout(Path::new(allowed)).is_ok(), "{allowed}");
        }
    }
}
