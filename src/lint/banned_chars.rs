//! `banned_chars`: a file must not hold characters, such as the em dash, that have something plain
//! to write instead.
//!
//! The characters come in [`GROUPS`], each on by default but `emoji`, and switched by the config,
//! which may also allow a character or ban one more: `allow` beats `ban`, and `ban` the groups.
//! Only text outside code is checked: code blocks and code spans are skipped, so a diagram in a
//! fenced block may use box-drawing characters and arrows. Frontmatter, headings, tables and HTML
//! are checked. The file is read as written, so an HTML entity such as `&mdash;` is not a
//! character of it.
//!
//! [`scan`] needs only the document: it finds every character outside code that is not ASCII,
//! but a byte order mark that opens the file. [`check`] picks out the ones the settings ban. On the
//! corpus, the default groups must flag four times as many `llm/` fixtures as `human/` ones.
//!
//! [`edits`] names a fix for a banned character only where its replacement is exact: named for
//! that character alone, by `ban` or a rule of one character, and holding no letter or digit. The
//! edit replaces the character's bytes and no others, and the replacement holds no character the
//! settings ban, as [`contradiction`] makes sure before a file is checked, so the fixed text
//! passes where the character was. A word such as `yes` or `section`, or the default of a rule for a range of characters, is a
//! guess at meaning, and is left to the writer.

use crate::config::{BannedChars, CharGroups};
use crate::document::{BlockKind, Body, Document, Edit, Location, PieceKind};
use crate::lint::{Keep, Mark, MarkKind, on_lines};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected banned characters!";

/// The characters from `first` to `last`, with a name for the report and what to write instead.
#[derive(Debug)]
pub struct Rule {
    /// The first character.
    pub first: char,
    /// The last character, the same as `first` for a rule of one.
    pub last: char,
    /// What the report calls them.
    pub name: &'static str,
    /// What to write instead; empty means delete it.
    pub instead: &'static str,
}

impl Rule {
    fn holds(&self, ch: char) -> bool {
        (self.first..=self.last).contains(&ch)
    }
}

/// A group of characters that the config switches on or off as one.
pub struct Group {
    /// Its key in `lints.banned_chars.groups`.
    pub name: &'static str,
    /// Whether it is on when the config does not say.
    pub on_by_default: bool,
    /// Its switch in the config.
    pub switch: fn(&CharGroups) -> Option<bool>,
    /// Its characters. A character in more than one rule takes the first.
    pub rules: &'static [Rule],
}

const fn one(ch: char, name: &'static str, instead: &'static str) -> Rule {
    Rule {
        first: ch,
        last: ch,
        name,
        instead,
    }
}

const fn range(first: char, last: char, name: &'static str, instead: &'static str) -> Rule {
    Rule {
        first,
        last,
        name,
        instead,
    }
}

