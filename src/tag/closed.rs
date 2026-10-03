//! The closed-class table: the few hundred function words whose tags the language fixes, written by
//! hand from English grammar and the annotation guide.
//!
//! Nothing here is counted from a corpus, and nothing in it is derived from a treebank. A word is
//! listed with every tag it plausibly has in technical English, most common first, so the first is
//! the best guess. A tag is listed when an annotator would meet it regularly, not for every sense a
//! dictionary gives: `may` is only an auxiliary, `will` is an auxiliary, a noun or a verb.
//!
//! Features are those of the best guess alone. A contraction is one token, read as its first part
//! with [`Features::CONTRACTION`], so `don't` is an auxiliary and `it's` a pronoun.
//!
//! - **Confidence.** A word with one tag is `Sure`. A word with several is `Unsure`: nothing here
//!   reads the context. A word not in the table is `Unknown`, a noun, except that `'s` after a
//!   word marks a possessive noun that a second word is fused on to.
//! - **Folding.** The word is matched in lower case with a curly apostrophe read straight, and by
//!   nothing else.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::{Confidence, Features, Reading, Tag, TagSet};
use Tag::{
    Adjective, Adposition, Adverb, Auxiliary, Conjunction, Determiner, Interjection, Noun, Numeral,
    Particle, Pronoun, ProperNoun, Verb,
};

/// One word of the table.
struct Entry {
    /// The word, in lower case with a straight apostrophe.
    word: &'static str,
    /// Every tag it may have, the best guess first.
    tags: &'static [Tag],
    /// The features of the best guess.
    features: Features,
}

const fn e(word: &'static str, tags: &'static [Tag], features: Features) -> Entry {
    Entry {
        word,
        tags,
        features,
    }
}

/// The flags of a table row, short because the table is long.
const NO: Features = Features::NONE;
const SG: Features = Features::SINGULAR;
const PL: Features = Features::PLURAL;
const P2: Features = Features::SECOND;
const SG1: Features = SG.union(Features::FIRST);
const PL1: Features = PL.union(Features::FIRST);
const SG3: Features = SG.union(Features::THIRD);
const PL3: Features = PL.union(Features::THIRD);
const FIN: Features = Features::FINITE;
const FIN_PRES: Features = FIN.union(Features::PRESENT);
const FIN_PAST: Features = FIN.union(Features::PAST);
const SG3_PRES: Features = FIN_PRES.union(SG3);
const INF: Features = Features::INFINITIVE;
const PP: Features = Features::PAST_PARTICIPLE;
const ING: Features = Features::PRESENT_PARTICIPLE;

/// `features` with a second word fused on after the first.
const fn con(features: Features) -> Features {
    features.union(Features::CONTRACTION)
}

