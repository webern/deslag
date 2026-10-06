//! The review's state machine: what the owner sees and does, with no terminal in it.
//!
//! A [`Session`] holds a CoNLL-U file one sentence at a time. A key goes in at [`Session::press`]
//! and the session changes: the cursor moves between words, a tag is set from a typed code, the
//! guide's entry for a tag is shown, and leaving a sentence writes it. Drawing is `screen.rs` and
//! the terminal is `terminal.rs`; neither decides anything.
//!
//! The file is read as one line per deslag token (`Kind=` in MISC), tagged or not. A word with no
//! tag starts blank unless deslag reads it at Likely or Sure; a blank word is never filled from a
//! reading below that, since a guess shown as the answer anchors the owner and flatters deslag
//! against him. A sentence cannot be left until every word has a tag. Leaving it sets `Prov=owner`
//! on its words, keeps what `Prov=` said as `Was=`, gives its other lines their UPOS and
//! `Prov=kind`, adds `# owner_reviewed = <date>`, and saves the file through a [`Store`] before
//! anything else changes.

use std::borrow::Cow;
use std::path::Path;

use deslag::document::{Token, TokenKind};
use deslag::tag::{Confidence, Context, Features, Origin, Reading};
use deslag_exam::conllu::{self, Block, Id};
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::kind_from_name;

use crate::code::{Base, Code, Form, Number};
use crate::data::upos_of_kind;
use crate::patch::{self, Columns, Comment, Patch};

/// Where a session saves its file.
pub trait Store {
    /// Replaces the whole file with `text`, all or nothing.
    fn save(&mut self, text: &str) -> Result<(), String>;
}

/// What deslag reads of a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guess {
    /// Its best guess, with the features the guide marks on that base.
    pub code: Code,
    /// How far deslag stands behind it.
    pub confidence: Confidence,
    /// The other tags it has not ruled out, as bases.
    pub kept: [Option<Base>; 12],
}

impl Guess {
    /// What deslag's `reading` is, in the guide's codes.
    pub fn of(reading: &Reading) -> Guess {
        let base = Base::from_tag(reading.tag);
        let has = |flag: Features| reading.features.contains(flag);
        let mut code = Code::bare(base);
        if base.takes_number() {
            code.number = if has(Features::SINGULAR) {
                Some(Number::Sing)
            } else if has(Features::PLURAL) {
                Some(Number::Plur)
            } else {
                None
            };
        }
        if base.takes_form() {
            code.form = if has(Features::FINITE) && has(Features::PRESENT) {
                Some(Form::Pr)
            } else if has(Features::FINITE) && has(Features::PAST) {
                Some(Form::Pa)
            } else if has(Features::FINITE) {
                Some(Form::Fi)
            } else if has(Features::INFINITIVE) {
                Some(Form::In)
            } else if has(Features::PRESENT_PARTICIPLE) {
                Some(Form::Ing)
            } else if has(Features::PAST_PARTICIPLE) {
                Some(Form::Pp)
            } else {
                None
            };
        }
        let mut kept = [None; 12];
        let others = reading
            .kept
            .iter()
            .filter(|tag| *tag != reading.tag)
            .map(Base::from_tag);
        for (slot, other) in kept.iter_mut().zip(others) {
            *slot = Some(other);
        }
        Guess {
            code,
            confidence: reading.confidence,
            kept,
        }
    }

    /// The code to fill a blank word with: the guess, when deslag commits to it and the guess
    /// carries every feature the guide asks of its base.
    pub fn prefill(&self) -> Option<Code> {
        (self.confidence.committed() && complete(&self.code)).then_some(self.code)
    }

    /// The tags kept besides the best guess.
    pub fn others(&self) -> impl Iterator<Item = Base> + '_ {
        self.kept.iter().flatten().copied()
    }
}

/// Whether `code` has the feature its base needs: a number on `N` and `PN`, a form on `V` and
/// `AX`.
pub fn complete(code: &Code) -> bool {
    match code.base {
        Base::N | Base::Pn => code.number.is_some(),
        Base::V | Base::Ax => code.form.is_some(),
        _ => true,
    }
}

/// A line that is a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordRow {
    /// Its line in the file.
    pub line: usize,
    /// Its form.
    pub form: String,
    /// The tag it has now, `None` while blank.
    pub tag: Option<Code>,
    /// Whether deslag's reading filled it.
    pub prefilled: bool,
    /// Whether the owner has set it this session.
    pub set: bool,
    /// `Prov=` as the file has it.
    pub prov: Option<String>,
    /// What deslag reads of it.
    pub guess: Option<Guess>,
    /// Where deslag says the word comes from.
    pub origin: Origin,
    upos: String,
    feats: String,
    misc: String,
}

/// A line that is not a word: shown and skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherRow {
    /// Its line in the file.
    pub line: usize,
    /// Its form.
    pub form: String,
    /// What kind of token it is.
    pub kind: TokenKind,
    upos: String,
    misc: String,
}

/// One line of a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A word, which the owner tags.
    Word(WordRow),
    /// A token that is not a word.
    Other(OtherRow),
}

