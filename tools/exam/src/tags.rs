//! The tags, the features and what a tagger says of a word, and the one mapping from UD.
//!
//! `Tag`, `TagSet`, `Features` and `Confidence` are deslag's own, small enough that the gold
//! and every imported file are read into them by the same table: [`map_upos`] and
//! [`Features::from_ud`]. A reading says what a tagger concluded, never how.

use crate::conllu;

/// A part of speech, as deslag tags it. The order of [`Tag::ALL`] is the order every table of the
/// report uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tag {
    /// `NOUN`.
    Noun,
    /// `PROPN`.
    ProperNoun,
    /// `VERB`.
    Verb,
    /// `AUX`.
    Auxiliary,
    /// `ADJ`.
    Adjective,
    /// `ADV`.
    Adverb,
    /// `PRON`.
    Pronoun,
    /// `DET`.
    Determiner,
    /// `ADP`, a preposition such as `in` or `of`.
    Adposition,
    /// `CCONJ` and `SCONJ`, written `CONJ`.
    Conjunction,
    /// `PART`.
    Particle,
    /// `NUM`.
    Numeral,
    /// `INTJ`.
    Interjection,
}

impl Tag {
    /// Every tag, in the fixed order.
    pub const ALL: [Tag; 13] = [
        Tag::Noun,
        Tag::ProperNoun,
        Tag::Verb,
        Tag::Auxiliary,
        Tag::Adjective,
        Tag::Adverb,
        Tag::Pronoun,
        Tag::Determiner,
        Tag::Adposition,
        Tag::Conjunction,
        Tag::Particle,
        Tag::Numeral,
        Tag::Interjection,
    ];

    /// The short code the report and an imported file's `Kept=` use.
    pub fn code(self) -> &'static str {
        match self {
            Tag::Noun => "NOUN",
            Tag::ProperNoun => "PROPN",
            Tag::Verb => "VERB",
            Tag::Auxiliary => "AUX",
            Tag::Adjective => "ADJ",
            Tag::Adverb => "ADV",
            Tag::Pronoun => "PRON",
            Tag::Determiner => "DET",
            Tag::Adposition => "ADP",
            Tag::Conjunction => "CONJ",
            Tag::Particle => "PART",
            Tag::Numeral => "NUM",
            Tag::Interjection => "INTJ",
        }
    }

    /// The tag whose short code is `code`.
    pub fn from_code(code: &str) -> Option<Tag> {
        Tag::ALL.into_iter().find(|tag| tag.code() == code)
    }

    /// Its place in [`Tag::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    fn bit(self) -> u16 {
        1 << self.index()
    }
}

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

/// A set of tags, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TagSet(u16);

impl TagSet {
    /// The empty set.
    pub const EMPTY: TagSet = TagSet(0);

    /// The set of just `tag`.
    pub fn of(tag: Tag) -> TagSet {
        TagSet(tag.bit())
    }

    /// This set and `tag`.
    pub fn with(self, tag: Tag) -> TagSet {
        TagSet(self.0 | tag.bit())
    }

    /// Whether `tag` is in the set.
    pub fn contains(self, tag: Tag) -> bool {
        self.0 & tag.bit() != 0
    }

    /// How many tags are in it.
    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// Whether it holds none.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Its tags, in the fixed order.
    pub fn iter(self) -> impl Iterator<Item = Tag> {
        Tag::ALL.into_iter().filter(move |tag| self.contains(*tag))
    }
}

impl FromIterator<Tag> for TagSet {
    fn from_iter<I: IntoIterator<Item = Tag>>(tags: I) -> TagSet {
        tags.into_iter().fold(TagSet::EMPTY, TagSet::with)
    }
}

/// The 14 flags a token's features may set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Features(u16);

impl Features {
    /// No flag set.
    pub const NONE: Features = Features(0);
    /// `Number=Sing`.
    pub const SINGULAR: Features = Features(1 << 0);
    /// `Number=Plur`.
    pub const PLURAL: Features = Features(1 << 1);
    /// `Person=1`.
    pub const FIRST: Features = Features(1 << 2);
    /// `Person=2`.
    pub const SECOND: Features = Features(1 << 3);
    /// `Person=3`.
    pub const THIRD: Features = Features(1 << 4);
    /// `VerbForm=Fin`.
    pub const FINITE: Features = Features(1 << 5);
    /// `VerbForm=Inf`.
    pub const INFINITIVE: Features = Features(1 << 6);
    /// `VerbForm=Part` with `Tense=Past`.
    pub const PAST_PARTICIPLE: Features = Features(1 << 7);
    /// `VerbForm=Ger`, or `VerbForm=Part` with `Tense=Pres`.
    pub const PRESENT_PARTICIPLE: Features = Features(1 << 8);
    /// `Tense=Pres`, of a finite verb or one with no `VerbForm`.
    pub const PRESENT: Features = Features(1 << 9);
    /// `Tense=Past`, of a finite verb or one with no `VerbForm`.
    pub const PAST: Features = Features(1 << 10);
    /// `Degree=Pos`.
    pub const POSITIVE: Features = Features(1 << 11);
    /// `Degree=Cmp`.
    pub const COMPARATIVE: Features = Features(1 << 12);
    /// `Degree=Sup`.
    pub const SUPERLATIVE: Features = Features(1 << 13);

