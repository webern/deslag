//! Guessing a word the tables lack from its shape.
//!
//! The closed-class table and the lexicon read a word on its own, and a word in neither is
//! `Unknown`. This module gives such a word a best guess, features and a set of tags to keep from
//! the way it is written, using what English and Markdown say about shapes and nothing counted from
//! a corpus: the endings English builds words with, capitals, digits, and the forms identifiers
//! take.
//!
//! **The level stays `Unknown`.** A guess from shape is not a reading from a table, and the unknown
//! rate is how the exam measures the lexicon's coverage. A guess changes the best guess, the
//! features and the kept set, never the level, so the rate does not move.
//!
//! **A capitalised word always keeps a proper noun**, whatever else its shape says, which the
//! proper-noun pass needs: a capital also starts a sentence, so the shape cannot rule a name out.
//!
//! The first rule that fits a word decides, in this order.
//!
//! 1. **A possessive** `x's` is a singular noun with a second word fused on, a proper noun when
//!    `x` is written as a name.
//! 2. **A version label**, `v` and digits (`v2`, `v1.2`), is a numeral.
//! 3. **A dotted word.** Single letters between dots are an abbreviation: initials in capitals
//!    (`U.S`), an adverb in lower case (`e.g`, `i.e`, `a.m`). A site (`www.adobe.com`) is a proper
//!    noun. A file, a module or a newsgroup (`report.pdf`, `node.js`) may or may not be named,
//!    which its shape does not say, so it is a noun that may be a name, as an acronym is.
//! 4. **An identifier**, a word with an underscore or a colon in it (`foo_bar`, `docs:setup`), is
//!    the name of a thing: a proper noun.
//! 5. **A word that starts with a digit.** An ordinal (`4th`) is an adjective. A decade (`1990s`)
//!    is a plural noun. A digit and a capitalised word (`1Password`) is a product, a proper noun.
//!    A number and a unit (`400k`, `8gb`) is a numeral.
//! 6. **Camel case** (`userId`, `PowerShell`), a capital straight after a lower-case letter, and
//!    **a digit after a letter** (`ESP32`, `x86`) are the names of things: proper nouns.
//! 7. **A word in capitals** is an acronym and a noun, and its plural (`APIs`) a plural noun. An
//!    acronym may be a name (`NASA`) or a thing (`API`), which the shape does not say, so it is a
//!    noun that may be a name.
//! 8. **An ending.** `-ly` is an adverb, `-ing` a verb's present participle, `-ed` a verb's past,
//!    `-ize`, `-ise` and `-ify` a verb, `-tion`, `-sion`, `-ment`, `-ness`, `-ity`, `-ism`,
//!    `-ship` and `-hood` a noun, `-ous`, `-ible`, `-able`, `-less`, `-ful`, `-ive` and `-al` an
//!    adjective, and a plain `-s` a plural noun that may be a verb's third person. Words in
//!    `-ss`, `-us` and `-is` are singular nouns (`class`, `status`, `analysis`). The ending needs
//!    a stem of three letters, and a plural a word of four.
//! 9. **A capital** starts a name. A capitalised word is a proper noun, unless its ending makes
//!    common nouns (`Reactivity`). The ending still adds the tags it allows, since a capital also
//!    starts a sentence. Which of them a capital means is for the pass that sees where it stands.
//! 10. **Nothing else** is a singular noun that may be a verb or an adjective: the open class is
//!     open, and a word that no table knows has no shape to say which.

use super::{Confidence, Features, Reading, Tag, TagSet};

/// What a rule says of a word: the best guess, its features, and the tags its shape allows, which
/// include the best guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    tag: Tag,
    features: Features,
    kept: TagSet,
}

impl Shape {
    /// A shape of `tag` that may also be any of `others`.
    const fn new(tag: Tag, features: Features, others: TagSet) -> Shape {
        Shape {
            tag,
            features,
            kept: others.with(tag),
        }
    }
}

/// An ending that builds a word of one kind.
struct Ending {
    /// The ending, in lower case.
    suffix: &'static str,
    /// What it builds.
    shape: Shape,
}

const fn ending(suffix: &'static str, shape: Shape) -> Ending {
    Ending { suffix, shape }
}