/// The table, by class. Order within a class is for reading only.
#[rustfmt::skip]
const ENTRIES: &[Entry] = &[
    // Determiners. A possessive such as `my` is a pronoun, as the annotation guide tags it.
    e("a", &[Determiner], NO),
    e("an", &[Determiner], NO),
    e("the", &[Determiner], NO),
    e("every", &[Determiner], NO),
    e("no", &[Determiner, Interjection, Adverb], NO),
    e("this", &[Determiner, Pronoun], SG),
    e("that", &[Conjunction, Pronoun, Determiner], NO),
    e("these", &[Determiner, Pronoun], PL),
    e("those", &[Determiner, Pronoun], PL),
    e("each", &[Determiner, Pronoun, Adverb], NO),
    e("all", &[Determiner, Pronoun, Adverb], NO),
    e("both", &[Determiner, Conjunction, Pronoun], NO),
    e("either", &[Determiner, Conjunction, Pronoun, Adverb], NO),
    e("neither", &[Determiner, Conjunction, Pronoun], NO),
    e("some", &[Determiner, Pronoun, Adverb], NO),
    e("any", &[Determiner, Pronoun, Adverb], NO),
    e("another", &[Determiner, Pronoun], NO),
    e("half", &[Determiner, Noun, Pronoun], NO),
    e("which", &[Pronoun, Determiner], NO),
    e("what", &[Pronoun, Determiner], NO),
    e("whose", &[Pronoun, Determiner], NO),
    e("whatever", &[Determiner, Pronoun], NO),
    e("whichever", &[Determiner, Pronoun], NO),

    // Quantifiers, which the guide tags as adjectives, or as adverbs before an adjective.
    e("many", &[Adjective, Pronoun], NO),
    e("few", &[Adjective, Pronoun], NO),
    e("several", &[Adjective, Pronoun], NO),
    e("much", &[Adjective, Adverb, Pronoun], NO),
    e("more", &[Adverb, Adjective, Pronoun], NO),
    e("most", &[Adjective, Adverb, Pronoun], NO),
    e("less", &[Adverb, Adjective, Pronoun], NO),
    e("least", &[Adverb, Adjective, Pronoun], NO),
    e("fewer", &[Adjective, Pronoun], NO),
    e("enough", &[Adjective, Adverb, Pronoun], NO),
    e("other", &[Adjective, Pronoun, Noun], NO),
    e("others", &[Pronoun, Noun], NO),
    e("same", &[Adjective, Pronoun], NO),
    e("own", &[Adjective, Verb, Pronoun], NO),
    e("such", &[Adjective, Determiner, Pronoun], NO),
    e("little", &[Adjective, Adverb, Noun, Pronoun], NO),

    // Personal pronouns, possessives and reflexives. `you` has no number.
    e("i", &[Pronoun], SG1),
    e("me", &[Pronoun], SG1),
    e("my", &[Pronoun], SG1),
    e("mine", &[Pronoun, Noun], SG1),
    e("myself", &[Pronoun], SG1),
    e("we", &[Pronoun], PL1),
    e("us", &[Pronoun, ProperNoun], PL1),
    e("our", &[Pronoun], PL1),
    e("ours", &[Pronoun], PL1),
    e("ourselves", &[Pronoun], PL1),
    e("you", &[Pronoun], P2),
    e("your", &[Pronoun], P2),
    e("yours", &[Pronoun], P2),
    e("yourself", &[Pronoun], SG.union(P2)),
    e("yourselves", &[Pronoun], PL.union(P2)),
    e("he", &[Pronoun], SG3),
    e("him", &[Pronoun], SG3),
    e("his", &[Pronoun], SG3),
    e("himself", &[Pronoun], SG3),
    e("she", &[Pronoun], SG3),
    e("her", &[Pronoun], SG3),
    e("hers", &[Pronoun], SG3),
    e("herself", &[Pronoun], SG3),
    e("it", &[Pronoun], SG3),
    e("its", &[Pronoun], SG3),
    e("itself", &[Pronoun], SG3),
    e("they", &[Pronoun], PL3),
    e("them", &[Pronoun], PL3),
    e("their", &[Pronoun], PL3),
    e("theirs", &[Pronoun], PL3),
    e("themselves", &[Pronoun], PL3),

    // Other pronouns. `there` is the expletive of `there is`, else an adverb of place.
    e("who", &[Pronoun], NO),
    e("whom", &[Pronoun], NO),
    e("whoever", &[Pronoun], NO),
    e("whomever", &[Pronoun], NO),
    e("something", &[Pronoun], SG),
    e("anything", &[Pronoun], SG),
    e("nothing", &[Pronoun], SG),
    e("everything", &[Pronoun], SG),
    e("someone", &[Pronoun], SG),
    e("anyone", &[Pronoun], SG),
    e("everyone", &[Pronoun], SG),
    e("somebody", &[Pronoun], SG),
    e("anybody", &[Pronoun], SG),
    e("everybody", &[Pronoun], SG),
    e("nobody", &[Pronoun], SG),
    e("none", &[Pronoun], NO),
    e("there", &[Pronoun, Adverb], NO),

    // Contractions of a pronoun, read as the pronoun.
    e("i'm", &[Pronoun], con(SG1)),
    e("i've", &[Pronoun], con(SG1)),
    e("i'll", &[Pronoun], con(SG1)),
    e("i'd", &[Pronoun], con(SG1)),
    e("you're", &[Pronoun], con(P2)),
    e("you've", &[Pronoun], con(P2)),
    e("you'll", &[Pronoun], con(P2)),
    e("you'd", &[Pronoun], con(P2)),
    e("y'all", &[Pronoun], con(P2)),
    e("he's", &[Pronoun], con(SG3)),
    e("he'll", &[Pronoun], con(SG3)),
    e("he'd", &[Pronoun], con(SG3)),
    e("she's", &[Pronoun], con(SG3)),
    e("she'll", &[Pronoun], con(SG3)),
    e("she'd", &[Pronoun], con(SG3)),
    e("it's", &[Pronoun], con(SG3)),
    e("it'll", &[Pronoun], con(SG3)),
    e("it'd", &[Pronoun], con(SG3)),
    e("we're", &[Pronoun], con(PL1)),
    e("we've", &[Pronoun], con(PL1)),
    e("we'll", &[Pronoun], con(PL1)),
    e("we'd", &[Pronoun], con(PL1)),
    e("they're", &[Pronoun], con(PL3)),
    e("they've", &[Pronoun], con(PL3)),
    e("they'll", &[Pronoun], con(PL3)),
    e("they'd", &[Pronoun], con(PL3)),
    e("that's", &[Pronoun], con(SG)),
    e("that'll", &[Pronoun], con(SG)),
    e("that'd", &[Pronoun], con(SG)),
    e("there's", &[Pronoun, Adverb], con(NO)),
    e("there'll", &[Pronoun, Adverb], con(NO)),
    e("there'd", &[Pronoun, Adverb], con(NO)),
    e("who's", &[Pronoun], con(NO)),
    e("who'll", &[Pronoun], con(NO)),
    e("who'd", &[Pronoun], con(NO)),
    e("who're", &[Pronoun], con(NO)),
    e("who've", &[Pronoun], con(NO)),
    e("what's", &[Pronoun], con(NO)),
    e("what'll", &[Pronoun], con(NO)),
    e("what're", &[Pronoun], con(NO)),
    e("what'd", &[Pronoun], con(NO)),
    e("something's", &[Pronoun], con(SG)),
    e("anything's", &[Pronoun], con(SG)),
    e("nothing's", &[Pronoun], con(SG)),
    e("everything's", &[Pronoun], con(SG)),
    e("someone's", &[Pronoun], con(SG)),
    e("anyone's", &[Pronoun], con(SG)),
    e("everyone's", &[Pronoun], con(SG)),
    e("somebody's", &[Pronoun], con(SG)),
    e("everybody's", &[Pronoun], con(SG)),
    e("nobody's", &[Pronoun], con(SG)),
    e("one's", &[Pronoun], con(SG)),
    // `let's` is the verb `let` with `us` fused on.
    e("let's", &[Verb], con(FIN)),
    // The adverbs `here`, `where`, `how`, `when` and `why` with `is` fused on.
    e("here's", &[Adverb], con(NO)),
    e("where's", &[Adverb], con(NO)),
    e("where'd", &[Adverb], con(NO)),
    e("how's", &[Adverb], con(NO)),
    e("how'd", &[Adverb], con(NO)),
    e("when's", &[Adverb], con(NO)),
    e("why's", &[Adverb], con(NO)),
    e("why'd", &[Adverb], con(NO)),

    // Prepositions. A phrasal-verb particle such as `up` in `set up` is one too, in the guide.
    e("about", &[Adposition, Adverb], NO),
    e("above", &[Adposition, Adverb, Adjective], NO),
    e("across", &[Adposition, Adverb], NO),
    e("after", &[Adposition, Conjunction, Adverb], NO),
    e("against", &[Adposition], NO),
    e("along", &[Adposition, Adverb], NO),
    e("alongside", &[Adposition, Adverb], NO),
    e("amid", &[Adposition], NO),
    e("amidst", &[Adposition], NO),
    e("among", &[Adposition], NO),
    e("amongst", &[Adposition], NO),
    e("around", &[Adposition, Adverb], NO),
    e("as", &[Adposition, Conjunction, Adverb], NO),
    e("at", &[Adposition], NO),
    e("atop", &[Adposition], NO),
    e("aboard", &[Adposition, Adverb], NO),
    e("before", &[Adposition, Conjunction, Adverb], NO),
    e("behind", &[Adposition, Adverb], NO),
    e("below", &[Adposition, Adverb], NO),
    e("beneath", &[Adposition, Adverb], NO),
    e("beside", &[Adposition], NO),
    e("besides", &[Adposition, Adverb], NO),
    e("between", &[Adposition, Adverb], NO),
    e("beyond", &[Adposition, Adverb], NO),
    e("by", &[Adposition, Adverb], NO),
    e("despite", &[Adposition], NO),
    e("down", &[Adposition, Adverb, Adjective, Verb], NO),
    e("during", &[Adposition], NO),
    e("except", &[Adposition, Conjunction, Verb], NO),
    e("for", &[Adposition, Conjunction], NO),
    e("from", &[Adposition], NO),
    e("in", &[Adposition, Adverb, Adjective], NO),
    e("inside", &[Adposition, Adverb, Noun, Adjective], NO),
    e("into", &[Adposition], NO),
    e("like", &[Adposition, Verb, Adjective, Noun], NO),
    e("minus", &[Adposition, Noun], NO),
    e("near", &[Adposition, Adjective, Adverb, Verb], NO),
    e("notwithstanding", &[Adposition, Adverb], NO),
    e("of", &[Adposition], NO),
    e("off", &[Adposition, Adverb, Adjective], NO),
    e("on", &[Adposition, Adverb, Adjective], NO),
    e("onto", &[Adposition], NO),
    e("out", &[Adposition, Adverb, Adjective], NO),
    e("outside", &[Adposition, Adverb, Noun, Adjective], NO),
    e("over", &[Adposition, Adverb, Adjective], NO),
    e("past", &[Adposition, Adjective, Noun, Adverb], NO),
    e("per", &[Adposition], NO),
    e("through", &[Adposition, Adverb], NO),
    e("throughout", &[Adposition, Adverb], NO),
    e("toward", &[Adposition], NO),
    e("towards", &[Adposition], NO),
    e("under", &[Adposition, Adverb], NO),
    e("underneath", &[Adposition, Adverb], NO),
    e("unlike", &[Adposition, Adjective], NO),
    e("unto", &[Adposition], NO),
    e("up", &[Adposition, Adverb, Adjective], NO),
    e("upon", &[Adposition], NO),
    e("versus", &[Adposition], NO),
    e("vs", &[Adposition], NO),
    e("via", &[Adposition], NO),
    e("with", &[Adposition], NO),
    e("within", &[Adposition, Adverb], NO),
    e("without", &[Adposition, Adverb], NO),

    // Conjunctions, coordinating and subordinating, and the wh-adverbs that join clauses.
    e("and", &[Conjunction], NO),
    e("or", &[Conjunction], NO),
    e("but", &[Conjunction, Adposition, Adverb], NO),
    e("nor", &[Conjunction], NO),
    e("yet", &[Adverb, Conjunction], NO),
    e("so", &[Adverb, Conjunction], NO),
    e("plus", &[Conjunction, Adposition, Noun], NO),
    e("if", &[Conjunction], NO),
    e("whether", &[Conjunction], NO),
    e("because", &[Conjunction], NO),
    e("although", &[Conjunction], NO),
    e("though", &[Conjunction, Adverb], NO),
    e("while", &[Conjunction, Noun], NO),
    e("whilst", &[Conjunction], NO),
    e("unless", &[Conjunction], NO),
    e("until", &[Conjunction, Adposition], NO),
    e("till", &[Conjunction, Adposition], NO),
    e("since", &[Conjunction, Adposition, Adverb], NO),
    e("than", &[Adposition, Conjunction], NO),
    e("once", &[Conjunction, Adverb], NO),
    e("whereas", &[Conjunction], NO),
    e("whenever", &[Conjunction, Adverb], NO),
    e("wherever", &[Conjunction, Adverb], NO),
    e("lest", &[Conjunction], NO),
    e("albeit", &[Conjunction], NO),
    e("when", &[Adverb, Conjunction], NO),
    e("where", &[Adverb, Conjunction], NO),
    e("why", &[Adverb], NO),
    e("how", &[Adverb, Conjunction], NO),
    e("whereby", &[Adverb, Conjunction], NO),
    e("wherein", &[Adverb, Conjunction], NO),

    // `be`, `have` and `do`. Each can also be a main verb, and `be` is a verb in `there is`.
    e("be", &[Auxiliary, Verb], INF),
    e("am", &[Auxiliary], SG1.union(FIN_PRES)),
    e("is", &[Auxiliary, Verb], SG3_PRES),
    e("are", &[Auxiliary, Verb], FIN_PRES),
    e("was", &[Auxiliary, Verb], SG.union(FIN_PAST)),
    e("were", &[Auxiliary, Verb], FIN_PAST),
    e("been", &[Auxiliary, Verb], PP),
    e("being", &[Auxiliary, Verb, Noun], ING),
    e("have", &[Auxiliary, Verb], FIN_PRES),
    e("has", &[Auxiliary, Verb], SG3_PRES),
    e("had", &[Auxiliary, Verb], FIN_PAST),
    e("having", &[Auxiliary, Verb], ING),
    e("do", &[Auxiliary, Verb], FIN_PRES),
    e("does", &[Auxiliary, Verb], SG3_PRES),
    e("did", &[Auxiliary, Verb], FIN_PAST),
    e("doing", &[Verb], ING),
    e("done", &[Verb, Adjective], PP),

    // Modals.
    e("can", &[Auxiliary, Verb, Noun], FIN),
    e("could", &[Auxiliary], FIN),
    e("will", &[Auxiliary, Noun, Verb], FIN),
    e("would", &[Auxiliary], FIN),
    e("shall", &[Auxiliary], FIN),
    e("should", &[Auxiliary], FIN),
    e("may", &[Auxiliary], FIN),
    e("might", &[Auxiliary], FIN),
    e("must", &[Auxiliary], FIN),
    e("ought", &[Auxiliary], FIN),
    e("need", &[Verb, Auxiliary, Noun], FIN_PRES),
    e("dare", &[Verb, Auxiliary], FIN_PRES),

    // Auxiliaries and modals with `not` fused on, and `cannot`.
    e("don't", &[Auxiliary], con(FIN_PRES)),
    e("doesn't", &[Auxiliary], con(SG3_PRES)),
    e("didn't", &[Auxiliary], con(FIN_PAST)),
    e("isn't", &[Auxiliary, Verb], con(SG3_PRES)),
    e("aren't", &[Auxiliary, Verb], con(FIN_PRES)),
    e("ain't", &[Auxiliary], con(FIN_PRES)),
    e("wasn't", &[Auxiliary, Verb], con(SG.union(FIN_PAST))),
    e("weren't", &[Auxiliary, Verb], con(FIN_PAST)),
    e("hasn't", &[Auxiliary], con(SG3_PRES)),
    e("haven't", &[Auxiliary], con(FIN_PRES)),
    e("hadn't", &[Auxiliary], con(FIN_PAST)),
    e("can't", &[Auxiliary], con(FIN)),
    e("cannot", &[Auxiliary], con(FIN)),
    e("couldn't", &[Auxiliary], con(FIN)),
    e("won't", &[Auxiliary], con(FIN)),
    e("wouldn't", &[Auxiliary], con(FIN)),
    e("shan't", &[Auxiliary], con(FIN)),
    e("shouldn't", &[Auxiliary], con(FIN)),
    e("mightn't", &[Auxiliary], con(FIN)),
    e("mustn't", &[Auxiliary], con(FIN)),
    e("oughtn't", &[Auxiliary], con(FIN)),
    e("needn't", &[Auxiliary], con(FIN)),
    e("daren't", &[Auxiliary], con(FIN)),
    // Modals with `have` fused on.
    e("could've", &[Auxiliary], con(FIN)),
    e("would've", &[Auxiliary], con(FIN)),
    e("should've", &[Auxiliary], con(FIN)),
    e("might've", &[Auxiliary], con(FIN)),
    e("must've", &[Auxiliary], con(FIN)),

    // The particles of the guide: `not`, and `to` before a verb.
    e("not", &[Particle], NO),
    e("to", &[Particle, Adposition], NO),

    // Cardinal numerals written as words. Ordinals are adjectives and are not here.
    e("zero", &[Numeral, Noun], NO),
    e("one", &[Numeral, Pronoun, Noun], NO),
    e("two", &[Numeral], NO),
    e("three", &[Numeral], NO),
    e("four", &[Numeral], NO),
    e("five", &[Numeral], NO),
    e("six", &[Numeral], NO),
    e("seven", &[Numeral], NO),
    e("eight", &[Numeral], NO),
    e("nine", &[Numeral], NO),
    e("ten", &[Numeral], NO),
    e("eleven", &[Numeral], NO),
    e("twelve", &[Numeral], NO),
    e("twenty", &[Numeral], NO),
    e("thirty", &[Numeral], NO),
    e("forty", &[Numeral], NO),
    e("fifty", &[Numeral], NO),
    e("sixty", &[Numeral], NO),
    e("seventy", &[Numeral], NO),
    e("eighty", &[Numeral], NO),
    e("ninety", &[Numeral], NO),
    e("hundred", &[Numeral, Noun], NO),
    e("thousand", &[Numeral, Noun], NO),
    e("million", &[Numeral, Noun], NO),
    e("billion", &[Numeral, Noun], NO),

    // Interjections.
    e("yes", &[Interjection], NO),
    e("yeah", &[Interjection], NO),
    e("yep", &[Interjection], NO),
    e("yup", &[Interjection], NO),
    e("nope", &[Interjection], NO),
    e("nah", &[Interjection], NO),
    e("ok", &[Interjection, Adjective], NO),
    e("okay", &[Interjection, Adjective, Adverb], NO),
    e("please", &[Interjection, Verb], NO),
    e("thanks", &[Interjection, Noun, Verb], NO),
    e("sorry", &[Adjective, Interjection], NO),
    e("hello", &[Interjection, Noun], NO),
    e("hi", &[Interjection], NO),
    e("hey", &[Interjection], NO),
    e("goodbye", &[Interjection, Noun], NO),
    e("bye", &[Interjection], NO),
    e("cheers", &[Interjection, Noun], NO),
    e("congrats", &[Interjection], NO),
    e("oh", &[Interjection], NO),
    e("ah", &[Interjection], NO),
    e("aha", &[Interjection], NO),
    e("aw", &[Interjection], NO),
    e("eh", &[Interjection], NO),
    e("er", &[Interjection], NO),
    e("uh", &[Interjection], NO),
    e("um", &[Interjection], NO),
    e("hm", &[Interjection], NO),
    e("heh", &[Interjection], NO),
    e("erm", &[Interjection], NO),
    e("aye", &[Interjection], NO),
    e("yuck", &[Interjection], NO),
    e("ahem", &[Interjection], NO),
    e("hurrah", &[Interjection], NO),
    e("hmm", &[Interjection], NO),
    e("huh", &[Interjection], NO),
    e("wow", &[Interjection], NO),
    e("whoa", &[Interjection], NO),
    e("oops", &[Interjection], NO),
    e("ouch", &[Interjection], NO),
    e("ugh", &[Interjection], NO),
    e("yay", &[Interjection], NO),
    e("hooray", &[Interjection], NO),
    e("alas", &[Interjection], NO),
    e("haha", &[Interjection], NO),
    e("lol", &[Interjection], NO),
];