impl Row {
    /// The word, if it is one.
    pub fn word(&self) -> Option<&WordRow> {
        match self {
            Row::Word(word) => Some(word),
            Row::Other(_) => None,
        }
    }
}

/// One sentence of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// Its `sent_id`.
    pub id: String,
    /// The text to show: `# text`, or the forms joined.
    pub text: String,
    /// The block it is in.
    pub context: Context,
    /// The date of its `# owner_reviewed`, once the owner has left it.
    pub reviewed: Option<String>,
    first_line: usize,
    /// Its lines, in order.
    pub rows: Vec<Row>,
}

impl Sentence {
    /// How many words have no tag.
    pub fn blanks(&self) -> usize {
        self.rows
            .iter()
            .filter_map(Row::word)
            .filter(|word| word.tag.is_none())
            .count()
    }

    /// The row index of the first word, or of the first blank one.
    fn first_to_do(&self) -> usize {
        let words = || {
            self.rows
                .iter()
                .enumerate()
                .filter_map(|(at, row)| row.word().map(|word| (at, word)))
        };
        words()
            .find(|(_, word)| word.tag.is_none())
            .or_else(|| words().next())
            .map_or(0, |(at, _)| at)
    }
}

/// A key the owner presses, as the state machine knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character.
    Char(char),
    /// Arrow down.
    Down,
    /// Arrow up.
    Up,
    /// Arrow right.
    Right,
    /// Arrow left.
    Left,
    /// Return.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
}

/// What the owner is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Moving between words.
    Browse,
    /// Typing a code, which is the text so far.
    Prompt(String),
    /// Reading the guide's entry for a base.
    Guide(Base),
}

/// What a key did to the session as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep going.
    Continue,
    /// The owner is done.
    Quit,
}

/// A file open for review.
#[derive(Debug)]
pub struct Session {
    source: String,
    today: String,
    /// The sentences, in file order.
    pub sentences: Vec<Sentence>,
    /// The sentence on screen.
    pub at: usize,
    /// The row the cursor is on.
    pub cursor: usize,
    /// What the owner is doing.
    pub mode: Mode,
    /// One line for the owner: what the last key did, or why it did nothing.
    pub notice: String,
}

/// The splits and names of files that are never opened.
const REFUSED_SPLITS: [&str; 2] = ["holdout", "test"];

/// Whether `path` names a file the review never opens: any `holdout*` or `en_ewt*` file, or
/// anything under `.ewt`. Decided on the path alone, before the file is read.
pub fn refuses_path(path: &Path) -> bool {
    path.components().any(|part| {
        let name = part.as_os_str().to_string_lossy().to_lowercase();
        name.starts_with("holdout") || name.starts_with("en_ewt") || name == ".ewt"
    })
}

const REFUSAL: &str = "the review does not open holdout or treebank test files";

/// The error for a file the review refuses. It names the file and no line of it.
pub fn refusal(path: &str) -> Error {
    Error::load(path, Place::File, REFUSAL)
}

impl Session {
    /// Opens `source`, the text of `path`, for review on `today`, a date as `YYYY-MM-DD`. The
    /// refusals name no line of the file.
    pub fn open(path: &str, source: String, today: &str) -> Result<Session, Error> {
        let load = |message: String| Error::load(path, Place::File, message);
        if refuses_path(Path::new(path)) {
            return Err(refusal(path));
        }
        let blocks = conllu::read(path, &source)?;
        let split = blocks
            .first()
            .and_then(|block| block.comment("exam.split"))
            .map(|comment| comment.value.as_str());
        if split.is_some_and(|split| REFUSED_SPLITS.contains(&split)) {
            return Err(refusal(path));
        }
        if blocks.is_empty() {
            return Err(load("the file has no sentences".into()));
        }
        let sentences = blocks
            .iter()
            .map(|block| read_sentence(path, block))
            .collect::<Result<Vec<_>, _>>()?;
        let at = sentences
            .iter()
            .position(|sentence| sentence.reviewed.is_none())
            .unwrap_or(0);
        let cursor = sentences[at].first_to_do();
        let notice = if sentences.iter().all(|s| s.reviewed.is_some()) {
            "every sentence is reviewed; opened at the first".to_string()
        } else {
            String::new()
        };
        Ok(Session {
            source,
            today: today.to_string(),
            sentences,
            at,
            cursor,
            mode: Mode::Browse,
            notice,
        })
    }

    /// The sentence on screen.
    pub fn sentence(&self) -> &Sentence {
        &self.sentences[self.at]
    }

    /// The word the cursor is on, if the sentence has one.
    pub fn current(&self) -> Option<&WordRow> {
        self.sentence().rows.get(self.cursor).and_then(Row::word)
    }

    /// How many sentences the owner has left.
    pub fn reviewed(&self) -> usize {
        self.sentences
            .iter()
            .filter(|sentence| sentence.reviewed.is_some())
            .count()
    }

    /// Whether the sentence on screen holds tags the owner has set and not yet saved.
    pub fn unsaved(&self) -> bool {
        self.sentence()
            .rows
            .iter()
            .filter_map(Row::word)
            .any(|word| word.set)
    }

