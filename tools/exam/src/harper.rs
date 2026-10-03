//! Harper's part-of-speech tagger, as a study-only candidate for the exam.
//!
//! NOTICE: this file is adapted from Harper, <https://github.com/Automattic/harper>, which is
//! licensed under the Apache License, Version 2.0 (<http://www.apache.org/licenses/LICENSE-2.0>).
//! It reimplements the tagging engine of Harper's `harper-pos-utils`
//! crate (`tagger/brill_tagger/mod.rs`, `tagger/brill_tagger/patch.rs`, `tagger/freq_dict.rs`,
//! `patch_criteria.rs` and `upos.rs`, at commit 88c53331ebb6c353d6c5168c3f2191983239a11a, release
//! 2.12.0), and reads the JSON model that `harper-brill` ships. Changes from the original: the
//! training code, the chunker and every dependency of Harper's but `serde` are gone; the tag
//! names are read into this file's own [`Upos`] rather than Harper's; criteria are checked with
//! slices of `&str` instead of `String`s; and the answer is wrapped as a [`Tagger`] of the exam.
//! The engine's behaviour, quirks included, is Harper's.
//!
//! The model, `trained_tagger_model.json`, is not part of deslag and is never checked in or
//! shipped. Harper trained it on the train splits of UD English GUM, EWT and LinES (CC BY-NC-SA
//! 4.0 and CC BY-SA 4.0). `make fetch-harper` downloads it, pinned by `scripts/harper/harper.lock`,
//! into the git-ignored `.harper/`, and the exam only measures it.
//!
//! # The engine
//!
//! A Brill tagger is a table and a list of patches. The table maps a lowercased word to one tag.
//! The patches are ordered rules, each "change tag A to tag B when a criterion holds". A sentence
//! is tagged by looking every word up (a word not in the table has no tag), then applying every
//! patch in order, each one left to right over the whole sentence, so a later position sees the
//! changes an earlier position just made. A word with no tag is never changed, and a patch that
//! asks about a neighbour's tag does not match a neighbour with none.
//!
//! The criteria are the six variants of [`Criterion`]. Two of them are looser than they read, and
//! are kept as Harper has them, since the exam grades Harper and not what it meant:
//!
//! - `WordIs` compares only as far as the shorter of the two words reaches, so `to` matches
//!   `too`, `today` and `t`.
//! - `AnyWordIsTaggedWith { max_relative: n }` looks at the positions from this word up to but
//!   not including `n` away. With `n` above zero that includes this word's own tag; with `n`
//!   below zero it does not.
//!
//! # What the exam sees
//!
//! Harper commits to one tag or none, and says nothing else. The exam maps it like this:
//!
//! - Harper tags every token of the sentence, punctuation too, as it does inside Harper, since
//!   the patches read the tags of neighbours. Only `Word` tokens get a reading.
//! - A word with a tag gets that tag, `Likely`: it is a best guess by rule, from the table and
//!   the patches, and others are possible. Harper does not say which words the patches changed
//!   and which they left as the table had them, and the exam does not guess; both are `Likely`.
//! - A word with no tag is not in Harper's table and no patch can give it one. It gets `Noun`,
//!   the most common tag of the table and of English text, at `Unknown`.
//! - A word Harper tags `PUNCT` or `SYM` is outside the 13 tags. It gets the same `Noun` at
//!   `Unknown`, as an imported line outside the 13 does.
//! - The kept set is the guess alone, there are no features, and there is no score.

use std::collections::HashMap;
use std::path::Path;

use deslag::document::TokenKind;
use serde::Deserialize;

use crate::error::Error;
use crate::tagger::{Sentence, Tagger};
use crate::tags::{Class, Confidence, Features, Reading, Tag, TagSet, map_upos};

/// Where `make fetch-harper` puts the model, from the repository root; the release is the one
/// `scripts/harper/harper.lock` pins, and a test keeps the two in step.
pub const DEFAULT_MODEL: &str = ".harper/2.12.0/trained_tagger_model.json";

