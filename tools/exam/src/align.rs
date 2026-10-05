//! Matching gold words to deslag tokens, when the two split the text differently.
//!
//! Alignment depends on the gold file and deslag's tokenizer alone, never on a tagger, so every
//! tagger is graded on the same scored tokens. A word is never guessed at: one that cannot be
//! matched is counted, with the reason, and left out of every metric.
//!
//! For a `ud` file the work is on byte spans of `# text`:
//!
//! 1. **Gold spans.** Each surface unit, a range line or a word line outside a range, is found in
//!    the text after the previous one, past whitespace. Every word gets its unit's span, so `do`
//!    and `n't` both get the span of `don't`. A form that is not there, or anything but
//!    whitespace left after the last unit, makes the sentence a *text mismatch*.
//! 2. **Groups.** Units and tokens whose spans overlap, chained, form a group; one that overlaps
//!    nothing is a group by itself.
//! 3. **Classes.** `PUNCT` and `SYM` words are punctuation and `X` words are X: counted, never
//!    scored, and never a reason a group fails. The rest are the group's tagged words, *G*; its
//!    `Word` tokens are *W*. With *G* empty nothing happens. With *W* empty the words of *G* are
//!    not word tokens. With two or more in *W* they are unalignable, *one word, several tokens*.
//!    With one token in *W* and a single tag across *G* it is a scored token, left out of the
//!    feature metrics when *G* has several words. With one token in *W* and several tags across
//!    *G* it is a scored token whose gold tag and features are those of the *first* word of *G*
//!    in gold order, as the annotation guide tags `don't` as `do`; this is a contraction, and
//!    `cannot`, `I'm` and `Bob` with `'s` score the same way.
//!
//! For a `deslag` file each line is one group with the token of its own index; a tagged word on a
//! line that is not a `Word` is not a word token, and one whose form no longer splits as its kind
//! is unalignable, *tokenizer drift*.

use std::ops::Range;

use deslag::document::{Token, TokenKind};

use crate::gold::{Gold, GoldSentence, TokenMode};
use crate::tags::{Class, Features, Tag};

/// Why a tagged gold word cannot be matched to a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// The sentence's words are not the text's: the gold's `# text` and its lines disagree.
    TextMismatch,
    /// A line of a `deslag` file whose form deslag's tokenizer no longer splits as its kind.
    TokenizerDrift,
    /// One gold word over several word tokens, as `e-mail` over `e`, `-` and `mail`.
    OneWordSeveralTokens,
}

impl Reason {
    /// Every reason, in the order the report lists them.
    pub const ALL: [Reason; 3] = [
        Reason::TextMismatch,
        Reason::TokenizerDrift,
        Reason::OneWordSeveralTokens,
    ];

    /// What the report calls it.
    pub fn label(self) -> &'static str {
        match self {
            Reason::TextMismatch => "text mismatch",
            Reason::TokenizerDrift => "tokenizer drift",
            Reason::OneWordSeveralTokens => "one word, several tokens",
        }
    }

    /// Its place in [`Reason::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A token the gold grades.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scored {
    /// The token's index in the sentence's tokens.
    pub token: usize,
    /// The index in the sentence's words of the token's gold word, or of the first when several
    /// stand behind it. The gold's word ID is this plus one, since IDs count words from 1.
    pub word: usize,
    /// The gold tag: the words' tag, or the first word's when they disagree.
    pub tag: Tag,
    /// The gold's features, or `None` when several agreeing words stand behind the token, which
    /// leaves it out of the feature metrics. When the words disagree, the first word's.
    pub features: Option<Features>,
    /// How many gold words the token stands for.
    pub words: usize,
    /// Whether the token is scored by its first word, because its words have different tags, as
    /// `don't` has `do` and `n't`.
    pub first_word: bool,
}

/// Tagged words that cannot be matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unalignable {
    /// Why.
    pub reason: Reason,
    /// The indexes of the words, in the sentence's words.
    pub words: Vec<usize>,
    /// The indexes of the tokens they overlap, in the sentence's tokens.
    pub tokens: Vec<usize>,
}

