//! The tags, the features and what a tagger says of a word, and the one mapping from UD.
//!
//! `Tag`, `TagSet`, `Features` and `Confidence` are deslag's own, in [`deslag::tag`], small enough
//! that the gold and every imported file are read into them by the same table: [`map_upos`] and
//! [`from_ud`]. UD is the exam's business and deslag knows nothing of it. A reading says what a
//! tagger concluded, never how.

use crate::conllu;

pub use deslag::tag::{Confidence, Features, Tag, TagSet};

/// What a gold word's UPOS makes of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A word the exam scores, as this tag.
    Tagged(Tag),
    /// `PUNCT` or `SYM`: counted as punctuation, never scored.
    Punctuation,
    /// `X`: counted, never scored.
    X,
}

/// The table from UD's UPOS to deslag's tags, in one place: gold and every imported file are read
/// through it. `None` for a value that is not one of UD's 17.
pub fn map_upos(upos: &str) -> Option<Class> {
    let tag = match upos {
        "NOUN" => Tag::Noun,
        "PROPN" => Tag::ProperNoun,
        "VERB" => Tag::Verb,
        "AUX" => Tag::Auxiliary,
        "ADJ" => Tag::Adjective,
        "ADV" => Tag::Adverb,
        "PRON" => Tag::Pronoun,
        "DET" => Tag::Determiner,
        "ADP" => Tag::Adposition,
        "CCONJ" | "SCONJ" => Tag::Conjunction,
        "PART" => Tag::Particle,
        "NUM" => Tag::Numeral,
        "INTJ" => Tag::Interjection,
        "PUNCT" | "SYM" => return Some(Class::Punctuation),
        "X" => return Some(Class::X),
        _ => return None,
    };
    Some(Class::Tagged(tag))
}

/// The flags a UD `FEATS` column sets. `_` sets none. Keys outside the 14 flags are ignored;
/// an entry with no `=` is an error, which the string names.
pub fn from_ud(feats: &str) -> Result<Features, String> {
    let entries = conllu::pairs(feats);
    if let Some((key, _)) = entries.iter().find(|(_, value)| value.is_empty()) {
        return Err(format!("FEATS entry `{key}` has no value"));
    }
    let has = |key: &str, value: &str| {
        entries
            .iter()
            .any(|(k, v)| *k == key && v.split(',').any(|v| v == value))
    };
    let form_given = entries.iter().any(|(key, _)| *key == "VerbForm");
    let mut out = Features::NONE;
    let mut set = |on: bool, flag: Features| {
        if on {
            out = out.union(flag);
        }
    };
    set(has("Number", "Sing"), Features::SINGULAR);
    set(has("Number", "Plur"), Features::PLURAL);
    set(has("Person", "1"), Features::FIRST);
    set(has("Person", "2"), Features::SECOND);
    set(has("Person", "3"), Features::THIRD);
    let finite = has("VerbForm", "Fin");
    set(finite, Features::FINITE);
    set(has("VerbForm", "Inf"), Features::INFINITIVE);
    set(has("VerbForm", "Ger"), Features::PRESENT_PARTICIPLE);
    let participle = has("VerbForm", "Part");
    set(
        participle && has("Tense", "Past"),
        Features::PAST_PARTICIPLE,
    );
    set(
        participle && has("Tense", "Pres"),
        Features::PRESENT_PARTICIPLE,
    );
    let tensed = finite || !form_given;
    set(tensed && has("Tense", "Pres"), Features::PRESENT);
    set(tensed && has("Tense", "Past"), Features::PAST);
    set(has("Degree", "Pos"), Features::POSITIVE);
    set(has("Degree", "Cmp"), Features::COMPARATIVE);
    set(has("Degree", "Sup"), Features::SUPERLATIVE);
    Ok(out)
}

/// What a tagger concluded about one word token: deslag's [`deslag::tag::Reading`], and the raw
/// score a tagger outside deslag may add.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// The best guess, always present.
    pub tag: Tag,
    /// The features of the best guess.
    pub features: Features,
    /// How far the tagger stands behind it.
    pub confidence: Confidence,
    /// Every tag still possible; the exam adds `tag` to it.
    pub kept: TagSet,
    /// The raw score of the best guess, `0.0..=1.0`, if the tagger has one.
    pub score: Option<f32>,
}

impl Reading {
    /// The tags the tagger has not ruled out: `kept` and the best guess.
    pub fn possible(&self) -> TagSet {
        self.kept.with(self.tag)
    }

    /// The same reading as deslag holds it, without the score.
    pub fn without_score(&self) -> deslag::tag::Reading {
        deslag::tag::Reading {
            tag: self.tag,
            features: self.features,
            confidence: self.confidence,
            kept: self.kept,
        }
    }
}

