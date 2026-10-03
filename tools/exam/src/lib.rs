//! deslag-exam: the exam, which grades part-of-speech taggers against gold sets.
//!
//! A **gold set** is sentences where a person has written the right tag for every word, in
//! CoNLL-U. The exam reads one ([`gold`]), matches its words to deslag's tokens ([`align`]), and,
//! given a [`tagger::Tagger`], scores what the tagger says of each token. This crate holds the
//! first half of that: reading and aligning gold, the tagger contract, the mapping from UD's tags
//! to deslag's ([`tags`]), the `tokens` skeleton an outside tagger fills ([`skeleton`]), and the
//! Words section of the report ([`words`]). `docs/design/exam.asbuilt.md` describes it.

pub mod align;
pub mod conllu;
pub mod disputes;
pub mod error;
pub mod gold;
pub mod skeleton;
pub mod tagger;
pub mod tags;
pub mod words;

pub use error::Error;
