//! The open-class lexicon: the nouns, verbs, adjectives and adverbs of everyday English, with the
//! inflected forms, as a generated text file that is checked in.
//!
//! `lexicon.txt` is made by `scripts/lexicon/generate.sh` from SCOWL, WordNet, Moby and AGID, which
//! are all shippable and whose notices are in `LICENSES/`. Nothing in it is counted from a corpus.
//! It opens with `#` lines that name the sources, the size level and the format, then holds one
//! word to a line, `word<TAB>readings`, sorted bytewise.
//!
//! The readings are tag letters in rank order, the best guess first, and after the first an
//! uppercase letter for the features of that guess: `runs<TAB>vZn` is a verb, finite present third
//! person singular, that may also be a noun. The ranking comes from how often WordNet's
//! senses of the lemma were met in SemCor, its sense-tagged corpus, for each part of speech (spread
//! over the forms the part of speech has), then Moby's priority order and WordNet's sense counts,
//! never from a treebank. A `!` last on a line marks a word whose best guess has nine tenths of its
//! counts and at least five of them (see `dominant`), which only the prior pass reads.
//!
//! - **Confidence.** Every word here is `Unsure`, whatever its tag count. The open class is open:
//!   a word the lexicon gives as a noun can be a verb in the next sentence, so one tag in it does
//!   not make the word `Sure`.
//! - **Possessives.** A word `x's` that the lexicon lacks is read from the stem `x`: the stem's
//!   noun or proper noun, with the stem's number and [`Features::CONTRACTION`]. The generator
//!   leaves possessives out.
//! - **Lookup.** The file stays as it is in the binary. The first lookup hashes each line's word
//!   into a table of line offsets, a few milliseconds and half a megabyte, and a lookup after that
//!   costs a hash and a probe or two, and allocates nothing.

use std::sync::OnceLock;

use super::{Confidence, Features, Reading, Tag, TagSet};

/// The lexicon file.
const TEXT: &str = include_str!("lexicon.txt");

/// The tag a letter of the file stands for.
const TAGS: [(char, Tag); 13] = [
    ('n', Tag::Noun),
    ('p', Tag::ProperNoun),
    ('v', Tag::Verb),
    ('x', Tag::Auxiliary),
    ('a', Tag::Adjective),
    ('r', Tag::Adverb),
    ('q', Tag::Pronoun),
    ('d', Tag::Determiner),
    ('i', Tag::Adposition),
    ('t', Tag::Particle),
    ('c', Tag::Conjunction),
    ('m', Tag::Numeral),
    ('j', Tag::Interjection),
];

/// The features a letter of the file stands for.
const FEATURES: [(char, Features); 10] = [
    ('S', Features::SINGULAR),
    ('P', Features::PLURAL),
    ('I', Features::INFINITIVE),
    ('D', Features::FINITE.union(Features::PAST)),
    ('E', Features::PAST_PARTICIPLE),
    ('G', Features::PRESENT_PARTICIPLE),
    (
        'Z',
        Features::FINITE
            .union(Features::PRESENT)
            .union(Features::SINGULAR)
            .union(Features::THIRD),
    ),
    ('O', Features::POSITIVE),
    ('C', Features::COMPARATIVE),
    ('T', Features::SUPERLATIVE),
];

/// The most bytes of a word the file holds, which the generator enforces.
pub const LONGEST: usize = 24;

/// The lines of the file after its header.
fn body() -> &'static str {
    static BODY: OnceLock<&'static str> = OnceLock::new();
    BODY.get_or_init(|| {
        let mut rest = TEXT;
        while rest.starts_with('#') {
            rest = rest.split_once('\n').map_or("", |(_, after)| after);
        }
        rest
    })
}

/// The FNV-1a hash of `word`, which takes a few steps for a short word. The table is fixed and its
/// keys are ours, so nothing here needs a hash that resists a chosen input.
fn hash(word: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in word.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash as usize
}