/// What alignment makes of one sentence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Alignment {
    /// The scored tokens, in order.
    pub scored: Vec<Scored>,
    /// The words it could not match.
    pub unalignable: Vec<Unalignable>,
    /// The indexes of tagged words that fall on no word token, such as a numeral on a `Number`.
    pub not_word: Vec<usize>,
    /// How many words are `PUNCT` or `SYM`.
    pub punctuation: usize,
    /// How many words are `X`.
    pub x: usize,
}

impl Alignment {
    /// The tagged words that were scored.
    pub fn scored_words(&self) -> usize {
        self.scored.iter().map(|scored| scored.words).sum()
    }

    /// The tagged words behind the tokens scored by their first word.
    pub fn first_word_words(&self) -> usize {
        self.scored
            .iter()
            .filter(|scored| scored.first_word)
            .map(|scored| scored.words)
            .sum()
    }

    /// The tokens scored by their first word.
    pub fn first_word_tokens(&self) -> usize {
        self.scored
            .iter()
            .filter(|scored| scored.first_word)
            .count()
    }

    /// The tagged words that could not be matched, for `reason`.
    pub fn unalignable_words(&self, reason: Reason) -> usize {
        self.unalignable
            .iter()
            .filter(|group| group.reason == reason)
            .map(|group| group.words.len())
            .sum()
    }

    /// All the tagged words: scored, unalignable and not on a word token.
    pub fn tagged_words(&self) -> usize {
        self.scored_words()
            + Reason::ALL
                .into_iter()
                .map(|reason| self.unalignable_words(reason))
                .sum::<usize>()
            + self.not_word.len()
    }
}

/// A gold sentence with its deslag tokens and what alignment made of them. It is made once per
/// gold file, and the Words section and every metric read it, so they cannot disagree.
#[derive(Debug)]
pub struct Aligned<'g> {
    /// The gold sentence.
    pub sentence: &'g GoldSentence,
    /// Its tokens, as a tagger is given them.
    pub tokens: Vec<Token<'g>>,
    /// How its words align to them.
    pub alignment: Alignment,
}

/// Aligns every sentence of `gold`, in order.
pub fn align_all(gold: &Gold) -> Vec<Aligned<'_>> {
    gold.sentences
        .iter()
        .map(|sentence| {
            let tokens = sentence.tokens();
            let alignment = sentence.align(&tokens);
            Aligned {
                sentence,
                tokens,
                alignment,
            }
        })
        .collect()
}