/// Every group, in the order a character is looked up in: a character in more than one group,
/// such as a check mark, which is also an emoji, takes the first group that is on.
pub const GROUPS: &[Group] = &[
    Group {
        name: "dashes",
        on_by_default: true,
        switch: |groups| groups.dashes,
        rules: &[
            one('\u{2014}', "em dash", "-"),
            one('\u{2013}', "en dash", "-"),
            one('\u{2212}', "minus sign", "-"),
            one('\u{2010}', "hyphen", "-"),
            one('\u{2011}', "non-breaking hyphen", "-"),
            one('\u{2012}', "figure dash", "-"),
            one('\u{2015}', "horizontal bar", "-"),
            one('\u{2E3A}', "two-em dash", "-"),
            one('\u{2E3B}', "three-em dash", "-"),
        ],
    },
    Group {
        name: "arrows",
        on_by_default: true,
        switch: |groups| groups.arrows,
        rules: &[
            one('\u{2192}', "rightwards arrow", "->"),
            one('\u{2190}', "leftwards arrow", "<-"),
            one('\u{2194}', "left right arrow", "<->"),
            one('\u{2191}', "upwards arrow", "up"),
            one('\u{2193}', "downwards arrow", "down"),
            one('\u{21D2}', "rightwards double arrow", "=>"),
            one('\u{21D0}', "leftwards double arrow", "<="),
            one('\u{21D4}', "left right double arrow", "<=>"),
            one('\u{27F6}', "long rightwards arrow", "->"),
            one('\u{27F5}', "long leftwards arrow", "<-"),
            one('\u{27F7}', "long left right arrow", "<->"),
            one('\u{27F9}', "long rightwards double arrow", "=>"),
            one('\u{27F8}', "long leftwards double arrow", "<="),
            one('\u{27A1}', "black rightwards arrow", "->"),
            one('\u{2B05}', "leftwards black arrow", "<-"),
            one('\u{2B06}', "upwards black arrow", "up"),
            one('\u{2B07}', "downwards black arrow", "down"),
            range('\u{2190}', '\u{21FF}', "arrow", "->"),
            range('\u{27F0}', '\u{27FF}', "arrow", "->"),
            range('\u{2900}', '\u{297F}', "arrow", "->"),
            // U+2795 to U+2797 are the heavy plus, minus and division signs.
            one('\u{2794}', "heavy wide-headed rightwards arrow", "->"),
            range('\u{2798}', '\u{27AF}', "arrow", "->"),
            range('\u{27B1}', '\u{27BE}', "arrow", "->"),
            range('\u{2B00}', '\u{2B11}', "arrow", "->"),
        ],
    },
    Group {
        name: "ellipsis",
        on_by_default: true,
        switch: |groups| groups.ellipsis,
        rules: &[
            one('\u{2026}', "horizontal ellipsis", "..."),
            one('\u{22EF}', "midline horizontal ellipsis", "..."),
        ],
    },
    Group {
        name: "bullets",
        on_by_default: true,
        switch: |groups| groups.bullets,
        rules: &[
            one('\u{2022}', "bullet", "-"),
            one('\u{00B7}', "middle dot", "-"),
            one('\u{2023}', "triangular bullet", "-"),
            one('\u{2043}', "hyphen bullet", "-"),
            one('\u{2219}', "bullet operator", "-"),
            one('\u{25E6}', "white bullet", "-"),
            range('\u{25A0}', '\u{25FF}', "geometric shape", "-"),
        ],
    },
    Group {
        name: "math",
        on_by_default: true,
        switch: |groups| groups.math,
        rules: &[
            one('\u{00D7}', "multiplication sign", "x"),
            one('\u{00F7}', "division sign", "/"),
            one('\u{00B1}', "plus-minus sign", "+/-"),
            one('\u{2248}', "almost equal to", "~"),
            one('\u{223C}', "tilde operator", "~"),
            one('\u{2260}', "not equal to", "!="),
            one('\u{2264}', "less-than or equal to", "<="),
            one('\u{2265}', "greater-than or equal to", ">="),
        ],
    },
    Group {
        name: "checks",
        on_by_default: true,
        switch: |groups| groups.checks,
        rules: &[
            one('\u{2705}', "white heavy check mark", "yes"),
            one('\u{2713}', "check mark", "yes"),
            one('\u{2714}', "heavy check mark", "yes"),
            one('\u{2611}', "ballot box with check", "yes"),
            one('\u{274C}', "cross mark", "no"),
            one('\u{274E}', "negative squared cross mark", "no"),
            one('\u{2717}', "ballot x", "no"),
            one('\u{2718}', "heavy ballot x", "no"),
            one('\u{2612}', "ballot box with x", "no"),
            one('\u{2610}', "ballot box", "[ ]"),
        ],
    },
    Group {
        name: "section",
        on_by_default: true,
        switch: |groups| groups.section,
        rules: &[one('\u{00A7}', "section sign", "section")],
    },
    Group {
        name: "box_drawing",
        on_by_default: true,
        switch: |groups| groups.box_drawing,
        rules: &[
            one('\u{2502}', "box drawings light vertical", "|"),
            one('\u{2503}', "box drawings heavy vertical", "|"),
            one('\u{2551}', "box drawings double vertical", "|"),
            one('\u{2550}', "box drawings double horizontal", "="),
            range('\u{250C}', '\u{254B}', "box-drawing corner", "+"),
            range('\u{2552}', '\u{256C}', "box-drawing corner", "+"),
            range('\u{2500}', '\u{257F}', "box-drawing character", "-"),
            range('\u{2580}', '\u{259F}', "block element", "#"),
        ],
    },
    Group {
        name: "spaces",
        on_by_default: true,
        switch: |groups| groups.spaces,
        rules: &[
            one('\u{00A0}', "no-break space", " "),
            one('\u{202F}', "narrow no-break space", " "),
            one('\u{205F}', "medium mathematical space", " "),
            range('\u{2000}', '\u{200A}', "space", " "),
        ],
    },
    Group {
        name: "invisible",
        on_by_default: true,
        switch: |groups| groups.invisible,
        rules: INVISIBLE,
    },
    Group {
        name: "quotes",
        on_by_default: true,
        switch: |groups| groups.quotes,
        rules: &[
            one('\u{2018}', "left single quotation mark", "'"),
            one('\u{2019}', "right single quotation mark", "'"),
            one('\u{201B}', "single high-reversed-9 quotation mark", "'"),
            one('\u{201C}', "left double quotation mark", "\""),
            one('\u{201D}', "right double quotation mark", "\""),
            one('\u{201F}', "double high-reversed-9 quotation mark", "\""),
            one('\u{2032}', "prime", "'"),
            one('\u{2033}', "double prime", "\""),
        ],
    },
    Group {
        name: "emoji",
        on_by_default: false,
        switch: |groups| groups.emoji,
        rules: &[
            one('\u{FE0F}', "variation selector-16", ""),
            one('\u{2B50}', "white medium star", ""),
            one('\u{2B55}', "heavy large circle", ""),
            range('\u{2600}', '\u{27BF}', "emoji", ""),
            range('\u{1F1E6}', '\u{1F1FF}', "regional indicator", ""),
            range('\u{1F300}', '\u{1F6FF}', "emoji", ""),
            range('\u{1F900}', '\u{1F9FF}', "emoji", ""),
            range('\u{1FA70}', '\u{1FAFF}', "emoji", ""),
        ],
    },
];