/// A hash table of the body's lines, built on first use: open addressing with linear probing, each
/// slot the offset of a line plus one, or zero when empty. The lines stay where they are, so the
/// table is four bytes a slot, and a word is found without a copy or an allocation.
fn table() -> &'static [u32] {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let text = body();
        // A line is about 12 bytes, so this is half as many slots again as words at the least,
        // rounded up to a power of two so that a mask picks the slot.
        let mut slots = vec![0u32; (text.len() / 8).next_power_of_two()];
        let mask = slots.len() - 1;
        let mut start = 0;
        for line in text.split_inclusive('\n') {
            let key = line.split('\t').next().unwrap_or(line);
            let mut at = hash(key) & mask;
            while slots[at] != 0 {
                at = (at + 1) & mask;
            }
            slots[at] = start as u32 + 1;
            start += line.len();
        }
        slots
    })
}

/// The readings field of the line for `word`, which must already be folded.
fn find(word: &str) -> Option<&'static str> {
    let text = body();
    let slots = table();
    let mask = slots.len() - 1;
    let mut at = hash(word) & mask;
    while slots[at] != 0 {
        let line = &text[slots[at] as usize - 1..];
        if let Some(rest) = line
            .strip_prefix(word)
            .and_then(|rest| rest.strip_prefix('\t'))
        {
            return Some(rest.split('\n').next().unwrap_or(rest));
        }
        at = (at + 1) & mask;
    }
    None
}

/// Every word of the file, in order, with its reading and whether it is marked dominant: see
/// `dominant`. A line that is not well formed is left out, as [`lookup`] reads it as no word.
pub(super) fn entries() -> impl Iterator<Item = (&'static str, Reading, bool)> {
    body().lines().filter_map(|line| {
        let (word, readings) = line.split_once('\t')?;
        Some((word, parse(readings)?, readings.ends_with('!')))
    })
}

/// Whether the lexicon's SemCor counts back the best guess of `word`, which must already be
/// folded: it has nine tenths of the word's counts, and the word has at least five (in units of an
/// adjective's). Only the generator says so, with a `!` after the readings. The tagger reads the
/// mark from [`super::table`], which holds it beside each reading.
#[cfg(test)]
pub fn dominant(word: &str) -> bool {
    find(word).is_some_and(|readings| readings.ends_with('!'))
}

/// The tag a letter stands for.
fn tag_of(letter: char) -> Option<Tag> {
    TAGS.iter()
        .find(|(code, _)| *code == letter)
        .map(|(_, tag)| *tag)
}

/// The features a letter stands for.
fn features_of(letter: char) -> Option<Features> {
    FEATURES
        .iter()
        .find(|(code, _)| *code == letter)
        .map(|(_, features)| *features)
}

/// The reading a `readings` field gives: its first tag is the best guess, its uppercase letter the
/// features of that guess, and every tag is kept; a last `!` is the marker of `dominant` and is
/// not read here. `None` when the field is not well formed.
fn parse(readings: &str) -> Option<Reading> {
    let readings = readings.strip_suffix('!').unwrap_or(readings);
    let mut best = None;
    let mut features = Features::NONE;
    let mut kept = TagSet::EMPTY;
    for letter in readings.chars() {
        if letter.is_ascii_uppercase() {
            // Only the best guess has features, and they follow it.
            if best.is_none() || features != Features::NONE {
                return None;
            }
            features = features_of(letter)?;
        } else {
            let tag = tag_of(letter)?;
            best.get_or_insert(tag);
            kept = kept.with(tag);
        }
    }
    Some(Reading {
        tag: best?,
        features,
        confidence: Confidence::Unsure,
        kept,
    })
}

/// What the lexicon says of `word`, folded as [`super::fold`] does: its readings if it has the
/// word, else the reading of the noun `x` has when the word is `x's`.
pub fn lookup(word: &str) -> Option<Reading> {
    match find(word) {
        Some(readings) => parse(readings),
        None => possessive(word),
    }
}

