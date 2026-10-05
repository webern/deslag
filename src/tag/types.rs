//! The types a tagger speaks in: a [`Tag`], a [`TagSet`], [`Features`], a [`Confidence`] and the
//! [`Reading`] that holds them, the [`Context`] a sentence is read in, and an [`Origin`].

/// A part of speech, as deslag tags it. The order of [`Tag::ALL`] is the order every table of the
/// exam's report uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
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

    /// The short code the exam's report and an imported file's `Kept=` use, and the golden tag
    /// stream.
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

    const fn bit(self) -> u16 {
        1 << (self as u8)
    }
}

/// A set of tags, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TagSet(u16);

impl TagSet {
    /// The empty set.
    pub const EMPTY: TagSet = TagSet(0);

    /// The set of just `tag`.
    pub const fn of(tag: Tag) -> TagSet {
        TagSet(tag.bit())
    }

    /// This set and `tag`.
    pub const fn with(self, tag: Tag) -> TagSet {
        TagSet(self.0 | tag.bit())
    }

    /// The tags in both this set and `other`.
    pub const fn intersection(self, other: TagSet) -> TagSet {
        TagSet(self.0 & other.0)
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

/// The flags a word's features may set: the 14 of the exam's gold, and [`Features::CONTRACTION`].
///
/// They come from the lexicon, never from context.
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
    /// A second word is fused on after the word the reading describes (`n't`, `'s`, `'re`, `'ll`,
    /// `'m`, `'ve`, `'d`, or a possessive `'s`), and has no reading of its own. A lint that cares
    /// which reads the token's text. Gold does not carry it and the exam does not score it.
    pub const CONTRACTION: Features = Features(1 << 14);

    /// Every flag, in bit order.
    pub const ALL: [Features; 15] = [
        Features::SINGULAR,
        Features::PLURAL,
        Features::FIRST,
        Features::SECOND,
        Features::THIRD,
        Features::FINITE,
        Features::INFINITIVE,
        Features::PAST_PARTICIPLE,
        Features::PRESENT_PARTICIPLE,
        Features::PRESENT,
        Features::PAST,
        Features::POSITIVE,
        Features::COMPARATIVE,
        Features::SUPERLATIVE,
        Features::CONTRACTION,
    ];

    /// Whether every flag of `other` is set here.
    pub fn contains(self, other: Features) -> bool {
        self.0 & other.0 == other.0
    }

    /// The flags of both.
    pub const fn union(self, other: Features) -> Features {
        Features(self.0 | other.0)
    }

    /// The flags of `self` that are also in `mask`.
    pub(super) fn only(self, mask: Features) -> Features {
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
}

/// How far a tagger stands behind its best guess. Deliberately not `Ord`: [`Confidence::ALL`] runs
/// most confident first, and [`Confidence::at_least`] compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Confidence {
    /// No other tag is possible here: [`Reading::possible`] is the best guess alone. The
    /// closed-class table gives the word one tag, or a pass removed every other or confirmed the
    /// one tag a lexicon word has.
    Sure,
    /// Other tags are still possible, and the tagger chose the best guess from the context: a rule
    /// over the neighbours, or a model's judgement. A rule that leans on a neighbour below `Likely`
    /// does not raise a word to `Likely`.
    Likely,
    /// The word is in a table with several tags, or in the lexicon with one, and no pass narrowed
    /// or confirmed it. The best guess is the tag the table ranks first for the word.
    Unsure,
    /// The word is in neither table. The best guess comes from its shape (an `-ly` ending), else
    /// it is [`Tag::Noun`]; `kept` holds what the shape allows.
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

    /// Whether it is as confident as `min` or more: `Sure` over `Likely` over `Unsure` over
    /// `Unknown`.
    pub fn at_least(self, min: Confidence) -> bool {
        self.index() <= min.index()
    }
}

/// What a tagger concluded about one word.
///
/// There is always a best guess, [`Reading::tag`], even when the tagger is unsure. Only the tags of
/// the other possibilities are kept, not their features: a pass that needs an alternative's
/// features asks the lexicon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reading {
    /// The best guess, always present.
    pub tag: Tag,
    /// The features of the best guess only.
    pub features: Features,
    /// How far the tagger stands behind it.
    pub confidence: Confidence,
    /// Every tag still possible, the best guess included. An outside tagger may leave `tag` out;
    /// use [`Reading::possible`].
    pub kept: TagSet,
}

impl Reading {
    /// The tags not ruled out: `kept` and the best guess.
    pub fn possible(&self) -> TagSet {
        self.kept.with(self.tag)
    }
}

/// The kind of block a sentence is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Context {
    /// Anything but the three below, quotes and footnotes included.
    Prose,
    /// Under a list item.
    ListItem,
    /// A heading.
    Heading,
    /// A table cell.
    TableCell,
}

impl Context {
    /// Every context, in the order the exam's report lists them.
    pub const ALL: [Context; 4] = [
        Context::Prose,
        Context::ListItem,
        Context::Heading,
        Context::TableCell,
    ];

    /// The name a gold file's `exam.context` uses.
    pub fn name(self) -> &'static str {
        match self {
            Context::Prose => "prose",
            Context::ListItem => "list-item",
            Context::Heading => "heading",
            Context::TableCell => "table-cell",
        }
    }

    /// The context named `name`.
    pub fn from_name(name: &str) -> Option<Context> {
        Context::ALL
            .into_iter()
            .find(|context| context.name() == name)
    }
}