/// The longest word of the table, in bytes. A longer token is not in it.
const LONGEST: usize = 16;

/// The table by word, built on first use.
fn index() -> &'static HashMap<&'static str, &'static Entry> {
    static INDEX: OnceLock<HashMap<&'static str, &'static Entry>> = OnceLock::new();
    INDEX.get_or_init(|| ENTRIES.iter().map(|entry| (entry.word, entry)).collect())
}

/// How many words the table holds.
#[cfg(test)]
fn len() -> usize {
    ENTRIES.len()
}

/// `text` folded into `buf` for a lookup: lower case, a curly apostrophe straight. `None` when it
/// is too long or is not ASCII once folded, so no entry can match it.
fn fold<'b>(text: &str, buf: &'b mut [u8; LONGEST]) -> Option<&'b str> {
    let mut used = 0;
    for ch in text.chars() {
        let ch = if ch == '\u{2019}' { '\'' } else { ch };
        if !ch.is_ascii() || used == LONGEST {
            return None;
        }
        buf[used] = ch.to_ascii_lowercase() as u8;
        used += 1;
    }
    std::str::from_utf8(&buf[..used]).ok()
}

/// What the table says of the word `text`: its entry's reading, else a noun at `Unknown`.
pub fn read(text: &str) -> Reading {
    let mut buf = [0; LONGEST];
    let found = fold(text, &mut buf).and_then(|word| index().get(word));
    match found {
        Some(entry) => Reading {
            tag: entry.tags[0],
            features: entry.features,
            confidence: if entry.tags.len() == 1 {
                Confidence::Sure
            } else {
                Confidence::Unsure
            },
            kept: entry.tags.iter().copied().collect(),
        },
        None => unknown(text),
    }
}

