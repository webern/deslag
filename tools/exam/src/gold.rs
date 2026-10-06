//! A gold file: CoNLL-U as UD defines it, plus the conventions the exam reads.
//!
//! The conventions are comments prefixed `exam.`, so they never collide with UD's own. Some are
//! about the file and may appear in the first sentence only (`tokens`, `split`, `trains`,
//! `source`); `tier` and `context` may appear in any sentence. In `MISC`, `Kind=` and
//! `SpaceAfter=No` describe a `deslag` file's tokens, `Prov=` says who vouches for a label and
//! `Was=` notes what it was before the owner reviewed it. Any key or value outside the lists below
//! is a load error, and so is every other way a file can disagree with the conventions: the exam
//! never guesses at a gold file.

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;

use deslag::document::{Token, TokenKind};
use deslag::tag::Origin;
use sha2::{Digest, Sha256};

use crate::conllu::{self, Block, Id};
use crate::error::{Error, Place};
use crate::tagger::Context;
use crate::tags::{Class, Features, from_ud, map_upos};

/// Whose tokens a gold file's lines are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenMode {
    /// The file splits words its own way, as UD does, and the exam aligns them to deslag's.
    Ud,
    /// One word line per deslag token, in order, punctuation, code and URLs included.
    Deslag,
}

/// Which part of a larger set a gold file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// Labels to train on.
    Train,
    /// Labels to tune on.
    Dev,
    /// Labels to report on.
    Test,
    /// Labels kept from every model and every reader: its reports name no word.
    Holdout,
}

/// Whether a gold file's labels may train a model whose weights ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trains {
    /// They may.
    Yes,
    /// They may not.
    No,
    /// Not yet decided, which is the default, and counts as no.
    Undecided,
}

/// Who wrote a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// A person.
    Human,
    /// A language model.
    Llm,
    /// Both.
    Mixed,
}

/// Who vouches for a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prov {
    /// The three taggers of the gold set's own build agreed.
    Agree,
    /// A model decided between them.
    Adjudicated,
    /// A gold dispute corrected it.
    Corrected,
    /// The owner edited it by hand, and it is final.
    Owner,
    /// No tagger decided it: the line is a token that is not a word, and its UPOS comes from the
    /// token's kind.
    Kind,
}

macro_rules! named {
    ($ty:ident { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $ty {
            /// Every value, in the order the report lists them.
            pub const ALL: &'static [$ty] = &[$($ty::$variant),+];

            /// The name a gold file writes it as.
            pub fn name(self) -> &'static str {
                match self { $($ty::$variant => $name),+ }
            }

            /// Its place in [`Self::ALL`].
            pub fn index(self) -> usize {
                self as usize
            }

            /// The value written `name`.
            pub fn from_name(name: &str) -> Option<$ty> {
                match name { $($name => Some($ty::$variant),)+ _ => None }
            }
        }
    };
}

named!(TokenMode { Ud => "ud", Deslag => "deslag" });
named!(Split { Train => "train", Dev => "dev", Test => "test", Holdout => "holdout" });
named!(Trains { Yes => "yes", No => "no", Undecided => "undecided" });
named!(Tier { Human => "human", Llm => "llm", Mixed => "mixed" });
named!(Prov {
    Agree => "agree",
    Adjudicated => "adjudicated",
    Corrected => "corrected",
    Owner => "owner",
    Kind => "kind",
});

/// The name a `deslag` file's `Kind=` uses for `kind`, which is the variant's name.
pub fn kind_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Word => "Word",
        TokenKind::Number => "Number",
        TokenKind::Punctuation => "Punctuation",
        TokenKind::Symbol => "Symbol",
        TokenKind::Code => "Code",
        TokenKind::Html => "Html",
        TokenKind::Image => "Image",
        TokenKind::Url => "Url",
        TokenKind::Footnote => "Footnote",
    }
}

/// The token kind a `Kind=` value names.
pub fn kind_from_name(name: &str) -> Option<TokenKind> {
    [
        TokenKind::Word,
        TokenKind::Number,
        TokenKind::Punctuation,
        TokenKind::Symbol,
        TokenKind::Code,
        TokenKind::Html,
        TokenKind::Image,
        TokenKind::Url,
        TokenKind::Footnote,
    ]
    .into_iter()
    .find(|kind| kind_name(*kind) == name)
}