/// The universal part-of-speech tags, as the model spells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Upos {
    /// Adjective.
    #[serde(rename = "ADJ")]
    Adj,
    /// Adposition.
    #[serde(rename = "ADP")]
    Adp,
    /// Adverb.
    #[serde(rename = "ADV")]
    Adv,
    /// Auxiliary.
    #[serde(rename = "AUX")]
    Aux,
    /// Coordinating conjunction.
    #[serde(rename = "CCONJ")]
    Cconj,
    /// Determiner.
    #[serde(rename = "DET")]
    Det,
    /// Interjection.
    #[serde(rename = "INTJ")]
    Intj,
    /// Noun.
    #[serde(rename = "NOUN")]
    Noun,
    /// Numeral.
    #[serde(rename = "NUM")]
    Num,
    /// Particle.
    #[serde(rename = "PART")]
    Part,
    /// Pronoun.
    #[serde(rename = "PRON")]
    Pron,
    /// Proper noun.
    #[serde(rename = "PROPN")]
    Propn,
    /// Punctuation.
    #[serde(rename = "PUNCT")]
    Punct,
    /// Subordinating conjunction.
    #[serde(rename = "SCONJ")]
    Sconj,
    /// Symbol.
    #[serde(rename = "SYM")]
    Sym,
    /// Verb.
    #[serde(rename = "VERB")]
    Verb,
}

impl Upos {
    /// The UD code of the tag, which the exam's one mapping reads.
    pub fn code(self) -> &'static str {
        match self {
            Upos::Adj => "ADJ",
            Upos::Adp => "ADP",
            Upos::Adv => "ADV",
            Upos::Aux => "AUX",
            Upos::Cconj => "CCONJ",
            Upos::Det => "DET",
            Upos::Intj => "INTJ",
            Upos::Noun => "NOUN",
            Upos::Num => "NUM",
            Upos::Part => "PART",
            Upos::Pron => "PRON",
            Upos::Propn => "PROPN",
            Upos::Punct => "PUNCT",
            Upos::Sconj => "SCONJ",
            Upos::Sym => "SYM",
            Upos::Verb => "VERB",
        }
    }
}

/// When a patch applies to the word at an index. The shape is the model's JSON: a variant name
/// holding its fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Criterion {
    /// The word `relative` places away has this tag.
    WordIsTaggedWith {
        /// How far from the word, negative for before it.
        relative: isize,
        /// The tag it must have.
        is_tagged: Upos,
    },
    /// Some word between this one and the one `max_relative` away has this tag. The far end is
    /// not included, and this word is, when `max_relative` is above zero.
    AnyWordIsTaggedWith {
        /// How far to look, negative for before the word.
        max_relative: isize,
        /// The tag it must have.
        is_tagged: Upos,
    },
    /// The word before has one tag and the word after has another.
    SandwichTaggedWith {
        /// The tag of the word before.
        prev_word_tagged: Upos,
        /// The tag of the word after.
        post_word_tagged: Upos,
    },
    /// The word `relative` places away is `word`, without regard to ASCII case, as far as the
    /// shorter of the two reaches.
    WordIs {
        /// How far from the word, negative for before it.
        relative: isize,
        /// The word, in lowercase.
        word: String,
    },
    /// A noun-phrase flag, which Harper's chunker sets. Harper's tagger runs without one, so this
    /// never holds.
    NounPhraseAt {
        /// The flag it must have.
        is_np: bool,
        /// How far from the word, negative for before it.
        relative: isize,
    },
    /// Both hold.
    Combined {
        /// The first.
        a: Box<Criterion>,
        /// The second.
        b: Box<Criterion>,
    },
}

/// The position `relative` places from `index`, if there is one.
fn offset(index: usize, relative: isize) -> Option<usize> {
    index.checked_add_signed(relative)
}

/// The tag at `index`, if the sentence has a word there and it has one.
fn tag_at(tags: &[Option<Upos>], index: usize) -> Option<Upos> {
    tags.get(index).copied().flatten()
}