/// The characters that take no space, which the report does not quote.
const INVISIBLE: &[Rule] = &[
    one('\u{200B}', "zero width space", ""),
    one('\u{2060}', "word joiner", ""),
    one('\u{00AD}', "soft hyphen", ""),
    one('\u{FEFF}', "zero width no-break space", ""),
    one('\u{180E}', "mongolian vowel separator", ""),
    range('\u{E0000}', '\u{E007F}', "tag character", ""),
];

/// A character outside code that is not ASCII.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// Where it is.
    pub location: Location,
    /// The character.
    pub ch: char,
}

/// One banned character and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banned {
    /// The character.
    pub ch: char,
    /// What the report calls it, when deslag knows.
    pub name: Option<&'static str>,
    /// What to write instead; empty means delete it.
    pub instead: String,
    /// Whether `instead` is the default of a rule for a range of characters, rather than named for
    /// this one.
    pub range_default: bool,
    /// Each place the file holds it, in order.
    pub locations: Vec<Location>,
}

/// A file that holds banned characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// Each banned character, in the order they first appear.
    pub banned: Vec<Banned>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

impl Over {
    /// How many banned characters the file holds.
    pub fn count(&self) -> usize {
        self.banned
            .iter()
            .map(|banned| banned.locations.len())
            .sum()
    }
}