    /// Whether every flag of `other` is set here.
    pub fn contains(self, other: Features) -> bool {
        self.0 & other.0 == other.0
    }

    /// The flags of both.
    pub fn union(self, other: Features) -> Features {
        Features(self.0 | other.0)
    }

    /// The flags of `self` that are also in `mask`.
    fn only(self, mask: Features) -> Features {
        Features(self.0 & mask.0)
    }

    /// The one flag of `group` that is set, or `None` when none or several are.
    fn one_of(self, group: Features) -> Option<Features> {
        let set = self.only(group);
        (set.0.count_ones() == 1).then_some(set)
    }

    /// The number value: `SINGULAR` or `PLURAL` when exactly one is set.
    pub fn number(self) -> Option<Features> {
        self.one_of(Features::SINGULAR.union(Features::PLURAL))
    }

    /// The verb form: the one of `FINITE`, `INFINITIVE`, `PAST_PARTICIPLE`, `PRESENT_PARTICIPLE`
    /// that is set, when exactly one is.
    pub fn verb_form(self) -> Option<Features> {
        self.one_of(
            Features::FINITE
                .union(Features::INFINITIVE)
                .union(Features::PAST_PARTICIPLE)
                .union(Features::PRESENT_PARTICIPLE),
        )
    }

    /// The tense: `PRESENT` or `PAST` when exactly one is set.
    pub fn tense(self) -> Option<Features> {
        self.one_of(Features::PRESENT.union(Features::PAST))
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
}

/// How far a tagger stands behind its best guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Confidence {
    /// The lexicon allows one tag, or context settles it.
    Sure,
    /// The best guess by rule, with others possible.
    Likely,
    /// It could not narrow; the guess is the word's most common tag.
    Unsure,
    /// Not in the lexicon; the guess comes from the word's shape.
    Unknown,
}

impl Confidence {
    /// Every level, most confident first.
    pub const ALL: [Confidence; 4] = [
        Confidence::Sure,
        Confidence::Likely,
        Confidence::Unsure,
        Confidence::Unknown,
    ];

    /// The name an imported file's `Conf=` uses.
    pub fn name(self) -> &'static str {
        match self {
            Confidence::Sure => "Sure",
            Confidence::Likely => "Likely",
            Confidence::Unsure => "Unsure",
            Confidence::Unknown => "Unknown",
        }
    }

    /// Its place in [`Confidence::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The level named `name`.
    pub fn from_name(name: &str) -> Option<Confidence> {
        Confidence::ALL
            .into_iter()
            .find(|level| level.name() == name)
    }

    /// Whether it is `Sure` or `Likely`: the levels a tagger commits to, and that accuracy counts.
    pub fn committed(self) -> bool {
        matches!(self, Confidence::Sure | Confidence::Likely)
    }
}

/// What a tagger concluded about one word token.
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

    #[test]
    fn the_fixed_order_is_the_indexes() {
        for (i, tag) in Tag::ALL.into_iter().enumerate() {
            assert_eq!(tag.index(), i);
        }
        assert_eq!(Tag::ALL.len(), 13);
    }

    #[test]
    fn tag_sets_hold_tags() {
        let set: TagSet = [Tag::Verb, Tag::Noun, Tag::Verb].into_iter().collect();
        assert_eq!(set.len(), 2);
        assert!(set.contains(Tag::Noun) && !set.contains(Tag::Adverb));
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![Tag::Noun, Tag::Verb]);
        assert!(TagSet::EMPTY.is_empty());
        assert_eq!(TagSet::of(Tag::Noun).with(Tag::Verb), set);
    }

    fn feats(text: &str) -> Features {
        Features::from_ud(text).unwrap()
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
        assert!(Features::from_ud("Number").unwrap_err().contains("Number"));
        assert!(Features::from_ud("Number=").is_err());
    }

    #[test]
    fn values_are_the_one_flag_set_or_none() {
        let f = feats("Number=Sing|VerbForm=Fin|Tense=Past");
        assert_eq!(f.number(), Some(Features::SINGULAR));
        assert_eq!(f.verb_form(), Some(Features::FINITE));
        assert_eq!(f.tense(), Some(Features::PAST));
        assert_eq!(Features::NONE.number(), None);
        assert_eq!(Features::NONE.verb_form(), None);
        assert_eq!(Features::NONE.tense(), None);
        let both = Features::FINITE.union(Features::INFINITIVE);
        assert_eq!(both.verb_form(), None);
        let tenses = Features::PRESENT.union(Features::PAST);
        assert_eq!(tenses.tense(), None);
    }

    #[test]
    fn confidence_levels_have_names_and_commit_at_likely() {
        for level in Confidence::ALL {
            assert_eq!(Confidence::from_name(level.name()), Some(level));
        }
        assert_eq!(Confidence::from_name("sure"), None);
        let committed: Vec<bool> = Confidence::ALL.iter().map(|c| c.committed()).collect();
        assert_eq!(committed, vec![true, true, false, false]);
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
}