/// A gold file, read and checked.
#[derive(Debug, Clone)]
pub struct Gold {
    /// The path it was read from, as given.
    pub path: String,
    /// The SHA-256 of its bytes, in lower-case hex.
    pub sha256: String,
    /// `exam.source`, or the file's name.
    pub source: String,
    /// `exam.split`, if it says.
    pub split: Option<Split>,
    /// `exam.trains`.
    pub trains: Trains,
    /// `exam.tokens`.
    pub tokens: TokenMode,
    /// Its sentences, in order.
    pub sentences: Vec<GoldSentence>,
}

/// One sentence of a gold file.
#[derive(Debug, Clone)]
pub struct GoldSentence {
    /// Its `sent_id`, unique in the file.
    pub sent_id: String,
    /// `exam.tier`, if it says.
    pub tier: Option<Tier>,
    /// `exam.context`.
    pub context: Context,
    /// The text its tokens index into: `# text` for a `ud` file, the forms joined for a `deslag`
    /// one.
    pub text: String,
    /// Its words, in order. Range lines and empty nodes are not words.
    pub words: Vec<Word>,
    /// Its surface units, in order: a range line with the words under it, or a word on its own.
    pub units: Vec<Unit>,
    /// A `deslag` file's token ranges in `text`, one per word. Empty for a `ud` file.
    pub spans: Vec<Range<usize>>,
    mode: TokenMode,
}

/// A word line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// The `FORM` column.
    pub form: String,
    /// What its UPOS makes of it.
    pub class: Class,
    /// The flags of its `FEATS`, for a tagged word. None for the others.
    pub features: Features,
    /// A `deslag` file's `Kind=`.
    pub kind: Option<TokenKind>,
    /// `Prov=`, if the line says.
    pub prov: Option<Prov>,
    /// `Was=`, if the line says: what the line's `Prov=` was before the owner reviewed it, or
    /// `prefill` for a word he accepted as deslag's reading filled it in. A note; nothing is
    /// graded by it.
    pub was: Option<String>,
    /// The line it is on.
    pub line: usize,
}

/// A stretch of the text the gold names as one thing: the words of a range line share its form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The range line's `FORM`, or the word's.
    pub form: String,
    /// The index in [`GoldSentence::words`] of its first word.
    pub first: usize,
    /// How many words it holds.
    pub count: usize,
}

impl Gold {
    /// Reads and checks the gold file at `path`.
    pub fn read(path: &Path) -> Result<Gold, Error> {
        let shown = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: shown.clone(),
            source,
        })?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::load(&shown, Place::File, "the file is not UTF-8"))?;
        let name = path
            .file_name()
            .map_or_else(|| shown.clone(), |name| name.to_string_lossy().into_owned());
        Gold::parse(&shown, &name, &text)
    }

    /// Checks `text`, the contents of the file `path` whose name is `name`.
    pub fn parse(path: &str, name: &str, text: &str) -> Result<Gold, Error> {
        let blocks = conllu::read(path, text)?;
        let Some(first) = blocks.first() else {
            return Err(Error::load(path, Place::File, "the file has no sentences"));
        };
        let head = Head::of(path, first, name)?;
        let mut gold = Gold {
            path: path.to_string(),
            sha256: hex(&Sha256::digest(text.as_bytes())),
            source: head.source,
            split: head.split,
            trains: head.trains,
            tokens: head.tokens,
            sentences: Vec::with_capacity(blocks.len()),
        };
        let mut seen = BTreeSet::new();
        for (index, block) in blocks.iter().enumerate() {
            let sentence = GoldSentence::read(path, gold.tokens, index, gold.holdout(), block)?;
            if !seen.insert(sentence.sent_id.clone()) {
                let line = block
                    .comment("sent_id")
                    .map_or(block.first_line, |c| c.line);
                // Holdout text never names a sentence but by its position.
                let message = if gold.holdout() {
                    format!("sentence {}: its sent_id is used twice", index + 1)
                } else {
                    format!("sent_id `{}` is used twice", sentence.sent_id)
                };
                return Err(Error::at(path, line, message));
            }
            gold.sentences.push(sentence);
        }
        Ok(gold)
    }

    /// Whether it is holdout text, whose reports must name no word.
    pub fn holdout(&self) -> bool {
        self.split == Some(Split::Holdout)
    }

    /// The first 12 hex digits of its SHA-256.
    pub fn sha12(&self) -> &str {
        &self.sha256[..12]
    }
}

