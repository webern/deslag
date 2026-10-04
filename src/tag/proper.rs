//! The proper-noun pass: a capitalised word in the middle of a sentence is a name.
//!
//! The tables fold case, so they read `Bush` as they read `bush`, and a name they do not list is a
//! noun. English marks a name with a capital, and a capital in the middle of a sentence is not
//! there because a sentence began, so it says something.
//!
//! **The rule.** In prose and list items, a word that starts with an upper-case letter, that can be
//! a proper noun, that follows a word or a comma in a sentence that is not written in title case,
//! and that the tables either do not know (`Unknown`) or rank a name first, becomes a proper noun
//! at `Likely`, and loses every reading that is not a noun, a proper noun or an adjective.
//!
//! What it leaves alone, and why.
//!
//! - **Where a capital says nothing.** The first word of a sentence, and a word after anything but
//!   a word or a comma, such as a colon, a quote, a bracket or a code span, any of which may open a
//!   clause. A heading or a table cell, whose words are capitalised by the style of the heading or
//!   the header row. A sentence in title case or in capitals: three or more of its words of four
//!   letters or more, not counting the first, and every one of them capitalised. Shorter words are
//!   not counted because titles lower them.
//! - **Capitals that are not names.** A word in capitals, which is as likely an acronym of a thing
//!   (`HTML`) as a name (`NASA`), and a lone capital letter, which is an initial, a grade or a
//!   label. Neither says which. So a word needs a lower-case letter after its capital, and the
//!   plural of an acronym (`APIs`) has none that counts.
//! - **Words that are not nouns first.** A capitalised adjective is usually a nationality or a
//!   title (`American`), and a capitalised verb is as often a word that opens a phrase as a name;
//!   the lexicon ranked each first, and the capital alone is weak evidence against it.
//! - **Common words with a name among their tags.** A lexicon word that ranks a noun first and
//!   lists a proper noun after it (`Service`, `Key`) was tried: a capital promoted it to a name,
//!   right 91% of the time on EWT dev and 6 of 10 on deslag dev, under the floor for `Likely`.
//!   Technical prose capitalises its common nouns as often as its names. It keeps its guess.
//! - **Words that cannot be names.** A word with no proper noun among its tags: a word of the
//!   closed-class table, where a capitalised `This` is still `this`, and a word the lexicon knows
//!   only as a common word, which a capital in running text marks as often by style as by name.
//!   The lexicon lists the names it knows, and a word it lacks may be one (`closed::unknown`),
//!   so a capital settles most where the lexicon is silent or lists the name too.
//! - **Words with one tag.** A lexicon name such as `Paris` has one tag left, so there is nothing
//!   to narrow and it stays `Unsure`, as the lexicon left it.
//!
//! The rule reads the neighbours' kind and text, never their readings, so no neighbour can be
//! ambiguous to it. That a capital in the middle of a sentence marks a name is the tagger's guess,
//! so the word is `Likely` and its other tags stay in `kept`.

use super::pass::View;
use super::{Confidence, Context, Tag, TagSet, starts_upper};
use crate::document::TokenKind;

/// The tags a capitalised word keeps: those of a word that is a noun, a name or the adjective
/// from a name. The rest, a verb or an adverb, a capital in the middle of a sentence does not mark.
const KEPT: TagSet = TagSet::of(Tag::Noun)
    .with(Tag::ProperNoun)
    .with(Tag::Adjective);

/// The shortest word, in letters, that counts when deciding whether a sentence is in title case.
const LONG: usize = 4;

/// The fewest long words after the first that a sentence in title case has.
const FEWEST_LONG: usize = 3;

/// Runs the pass over one sentence.
pub(super) fn run(view: &mut View<'_, '_>) {
    if !matches!(view.context(), Context::Prose | Context::ListItem) || title_cased(view) {
        return;
    }
    for at in 1..view.len() {
        if view.kind(at) != TokenKind::Word
            || !is_capitalised(view.text(at))
            || !follows_a_word_or_comma(view, at)
        {
            continue;
        }
        let Some(reading) = view.reading(at) else {
            continue;
        };
        if reading.possible().contains(Tag::ProperNoun)
            && (reading.confidence == Confidence::Unknown || reading.tag == Tag::ProperNoun)
        {
            view.narrow(at, KEPT, Tag::ProperNoun);
        }
    }
}