/// The reading of `x's` from the lexicon's `x`: its best noun or proper noun, `Unsure`, with the
/// stem's number and the second word fused on. `None` when `x` is not a noun there.
fn possessive(word: &str) -> Option<Reading> {
    let stem = word.strip_suffix("'s").filter(|stem| !stem.is_empty())?;
    let readings = find(stem)?;
    let stem_reading = parse(readings)?;
    let noun = [Tag::Noun, Tag::ProperNoun]
        .into_iter()
        .filter(|tag| stem_reading.kept.contains(*tag))
        .collect::<TagSet>();
    // The first of the two in the stem's rank order.
    let tag = readings
        .chars()
        .filter_map(tag_of)
        .find(|tag| noun.contains(*tag))?;
    let number = if stem_reading.tag == Tag::Noun && stem_reading.features == Features::PLURAL {
        Features::PLURAL
    } else {
        Features::SINGULAR
    };
    Some(Reading {
        tag,
        features: number.union(Features::CONTRACTION),
        confidence: Confidence::Unsure,
        kept: noun,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// The lines of the body, split into word and readings.
    fn entries() -> Vec<(&'static str, &'static str)> {
        body()
            .lines()
            .map(|line| line.split_once('\t').expect("a tab in every line"))
            .collect()
    }

    fn word(text: &str) -> Reading {
        lookup(text).unwrap_or_else(|| panic!("{text} is not in the lexicon"))
    }

    #[test]
    fn the_header_is_comment_lines_and_the_body_has_none() {
        assert!(TEXT.starts_with('#'));
        assert!(body().lines().all(|line| !line.starts_with('#')));
        assert!(!body().is_empty());
        assert!(TEXT.ends_with('\n'));
    }

    #[test]
    fn the_header_counts_the_words() {
        let counted: usize = TEXT
            .lines()
            .find_map(|line| line.strip_prefix("# words "))
            .expect("a words line in the header")
            .parse()
            .unwrap();
        assert_eq!(counted, body().lines().count());
    }

    #[test]
    fn the_header_names_the_size_and_every_source() {
        let size: u32 = TEXT
            .lines()
            .find_map(|line| line.strip_prefix("# size "))
            .expect("a size line in the header")
            .parse()
            .unwrap();
        assert!([10, 20, 35, 40, 50, 55, 60, 70, 80, 95].contains(&size));
        let sources = TEXT
            .lines()
            .filter(|line| line.starts_with("# source "))
            .count();
        assert_eq!(sources, 5);
    }

    #[test]
    fn the_file_is_ascii_and_under_the_limit_for_text_in_the_tree() {
        assert!(TEXT.is_ascii());
        assert!(TEXT.len() < 1_000_000, "{} bytes", TEXT.len());
    }

    #[test]
    fn the_words_are_sorted_bytewise_without_a_repeat() {
        let words: Vec<&str> = entries().into_iter().map(|(word, _)| word).collect();
        for pair in words.windows(2) {
            assert!(pair[0] < pair[1], "{} is not before {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn every_word_is_lower_case_letters_and_apostrophes_the_loader_can_fold() {
        for (word, _) in entries() {
            assert!(word.len() <= LONGEST, "{word} is too long");
            assert!(
                word.bytes().all(|b| b.is_ascii_lowercase() || b == b'\''),
                "{word}"
            );
            assert!(!word.starts_with('\'') && !word.ends_with('\''), "{word}");
            assert!(!word.ends_with("'s"), "{word} is a possessive");
        }
    }

    #[test]
    fn every_line_is_well_formed_and_every_word_found_in_the_table() {
        for (word, readings) in entries() {
            let reading = parse(readings).unwrap_or_else(|| panic!("{word}: {readings}"));
            assert_eq!(reading.confidence, Confidence::Unsure, "{word}");
            assert!(reading.kept.contains(reading.tag), "{word}");
            let letters = readings.chars().filter(char::is_ascii_lowercase).count();
            assert_eq!(reading.kept.len(), letters, "{word} repeats a tag");
            assert_eq!(find(word), Some(readings), "{word}");
        }
    }

    #[test]
    fn the_features_fit_the_best_guess() {
        let verb_form = Features::FINITE
            .union(Features::INFINITIVE)
            .union(Features::PAST_PARTICIPLE)
            .union(Features::PRESENT_PARTICIPLE);
        let degree = Features::POSITIVE
            .union(Features::COMPARATIVE)
            .union(Features::SUPERLATIVE);
        let number = Features::SINGULAR.union(Features::PLURAL);
        let has = |features: Features, group: Features| {
            Features::ALL
                .into_iter()
                .any(|f| group.contains(f) && features.contains(f))
        };
        for (word, readings) in entries() {
            let Reading { tag, features, .. } = parse(readings).unwrap();
            assert_eq!(
                has(features, verb_form),
                tag == Tag::Verb && features != Features::NONE,
                "{word}"
            );
            assert!(
                !has(features, degree) || matches!(tag, Tag::Adjective | Tag::Adverb),
                "{word}"
            );
            assert!(
                !has(features, number) || matches!(tag, Tag::Noun | Tag::ProperNoun | Tag::Verb),
                "{word}"
            );
            assert!(!features.contains(Features::CONTRACTION), "{word}");
        }
    }

    #[test]
    fn a_word_is_unsure_with_its_tags_ranked_and_kept() {
        let run = word("run");
        assert_eq!(run.tag, Tag::Verb);
        assert_eq!(run.features, Features::INFINITIVE);
        assert_eq!(run.confidence, Confidence::Unsure);
        assert_eq!(run.kept, TagSet::of(Tag::Verb).with(Tag::Noun));
        let good = word("good");
        assert_eq!(good.tag, Tag::Adjective);
        assert_eq!(good.features, Features::POSITIVE);
        assert!(good.kept.contains(Tag::Noun) && good.kept.contains(Tag::Adverb));
        // A word with one tag is no more than Unsure.
        let aardvark = word("aardvark");
        assert_eq!(aardvark.kept, TagSet::of(Tag::Noun));
        assert_eq!(aardvark.confidence, Confidence::Unsure);
    }

    #[test]
    fn readings_are_ranked_by_the_semcor_counts_of_each_part_of_speech() {
        // Each of these has two or more parts of speech, and the counts put one clearly first.
        for (text, tag) in [
            ("project", Tag::Noun),
            ("name", Tag::Noun),
            ("process", Tag::Noun),
            ("use", Tag::Verb),
            ("file", Tag::Verb),
            ("free", Tag::Adjective),
            ("still", Tag::Adverb),
        ] {
            let reading = word(text);
            assert_eq!(reading.tag, tag, "{text}");
            assert!(
                reading.kept.len() > 1,
                "{text} has one tag, so ranks nothing"
            );
        }
        // A form takes the rank of its lemma's part of speech: `saw` is the past of `see`.
        assert_eq!(word("saw").tag, Tag::Verb);
        assert_eq!(word("uses").tag, Tag::Verb);
        // A name has no count of its own and ranks after the common readings.
        assert_eq!(word("march").kept.iter().next(), Some(Tag::Noun));
    }

    #[test]
    fn a_marked_word_is_backed_by_its_counts_and_reads_as_unmarked() {
        let mut marked = 0;
        for (word, readings) in entries() {
            if !readings.ends_with('!') {
                continue;
            }
            marked += 1;
            assert!(dominant(word), "{word}");
            // The marker is not part of the reading, and only a noun, verb, adjective or adverb
            // that has no function word among its tags carries it.
            let reading = parse(readings).unwrap();
            assert_eq!(
                Some(reading),
                parse(readings.trim_end_matches('!')),
                "{word}"
            );
            let allowed = [
                Tag::Noun,
                Tag::Verb,
                Tag::Adjective,
                Tag::Adverb,
                Tag::ProperNoun,
            ]
            .into_iter()
            .collect::<TagSet>();
            assert_eq!(reading.kept.intersection(allowed), reading.kept, "{word}");
            assert!(
                allowed.contains(reading.tag) && reading.tag != Tag::ProperNoun,
                "{word}"
            );
        }
        assert!(marked > 1_000, "{marked} words are marked");
        assert!(dominant("accepted") && !dominant("work") && !dominant("frobnicator"));
    }

    #[test]
    fn inflected_forms_carry_their_features() {
        let runs = word("runs");
        assert_eq!(runs.tag, Tag::Verb);
        assert!(runs.features.contains(Features::THIRD));
        assert!(runs.features.contains(Features::SINGULAR));
        assert!(runs.features.contains(Features::PRESENT));
        assert!(runs.features.contains(Features::FINITE));
        let walked = word("walked");
        assert_eq!(walked.tag, Tag::Verb);
        assert_eq!(walked.features, Features::FINITE.union(Features::PAST));
        let walking = word("walking");
        assert!(walking.features.contains(Features::PRESENT_PARTICIPLE));
        assert_eq!(word("children").tag, Tag::Noun);
        assert_eq!(word("children").features, Features::PLURAL);
        assert_eq!(word("aardvarks").features, Features::PLURAL);
        assert_eq!(word("faster").features, Features::COMPARATIVE);
        assert_eq!(word("fastest").features, Features::SUPERLATIVE);
        assert_eq!(word("quickly").tag, Tag::Adverb);
        // A past participle that differs from the past tense is its own reading.
        assert!(word("taken").features.contains(Features::PAST_PARTICIPLE));
        assert!(word("took").features.contains(Features::PAST));
    }

    #[test]
    fn a_name_is_a_proper_noun() {
        let paris = word("paris");
        assert_eq!(paris.tag, Tag::ProperNoun);
        assert_eq!(paris.features, Features::SINGULAR);
        // A common word that is also a name keeps the name, ranked after the rest.
        let march = word("march");
        assert!(march.kept.contains(Tag::ProperNoun));
        assert_ne!(march.tag, Tag::ProperNoun);
    }

    #[test]
    fn a_word_it_lacks_is_not_found() {
        for text in ["", "frobnicator", "zzzz", "runz", "ru", "runss"] {
            assert_eq!(lookup(text), None, "{text:?}");
        }
        // Past the first and last lines.
        assert_eq!(find("\u{0}"), None);
        assert_eq!(find("zzzzzzzz"), None);
    }

    #[test]
    fn a_possessive_is_read_from_its_stem() {
        let user = lookup("user's").expect("user is a noun");
        assert_eq!(user.tag, Tag::Noun);
        assert_eq!(
            user.features,
            Features::SINGULAR.union(Features::CONTRACTION)
        );
        assert_eq!(user.confidence, Confidence::Unsure);
        assert_eq!(user.kept, TagSet::of(Tag::Noun));
        let paris = lookup("paris's").unwrap();
        assert_eq!(paris.tag, Tag::ProperNoun);
        // A plural stem gives a plural possessive.
        let women = lookup("women's").unwrap();
        assert_eq!(
            women.features,
            Features::PLURAL.union(Features::CONTRACTION)
        );
        // A stem that is no noun, or not there, gives nothing.
        assert_eq!(lookup("quickly's"), None);
        assert_eq!(lookup("frobnicator's"), None);
        assert_eq!(lookup("'s"), None);
    }

    #[test]
    fn a_possessive_keeps_only_the_noun_tags_of_its_stem() {
        let march = lookup("march's").unwrap();
        assert!(march.kept.contains(Tag::Noun) || march.kept.contains(Tag::ProperNoun));
        assert!(!march.kept.contains(Tag::Verb));
        assert!(matches!(march.tag, Tag::Noun | Tag::ProperNoun));
    }

    #[test]
    fn no_two_letters_of_a_table_repeat() {
        let tags: HashSet<char> = TAGS.iter().map(|(c, _)| *c).collect();
        assert_eq!(tags.len(), TAGS.len());
        let tag_set: TagSet = TAGS.iter().map(|(_, t)| *t).collect();
        assert_eq!(tag_set.len(), Tag::ALL.len());
        let features: HashSet<char> = FEATURES.iter().map(|(c, _)| *c).collect();
        assert_eq!(features.len(), FEATURES.len());
        assert!(TAGS.iter().all(|(c, _)| c.is_ascii_lowercase()));
        assert!(FEATURES.iter().all(|(c, _)| c.is_ascii_uppercase()));
    }
}
