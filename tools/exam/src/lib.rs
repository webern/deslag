//! deslag-exam: the exam, which grades part-of-speech taggers against gold sets.
//!
//! A **gold set** is sentences where a person has written the right tag for every word, in
//! CoNLL-U. The exam reads one ([`gold`]), matches its words to deslag's tokens ([`align`]), and,
//! given a [`tagger::Tagger`] or a file another program filled ([`import`]), scores what it says
//! of each token ([`score`], [`metrics`]), with sentence-bootstrap intervals ([`stats`]), and
//! prints a report ([`report`]), a saved run ([`saved`]) and a paired comparison of two
//! ([`compare`]), or judges a run against the gates a file sets ([`gate`]), among them a list of words that must
//! stay right ([`mustpass`]).
//! `docs/design/exam.asbuilt.md` describes it.

pub mod align;
pub mod compare;
pub mod conllu;
pub mod disputes;
pub mod error;
pub mod gate;
pub mod gold;
pub mod harper;
pub mod import;
pub mod metrics;
pub mod most_common;
pub mod mustpass;
pub mod report;
pub mod saved;
pub mod score;
pub mod skeleton;
pub mod stats;
pub mod strata;
pub mod tagger;
pub mod tags;
pub mod words;

pub use error::Error;