impl GoldSentence {
    /// The tokens a tagger is given: deslag's split of the text for a `ud` file, one token per
    /// line for a `deslag` file.
    pub fn tokens(&self) -> Vec<Token<'_>> {
        match self.mode {
            TokenMode::Ud => Token::split(&self.text),
            TokenMode::Deslag => self
                .words
                .iter()
                .zip(&self.spans)
                .map(|(word, span)| Token {
                    kind: word.kind.unwrap_or(TokenKind::Word),
                    range: span.clone(),
                    text: self.text[span.clone()].into(),
                    reading: None,
                    origin: Origin::English,
                })
                .collect(),
        }
    }

    /// Whose tokens the lines are.
    pub fn mode(&self) -> TokenMode {
        self.mode
    }

    fn read(
        path: &str,
        mode: TokenMode,
        index: usize,
        holdout: bool,
        block: &Block,
    ) -> Result<GoldSentence, Error> {
        let sent_id = block
            .comment("sent_id")
            .map(|comment| comment.value.clone())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::at(path, block.first_line, "no `# sent_id = ` comment"))?;
        // How an error names this sentence: by its `sent_id`, or by position in a holdout file.
        let place = || {
            Place::Sentence(if holdout {
                (index + 1).to_string()
            } else {
                sent_id.clone()
            })
        };
        let mut tier = None;
        let mut context = Context::Prose;
        let mut seen = BTreeSet::new();
        for comment in &block.comments {
            if !comment.key.starts_with("exam.") {
                continue;
            }
            if !seen.insert(comment.key.as_str()) {
                return Err(Error::at(
                    path,
                    comment.line,
                    format!("`{}` twice", comment.key),
                ));
            }
            match comment.key.as_str() {
                "exam.tier" => {
                    tier = Some(choose(
                        path,
                        comment,
                        Tier::from_name,
                        Tier::ALL,
                        Tier::name,
                    )?)
                }
                "exam.context" => {
                    context = Context::from_name(&comment.value)
                        .ok_or_else(|| bad_value(path, comment, &Context::ALL.map(Context::name)))?
                }
                key if FILE_KEYS.contains(&key) => {
                    if index > 0 {
                        return Err(Error::at(
                            path,
                            comment.line,
                            format!(
                                "`{key}` is about the file, so it belongs in the first sentence only"
                            ),
                        ));
                    }
                }
                key => {
                    return Err(Error::at(
                        path,
                        comment.line,
                        format!("unknown key `{key}`"),
                    ));
                }
            }
        }

        let mut words: Vec<Word> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut spans: Vec<Range<usize>> = Vec::new();
        let mut text = String::new();
        let mut inside = 0usize;
        for line in &block.lines {
            match line.id {
                Id::Empty if mode == TokenMode::Deslag => {
                    return Err(Error::at(
                        path,
                        line.number,
                        "an empty node in a file with exam.tokens = deslag",
                    ));
                }
                Id::Range(..) if mode == TokenMode::Deslag => {
                    return Err(Error::at(
                        path,
                        line.number,
                        "a range line in a file with exam.tokens = deslag",
                    ));
                }
                Id::Empty => {}
                Id::Range(first, last) => {
                    inside = (last - first + 1) as usize;
                    units.push(Unit {
                        form: line.form.clone(),
                        first: words.len(),
                        count: inside,
                    });
                }
                Id::Word(_) => {
                    let (word, space_after) = Word::read(path, mode, holdout, line)?;
                    if inside > 0 {
                        inside -= 1;
                    } else {
                        units.push(Unit {
                            form: line.form.clone(),
                            first: words.len(),
                            count: 1,
                        });
                    }
                    if mode == TokenMode::Deslag {
                        let start = text.len();
                        text.push_str(&word.form);
                        spans.push(start..text.len());
                        if space_after {
                            text.push(' ');
                        }
                    }
                    words.push(word);
                }
            }
        }
        if words.is_empty() {
            return Err(Error::load(path, place(), "no words"));
        }
        let text = match mode {
            TokenMode::Ud => block
                .comment("text")
                .map(|comment| comment.value.clone())
                .ok_or_else(|| {
                    Error::load(
                        path,
                        place(),
                        "no `# text = ` comment, which a file with exam.tokens = ud needs",
                    )
                })?,
            TokenMode::Deslag => text.trim_end().to_string(),
        };
        Ok(GoldSentence {
            sent_id,
            tier,
            context,
            text,
            words,
            units,
            spans,
            mode,
        })
    }
}