/// The word without a possessive `'s` after it, straight or curly.
fn stem(text: &str) -> &str {
    text.strip_suffix("'s")
        .or_else(|| text.strip_suffix("\u{2019}s"))
        .unwrap_or(text)
}

/// How many letters `text` has.
fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphabetic()).count()
}

/// Whether the word starts with a capital and has a lower-case letter too, which a lone capital
/// and a word in capitals lack. The plural of an acronym (`APIs`) is a word in capitals with an `s`,
/// so it has none either.
fn is_capitalised(text: &str) -> bool {
    // The stem starts as the word does, so this settles most words without reading them.
    if !starts_upper(text) {
        return false;
    }
    let stem = stem(text);
    let acronym_plural = stem
        .strip_suffix('s')
        .is_some_and(|body| body.chars().count() > 1 && !body.chars().any(char::is_lowercase));
    starts_upper(stem) && stem.chars().any(char::is_lowercase) && !acronym_plural
}

/// Whether the token before the one at `at` is a word or a comma, the places where a capital is
/// not explained by the start of a clause.
fn follows_a_word_or_comma(view: &View<'_, '_>, at: usize) -> bool {
    at > 0
        && match view.kind(at - 1) {
            TokenKind::Word => true,
            TokenKind::Punctuation => view.text(at - 1) == ",",
            _ => false,
        }
}