impl GoldSentence {
    /// Aligns the sentence's words to `tokens`, which must be [`GoldSentence::tokens`].
    pub fn align(&self, tokens: &[Token<'_>]) -> Alignment {
        align(self, tokens)
    }
}

/// Aligns `sentence`'s gold words to `tokens`, the tokens of its text.
pub fn align(sentence: &GoldSentence, tokens: &[Token<'_>]) -> Alignment {
    let mut out = Alignment::default();
    for word in &sentence.words {
        match word.class {
            Class::Punctuation => out.punctuation += 1,
            Class::X => out.x += 1,
            Class::Tagged(_) => {}
        }
    }
    match sentence.mode() {
        TokenMode::Deslag => by_line(sentence, tokens, &mut out),
        TokenMode::Ud => by_span(sentence, tokens, &mut out),
    }
    out
}

/// The tag of a gold word, if it is a tagged one.
fn tagged(sentence: &GoldSentence, word: usize) -> Option<Tag> {
    match sentence.words[word].class {
        Class::Tagged(tag) => Some(tag),
        _ => None,
    }
}

fn by_line(sentence: &GoldSentence, tokens: &[Token<'_>], out: &mut Alignment) {
    for (index, token) in tokens.iter().enumerate() {
        let Some(tag) = tagged(sentence, index) else {
            continue;
        };
        let guarded = matches!(
            token.kind,
            TokenKind::Word
                | TokenKind::Number
                | TokenKind::Punctuation
                | TokenKind::Symbol
                | TokenKind::Url
        );
        if guarded && !splits_as(&token.text, token.kind) {
            out.unalignable.push(Unalignable {
                reason: Reason::TokenizerDrift,
                words: vec![index],
                tokens: vec![index],
            });
        } else if token.kind != TokenKind::Word {
            out.not_word.push(index);
        } else {
            out.scored.push(Scored {
                token: index,
                word: index,
                tag,
                features: Some(sentence.words[index].features),
                words: 1,
                first_word: false,
            });
        }
    }
}

/// Whether deslag's tokenizer splits `form` into exactly one token, of `kind`, with that text.
fn splits_as(form: &str, kind: TokenKind) -> bool {
    let split = Token::split(form);
    matches!(split.as_slice(), [one] if one.kind == kind && one.text == form)
}

/// What a group holds, as indexes.
#[derive(Default)]
struct Group {
    units: Vec<usize>,
    tokens: Vec<usize>,
}

fn by_span(sentence: &GoldSentence, tokens: &[Token<'_>], out: &mut Alignment) {
    let Some(spans) = unit_spans(sentence) else {
        let words: Vec<usize> = (0..sentence.words.len())
            .filter(|word| tagged(sentence, *word).is_some())
            .collect();
        if !words.is_empty() {
            out.unalignable.push(Unalignable {
                reason: Reason::TextMismatch,
                words,
                tokens: Vec::new(),
            });
        }
        return;
    };
    for group in groups(&spans, tokens) {
        let words: Vec<usize> = group
            .units
            .iter()
            .flat_map(|unit| {
                let unit = &sentence.units[*unit];
                unit.first..unit.first + unit.count
            })
            .filter(|word| tagged(sentence, *word).is_some())
            .collect();
        if words.is_empty() {
            continue;
        }
        let word_tokens: Vec<usize> = group
            .tokens
            .iter()
            .copied()
            .filter(|token| tokens[*token].kind == TokenKind::Word)
            .collect();
        let tags: Vec<Tag> = words
            .iter()
            .filter_map(|word| tagged(sentence, *word))
            .collect();
        match word_tokens.as_slice() {
            [] => out.not_word.extend(words),
            [token] => {
                let agree = tags.iter().all(|tag| *tag == tags[0]);
                out.scored.push(Scored {
                    token: *token,
                    word: words[0],
                    tag: tags[0],
                    features: (!agree || words.len() == 1)
                        .then(|| sentence.words[words[0]].features),
                    words: words.len(),
                    first_word: !agree,
                });
            }
            _ => out.unalignable.push(Unalignable {
                reason: Reason::OneWordSeveralTokens,
                words,
                tokens: word_tokens,
            }),
        }
    }
}

/// Where each surface unit of the sentence is in its text, or `None` for a text mismatch.
fn unit_spans(sentence: &GoldSentence) -> Option<Vec<Range<usize>>> {
    let text = sentence.text.as_str();
    let mut cursor = 0;
    let mut spans = Vec::with_capacity(sentence.units.len());
    for unit in &sentence.units {
        let rest = &text[cursor..];
        let trimmed = rest.trim_start();
        let start = cursor + (rest.len() - trimmed.len());
        if !trimmed.starts_with(unit.form.as_str()) {
            return None;
        }
        let end = start + unit.form.len();
        spans.push(start..end);
        cursor = end;
    }
    text[cursor..].trim().is_empty().then_some(spans)
}

/// The groups of units and tokens that chain together by overlapping, in order of position.
fn groups(units: &[Range<usize>], tokens: &[Token<'_>]) -> Vec<Group> {
    // Each item: where it starts and ends, and whether it is a unit (true) or a token, and which.
    let mut items: Vec<(usize, usize, bool, usize)> = Vec::new();
    items.extend(
        units
            .iter()
            .enumerate()
            .map(|(i, s)| (s.start, s.end, true, i)),
    );
    items.extend(
        tokens
            .iter()
            .enumerate()
            .map(|(i, t)| (t.range.start, t.range.end, false, i)),
    );
    // Stable in position, units before tokens that start together, so the groups are the same
    // however the two lists interleave.
    items.sort_by_key(|(start, _, is_unit, index)| (*start, !*is_unit, *index));
    let mut groups: Vec<Group> = Vec::new();
    let mut reach = 0;
    for (start, end, is_unit, index) in items {
        if groups.is_empty() || start >= reach {
            groups.push(Group::default());
            reach = end;
        } else {
            reach = reach.max(end);
        }
        let group = groups.last_mut().expect("a group was just pushed");
        if is_unit {
            group.units.push(index);
        } else {
            group.tokens.push(index);
        }
    }
    groups
}