/// A word the table lacks: a noun at `Unknown`. A word of more than a letter that ends in `'s`
/// is a singular noun with a second word fused on, whatever its stem.
fn unknown(text: &str) -> Reading {
    let mut features = Features::NONE;
    let mut rest = text.chars().rev();
    if let (Some('s' | 'S'), Some('\'' | '\u{2019}'), Some(_)) =
        (rest.next(), rest.next(), rest.next())
    {
        features = Features::SINGULAR.union(Features::CONTRACTION);
    }
    Reading {
        tag: Tag::Noun,
        features,
        confidence: Confidence::Unknown,
        kept: TagSet::of(Tag::Noun),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn word(text: &str) -> Reading {
        read(text)
    }

    #[test]
    fn the_table_is_a_few_hundred_words() {
        assert!((300..=600).contains(&len()), "{} words", len());
    }

    #[test]
    fn every_word_is_listed_once_in_lower_case_ascii() {
        let mut seen = HashSet::new();
        for entry in ENTRIES {
            assert!(seen.insert(entry.word), "{} is listed twice", entry.word);
            assert!(
                entry
                    .word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '\''),
                "{} is not lower case ASCII with a straight apostrophe",
                entry.word
            );
            assert!(entry.word.len() <= LONGEST, "{} is too long", entry.word);
        }
    }

    #[test]
    fn every_entry_has_tags_without_a_repeat() {
        for entry in ENTRIES {
            assert!(!entry.tags.is_empty(), "{} has no tag", entry.word);
            let set: TagSet = entry.tags.iter().copied().collect();
            assert_eq!(set.len(), entry.tags.len(), "{} repeats a tag", entry.word);
        }
    }

    #[test]
    fn a_word_is_a_contraction_when_it_has_a_clitic() {
        const CLITICS: [&str; 7] = ["n't", "'s", "'re", "'ll", "'m", "'ve", "'d"];
        for entry in ENTRIES {
            let flagged = entry.features.contains(Features::CONTRACTION);
            let clitic = CLITICS.iter().any(|c| entry.word.ends_with(c))
                || entry.word == "cannot"
                || entry.word == "y'all";
            assert_eq!(flagged, clitic, "{}", entry.word);
        }
    }

    #[test]
    fn features_fit_the_tag_they_describe() {
        let verb_form = Features::FINITE
            .union(Features::INFINITIVE)
            .union(Features::PAST_PARTICIPLE)
            .union(Features::PRESENT_PARTICIPLE)
            .union(Features::PRESENT)
            .union(Features::PAST);
        for entry in ENTRIES {
            let verbal = matches!(entry.tags[0], Tag::Auxiliary | Tag::Verb);
            let has_verb_form = Features::ALL
                .into_iter()
                .any(|f| verb_form.contains(f) && entry.features.contains(f));
            assert_eq!(has_verb_form, verbal, "{}", entry.word);
            let person = Features::FIRST
                .union(Features::SECOND)
                .union(Features::THIRD);
            let has_person = Features::ALL
                .into_iter()
                .any(|f| person.contains(f) && entry.features.contains(f));
            if has_person {
                assert!(
                    matches!(entry.tags[0], Tag::Pronoun | Tag::Auxiliary),
                    "{} has a person",
                    entry.word
                );
            }
            assert!(
                !entry.features.contains(Features::SINGULAR)
                    || !entry.features.contains(Features::PLURAL),
                "{} is singular and plural",
                entry.word
            );
        }
    }

    #[test]
    fn a_word_with_one_tag_is_sure_and_keeps_only_it() {
        let the = word("the");
        assert_eq!(the.tag, Tag::Determiner);
        assert_eq!(the.confidence, Confidence::Sure);
        assert_eq!(the.kept, TagSet::of(Tag::Determiner));
        assert_eq!(the.features, Features::NONE);
        let and = word("and");
        assert_eq!(
            (and.tag, and.confidence),
            (Tag::Conjunction, Confidence::Sure)
        );
    }

    #[test]
    fn a_word_with_several_tags_is_unsure_and_keeps_all_of_them() {
        let that = word("that");
        assert_eq!(that.tag, Tag::Conjunction);
        assert_eq!(that.confidence, Confidence::Unsure);
        let all: TagSet = [Tag::Conjunction, Tag::Pronoun, Tag::Determiner]
            .into_iter()
            .collect();
        assert_eq!(that.kept, all);
        let to = word("to");
        assert_eq!(to.tag, Tag::Particle);
        assert_eq!(to.confidence, Confidence::Unsure);
        assert_eq!(to.kept, TagSet::of(Tag::Particle).with(Tag::Adposition));
        assert_eq!(word("there").tag, Tag::Pronoun);
        assert!(word("there").kept.contains(Tag::Adverb));
    }

    #[test]
    fn nothing_is_likely() {
        assert!(
            ENTRIES
                .iter()
                .all(|e| !matches!(read(e.word).confidence, Confidence::Likely))
        );
    }

    #[test]
    fn a_contraction_is_read_as_its_first_part() {
        let dont = word("don't");
        assert_eq!(dont.tag, Tag::Auxiliary);
        assert_eq!(dont.confidence, Confidence::Sure);
        assert!(dont.features.contains(Features::CONTRACTION));
        assert!(dont.features.contains(Features::FINITE));
        assert!(dont.features.contains(Features::PRESENT));
        let its = word("it's");
        assert_eq!(its.tag, Tag::Pronoun);
        assert!(its.features.contains(Features::SINGULAR));
        assert!(its.features.contains(Features::THIRD));
        assert!(its.features.contains(Features::CONTRACTION));
        let lets = word("let's");
        assert_eq!(lets.tag, Tag::Verb);
        assert!(lets.features.contains(Features::FINITE));
        assert!(word("cannot").features.contains(Features::CONTRACTION));
        let didnt = word("didn't");
        assert!(didnt.features.contains(Features::PAST));
    }

    #[test]
    fn case_and_the_style_of_apostrophe_do_not_count() {
        assert_eq!(word("The"), word("the"));
        assert_eq!(word("THE"), word("the"));
        assert_eq!(word("Don\u{2019}t"), word("don't"));
        assert_eq!(word("DON'T"), word("don't"));
    }

    #[test]
    fn a_word_outside_the_table_is_an_unknown_noun() {
        for text in [
            "frobnicate",
            "Kubernetes",
            "foo_bar",
            "x86_64",
            "über",
            "a-very-long-identifier",
        ] {
            let reading = word(text);
            assert_eq!(reading.tag, Tag::Noun, "{text}");
            assert_eq!(reading.confidence, Confidence::Unknown, "{text}");
            assert_eq!(reading.kept, TagSet::of(Tag::Noun), "{text}");
            assert_eq!(reading.features, Features::NONE, "{text}");
        }
    }

    #[test]
    fn a_possessive_outside_the_table_is_a_noun_with_a_word_fused_on() {
        for text in ["user's", "Python\u{2019}s", "musxdom's", "GITHUB'S"] {
            let reading = word(text);
            assert_eq!(reading.tag, Tag::Noun, "{text}");
            assert_eq!(reading.confidence, Confidence::Unknown, "{text}");
            assert_eq!(
                reading.features,
                Features::SINGULAR.union(Features::CONTRACTION),
                "{text}"
            );
        }
        // The table's own contractions are read from it.
        assert_eq!(word("it's").tag, Tag::Pronoun);
        // A lone `s` or `'s` is no possessive.
        assert_eq!(word("s").features, Features::NONE);
        assert_eq!(word("'s").features, Features::NONE);
    }

    #[test]
    fn a_long_or_non_ascii_token_is_not_in_the_table() {
        assert_eq!(
            word("thethethethethethethe").confidence,
            Confidence::Unknown
        );
        assert_eq!(word("the\u{301}").confidence, Confidence::Unknown);
    }
}