/// Whether the sentence is in title case or in capitals: [`FEWEST_LONG`] or more of its words of
/// [`LONG`] letters or more, not counting the first word, which a capital begins whatever it says,
/// and every one of them starts with a capital.
fn title_cased(view: &View<'_, '_>) -> bool {
    let mut words = (0..view.len()).filter(|at| view.kind(*at) == TokenKind::Word);
    words.next();
    let mut long = 0;
    for at in words {
        let text = view.text(at);
        if letters(text) >= LONG {
            if !starts_upper(text) {
                return false;
            }
            long += 1;
        }
    }
    long >= FEWEST_LONG
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Token};
    use crate::tag::{Confidence, Features, Reading, pass, sentence};
    use Tag::{Adjective, Adverb, Noun, ProperNoun, Verb};

    /// A reading as the tables leave a word: `Unsure`, the tags in `kept` from the best guess on.
    fn open(tag: Tag, kept: &[Tag]) -> Reading {
        Reading {
            tag,
            features: Features::NONE,
            confidence: Confidence::Unsure,
            kept: kept.iter().copied().collect(),
        }
    }

    /// The tokens of `text`, every word read by `reading`, run through the passes in `context`.
    fn pass_over(
        text: &str,
        context: Context,
        reading: impl Fn(&str) -> Reading,
    ) -> Vec<(String, Option<Reading>)> {
        let mut tokens = Token::split(text);
        for token in &mut tokens {
            token.reading = (token.kind == TokenKind::Word).then(|| reading(&token.text));
        }
        pass::run(&mut tokens, context);
        tokens
            .into_iter()
            .map(|token| (token.text.into_owned(), token.reading))
            .collect()
    }

    /// The reading `word` has after the pass, in `text`, where every other word is read as the
    /// open-class noun `bag` is and `word` as `reading` says.
    fn after(text: &str, context: Context, word: &str, reading: Reading) -> Reading {
        let bag = open(Noun, &[Noun, Verb]);
        let read = pass_over(text, context, |w| if w == word { reading } else { bag });
        read.into_iter()
            .find(|(w, _)| w == word)
            .and_then(|(_, r)| r)
            .unwrap()
    }

    /// A word the tables do not know that starts with a capital, as the shape reading leaves it.
    fn name_like() -> Reading {
        Reading {
            confidence: Confidence::Unknown,
            ..open(Noun, &[Noun, ProperNoun, Verb, Adjective, Adverb])
        }
    }

    /// Asserts that the word is read as a name, `Likely`, keeping only a noun, a proper noun and
    /// an adjective of what it had.
    fn assert_a_name(reading: Reading) {
        assert_eq!(reading.tag, ProperNoun);
        assert_eq!(reading.confidence, Confidence::Likely);
        assert_eq!(
            reading.kept,
            [Noun, ProperNoun, Adjective]
                .into_iter()
                .collect::<TagSet>()
        );
        assert_eq!(reading.features, Features::SINGULAR);
    }

    fn assert_untouched(text: &str, context: Context, word: &str, before: Reading) {
        assert_eq!(
            after(text, context, word, before),
            before,
            "{word} in {text}"
        );
    }

    #[test]
    fn a_capital_in_the_middle_of_a_sentence_makes_a_name() {
        let before = name_like();
        assert_ne!(before.tag, ProperNoun);
        let read = after("We met Bush in town", Context::Prose, "Bush", before);
        assert_a_name(read);
        // The same in a list item, and after a comma.
        assert_a_name(after(
            "We met Bush in town",
            Context::ListItem,
            "Bush",
            before,
        ));
        assert_a_name(after("Dear all, Bush left", Context::Prose, "Bush", before));
        // A possessive is still a name, and a name with a plural keeps it.
        assert_a_name(after(
            "We met Bush's team",
            Context::Prose,
            "Bush's",
            before,
        ));
    }

    #[test]
    fn a_common_word_with_a_name_among_its_tags_is_left_alone() {
        // `Service` or `Key`: the lexicon ranks the noun first and lists a name after it.
        let before = open(Noun, &[Noun, ProperNoun, Verb]);
        assert_untouched("We met Service in town", Context::Prose, "Service", before);
        assert_untouched(
            "We met Service in town",
            Context::ListItem,
            "Service",
            before,
        );
    }

    #[test]
    fn a_name_the_lexicon_ranks_first_becomes_likely() {
        let before = open(ProperNoun, &[ProperNoun, Noun]);
        let read = after("We met Bush in town", Context::Prose, "Bush", before);
        assert_eq!(read.tag, ProperNoun);
        assert_eq!(read.confidence, Confidence::Likely);
        assert_eq!(read.kept, TagSet::of(ProperNoun).with(Noun));
    }

    #[test]
    fn a_word_that_is_not_a_noun_first_is_left_alone() {
        let adjective = open(Adjective, &[Adjective, ProperNoun, Noun]);
        let verb = open(Verb, &[Verb, ProperNoun, Noun]);
        for before in [adjective, verb] {
            assert_untouched("We met Bush in town", Context::Prose, "Bush", before);
        }
    }

    #[test]
    fn a_word_that_cannot_be_a_name_is_left_alone() {
        let this = open(Tag::Determiner, &[Tag::Determiner, Tag::Pronoun]);
        assert_untouched("We met This in town", Context::Prose, "This", this);
        let noun = open(Noun, &[Noun, Verb]);
        assert_untouched("We met Bush in town", Context::Prose, "Bush", noun);
    }

    #[test]
    fn a_word_with_one_tag_stays_as_it_is() {
        let paris = open(ProperNoun, &[ProperNoun]);
        assert_untouched("We met Paris in town", Context::Prose, "Paris", paris);
        let sure = Reading {
            confidence: Confidence::Sure,
            ..paris
        };
        assert_untouched("We met Paris in town", Context::Prose, "Paris", sure);
    }

    #[test]
    fn the_start_of_a_sentence_says_nothing() {
        let before = name_like();
        assert_untouched("Bush met us in town", Context::Prose, "Bush", before);
        // Nor does a word after a mark that may open a clause.
        for text in [
            "We said: Bush met us",
            "We said \"Bush met us\"",
            "We met (Bush) here",
            "We met `bush` Bush here",
            "We met. Bush left",
            "We met 5 Bush here",
        ] {
            assert_untouched(text, Context::Prose, "Bush", before);
        }
    }

    #[test]
    fn a_heading_or_a_table_cell_says_nothing() {
        let before = name_like();
        for context in [Context::Heading, Context::TableCell] {
            assert_untouched("We met Bush in town", context, "Bush", before);
        }
    }

    #[test]
    fn capitals_that_do_not_mark_a_name_are_left_alone() {
        let before = name_like();
        for word in ["BUSH", "B", "BUSH's", "APIs", "CDNs"] {
            assert_untouched(
                &format!("We met {word} in town"),
                Context::Prose,
                word,
                before,
            );
        }
    }

    #[test]
    fn a_sentence_in_title_case_says_nothing() {
        let before = name_like();
        // Three long words after the first, each capitalised: a title.
        assert_untouched("Ship Quick Orders Today", Context::Prose, "Orders", before);
        // Short words are lower in a title and are not counted.
        assert_untouched(
            "Lord of the Rings Movie Night",
            Context::Prose,
            "Rings",
            before,
        );
        // One lower-case long word makes it a sentence again.
        assert_a_name(after(
            "Ship Quick Orders today",
            Context::Prose,
            "Orders",
            before,
        ));
        // Two are too few to call a title.
        assert_a_name(after("Ship Quick Orders", Context::Prose, "Orders", before));
        // The first word is not counted, so a command naming two things is no title.
        assert_a_name(after(
            "Install Docker on Ubuntu",
            Context::Prose,
            "Docker",
            before,
        ));
    }

    #[test]
    fn a_pass_over_a_sentence_with_no_capital_changes_nothing() {
        let before = name_like();
        let read = pass_over("we met bush in town", Context::Prose, |_| before);
        for (word, reading) in read {
            if word.chars().all(char::is_alphabetic) {
                assert_eq!(reading, Some(before), "{word}");
            }
        }
    }

    /// The reading of the first word in `markdown` that is `word`, tagged as `Document` does.
    fn read_in(markdown: &str, word: &str) -> Reading {
        let doc = Document::markdown(markdown);
        doc.tokens
            .iter()
            .find(|token| token.text == word)
            .and_then(|token| token.reading)
            .unwrap_or_else(|| panic!("no read {word}"))
    }

    #[test]
    fn a_made_up_name_is_unknown_before_and_likely_after() {
        // The tables have no such word: before the pass its shape says it may be a name, and the
        // level is `Unknown`.
        let lookup = crate::tag::read("Frobnitz");
        assert_eq!(lookup.tag, ProperNoun);
        assert_eq!(lookup.confidence, Confidence::Unknown);
        assert_eq!(lookup.kept, TagSet::of(Noun).with(ProperNoun));
        let mut tokens = Token::split("We use Frobnitz daily.");
        sentence(&mut tokens, Context::Prose);
        let read = tokens[2].reading.unwrap();
        assert_eq!(read.tag, ProperNoun);
        assert_eq!(read.confidence, Confidence::Likely);
        assert_eq!(read.kept, TagSet::of(Noun).with(ProperNoun));
        // A lower-case word is the noun it was, and the pass leaves it at `Unknown`.
        let mut tokens = Token::split("We use frobnitz daily.");
        sentence(&mut tokens, Context::Prose);
        let read = tokens[2].reading.unwrap();
        assert_eq!(read.tag, Noun);
        assert_eq!(read.confidence, Confidence::Unknown);
        assert!(!read.kept.contains(ProperNoun));
    }

    #[test]
    fn the_context_of_the_block_decides() {
        let md = "# We use Frobnitz\n\nWe use Frobnitz daily.\n\n- We use Frobnitz daily\n\n| a |\n|---|\n| we use Frobnitz |\n";
        let doc = Document::markdown(md);
        let reads: Vec<Reading> = doc
            .tokens
            .iter()
            .filter(|token| token.text == "Frobnitz")
            .filter_map(|token| token.reading)
            .collect();
        let levels: Vec<Confidence> = reads.iter().map(|r| r.confidence).collect();
        // A heading and a table cell: left as the shape guessed. Prose and a list item: a name the
        // pass is `Likely` of.
        assert_eq!(
            levels,
            vec![
                Confidence::Unknown,
                Confidence::Likely,
                Confidence::Likely,
                Confidence::Unknown
            ]
        );
        assert!(reads.iter().all(|read| read.tag == ProperNoun));
        assert_eq!(
            read_in("We use Frobnitz daily.", "Frobnitz").tag,
            ProperNoun
        );
    }
}