/// Finds every character of `document` that is outside code and not ASCII. A byte order mark
/// that opens the file is not counted.
pub fn scan(document: &Document<'_>) -> Vec<Found> {
    let mut found = Vec::new();
    for (block, _) in document.walk() {
        let pieces = match (&block.kind, &block.body) {
            (BlockKind::Code { .. }, _) => continue,
            (_, Body::Raw(pieces)) => pieces.as_slice(),
            _ => document.pieces_of(block),
        };
        let written = pieces
            .iter()
            .filter(|piece| matches!(piece.kind, PieceKind::Text | PieceKind::Html));
        for piece in written {
            // The source, not the piece's text, so that an entity is read as it is written.
            for (at, ch) in document.source[piece.range.clone()].char_indices() {
                let offset = piece.range.start + at;
                if !ch.is_ascii() && !(offset == 0 && ch == '\u{FEFF}') {
                    found.push(Found {
                        location: document.locate(offset..offset + ch.len_utf8()),
                        ch,
                    });
                }
            }
        }
    }
    found
}

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&BannedChars>) -> Option<Over> {
    let settings = settings?;
    let mut banned: Vec<Banned> = Vec::new();

    for Found { location, ch } in scan(document) {
        let Some((instead, range_default)) = verdict(settings, ch) else {
            continue;
        };
        match banned.iter_mut().find(|seen| seen.ch == ch) {
            Some(seen) => seen.locations.push(location),
            None => banned.push(Banned {
                ch,
                name: name(ch),
                instead,
                range_default,
                locations: vec![location],
            }),
        }
    }

    (!banned.is_empty()).then(|| Over {
        banned,
        message: settings.message.clone(),
    })
}

/// Why `settings`, resolved for one file, contradict each other: a value in `ban` that holds a
/// character they ban, which the report would advise writing and then report. Whether one does
/// depends on every table merged for the file, and on the groups, which the config does not know.
pub fn contradiction(settings: &BannedChars) -> Option<String> {
    settings.ban.iter().flatten().find_map(|(ch, instead)| {
        instead
            .chars()
            .find(|within| verdict(settings, *within).is_some())
            .map(|within| {
                format!(
                    "banned_chars.ban maps {ch:?} to {instead:?}, which holds U+{:04X}, a \
                     character it bans",
                    within as u32
                )
            })
    })
}

/// What to write instead of `ch`, and whether that is the default of a rule for a range of
/// characters; `None` when `settings` do not ban it.
fn verdict(settings: &BannedChars, ch: char) -> Option<(String, bool)> {
    let key = ch.to_string();
    if settings
        .allow
        .iter()
        .flatten()
        .any(|allowed| *allowed == key)
    {
        return None;
    }
    if let Some(instead) = settings.ban.as_ref().and_then(|ban| ban.get(&key)) {
        return Some((instead.clone(), false));
    }
    GROUPS
        .iter()
        .filter(|group| (group.switch)(&settings.groups).unwrap_or(group.on_by_default))
        .find_map(|group| rule(group.rules, ch))
        .map(|rule| (rule.instead.to_string(), rule.first != rule.last))
}

/// What the report calls `ch`, from the first rule of any group that holds it, on or off.
fn name(ch: char) -> Option<&'static str> {
    GROUPS
        .iter()
        .find_map(|group| rule(group.rules, ch))
        .map(|rule| rule.name)
}

fn rule(rules: &'static [Rule], ch: char) -> Option<&'static Rule> {
    rules.iter().find(|rule| rule.holds(ch))
}

/// The report for one file at `path` that holds banned characters, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => message.replace("{path}", path),
        None => DEFAULT_ADVICE.to_string(),
    };
    let listed: String = over
        .banned
        .iter()
        .map(|banned| format!("\n  {}", describe(banned)))
        .collect();

    format!(
        "{HEADING}\n\
         \n\
         {path} has {count}.\n\
         \n\
         {advice}\n\
         \n\
         The characters, and what to write instead:{listed}",
        count = match over.count() {
            1 => "1 banned character".to_string(),
            count => format!("{count} banned characters"),
        },
    )
}