    /// The file as it is now.
    #[cfg(test)]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Does what `key` asks in the mode the session is in. `store` is where a sentence is saved
    /// when the key leaves it.
    pub fn press(&mut self, key: Key, store: &mut dyn Store) -> Outcome {
        match self.mode.clone() {
            Mode::Guide(_) => {
                self.mode = Mode::Browse;
                Outcome::Continue
            }
            Mode::Prompt(typed) => {
                self.prompt(typed, key);
                Outcome::Continue
            }
            Mode::Browse => self.browse(key, store),
        }
    }

    fn browse(&mut self, key: Key, store: &mut dyn Store) -> Outcome {
        self.notice.clear();
        match key {
            Key::Down | Key::Char('j') => self.step(true),
            Key::Up | Key::Char('k') => self.step(false),
            Key::Enter | Key::Char('a') => self.accept(),
            Key::Char('t') => {
                if self.current().is_some() {
                    self.mode = Mode::Prompt(String::new());
                }
            }
            Key::Char('?') | Key::Char('g') => self.show_guide(),
            Key::Right | Key::Char('n') => self.leave(true, store),
            Key::Left | Key::Char('p') => self.leave(false, store),
            Key::Esc | Key::Char('q') => return Outcome::Quit,
            _ => {}
        }
        Outcome::Continue
    }

    /// Moves to the next or the previous word, skipping what is not one.
    fn step(&mut self, forward: bool) {
        let rows = &self.sentence().rows;
        let target = if forward {
            (self.cursor + 1..rows.len()).find(|at| rows[*at].word().is_some())
        } else {
            (0..self.cursor).rev().find(|at| rows[*at].word().is_some())
        };
        match target {
            Some(at) => self.cursor = at,
            None => {
                self.notice = if forward {
                    "the last word; n leaves the sentence".into()
                } else {
                    "the first word".into()
                }
            }
        }
    }

    fn accept(&mut self) {
        match self.current() {
            None => {}
            Some(word) if word.tag.is_none() => {
                self.notice = "no tag yet; t to type one".into();
            }
            Some(_) => self.step(true),
        }
    }

    fn show_guide(&mut self) {
        let base = self
            .current()
            .and_then(|word| word.tag.or(word.guess.map(|guess| guess.code)))
            .map(|code| code.base);
        match base {
            Some(base) => self.mode = Mode::Guide(base),
            None => self.notice = "no word here".into(),
        }
    }

    fn prompt(&mut self, mut typed: String, key: Key) {
        self.notice.clear();
        match key {
            Key::Esc => {
                self.mode = Mode::Browse;
                return;
            }
            Key::Backspace => {
                typed.pop();
            }
            Key::Char('?') => {
                let base = typed.split('.').next().unwrap_or("").to_ascii_uppercase();
                match Base::from_code(&base) {
                    Some(base) => self.mode = Mode::Guide(base),
                    None => self.notice = "type a base first, as in N or PN.s, then ?".into(),
                }
                return;
            }
            Key::Char(c) if c.is_ascii_alphanumeric() || c == '.' => typed.push(c),
            Key::Enter => {
                match parse_typed(&typed) {
                    Ok(code) => {
                        self.set(code);
                        self.mode = Mode::Browse;
                        self.step(true);
                        // `step` explains a stop at the last word; a tag that was set needs no
                        // such nag.
                        if self.notice.starts_with("the last word") {
                            self.notice = format!("set {code}");
                        }
                        return;
                    }
                    Err(message) => {
                        self.notice = message;
                    }
                }
            }
            _ => {}
        }
        self.mode = Mode::Prompt(typed);
    }

    /// Sets the tag of the word the cursor is on.
    fn set(&mut self, code: Code) {
        if let Some(Row::Word(word)) = self.sentences[self.at].rows.get_mut(self.cursor) {
            word.tag = Some(code);
            word.prefilled = false;
            word.set = true;
        }
    }

    /// Leaves the sentence for the next or the previous one, saving it.
    fn leave(&mut self, forward: bool, store: &mut dyn Store) {
        let blanks = self.sentence().blanks();
        if blanks > 0 {
            let first = self
                .sentence()
                .rows
                .iter()
                .position(|row| row.word().is_some_and(|word| word.tag.is_none()));
            if let Some(first) = first {
                self.cursor = first;
            }
            self.notice = format!(
                "{blanks} word{} still without a tag; every word needs one before you leave",
                if blanks == 1 { " is" } else { "s are" }
            );
            return;
        }
        let at_edge = if forward {
            self.at + 1 == self.sentences.len()
        } else {
            self.at == 0
        };
        if let Err(message) = self.save(store) {
            self.notice = format!("not saved: {message}");
            return;
        }
        if at_edge {
            self.notice = format!(
                "saved; {} of {} sentences reviewed",
                self.reviewed(),
                self.sentences.len()
            );
            return;
        }
        self.at = if forward { self.at + 1 } else { self.at - 1 };
        self.cursor = self.sentence().first_to_do();
        self.notice = "saved".into();
    }