impl Criterion {
    /// Whether the criterion holds for the word at `index`, given the sentence's words and the
    /// tags the patches so far have left.
    fn fulfils(&self, words: &[&str], tags: &[Option<Upos>], index: usize) -> bool {
        match self {
            Criterion::WordIsTaggedWith {
                relative,
                is_tagged,
            } => offset(index, *relative).is_some_and(|at| tag_at(tags, at) == Some(*is_tagged)),
            Criterion::AnyWordIsTaggedWith {
                max_relative,
                is_tagged,
            } => {
                let Some(far) = offset(index, *max_relative) else {
                    return false;
                };
                // Positions past the sentence have no tag, so stopping at its end changes nothing.
                let (low, high) = (far.min(index), far.max(index).min(tags.len()));
                (low..high).any(|at| tag_at(tags, at) == Some(*is_tagged))
            }
            Criterion::SandwichTaggedWith {
                prev_word_tagged,
                post_word_tagged,
            } => {
                index > 0
                    && tag_at(tags, index - 1) == Some(*prev_word_tagged)
                    && tag_at(tags, index + 1) == Some(*post_word_tagged)
            }
            Criterion::WordIs { relative, word } => offset(index, *relative)
                .and_then(|at| words.get(at))
                .is_some_and(|w| {
                    w.chars()
                        .zip(word.chars())
                        .all(|(a, b)| a.eq_ignore_ascii_case(&b))
                }),
            Criterion::NounPhraseAt { .. } => false,
            Criterion::Combined { a, b } => {
                a.fulfils(words, tags, index) && b.fulfils(words, tags, index)
            }
        }
    }
}

/// One rule: change `from` to `to` where `criteria` holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Patch {
    /// The tag it replaces.
    pub from: Upos,
    /// The tag it puts there.
    pub to: Upos,
    /// When it applies.
    pub criteria: Criterion,
}

/// The table of most common tags, as the model has it.
#[derive(Debug, Deserialize)]
struct Base {
    mapping: HashMap<String, Upos>,
}

/// The engine: a table from lowercased word to tag, and the patches, in order.
#[derive(Debug, Deserialize)]
pub struct Brill {
    base: Base,
    patches: Vec<Patch>,
}

impl Brill {
    /// Reads a model from the JSON `harper-brill` ships.
    pub fn parse(json: &str) -> Result<Brill, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// How many words the table holds.
    pub fn words(&self) -> usize {
        self.base.mapping.len()
    }

    /// How many patches there are.
    pub fn patches(&self) -> usize {
        self.patches.len()
    }

    /// One entry per word: its tag, or `None` for a word the table lacks that no patch can tag.
    pub fn tag_sentence(&self, words: &[&str]) -> Vec<Option<Upos>> {
        let mut tags: Vec<Option<Upos>> = words
            .iter()
            .map(|word| self.base.mapping.get(&word.to_lowercase()).copied())
            .collect();
        for patch in &self.patches {
            for index in 0..words.len() {
                if tags[index] == Some(patch.from) && patch.criteria.fulfils(words, &tags, index) {
                    tags[index] = Some(patch.to);
                }
            }
        }
        tags
    }
}

/// Harper's tagger as the exam's candidate called `harper`.
#[derive(Debug)]
pub struct Harper {
    brill: Brill,
}

impl Harper {
    /// The candidate, from a model already read.
    pub fn new(brill: Brill) -> Harper {
        Harper { brill }
    }

    /// The candidate, from the model file at `path`, which `make fetch-harper` puts in `.harper/`.
    pub fn read(path: &Path) -> Result<Harper, Error> {
        let shown = path.display().to_string();
        let json = std::fs::read_to_string(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::Cannot(format!(
                    "{shown}: no Harper model there; `make fetch-harper` downloads it, or \
                     --harper-model names another"
                ))
            } else {
                Error::Io {
                    path: shown.clone(),
                    source,
                }
            }
        })?;
        let brill = Brill::parse(&json)
            .map_err(|e| Error::at(&shown, e.line(), format!("not a Harper tagger model: {e}")))?;
        Ok(Harper::new(brill))
    }
}

impl Tagger for Harper {
    fn name(&self) -> &str {
        "harper"
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        let words: Vec<&str> = sentence.tokens.iter().map(|t| t.text.as_ref()).collect();
        let tags = self.brill.tag_sentence(&words);
        sentence
            .tokens
            .iter()
            .zip(tags)
            .map(|(token, tag)| (token.kind == TokenKind::Word).then(|| reading(tag)))
            .collect()
    }
}

/// What the exam makes of Harper's answer for one word.
fn reading(tag: Option<Upos>) -> Reading {
    let mapped = tag.and_then(|upos| match map_upos(upos.code()) {
        Some(Class::Tagged(tag)) => Some(tag),
        _ => None,
    });
    match mapped {
        Some(tag) => Reading {
            tag,
            features: Features::NONE,
            confidence: Confidence::Likely,
            kept: TagSet::of(tag),
            score: None,
        },
        None => Reading {
            tag: Tag::Noun,
            features: Features::NONE,
            confidence: Confidence::Unknown,
            kept: TagSet::of(Tag::Noun),
            score: None,
        },
    }
}