impl From<deslag::tag::Reading> for Reading {
    fn from(reading: deslag::tag::Reading) -> Reading {
        Reading {
            tag: reading.tag,
            features: reading.features,
            confidence: reading.confidence,
            kept: reading.kept,
            score: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_of_the_upos_table() {
        let rows = [
            ("NOUN", Class::Tagged(Tag::Noun), "NOUN"),
            ("PROPN", Class::Tagged(Tag::ProperNoun), "PROPN"),
            ("VERB", Class::Tagged(Tag::Verb), "VERB"),
            ("AUX", Class::Tagged(Tag::Auxiliary), "AUX"),
            ("ADJ", Class::Tagged(Tag::Adjective), "ADJ"),
            ("ADV", Class::Tagged(Tag::Adverb), "ADV"),
            ("PRON", Class::Tagged(Tag::Pronoun), "PRON"),
            ("DET", Class::Tagged(Tag::Determiner), "DET"),
            ("ADP", Class::Tagged(Tag::Adposition), "ADP"),
            ("CCONJ", Class::Tagged(Tag::Conjunction), "CONJ"),
            ("SCONJ", Class::Tagged(Tag::Conjunction), "CONJ"),
            ("PART", Class::Tagged(Tag::Particle), "PART"),
            ("NUM", Class::Tagged(Tag::Numeral), "NUM"),
            ("INTJ", Class::Tagged(Tag::Interjection), "INTJ"),
            ("PUNCT", Class::Punctuation, ""),
            ("SYM", Class::Punctuation, ""),
            ("X", Class::X, ""),
        ];
        for (upos, class, code) in rows {
            assert_eq!(map_upos(upos), Some(class), "{upos}");
            if let Class::Tagged(tag) = class {
                assert_eq!(tag.code(), code);
                assert_eq!(Tag::from_code(code), Some(tag));
            }
        }
        for bad in ["_", "", "noun", "CONJ", "ADJ|"] {
            assert_eq!(map_upos(bad), None, "{bad}");
        }
    }

    fn feats(text: &str) -> Features {
        from_ud(text).unwrap()
    }

    #[test]
    fn every_row_of_the_feature_table() {
        let rows = [
            ("Number=Sing", Features::SINGULAR),
            ("Number=Plur", Features::PLURAL),
            ("Person=1", Features::FIRST),
            ("Person=2", Features::SECOND),
            ("Person=3", Features::THIRD),
            ("VerbForm=Fin", Features::FINITE),
            ("VerbForm=Inf", Features::INFINITIVE),
            ("VerbForm=Ger", Features::PRESENT_PARTICIPLE),
            ("VerbForm=Part|Tense=Past", Features::PAST_PARTICIPLE),
            ("VerbForm=Part|Tense=Pres", Features::PRESENT_PARTICIPLE),
            ("Tense=Pres", Features::PRESENT),
            ("Tense=Past", Features::PAST),
            (
                "VerbForm=Fin|Tense=Pres",
                Features::FINITE.union(Features::PRESENT),
            ),
            (
                "Tense=Past|VerbForm=Fin",
                Features::FINITE.union(Features::PAST),
            ),
            ("Degree=Pos", Features::POSITIVE),
            ("Degree=Cmp", Features::COMPARATIVE),
            ("Degree=Sup", Features::SUPERLATIVE),
        ];
        for (text, expect) in rows {
            assert_eq!(feats(text), expect, "{text}");
        }
    }

    #[test]
    fn features_the_table_leaves_out_are_ignored() {
        assert_eq!(feats("_"), Features::NONE);
        assert_eq!(
            feats("Mood=Imp|PronType=Prs|Definite=Def|Foreign=Yes"),
            Features::NONE
        );
        // Part with no tense, and tense on a non-finite form, set nothing of their own.
        assert_eq!(feats("VerbForm=Part"), Features::NONE);
        assert_eq!(feats("VerbForm=Inf|Tense=Pres"), Features::INFINITIVE);
        assert_eq!(feats("VerbForm=Part|Tense=Past"), Features::PAST_PARTICIPLE);
        assert_eq!(
            feats("Mood=Ind|Number=Sing|Person=3|Tense=Pres|VerbForm=Fin"),
            Features::SINGULAR
                .union(Features::THIRD)
                .union(Features::PRESENT)
                .union(Features::FINITE)
        );
    }

    #[test]
    fn a_list_value_sets_each_flag() {
        assert_eq!(
            feats("Number=Sing,Plur"),
            Features::SINGULAR.union(Features::PLURAL)
        );
        assert_eq!(feats("Number=Sing,Plur").number(), None);
    }

    #[test]
    fn a_feats_entry_with_no_value_is_an_error() {
        assert!(from_ud("Number").unwrap_err().contains("Number"));
        assert!(from_ud("Number=").is_err());
    }

    #[test]
    fn a_reading_is_possible_as_kept_plus_best_guess() {
        let reading = Reading {
            tag: Tag::Verb,
            features: Features::NONE,
            confidence: Confidence::Likely,
            kept: TagSet::of(Tag::Noun),
            score: None,
        };
        assert_eq!(reading.possible().len(), 2);
    }

    #[test]
    fn a_reading_goes_to_the_exam_and_back_unchanged() {
        let ours = deslag::tag::Reading {
            tag: Tag::Auxiliary,
            features: Features::FINITE
                .union(Features::PRESENT)
                .union(Features::CONTRACTION),
            confidence: Confidence::Sure,
            kept: TagSet::of(Tag::Auxiliary),
        };
        let theirs = Reading::from(ours);
        assert_eq!(theirs.score, None);
        assert_eq!(theirs.without_score(), ours);
        let scored = Reading {
            score: Some(0.5),
            ..theirs
        };
        assert_eq!(scored.without_score(), ours);
    }
}