    /// Writes the sentence on screen into the file. The session changes only if the store took it.
    fn save(&mut self, store: &mut dyn Store) -> Result<(), String> {
        let sentence = self.sentence();
        let mut patch = Patch::default();
        for row in &sentence.rows {
            match row {
                Row::Word(word) => {
                    let tag = word.tag.ok_or("a word has no tag")?;
                    let mut columns = Columns {
                        misc: Some(owner_misc(&word.misc)),
                        ..Columns::default()
                    };
                    let before = Code::from_conllu(&word.upos, &word.feats).ok();
                    if before != Some(tag) || word.upos == "_" {
                        columns.upos = Some(tag.upos(&word.form).to_string());
                        columns.feats = Some(tag.feats());
                    }
                    patch.lines.insert(word.line, columns);
                }
                Row::Other(other) => {
                    let mut columns = Columns::default();
                    if other.upos == "_" {
                        columns.upos = Some(upos_of_kind(other.kind).to_string());
                    }
                    if !conllu::pairs(&other.misc)
                        .iter()
                        .any(|(key, _)| *key == "Prov")
                    {
                        columns.misc = Some(with_pair(&other.misc, "Prov", "kind"));
                    }
                    if columns != Columns::default() {
                        patch.lines.insert(other.line, columns);
                    }
                }
            }
        }
        patch.comments.push(Comment {
            block_first_line: sentence.first_line,
            key: "owner_reviewed".into(),
            value: self.today.clone(),
        });
        let text = patch::apply(&self.source, &patch)?;
        // The new text is read before it is stored, so text that does not load never replaces
        // the file.
        let blocks = self.read_back(&text)?;
        store.save(&text)?;
        self.source = text;
        self.refresh(&blocks);
        Ok(())
    }

    /// `text` read as the file the session would hold, or why it is not.
    fn read_back(&self, text: &str) -> Result<Vec<Block>, String> {
        let blocks = conllu::read("the saved file", text).map_err(|e| e.to_string())?;
        let same = blocks.len() == self.sentences.len()
            && blocks
                .iter()
                .zip(&self.sentences)
                .all(|(block, sentence)| block.lines.len() == sentence.rows.len());
        if !same {
            return Err("the saved file has different sentences".into());
        }
        Ok(blocks)
    }

    /// Takes line numbers and columns from `blocks`, the file as saved, so they are the file's
    /// own.
    fn refresh(&mut self, blocks: &[Block]) {
        let (at, cursor) = (self.at, self.cursor);
        for (index, (sentence, block)) in self.sentences.iter_mut().zip(blocks).enumerate() {
            sentence.first_line = block.first_line;
            sentence.reviewed = reviewed_on(block);
            for (row, line) in sentence.rows.iter_mut().zip(&block.lines) {
                match row {
                    Row::Word(word) => {
                        word.line = line.number;
                        word.upos = line.upos.clone();
                        word.feats = line.feats.clone();
                        word.misc = line.misc.clone();
                        word.prov = prov_of(&line.misc);
                        if index == at {
                            word.set = false;
                            word.prefilled = false;
                        }
                    }
                    Row::Other(other) => {
                        other.line = line.number;
                        other.upos = line.upos.clone();
                        other.misc = line.misc.clone();
                    }
                }
            }
        }
        self.at = at;
        self.cursor = cursor;
    }
}

/// A typed code as the guide writes it, whatever the case of the letters.
fn parse_typed(typed: &str) -> Result<Code, String> {
    let (base, feature) = match typed.split_once('.') {
        Some((base, feature)) => (
            base.to_ascii_uppercase(),
            Some(feature.to_ascii_lowercase()),
        ),
        None => (typed.to_ascii_uppercase(), None),
    };
    let text = match feature {
        Some(feature) => format!("{base}.{feature}"),
        None => base,
    };
    Code::parse(&text)
}

/// A MISC column with `Prov=owner`, the old `Prov=` kept as `Was=`.
pub fn owner_misc(misc: &str) -> String {
    let mut pairs: Vec<(&str, &str)> = conllu::pairs(misc);
    let old = pairs
        .iter()
        .find(|(key, _)| *key == "Prov")
        .map(|(_, value)| *value);
    let was = match old {
        Some("owner") => pairs.iter().find(|(key, _)| *key == "Was").map(|(_, v)| *v),
        other => other,
    };
    pairs.retain(|(key, _)| !matches!(*key, "Prov" | "Was"));
    pairs.push(("Prov", "owner"));
    if let Some(was) = was.filter(|was| !was.is_empty()) {
        pairs.push(("Was", was));
    }
    join_pairs(&mut pairs)
}

/// `misc` with `key=value` added, or `_` made into it.
fn with_pair(misc: &str, key: &str, value: &str) -> String {
    let mut pairs = conllu::pairs(misc);
    pairs.push((key, value));
    join_pairs(&mut pairs)
}