/// The places the report lists: each character wherever the file holds it.
pub fn marks(over: &Over) -> Vec<Mark> {
    edits(over).into_iter().map(|(mark, _)| mark).collect()
}

/// Each place the report lists, with the edit that writes what the report says to write there,
/// or why a fix may not.
pub fn edits(over: &Over) -> Vec<(Mark, Result<Edit, &'static str>)> {
    over.banned
        .iter()
        .flat_map(|banned| {
            let note = note(banned);
            let unfixable = unfixable(banned);
            banned.locations.iter().map(move |location| {
                let mark = Mark {
                    kind: MarkKind::Occurrence,
                    location: *location,
                    note: note.clone(),
                };
                let edit = match unfixable {
                    Some(why) => Err(why),
                    None => Ok(Edit {
                        range: location.start..location.end,
                        replacement: banned.instead.clone(),
                    }),
                };
                (mark, edit)
            })
        })
        .collect()
}

/// The part of `over` that `keep` keeps: the places of each character, each an occurrence, that
/// it keeps.
pub fn retain(over: &Over, keep: &dyn Keep) -> Option<Over> {
    let banned: Vec<Banned> = over
        .banned
        .iter()
        .filter_map(|banned| {
            let locations: Vec<Location> = banned
                .locations
                .iter()
                .filter(|location| keep.occurrence(location))
                .copied()
                .collect();
            (!locations.is_empty()).then(|| Banned {
                locations,
                ..banned.clone()
            })
        })
        .collect();
    (!banned.is_empty()).then(|| Over {
        banned,
        message: over.message.clone(),
    })
}

/// Why a fix may not write what the report says to write in place of `banned`, or `None` when it
/// may.
fn unfixable(banned: &Banned) -> Option<&'static str> {
    if banned.range_default {
        Some(
            "the replacement is the default for a range of characters, a guess at what this one \
             means",
        )
    } else if banned.instead.chars().any(char::is_alphanumeric) {
        Some(
            "the replacement holds letters or digits, a guess at meaning that can run into the \
             text beside it",
        )
    } else {
        None
    }
}

/// One line of the report: the character, the lines it is on, and what to write instead.
fn describe(banned: &Banned) -> String {
    let mut lines: Vec<usize> = banned
        .locations
        .iter()
        .map(|location| location.line)
        .collect();
    lines.dedup();
    format!("{}: {}", on_lines(&lines), note(banned))
}

/// What the report says of a banned character wherever it is: what it is, and what to write
/// instead.
fn note(banned: &Banned) -> String {
    let ch = banned.ch;
    let mut what = format!("U+{:04X}", ch as u32);
    if let Some(name) = banned.name {
        what.push_str(&format!(" {name}"));
    }
    let hidden = ch.is_whitespace()
        || rule(INVISIBLE, ch).is_some()
        || ('\u{FE00}'..='\u{FE0F}').contains(&ch);
    if !hidden {
        what.push_str(&format!(" \"{ch}\""));
    }
    let instead = match banned.instead.as_str() {
        "" => "delete it".to_string(),
        " " => "write a plain space".to_string(),
        instead => format!("write `{instead}`"),
    };
    format!("{what}; {instead}")
}

/// The advice for a file that holds banned characters.
const DEFAULT_ADVICE: &str = "This repo writes in plain characters, and each of these has a \
    plain equivalent. Write it as listed, or reword: an em dash often reads better as a comma, a \
    colon, parentheses or a new sentence.\n\
    \n\
    Code blocks and code spans are not checked, so a diagram may use these characters inside a \
    fenced code block.\n\
    \n\
    Do not write the characters as HTML entities, and do not change the config to get past this \
    check. Only a human can tell you to do that, and I am a linter, not a human.";