impl Word {
    /// The word on `line`, and whether a space follows it. On holdout text no error echoes a value
    /// of the line: a line whose columns are shifted would put a word in one.
    fn read(
        path: &str,
        mode: TokenMode,
        holdout: bool,
        line: &conllu::Line,
    ) -> Result<(Word, bool), Error> {
        let class = map_upos(&line.upos).ok_or_else(|| {
            let what = match line.upos.as_str() {
                "_" => "a word line with no UPOS".to_string(),
                _ if holdout => "UPOS is not one of the 17 UD tags".to_string(),
                other => format!("UPOS `{other}` is not one of the 17 UD tags"),
            };
            Error::at(path, line.number, what)
        })?;
        let features = match class {
            Class::Tagged(_) => from_ud(&line.feats).map_err(|message| {
                let message = if holdout {
                    "a FEATS entry has no value".to_string()
                } else {
                    message
                };
                Error::at(path, line.number, message)
            })?,
            _ => Features::NONE,
        };
        let mut kind = None;
        let mut prov = None;
        let mut was = None;
        let mut space_after = true;
        for (key, value) in conllu::pairs(&line.misc) {
            match key {
                "SpaceAfter" => space_after = value != "No",
                "Was" => was = Some(value.to_string()),
                "Prov" => {
                    prov = Some(Prov::from_name(value).ok_or_else(|| {
                        let names = Prov::ALL.iter().map(|p| p.name()).collect::<Vec<_>>();
                        let shown = if holdout {
                            "Prov is unknown".to_string()
                        } else {
                            format!("unknown Prov `{value}`")
                        };
                        Error::at(
                            path,
                            line.number,
                            format!("{shown}; it is one of {}", names.join(", ")),
                        )
                    })?)
                }
                "Kind" if mode == TokenMode::Deslag => {
                    kind = Some(kind_from_name(value).ok_or_else(|| {
                        let what = if holdout {
                            "Kind is unknown".to_string()
                        } else {
                            format!("unknown Kind `{value}`")
                        };
                        Error::at(path, line.number, what)
                    })?)
                }
                _ => {}
            }
        }
        if mode == TokenMode::Deslag {
            if kind.is_none() {
                return Err(Error::at(
                    path,
                    line.number,
                    "no Kind= in MISC, which a deslag file needs on every line",
                ));
            }
            if prov.is_none() {
                return Err(Error::at(
                    path,
                    line.number,
                    "no Prov= in MISC, which a deslag file needs on every line",
                ));
            }
        }
        let word = Word {
            form: line.form.clone(),
            class,
            features,
            kind,
            prov,
            was,
            line: line.number,
        };
        Ok((word, space_after))
    }
}

/// The comments that are about the file.
const FILE_KEYS: [&str; 4] = ["exam.tokens", "exam.split", "exam.trains", "exam.source"];

/// What the first sentence says about the file.
struct Head {
    tokens: TokenMode,
    split: Option<Split>,
    trains: Trains,
    source: String,
}

impl Head {
    fn of(path: &str, first: &Block, name: &str) -> Result<Head, Error> {
        let mut head = Head {
            tokens: TokenMode::Ud,
            split: None,
            trains: Trains::Undecided,
            source: name.to_string(),
        };
        for comment in &first.comments {
            match comment.key.as_str() {
                "exam.tokens" => {
                    head.tokens = choose(
                        path,
                        comment,
                        TokenMode::from_name,
                        TokenMode::ALL,
                        TokenMode::name,
                    )?
                }
                "exam.split" => {
                    head.split = Some(choose(
                        path,
                        comment,
                        Split::from_name,
                        Split::ALL,
                        Split::name,
                    )?)
                }
                "exam.trains" => {
                    head.trains =
                        choose(path, comment, Trains::from_name, Trains::ALL, Trains::name)?
                }
                "exam.source" => head.source = comment.value.clone(),
                _ => {}
            }
        }
        if head.split == Some(Split::Holdout) && head.trains != Trains::No {
            let line = first
                .comment("exam.split")
                .map_or(first.first_line, |c| c.line);
            return Err(Error::at(
                path,
                line,
                format!(
                    "exam.split = holdout needs exam.trains = no, and this file's is {}: holdout is never training data",
                    head.trains.name()
                ),
            ));
        }
        Ok(head)
    }
}

/// The value of `comment` as one of `all`, or a load error listing them.
fn choose<T: Copy>(
    path: &str,
    comment: &conllu::Comment,
    parse: fn(&str) -> Option<T>,
    all: &[T],
    name: fn(T) -> &'static str,
) -> Result<T, Error> {
    parse(&comment.value).ok_or_else(|| {
        let names: Vec<&str> = all.iter().map(|value| name(*value)).collect();
        bad_value(path, comment, &names)
    })
}

fn bad_value(path: &str, comment: &conllu::Comment, names: &[&str]) -> Error {
    Error::at(
        path,
        comment.line,
        format!(
            "`{}` is `{}`, and it must be one of {}",
            comment.key,
            comment.value,
            names.join(", ")
        ),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