/// The pairs as a column, keys in the alphabetical order UD asks for.
fn join_pairs(pairs: &mut [(&str, &str)]) -> String {
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(key, value)| {
            if value.is_empty() {
                (*key).to_string()
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn prov_of(misc: &str) -> Option<String> {
    conllu::pairs(misc)
        .into_iter()
        .find(|(key, _)| *key == "Prov")
        .map(|(_, value)| value.to_string())
}

fn reviewed_on(block: &Block) -> Option<String> {
    block
        .comment("owner_reviewed")
        .map(|comment| comment.value.clone())
}

/// One sentence of the file, with deslag's reading of its words.
fn read_sentence(path: &str, block: &Block) -> Result<Sentence, Error> {
    let id = block
        .comment("sent_id")
        .map(|comment| comment.value.clone())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::at(path, block.first_line, "no `# sent_id = ` comment"))?;
    let context = block
        .comment("exam.context")
        .and_then(|comment| Context::from_name(&comment.value))
        .unwrap_or(Context::Prose);
    // The tokens deslag reads are the file's lines, laid out as the exam lays out a `deslag`
    // file: forms in order, one space between them but where `SpaceAfter=No` says none.
    let mut joined = String::new();
    let mut spans = Vec::new();
    let mut kinds = Vec::new();
    for (index, line) in block.lines.iter().enumerate() {
        if !matches!(line.id, Id::Word(_)) {
            return Err(Error::at(
                path,
                line.number,
                "a range or empty-node line; the review reads one line per deslag token",
            ));
        }
        let pairs = conllu::pairs(&line.misc);
        let kind = pairs
            .iter()
            .find(|(key, _)| *key == "Kind")
            .and_then(|(_, value)| kind_from_name(value))
            .ok_or_else(|| {
                Error::at(
                    path,
                    line.number,
                    "no Kind= in MISC; the review reads one line per deslag token",
                )
            })?;
        let start = joined.len();
        joined.push_str(&line.form);
        spans.push(start..joined.len());
        kinds.push(kind);
        let space = !pairs.contains(&("SpaceAfter", "No"));
        if space && index + 1 < block.lines.len() {
            joined.push(' ');
        }
    }
    let mut tokens: Vec<Token<'_>> = spans
        .iter()
        .zip(&kinds)
        .map(|(span, kind)| Token {
            kind: *kind,
            range: span.clone(),
            text: Cow::Borrowed(&joined[span.clone()]),
            reading: None,
            origin: Origin::English,
        })
        .collect();
    deslag::tag::sentence(&mut tokens, context);
    let mut rows = Vec::with_capacity(tokens.len());
    for (line, token) in block.lines.iter().zip(&tokens) {
        if token.kind != TokenKind::Word {
            rows.push(Row::Other(OtherRow {
                line: line.number,
                form: line.form.clone(),
                kind: token.kind,
                upos: line.upos.clone(),
                misc: line.misc.clone(),
            }));
            continue;
        }
        let guess = token.reading.as_ref().map(Guess::of);
        let (tag, prefilled) = if line.upos == "_" {
            match guess.and_then(|guess| guess.prefill()) {
                Some(code) => (Some(code), true),
                None => (None, false),
            }
        } else {
            let code = Code::from_conllu(&line.upos, &line.feats)
                .map_err(|message| Error::at(path, line.number, message))?;
            (Some(code), false)
        };
        rows.push(Row::Word(WordRow {
            line: line.number,
            form: line.form.clone(),
            tag,
            prefilled,
            set: false,
            prov: prov_of(&line.misc),
            guess,
            origin: token.origin,
            upos: line.upos.clone(),
            feats: line.feats.clone(),
            misc: line.misc.clone(),
        }));
    }
    let text = block
        .comment("text")
        .map_or(joined.clone(), |comment| comment.value.clone());
    Ok(Sentence {
        id,
        text,
        context,
        reviewed: reviewed_on(block),
        first_line: block.first_line,
        rows,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A store that keeps the last text it was given, or fails.
    #[derive(Default)]
    pub struct Memory {
        pub saved: Vec<String>,
        pub fail: bool,
    }

    impl Store for Memory {
        fn save(&mut self, text: &str) -> Result<(), String> {
            if self.fail {
                return Err("disk full".into());
            }
            self.saved.push(text.to_string());
            Ok(())
        }
    }

    /// Two sentences of corpus-style text as a skeleton: `Run cargo build, then stop.` and
    /// `See README.md for details.`
    pub const SKELETON: &str = "\
# sent_id = s1
# exam.context = prose
# text = Run `cargo` now, then stop.
1\tRun\t_\t_\t_\t_\t_\t_\t_\tKind=Word
2\t`cargo`\t_\t_\t_\t_\t_\t_\t_\tKind=Code
3\tnow\t_\t_\t_\t_\t_\t_\t_\tKind=Word|SpaceAfter=No
4\t,\t_\t_\t_\t_\t_\t_\t_\tKind=Punctuation
5\tthen\t_\t_\t_\t_\t_\t_\t_\tKind=Word
6\tstop\t_\t_\t_\t_\t_\t_\t_\tKind=Word|SpaceAfter=No
7\t.\t_\t_\t_\t_\t_\t_\t_\tKind=Punctuation

# sent_id = s2
# text = See README.md for details.
1\tSee\t_\t_\t_\t_\t_\t_\t_\tKind=Word
2\tREADME.md\t_\t_\t_\t_\t_\t_\t_\tKind=Word|Origin=Path
3\tfor\t_\t_\t_\t_\t_\t_\t_\tKind=Word
4\tdetails\t_\t_\t_\t_\t_\t_\t_\tKind=Word|SpaceAfter=No
5\t.\t_\t_\t_\t_\t_\t_\t_\tKind=Punctuation
";

    pub fn open(source: &str) -> Session {
        Session::open("owner.conllu", source.to_string(), "2026-10-06").unwrap()
    }

    pub fn type_code(session: &mut Session, store: &mut Memory, code: &str) {
        session.press(Key::Char('t'), store);
        for c in code.chars() {
            session.press(Key::Char(c), store);
        }
        session.press(Key::Enter, store);
    }

    /// Tags every blank word `N.s`, as a stand-in for the owner's choices.
    pub fn fill(session: &mut Session, store: &mut Memory) {
        while session.sentence().blanks() > 0 {
            let blank = session
                .sentence()
                .rows
                .iter()
                .position(|row| row.word().is_some_and(|w| w.tag.is_none()))
                .unwrap();
            session.cursor = blank;
            type_code(session, store, "n.s");
        }
    }

    #[test]
    fn a_skeleton_opens_with_blank_words_and_deslag_s_reading_beside_them() {
        let session = open(SKELETON);
        assert_eq!(session.sentences.len(), 2);
        let sentence = session.sentence();
        assert_eq!(sentence.rows.len(), 7);
        let words: Vec<_> = sentence.rows.iter().filter_map(Row::word).collect();
        assert_eq!(words.len(), 4);
        for word in &words {
            assert!(word.guess.is_some(), "{}", word.form);
            // A word is pre-filled only where deslag commits to a complete code.
            let committed = word.guess.unwrap().prefill();
            assert_eq!(word.tag, committed, "{}", word.form);
            assert_eq!(word.prefilled, committed.is_some());
            assert!(word.prov.is_none());
        }
        assert_eq!(
            sentence.blanks(),
            words.iter().filter(|w| w.tag.is_none()).count()
        );
        let second = open(SKELETON).sentences[1].clone();
        let path = second.rows.iter().filter_map(Row::word).nth(1).unwrap();
        assert_eq!(path.origin, Origin::Path);
    }

    #[test]
    fn nothing_below_likely_is_filled_in() {
        let session = open(SKELETON);
        for sentence in &session.sentences {
            for word in sentence.rows.iter().filter_map(Row::word) {
                if let Some(guess) = word.guess
                    && !guess.confidence.committed()
                {
                    assert!(word.tag.is_none(), "{}", word.form);
                }
            }
        }
    }

    #[test]
    fn the_cursor_moves_between_words_and_skips_the_rest() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        session.cursor = 0;
        session.press(Key::Down, &mut store);
        assert_eq!(session.cursor, 2, "the code span is skipped");
        session.press(Key::Char('j'), &mut store);
        assert_eq!(session.cursor, 4, "and so is the comma");
        session.press(Key::Down, &mut store);
        session.press(Key::Down, &mut store);
        assert_eq!(session.cursor, 5);
        assert!(session.notice.contains("last word"));
        session.press(Key::Up, &mut store);
        session.press(Key::Char('k'), &mut store);
        session.press(Key::Char('k'), &mut store);
        assert_eq!(session.cursor, 0);
        session.press(Key::Up, &mut store);
        assert!(session.notice.contains("first word"));
    }

    #[test]
    fn a_typed_code_sets_the_word_and_moves_on() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        session.cursor = 0;
        type_code(&mut session, &mut store, "v.fi");
        let word = session.sentence().rows[0].word().unwrap().clone();
        assert_eq!(word.tag.unwrap().to_string(), "V.fi");
        assert!(word.set && !word.prefilled);
        assert_eq!(session.cursor, 2);
        assert_eq!(session.mode, Mode::Browse);
        type_code(&mut session, &mut store, "PN.P");
        assert_eq!(
            session.sentence().rows[2]
                .word()
                .unwrap()
                .tag
                .unwrap()
                .to_string(),
            "PN.p",
            "case does not matter"
        );
    }

    #[test]
    fn a_bad_code_stays_in_the_prompt_and_says_why() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        session.cursor = 0;
        type_code(&mut session, &mut store, "n");
        assert!(matches!(session.mode, Mode::Prompt(_)));
        assert!(
            session.notice.contains("needs a number"),
            "{}",
            session.notice
        );
        session.press(Key::Backspace, &mut store);
        session.press(Key::Char('d'), &mut store);
        session.press(Key::Enter, &mut store);
        assert_eq!(session.mode, Mode::Browse);
        assert_eq!(
            session.sentence().rows[0]
                .word()
                .unwrap()
                .tag
                .unwrap()
                .to_string(),
            "D"
        );
        session.press(Key::Char('t'), &mut store);
        session.press(Key::Char('x'), &mut store);
        session.press(Key::Esc, &mut store);
        assert_eq!(session.mode, Mode::Browse, "escape cancels the prompt");
    }

    #[test]
    fn accept_needs_a_tag_and_a_sentence_cannot_be_left_without_all_of_them() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        let blank = session
            .sentence()
            .rows
            .iter()
            .position(|row| row.word().is_some_and(|w| w.tag.is_none()));
        if let Some(blank) = blank {
            session.cursor = blank;
            session.press(Key::Enter, &mut store);
            assert!(session.notice.contains("no tag yet"));
            assert_eq!(session.cursor, blank);
            session.press(Key::Char('n'), &mut store);
            assert!(
                session.notice.contains("without a tag"),
                "{}",
                session.notice
            );
            assert!(store.saved.is_empty(), "nothing is written");
            assert_eq!(session.at, 0);
        }
    }

    #[test]
    fn the_guide_opens_for_the_tag_or_deslag_s_guess_and_any_key_closes_it() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        session.cursor = 0;
        session.press(Key::Char('?'), &mut store);
        let Mode::Guide(base) = session.mode.clone() else {
            panic!("{:?}", session.mode);
        };
        assert_eq!(base, session.current().unwrap().guess.unwrap().code.base);
        session.press(Key::Char('x'), &mut store);
        assert_eq!(session.mode, Mode::Browse);
        session.press(Key::Char('t'), &mut store);
        for c in "pn.s?".chars() {
            session.press(Key::Char(c), &mut store);
        }
        assert_eq!(session.mode, Mode::Guide(Base::Pn));
    }

    #[test]
    fn leaving_a_sentence_writes_owner_was_the_comment_and_the_kinds() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        fill(&mut session, &mut store);
        session.press(Key::Char('n'), &mut store);
        assert_eq!(session.at, 1);
        assert_eq!(session.notice, "saved");
        let saved = store.saved.last().unwrap();
        assert_eq!(saved, session.source());
        let lines: Vec<&str> = saved.lines().collect();
        assert_eq!(lines[3], "# owner_reviewed = 2026-10-06");
        for line in &lines[4..11] {
            let cells: Vec<&str> = line.split('\t').collect();
            let misc = cells[9];
            if misc.contains("Kind=Word") {
                assert!(misc.contains("Prov=owner"), "{line}");
                assert!(!misc.contains("Was="), "no Prov to keep: {line}");
                assert_ne!(cells[3], "_");
            } else {
                assert!(misc.contains("Prov=kind"), "{line}");
            }
        }
        assert!(lines[5].contains("\tX\t") && lines[5].contains("Kind=Code|Prov=kind"));
        assert!(lines[7].contains("\tPUNCT\t"));
        assert!(
            lines[8..].join("\n").contains("# sent_id = s2"),
            "the second sentence is untouched"
        );
        let rest = saved.split("# sent_id = s2").nth(1).unwrap();
        let original_rest = SKELETON.split("# sent_id = s2").nth(1).unwrap();
        assert_eq!(rest, original_rest);
    }

    #[test]
    fn the_saved_file_loads_as_gold_and_reopens_at_the_first_unreviewed_sentence() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        fill(&mut session, &mut store);
        session.press(Key::Right, &mut store);
        fill(&mut session, &mut store);
        session.press(Key::Right, &mut store);
        assert!(session.notice.contains("2 of 2 sentences reviewed"));
        assert_eq!(session.at, 1, "the last sentence stays on screen");
        let text = store.saved.last().unwrap().clone();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("owner.conllu");
        std::fs::write(&file, format!("# exam.tokens = deslag\n{text}")).unwrap();
        let gold = deslag_exam::gold::Gold::read(&file).expect("gold reads it");
        assert_eq!(gold.sentences.len(), 2);
        let owned = gold.sentences[0]
            .words
            .iter()
            .filter(|w| w.prov == Some(deslag_exam::gold::Prov::Owner))
            .count();
        assert_eq!(owned, 4);
        // Reopened: all reviewed, so it starts at the first and says so.
        let again = open(&text);
        assert_eq!(again.at, 0);
        assert!(again.notice.contains("every sentence is reviewed"));
        // A file with only the first reviewed reopens at the second.
        let mut half = open(SKELETON);
        fill(&mut half, &mut store);
        half.press(Key::Char('n'), &mut store);
        let reopened = open(half.source());
        assert_eq!(reopened.at, 1);
        assert_eq!(reopened.reviewed(), 1);
    }

    #[test]
    fn a_label_already_in_the_file_is_kept_as_was_and_unchanged_bytes_stay() {
        let source = "\
# sent_id = a
# free comment
1\tRun\tRun\tVERB\tx\tVerbForm=Fin\t0\troot\t_\tKind=Word|Prov=agree
2\tit\t_\tPRON\t_\tMood=Imp\t_\t_\t_\tKind=Word|Prov=adjudicated|SpaceAfter=No
3\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind
";
        let mut session = open(source);
        let mut store = Memory::default();
        session.cursor = 0;
        type_code(&mut session, &mut store, "v.in");
        session.press(Key::Char('n'), &mut store);
        let saved = store.saved.last().unwrap();
        assert_eq!(
            saved,
            "\
# sent_id = a
# free comment
# owner_reviewed = 2026-10-06
1\tRun\tRun\tVERB\tx\tVerbForm=Inf\t0\troot\t_\tKind=Word|Prov=owner|Was=agree
2\tit\t_\tPRON\t_\tMood=Imp\t_\t_\t_\tKind=Word|Prov=owner|SpaceAfter=No|Was=adjudicated
3\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind
",
            "the unchanged word keeps its UPOS and FEATS, and every other column is copied"
        );
    }

    #[test]
    fn reviewing_again_keeps_the_first_was() {
        assert_eq!(
            owner_misc("Kind=Word|Prov=agree|SpaceAfter=No"),
            "Kind=Word|Prov=owner|SpaceAfter=No|Was=agree"
        );
        assert_eq!(
            owner_misc("Kind=Word|Prov=owner|Was=agree"),
            "Kind=Word|Prov=owner|Was=agree"
        );
        assert_eq!(owner_misc("Kind=Word"), "Kind=Word|Prov=owner");
        assert_eq!(owner_misc("_"), "Prov=owner");
    }

    #[test]
    fn a_failed_save_changes_nothing() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        fill(&mut session, &mut store);
        store.fail = true;
        session.press(Key::Char('n'), &mut store);
        assert!(session.notice.contains("not saved: disk full"));
        assert_eq!(session.at, 0);
        assert_eq!(session.source(), SKELETON);
        assert!(session.sentence().reviewed.is_none());
        store.fail = false;
        session.press(Key::Char('n'), &mut store);
        assert_eq!(session.at, 1);
    }

    #[test]
    fn quitting_is_a_quit_and_unsaved_work_is_known() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        assert!(!session.unsaved());
        session.cursor = 0;
        type_code(&mut session, &mut store, "v.fi");
        assert!(session.unsaved());
        assert_eq!(session.press(Key::Char('q'), &mut store), Outcome::Quit);
        assert!(store.saved.is_empty());
    }

    #[test]
    fn holdout_and_test_files_are_refused_without_naming_a_line() {
        for path in [
            "tests/gold/holdout.conllu",
            "holdout-2.conllu",
            "x/en_ewt-ud-test.conllu",
            ".ewt/r2.15/en_ewt-ud-dev.conllu",
            "a/.ewt/b.conllu",
        ] {
            assert!(refuses_path(Path::new(path)), "{path}");
            let error = Session::open(path, SKELETON.into(), "d")
                .unwrap_err()
                .to_string();
            assert!(error.contains("does not open holdout"), "{error}");
        }
        assert!(!refuses_path(Path::new("tests/gold/owner.conllu")));
        for split in ["holdout", "test"] {
            let text = format!("# exam.split = {split}\n{SKELETON}");
            let error = Session::open("owner.conllu", text, "d")
                .unwrap_err()
                .to_string();
            assert_eq!(error, format!("owner.conllu: {REFUSAL}"));
        }
        let dev = format!("# exam.split = dev\n{SKELETON}");
        assert!(Session::open("owner.conllu", dev, "d").is_ok());
    }

    #[test]
    fn files_the_review_cannot_read_are_errors_that_name_a_line() {
        let ud = "# sent_id = a\n1-2\tab\t_\t_\t_\t_\t_\t_\t_\t_\n1\ta\t_\t_\t_\t_\t_\t_\t_\t_\n2\tb\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let error = Session::open("f.conllu", ud.into(), "d")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("f.conllu:2:"), "{error}");
        let no_kind = "# sent_id = a\n1\ta\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let error = Session::open("f.conllu", no_kind.into(), "d")
            .unwrap_err()
            .to_string();
        assert!(error.contains("no Kind="), "{error}");
        let bad = "# sent_id = a\n1\ta\t_\tWORD\t_\t_\t_\t_\t_\tKind=Word\n";
        assert!(Session::open("f.conllu", bad.into(), "d").is_err());
        assert!(Session::open("f.conllu", String::new(), "d").is_err());
    }

    #[test]
    fn the_state_machine_needs_no_terminal_for_a_whole_pass() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        let mut keys = 0;
        while session.reviewed() < session.sentences.len() {
            fill(&mut session, &mut store);
            session.press(Key::Char('n'), &mut store);
            keys += 1;
            assert!(keys < 10);
        }
        assert_eq!(store.saved.len(), 2);
    }

    #[test]
    fn a_file_with_a_byte_order_mark_is_saved_whole_and_still_loads() {
        let mut session = open(&format!("\u{feff}{SKELETON}"));
        let mut store = Memory::default();
        fill(&mut session, &mut store);
        session.press(Key::Char('n'), &mut store);
        assert_eq!(session.notice, "saved");
        let saved = store.saved.last().unwrap();
        assert!(saved.starts_with("\u{feff}# sent_id = s1\n"), "{saved:?}");
        assert_eq!(saved.matches('\u{feff}').count(), 1);
        assert!(conllu::read("saved", saved).is_ok());
        assert_eq!(open(saved).reviewed(), 1);
    }

    #[test]
    fn text_that_does_not_load_is_never_handed_to_the_store() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        fill(&mut session, &mut store);
        // A session that has lost track of a row writes a file it cannot read back.
        session.sentences[0].rows.pop();
        session.press(Key::Char('n'), &mut store);
        assert!(session.notice.contains("not saved"), "{}", session.notice);
        assert!(store.saved.is_empty(), "nothing reached the file");
        assert_eq!(session.source(), SKELETON);
    }
}