/// The fewest letters before an ending, so a short word that merely ends alike (`sing`, `bled`) is
/// not read as built from it.
const STEM: usize = 3;

/// The fewest letters of a word that a plain `-s` makes a plural of: `pls` is not one.
const PLURAL: usize = 4;

const NOUN: TagSet = TagSet::of(Tag::Noun);
const NAME: TagSet = TagSet::of(Tag::ProperNoun);
const VERB: TagSet = TagSet::of(Tag::Verb);
const ADJECTIVE: TagSet = TagSet::of(Tag::Adjective);

const SINGULAR: Features = Features::SINGULAR;
const POSITIVE: Features = Features::POSITIVE;
/// The finite past of a regular verb, which is how the lexicon reads `installed`.
const PAST: Features = Features::FINITE.union(Features::PAST);

/// The endings, in the order they are tried. `-ly` goes first, so `-fully` is an adverb and not an
/// adjective, and a noun ending goes before the plain `-s`, so `-ness` is no plural.
const ENDINGS: [Ending; 21] = [
    ending("ly", Shape::new(Tag::Adverb, POSITIVE, ADJECTIVE)),
    ending(
        "ing",
        Shape::new(
            Tag::Verb,
            Features::PRESENT_PARTICIPLE,
            NOUN.with(Tag::Adjective),
        ),
    ),
    ending("ed", Shape::new(Tag::Verb, PAST, ADJECTIVE)),
    ending("ize", Shape::new(Tag::Verb, Features::INFINITIVE, NOUN)),
    ending("ise", Shape::new(Tag::Verb, Features::INFINITIVE, NOUN)),
    ending("ify", Shape::new(Tag::Verb, Features::INFINITIVE, NOUN)),
    ending("tion", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("sion", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("ment", Shape::new(Tag::Noun, SINGULAR, VERB)),
    ending("ness", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("ity", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("ism", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("ship", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("hood", Shape::new(Tag::Noun, SINGULAR, NOUN)),
    ending("ous", Shape::new(Tag::Adjective, POSITIVE, ADJECTIVE)),
    ending("ible", Shape::new(Tag::Adjective, POSITIVE, ADJECTIVE)),
    ending("able", Shape::new(Tag::Adjective, POSITIVE, ADJECTIVE)),
    ending("less", Shape::new(Tag::Adjective, POSITIVE, ADJECTIVE)),
    ending("ful", Shape::new(Tag::Adjective, POSITIVE, NOUN)),
    ending("ive", Shape::new(Tag::Adjective, POSITIVE, NOUN)),
    ending("al", Shape::new(Tag::Adjective, POSITIVE, NOUN)),
];

/// The last parts of an address that are not also the usual ending of a file: `.com` and `.uk`, not
/// `.md`, `.py` or `.rs`, which are countries' as well but files first.
const DOMAINS: [&str; 14] = [
    "com", "net", "org", "edu", "gov", "mil", "int", "info", "biz", "io", "co", "uk", "us", "de",
];

/// Guesses the reading of `text`, which the tables do not have: from its shape, at `Unknown`.
pub(super) fn guess(text: &str) -> Reading {
    let shape = shape(text);
    Reading {
        tag: shape.tag,
        features: shape.features,
        confidence: Confidence::Unknown,
        kept: if starts_upper(text) {
            shape.kept.with(Tag::ProperNoun)
        } else {
            shape.kept
        },
    }
}

/// What the rules make of `text`.
fn shape(text: &str) -> Shape {
    if let Some(stem) = possessive_stem(text) {
        return possessive(stem);
    }
    if let Some(shape) = identifier(text) {
        return shape;
    }
    let ending = ending_of(text);
    capitalised(text, ending).or(ending).unwrap_or(Shape::new(
        Tag::Noun,
        SINGULAR,
        VERB.with(Tag::Adjective),
    ))
}

// ---- rule 1: a possessive -------------------------------------------------------------------

/// The word before a possessive `'s`, straight or curly, when there is one.
fn possessive_stem(text: &str) -> Option<&str> {
    let stem = text.strip_suffix(['s', 'S'])?;
    let stem = stem
        .strip_suffix('\'')
        .or_else(|| stem.strip_suffix('\u{2019}'))?;
    (!stem.is_empty()).then_some(stem)
}

/// A possessive `x's`: a singular noun with a second word fused on, a proper noun when `x` is
/// written as a name, and the nouns `x` may be kept.
fn possessive(stem: &str) -> Shape {
    let of_stem = shape(stem);
    let tag = if of_stem.tag == Tag::ProperNoun {
        Tag::ProperNoun
    } else {
        Tag::Noun
    };
    let nouns = intersect(of_stem.kept, NOUN.with(Tag::ProperNoun));
    Shape::new(
        tag,
        SINGULAR.union(Features::CONTRACTION),
        nouns.with(Tag::Noun),
    )
}

// ---- rules 2 to 6: versions, dots, identifiers and digits -----------------------------------

/// The shape of a version label, a dotted word, an identifier or a word with a digit in it, if
/// `text` is one.
fn identifier(text: &str) -> Option<Shape> {
    let starts_with_digit = text.starts_with(|c: char| c.is_ascii_digit());
    if is_version(text) {
        Some(Shape::new(Tag::Numeral, Features::NONE, NOUN))
    } else if text.contains('.') && !starts_with_digit {
        Some(dotted(text))
    } else if text.contains(['_', ':']) {
        Some(Shape::new(Tag::ProperNoun, SINGULAR, NOUN))
    } else if starts_with_digit {
        starting_with_a_digit(text)
    } else if is_camel_case(text) || text.chars().any(|c| c.is_ascii_digit()) {
        Some(Shape::new(Tag::ProperNoun, SINGULAR, NOUN))
    } else {
        None
    }
}

/// Whether `text` is `v` and digits, with dots between groups: `v2`, `V1.2.3`.
fn is_version(text: &str) -> bool {
    let Some(rest) = text.strip_prefix(['v', 'V']) else {
        return false;
    };
    rest.starts_with(|c: char| c.is_ascii_digit())
        && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !rest.ends_with('.')
}

/// A word with dots in it: an abbreviation, a site, or a thing that may be named.
fn dotted(text: &str) -> Shape {
    let abbreviation = text
        .split('.')
        .all(|part| part.chars().count() == 1 && part.chars().all(char::is_alphabetic));
    let last = text.rsplit('.').next().unwrap_or("");
    let site = text
        .get(..4)
        .is_some_and(|start| start.eq_ignore_ascii_case("www."))
        || DOMAINS
            .iter()
            .any(|domain| last.eq_ignore_ascii_case(domain));
    if abbreviation && starts_upper(text) {
        // `U.S`, `J.M`: initials.
        Shape::new(Tag::ProperNoun, SINGULAR, NOUN.with(Tag::Adverb))
    } else if abbreviation {
        // `e.g`, `i.e`, `a.m`: the adverbs that Latin and the clock gave English.
        Shape::new(Tag::Adverb, Features::NONE, NOUN)
    } else if site {
        Shape::new(Tag::ProperNoun, SINGULAR, NOUN)
    } else {
        Shape::new(Tag::Noun, SINGULAR, NAME)
    }
}

/// A word that starts with digits and goes on with letters, which is `None` when nothing follows
/// the digits.
fn starting_with_a_digit(text: &str) -> Option<Shape> {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | ','));
    if rest.is_empty() {
        return None;
    }
    let ordinal = ["st", "nd", "rd", "th"]
        .iter()
        .any(|suffix| rest.eq_ignore_ascii_case(suffix));
    Some(if ordinal {
        // The adjective of `4th place`. Alone it is a noun or an adverb, or a numeral.
        Shape::new(
            Tag::Adjective,
            POSITIVE,
            NOUN.with(Tag::Numeral).with(Tag::Adverb),
        )
    } else if rest.eq_ignore_ascii_case("s") {
        // A decade, the plural of a number: `1990s`.
        Shape::new(Tag::Noun, Features::PLURAL, TagSet::of(Tag::Numeral))
    } else if is_capitalised_word(rest) {
        // A product written with a number first: `1Password`.
        Shape::new(Tag::ProperNoun, SINGULAR, NOUN)
    } else {
        // A number with a unit: `400k`, `8gb`.
        Shape::new(Tag::Numeral, Features::NONE, NOUN)
    })
}

/// Whether a lower-case letter is followed straight away by a capital: `userId`, `PowerShell`,
/// `xDS`. A word in capitals, or with only its first letter a capital, has no such pair.
fn is_camel_case(text: &str) -> bool {
    let mut after_lower = false;
    for ch in text.chars() {
        if after_lower && ch.is_uppercase() {
            return true;
        }
        after_lower = ch.is_lowercase();
    }
    false
}

// ---- rules 7 and 9: capitals ---------------------------------------------------------------

/// Whether `text` starts with an upper-case letter.
fn starts_upper(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_uppercase)
}

/// Whether `text` is a capital and then letters with a lower-case one among them: `Frobnitz`.
fn is_capitalised_word(text: &str) -> bool {
    starts_upper(text) && text.chars().any(char::is_lowercase)
}

/// Whether `text` is two or more letters and no lower case: an acronym, `API`.
fn is_all_capitals(text: &str) -> bool {
    text.chars().filter(|c| c.is_alphabetic()).count() >= 2 && !text.chars().any(char::is_lowercase)
}

/// The shape of an acronym or a capitalised word, or `None` for a word with no capital to read or
/// a lone capital letter. `ending` is what the word's ending says of it, which adds to what a
/// capitalised word may be, since a capital also starts a sentence.
fn capitalised(text: &str, ending: Option<Shape>) -> Option<Shape> {
    if !starts_upper(text) {
        return None;
    }
    if text.strip_suffix('s').is_some_and(is_all_capitals) {
        // The plural of an acronym: `APIs`, `CDNs`.
        return Some(Shape::new(Tag::Noun, Features::PLURAL, NOUN));
    }
    // An ending that makes common nouns makes no name (`Reactivity`).
    let common = |ending: Shape| ending.tag == Tag::Noun && ending.features == SINGULAR;
    let shape = if is_all_capitals(text) || ending.is_some_and(common) {
        Shape::new(Tag::Noun, SINGULAR, NAME)
    } else if is_capitalised_word(text) {
        Shape::new(Tag::ProperNoun, SINGULAR, NOUN)
    } else {
        return None;
    };
    Some(Shape {
        kept: ending.map_or(shape.kept, |ending| union(shape.kept, ending.kept)),
        ..shape
    })
}

/// The tags in `a` or `b`.
fn union(a: TagSet, b: TagSet) -> TagSet {
    b.iter().fold(a, TagSet::with)
}

/// The tags in both `a` and `b`.
fn intersect(a: TagSet, b: TagSet) -> TagSet {
    a.iter().filter(|tag| b.contains(*tag)).collect()
}

// ---- rule 8: endings ------------------------------------------------------------------------

/// What an ending says of `text`, whatever its case.
fn ending_of(text: &str) -> Option<Shape> {
    let letters = text.chars().count();
    ENDINGS
        .iter()
        .find(|ending| {
            letters >= ending.suffix.len() + STEM && ends_with_ignore_case(text, ending.suffix)
        })
        .map(|ending| ending.shape)
        .or_else(|| plural(text))
}

/// Whether `text` ends in `suffix`, an ASCII string in lower case, whatever the case of `text`.
fn ends_with_ignore_case(text: &str, suffix: &str) -> bool {
    let (text, suffix) = (text.as_bytes(), suffix.as_bytes());
    text.len() >= suffix.len() && text[text.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// A plain `-s`: a plural noun, or a verb's third person. Not after `s`, `u` or `i`, which end
/// singular nouns (`class`, `status`, `analysis`).
fn plural(text: &str) -> Option<Shape> {
    let before = text.strip_suffix(['s', 'S'])?;
    (text.chars().count() >= PLURAL && !before.ends_with(['s', 'S', 'u', 'U', 'i', 'I']))
        .then_some(Shape::new(Tag::Noun, Features::PLURAL, VERB))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{closed, fold, lexicon};
    use Tag::{Adjective, Adverb, Noun, Numeral, ProperNoun, Verb};

    fn set(tags: &[Tag]) -> TagSet {
        tags.iter().copied().collect()
    }

    /// What the closed-class table or the lexicon says of `text`, if either has it.
    fn table(text: &str) -> Option<Reading> {
        let mut buf = [0; crate::tag::LONGEST];
        let word = fold(text, &mut buf)?;
        closed::lookup(word).or_else(|| lexicon::lookup(word))
    }

    /// What `text` is read as. It is a word that no table has, so its reading is the guess.
    fn read(text: &str) -> Reading {
        assert!(table(text).is_none(), "{text} is in a table");
        let reading = guess(text);
        assert_eq!(reading, closed::unknown(text), "{text}");
        reading
    }

    /// Asserts the best guess, the features and the kept tags of `text`, and that it is `Unknown`.
    fn assert_guess(text: &str, tag: Tag, features: Features, kept: &[Tag]) {
        let reading = read(text);
        assert_eq!(reading.tag, tag, "{text}");
        assert_eq!(reading.features, features, "{text}");
        assert_eq!(reading.kept, set(kept), "{text}");
        assert_eq!(reading.confidence, Confidence::Unknown, "{text}");
    }

    /// The kept tags of a word of no shape: a singular noun that may be a verb or an adjective.
    const OPEN: [Tag; 3] = [Noun, Verb, Adjective];

    #[test]
    fn an_ending_makes_an_adverb() {
        assert_guess("frobnicately", Adverb, POSITIVE, &[Adverb, Adjective]);
        // Not enough before it.
        assert_guess("frly", Noun, SINGULAR, &OPEN);
    }

    #[test]
    fn an_ending_makes_a_verb_form() {
        assert_guess(
            "snapshotting",
            Verb,
            Features::PRESENT_PARTICIPLE,
            &[Verb, Noun, Adjective],
        );
        assert_guess("recomputed", Verb, PAST, &[Verb, Adjective]);
        for word in ["parallelize", "frobnicise", "containerify"] {
            assert_guess(word, Verb, Features::INFINITIVE, &[Verb, Noun]);
        }
        // Too little before the ending.
        assert_guess("fring", Noun, SINGULAR, &OPEN);
        assert_guess("fzed", Noun, SINGULAR, &OPEN);
        assert_guess("frize", Noun, SINGULAR, &OPEN);
    }

    #[test]
    fn an_ending_makes_a_noun() {
        for word in [
            "reconfiguration",
            "frobnification",
            "reactiveness",
            "extensibility",
            "lifecycleism",
            "mentorship",
            "frobnicohood",
        ] {
            assert_guess(word, Noun, SINGULAR, &[Noun]);
        }
        // A noun that may also be a verb.
        assert_guess("frobnicament", Noun, SINGULAR, &[Noun, Verb]);
    }

    #[test]
    fn an_ending_makes_an_adjective() {
        for word in ["frobnicatious", "frobnicable", "frobnicible", "frobless"] {
            assert_guess(word, Adjective, POSITIVE, &[Adjective]);
        }
        // An adjective that may also be a noun.
        for word in ["frobful", "frobnicative", "frobnicational"] {
            assert_guess(word, Adjective, POSITIVE, &[Adjective, Noun]);
        }
    }

    #[test]
    fn a_plain_s_is_a_plural_noun_that_may_be_a_verb() {
        assert_guess("frobnicators", Noun, Features::PLURAL, &[Noun, Verb]);
        assert_guess("frobnicates", Noun, Features::PLURAL, &[Noun, Verb]);
        // These end a singular noun, or are too short.
        for word in ["frobnicass", "frobnicus", "frobnicis", "frs"] {
            assert_guess(word, Noun, SINGULAR, &OPEN);
        }
    }

    #[test]
    fn a_word_of_no_shape_is_a_singular_noun_that_may_be_a_verb_or_an_adjective() {
        assert_guess("frobnitz", Noun, SINGULAR, &OPEN);
        assert_guess("über", Noun, SINGULAR, &OPEN);
    }

    #[test]
    fn a_capitalised_word_is_a_proper_noun_that_may_be_a_noun() {
        assert_guess("Frobnitz", ProperNoun, SINGULAR, &[ProperNoun, Noun]);
        assert_guess("Über", ProperNoun, SINGULAR, &[ProperNoun, Noun]);
        assert_guess(
            "Frobnitzes",
            ProperNoun,
            SINGULAR,
            &[ProperNoun, Noun, Verb],
        );
    }

    #[test]
    fn the_ending_of_a_capitalised_word_adds_what_it_may_be() {
        assert_guess(
            "Frobnicately",
            ProperNoun,
            SINGULAR,
            &[ProperNoun, Noun, Adverb, Adjective],
        );
        for word in ["Frobnicating", "Frobnicated"] {
            assert_guess(
                word,
                ProperNoun,
                SINGULAR,
                &[ProperNoun, Noun, Verb, Adjective],
            );
        }
    }

    #[test]
    fn a_capitalised_word_with_an_ending_that_makes_common_nouns_is_one() {
        for word in ["Frobnicity", "Frobnication", "Frobnicness", "Frobnicism"] {
            assert_guess(word, Noun, SINGULAR, &[Noun, ProperNoun]);
        }
        // An ending that makes a plural does not.
        assert_eq!(read("Frobnicators").tag, ProperNoun);
    }

    #[test]
    fn an_acronym_is_a_noun_that_may_be_a_name_and_its_plural_a_plural_noun() {
        assert_guess("FROBZ", Noun, SINGULAR, &[Noun, ProperNoun]);
        assert_guess("FZ", Noun, SINGULAR, &[Noun, ProperNoun]);
        assert_guess("FROBZs", Noun, Features::PLURAL, &[Noun, ProperNoun]);
        // A lone capital is no acronym and has no shape, but may be a name.
        let lone = guess("Q");
        assert_eq!(lone.tag, Noun);
        assert_eq!(lone.kept, set(&[Noun, Verb, Adjective, ProperNoun]));
    }

    #[test]
    fn a_possessive_is_a_singular_noun_with_a_word_fused_on() {
        let fused = SINGULAR.union(Features::CONTRACTION);
        assert_guess("frobnitz's", Noun, fused, &[Noun]);
        assert_guess("FROBZ\u{2019}S", Noun, fused, &[Noun, ProperNoun]);
        // A name when the stem is written as one.
        assert_guess("Frobnitz\u{2019}s", ProperNoun, fused, &[Noun, ProperNoun]);
        assert_guess("userId's", ProperNoun, fused, &[Noun, ProperNoun]);
        // A lone `'s` or `s` is no possessive.
        assert_eq!(possessive_stem("'s"), None);
        assert_eq!(possessive_stem("s"), None);
    }

    #[test]
    fn a_version_label_is_a_numeral() {
        for word in ["v2", "v58", "v1.2", "v1.2.3"] {
            assert_guess(word, Numeral, Features::NONE, &[Numeral, Noun]);
        }
        // A capital keeps a name possible, as it does for every word.
        assert_guess(
            "V1.2",
            Numeral,
            Features::NONE,
            &[Numeral, Noun, ProperNoun],
        );
        // Not a label: no digits, a trailing dot, or a word.
        assert_guess("vx", Noun, SINGULAR, &OPEN);
        assert_ne!(shape("v1.").tag, Numeral);
        assert_guess("vfrobnitz", Noun, SINGULAR, &OPEN);
    }

    #[test]
    fn a_dotted_abbreviation_is_initials_in_capitals_and_an_adverb_in_lower_case() {
        for word in ["e.g", "i.e", "a.m"] {
            assert_guess(word, Adverb, Features::NONE, &[Adverb, Noun]);
        }
        for word in ["U.S", "J.M", "W.H.S"] {
            assert_guess(word, ProperNoun, SINGULAR, &[ProperNoun, Noun, Adverb]);
        }
    }

    #[test]
    fn a_site_is_a_proper_noun() {
        for word in [
            "frobnitz.com",
            "www.frobnitz.example",
            "irc.frobnitz.net",
            "frobnitz.Co.UK",
        ] {
            assert_guess(word, ProperNoun, SINGULAR, &[ProperNoun, Noun]);
        }
    }

    #[test]
    fn a_file_or_a_module_is_a_noun_that_may_be_a_name() {
        for word in [
            "frobnitz.pdf",
            "Frobnitz.json",
            "frobnitz.md",
            "frobnitz.rs",
            "frobnitz.sent",
            "alt.frobs.cat",
        ] {
            let reading = read(word);
            assert_eq!(reading.tag, Noun, "{word}");
            assert_eq!(reading.features, SINGULAR, "{word}");
            assert_eq!(reading.kept, set(&[Noun, ProperNoun]), "{word}");
        }
    }

    #[test]
    fn an_identifier_with_an_underscore_or_a_colon_is_a_proper_noun() {
        for word in ["foo_bar", "0001_initial", "docs:setup", "x86_64"] {
            assert_guess(word, ProperNoun, SINGULAR, &[ProperNoun, Noun]);
        }
    }

    #[test]
    fn camel_case_is_a_proper_noun() {
        for word in ["userId", "backgroundColor", "PowerShell", "xDS", "McFrob"] {
            assert_guess(word, ProperNoun, SINGULAR, &[ProperNoun, Noun]);
        }
        // Capitals only at the start, or all of them, are no camel.
        assert!(!is_camel_case("Frobnitz"));
        assert!(!is_camel_case("FROBNITZ"));
    }

    #[test]
    fn a_word_with_a_digit_after_a_letter_is_a_proper_noun() {
        for word in ["ESP32", "x86", "tik4net", "ES2015", "S3", "k8s"] {
            assert_guess(word, ProperNoun, SINGULAR, &[ProperNoun, Noun]);
        }
    }

    #[test]
    fn a_word_that_starts_with_a_digit_is_an_ordinal() {
        for word in ["4th", "22nd", "1st", "3RD"] {
            assert_guess(
                word,
                Adjective,
                POSITIVE,
                &[Adjective, Noun, Numeral, Adverb],
            );
        }
    }

    #[test]
    fn a_decade_is_a_plural_noun() {
        for word in ["1990s", "80s"] {
            assert_guess(word, Noun, Features::PLURAL, &[Noun, Numeral]);
        }
    }

    #[test]
    fn a_number_with_a_unit_is_a_numeral_and_a_product_with_a_number_first_a_name() {
        for word in ["400k", "8gb", "45p", "100ms", "3G", "2.5mb"] {
            assert_guess(word, Numeral, Features::NONE, &[Numeral, Noun]);
        }
        assert_guess("1Password", ProperNoun, SINGULAR, &[ProperNoun, Noun]);
    }

    #[test]
    fn a_guess_is_always_unknown_and_keeps_its_best_guess() {
        for word in [
            "frobnitz",
            "Frobnitz",
            "FROBZ",
            "FROBZs",
            "v2",
            "4th",
            "foo_bar",
            "e.g",
            "x86",
            "frobnitz's",
            "frobnitzes",
            "recomputed",
            "über",
            "Q",
            "A.B.C",
            "1",
            "-",
            "'s",
        ] {
            let reading = guess(word);
            assert_eq!(reading.confidence, Confidence::Unknown, "{word}");
            assert!(reading.kept.contains(reading.tag), "{word}");
        }
    }

    #[test]
    fn a_capitalised_word_always_keeps_a_proper_noun_for_the_pass_that_reads_where_it_stands() {
        for word in [
            "Frobnitz",
            "Frobnicately",
            "Frobnicating",
            "Frobnicated",
            "Frobnicity",
            "Frobnications",
            "Frobnitzes",
            "FROBZ",
            "FROBZs",
            "Über",
            "Q",
            "V1.2",
            "Frobnitz.pdf",
            "Frobnitz's",
        ] {
            assert!(guess(word).kept.contains(ProperNoun), "{word}");
        }
    }

    #[test]
    fn a_word_in_a_table_is_read_from_it_and_not_guessed_at() {
        let mut tokens = crate::document::Token::split("In 4th place, running.");
        crate::tag::sentence(&mut tokens, crate::tag::Context::Prose);
        let reading = |word: &str| tokens.iter().find(|t| t.text == word).unwrap().reading;
        assert_eq!(reading("running").unwrap().confidence, Confidence::Unsure);
        assert_eq!(reading("4th").unwrap().confidence, Confidence::Unknown);
        assert_eq!(reading("4th").unwrap().tag, Adjective);
    }

    #[test]
    fn an_ending_is_in_lower_case_and_keeps_what_it_builds() {
        for ending in &ENDINGS {
            assert_eq!(ending.suffix, ending.suffix.to_ascii_lowercase());
            assert!(ending.shape.kept.contains(ending.shape.tag));
        }
    }
}