/// Where a word comes from, which is not what it does in its sentence: `grep` is a command in
/// *grep the logs* and in *run grep*. It is not a tag; the tags and the mapping to UD stay as they
/// are. A tagger reads it from the token and its neighbours, and every tagger and import shares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Origin {
    /// Ordinary English, which is every word until a cue says otherwise.
    #[default]
    English,
    /// A name from code, written as one: `foo_bar`, `FrobulatorFactory`, `userId`.
    Symbol,
    /// The name of a program: `grep`, or a git subcommand right after `git`.
    Command,
    /// A file name with an extension: `main.rs`.
    Path,
    /// A word joined to a leading `-` or `--`: `--locked`.
    Flag,
}

impl Origin {
    /// Every origin, in the order the exam's report lists them.
    pub const ALL: [Origin; 5] = [
        Origin::English,
        Origin::Symbol,
        Origin::Command,
        Origin::Path,
        Origin::Flag,
    ];

    /// The name an `Origin=` in the exam's token skeleton uses.
    pub fn name(self) -> &'static str {
        match self {
            Origin::English => "English",
            Origin::Symbol => "Symbol",
            Origin::Command => "Command",
            Origin::Path => "Path",
            Origin::Flag => "Flag",
        }
    }

    /// The origin named `name`.
    pub fn from_name(name: &str) -> Option<Origin> {
        Origin::ALL.into_iter().find(|origin| origin.name() == name)
    }

    /// Its place in [`Origin::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

// A reading is six bytes and `Option<Reading>` no more, so `Token` does not grow.
const _: () = assert!(size_of::<Reading>() == 6);
const _: () = assert!(size_of::<Option<Reading>>() == 6);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_order_is_the_indexes() {
        for (i, tag) in Tag::ALL.into_iter().enumerate() {
            assert_eq!(tag.index(), i);
        }
        assert_eq!(Tag::ALL.len(), 13);
    }

    #[test]
    fn codes_round_trip() {
        for tag in Tag::ALL {
            assert_eq!(Tag::from_code(tag.code()), Some(tag));
        }
        assert_eq!(Tag::from_code("noun"), None);
        assert_eq!(Tag::from_code("CCONJ"), None);
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

    #[test]
    fn tag_sets_intersect() {
        let a = TagSet::of(Tag::Noun).with(Tag::Verb);
        let b = TagSet::of(Tag::Verb).with(Tag::Adverb);
        assert_eq!(a.intersection(b), TagSet::of(Tag::Verb));
        assert_eq!(a.intersection(TagSet::EMPTY), TagSet::EMPTY);
        assert_eq!(a.intersection(a), a);
    }

    #[test]
    fn tag_sets_build_in_const_context() {
        const VERBISH: TagSet = TagSet::of(Tag::Verb).with(Tag::Auxiliary);
        assert_eq!(VERBISH.len(), 2);
    }

    #[test]
    fn the_flags_are_fifteen_distinct_bits_in_order() {
        let mut seen = Features::NONE;
        for (bit, flag) in Features::ALL.into_iter().enumerate() {
            assert_eq!(flag, Features(1 << bit));
            assert!(!seen.contains(flag));
            seen = seen.union(flag);
        }
        assert_eq!(Features::CONTRACTION, Features(1 << 14));
        assert_eq!(Features::ALL[13], Features::SUPERLATIVE);
    }

    #[test]
    fn values_are_the_one_flag_set_or_none() {
        let f = Features::SINGULAR
            .union(Features::FINITE)
            .union(Features::PAST);
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
        assert_eq!(
            f.union(Features::CONTRACTION).number(),
            Some(Features::SINGULAR)
        );
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
    fn at_least_runs_from_sure_down_to_unknown() {
        for (i, level) in Confidence::ALL.into_iter().enumerate() {
            for (j, min) in Confidence::ALL.into_iter().enumerate() {
                assert_eq!(level.at_least(min), i <= j, "{level:?} at least {min:?}");
            }
            assert!(level.at_least(Confidence::Unknown));
        }
        assert!(Confidence::Sure.at_least(Confidence::Sure));
        assert!(!Confidence::Likely.at_least(Confidence::Sure));
    }

    #[test]
    fn a_reading_is_possible_as_kept_plus_best_guess() {
        let reading = Reading {
            tag: Tag::Verb,
            features: Features::NONE,
            confidence: Confidence::Likely,
            kept: TagSet::of(Tag::Noun),
        };
        assert_eq!(reading.possible().len(), 2);
        assert!(reading.possible().contains(Tag::Verb));
    }

    #[test]
    fn origins_have_names_and_english_is_the_default() {
        for (i, origin) in Origin::ALL.into_iter().enumerate() {
            assert_eq!(Origin::from_name(origin.name()), Some(origin));
            assert_eq!(origin.index(), i);
        }
        assert_eq!(Origin::from_name("english"), None);
        assert_eq!(Origin::default(), Origin::English);
    }

    #[test]
    fn contexts_have_names() {
        for context in Context::ALL {
            assert_eq!(Context::from_name(context.name()), Some(context));
        }
        assert_eq!(Context::from_name("Prose"), None);
    }
}
