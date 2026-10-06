//! What the review looks like: a [`Session`] as lines of text.
//!
//! Drawing reads the session and decides nothing. [`lines`] lays one screen out as plain lines,
//! each with a look; [`text`] shows them as text, so a screen can be tested and printed with no
//! terminal, and [`draw`] writes them to one.

use std::io::Write;

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{Clear, ClearType};
use deslag_exam::gold::kind_name;

use crate::guide;
use crate::review::{Mode, Row, Session};

/// The keys, as the footer lists them.
const KEYS: &str =
    "j/k move, t tag, ? guide, a accept, n/p save and go to the next or previous sentence, q quit";

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// As it is.
    Plain,
    /// Bold.
    Bold,
    /// Dimmed.
    Dim,
    /// Underlined.
    Underlined,
    /// Reversed: the line the cursor is on.
    Reversed,
}

/// One line of the screen, never wider than the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// What it says.
    pub text: String,
    /// How it is drawn.
    pub look: Look,
}

fn line(text: impl Into<String>, look: Look) -> Line {
    Line {
        text: text.into(),
        look,
    }
}

/// `text` cut to `width` characters.
fn clip(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

/// `text` cut or padded with spaces to exactly `width` characters.
fn cell(text: &str, width: usize) -> String {
    let mut cell = clip(text, width);
    let short = width - cell.chars().count();
    cell.extend(std::iter::repeat_n(' ', short));
    cell
}

/// `text` broken at spaces into lines of at most `width` characters; a word longer than that is
/// cut.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word;
            while word.chars().count() > width {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                out.push(clip(word, width));
                word = &word[word
                    .char_indices()
                    .nth(width)
                    .map_or(word.len(), |(at, _)| at)..];
            }
            let joined = current.chars().count() + usize::from(!current.is_empty());
            if !current.is_empty() && joined + word.chars().count() > width {
                out.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        out.push(current);
    }
    out
}

/// One table row: the cursor's mark, then the columns, a space between.
fn table_row(mark: char, cells: &[(&str, usize)], width: usize) -> String {
    let mut row = String::from(mark);
    for (at, (text, wide)) in cells.iter().enumerate() {
        if at > 0 {
            row.push(' ');
        }
        row.push_str(&cell(text, *wide));
    }
    clip(row.trim_end(), width)
}

/// The words of the sentence as a table, `height` lines, scrolled so the cursor shows.
fn table(session: &Session, width: usize, height: usize) -> Vec<Line> {
    let sentence = session.sentence();
    let reads = width.saturating_sub(1 + 3 + 18 + 13 + 8 + 8 + 5).max(20);
    let mut out = vec![line(
        table_row(
            ' ',
            &[
                ("#", 3),
                ("word", 18),
                ("tag", 13),
                ("by", 8),
                ("deslag reads", reads),
                ("origin", 8),
            ],
            width,
        ),
        Look::Underlined,
    )];
    let room = height.saturating_sub(1);
    let first = (session.cursor + 1).saturating_sub(room);
    for (at, row) in sentence.rows.iter().enumerate().skip(first).take(room) {
        let mark = if at == session.cursor { '>' } else { ' ' };
        let number = (at + 1).to_string();
        let (text, look) = match row {
            Row::Word(word) => {
                let tag = word.tag.map_or("?".to_string(), |tag| tag.to_string());
                let by = if word.set {
                    "owner*".to_string()
                } else if word.prefilled {
                    "prefill".to_string()
                } else {
                    word.prov.clone().unwrap_or_else(|| "-".into())
                };
                let reading = word.guess.map_or(String::new(), |guess| {
                    let mut text = format!("{} {}", guess.code, guess.confidence.name());
                    let others: Vec<&str> = guess.others().map(|base| base.code()).collect();
                    if !others.is_empty() {
                        text.push_str(&format!("  also {}", others.join(" ")));
                    }
                    text
                });
                let origin = match word.origin.name() {
                    "English" => "",
                    other => other,
                };
                let look = if word.tag.is_none() {
                    Look::Bold
                } else {
                    Look::Plain
                };
                let cells = [
                    (number.as_str(), 3),
                    (word.form.as_str(), 18),
                    (tag.as_str(), 13),
                    (by.as_str(), 8),
                    (reading.as_str(), reads),
                    (origin, 8),
                ];
                (table_row(mark, &cells, width), look)
            }
            Row::Other(other) => {
                let kind = format!("({})", kind_name(other.kind).to_lowercase());
                let cells = [
                    (number.as_str(), 3),
                    (other.form.as_str(), 18),
                    (kind.as_str(), 13),
                ];
                (table_row(mark, &cells, width), Look::Dim)
            }
        };
        let look = if at == session.cursor {
            Look::Reversed
        } else {
            look
        };
        out.push(line(text, look));
    }
    out
}

/// The screen for `session`, `width` columns by `height` lines: exactly `height` lines.
pub fn lines(session: &Session, width: u16, height: u16) -> Vec<Line> {
    let (width, height) = (usize::from(width.max(1)), usize::from(height));
    let sentence = session.sentence();
    let mut out = Vec::new();

    let reviewed = sentence
        .reviewed
        .as_deref()
        .map_or("not reviewed".to_string(), |date| {
            format!("reviewed {date}")
        });
    out.push(line(
        clip(
            &format!(
                "sentence {} of {}  {}  {}  ({} of {} done)",
                session.at + 1,
                session.sentences.len(),
                sentence.context.name(),
                reviewed,
                session.reviewed(),
                session.sentences.len()
            ),
            width,
        ),
        Look::Bold,
    ));
    let mut text = wrap(&sentence.text, width);
    text.truncate(4);
    out.extend(text.into_iter().map(|text| line(text, Look::Plain)));

    let keys: Vec<Line> = {
        let mut keys = wrap(KEYS, width);
        keys.truncate(2);
        keys.into_iter().map(|text| line(text, Look::Dim)).collect()
    };
    let body = height.saturating_sub(out.len() + 1 + keys.len()).max(3);
    if let Mode::Guide(base) = &session.mode {
        let entry = guide::entry(*base).unwrap_or_else(|| "the guide has no entry".into());
        let title = "guide; any key returns";
        out.push(line(
            clip(&format!("-- {title} {}", "-".repeat(width)), width),
            Look::Plain,
        ));
        let mut entry = wrap(&entry, width);
        entry.truncate(body - 1);
        out.extend(entry.into_iter().map(|text| line(text, Look::Plain)));
    } else {
        out.extend(table(session, width, body));
    }
    out.truncate(height.saturating_sub(1 + keys.len()));
    while out.len() + 1 + keys.len() < height {
        out.push(line("", Look::Plain));
    }

    let notice = match &session.mode {
        Mode::Prompt(typed) => format!(
            "tag> {typed}_   (a base, then .feature as in n.s or v.pp; Enter sets, Esc cancels)"
        ),
        _ => session.notice.clone(),
    };
    out.push(line(clip(&notice, width), Look::Plain));
    out.extend(keys);
    out.truncate(height);
    out
}

/// One screen of `session`, `width` columns by `height` rows, as text.
pub fn text(session: &Session, width: u16, height: u16) -> String {
    let mut out = String::new();
    for line in lines(session, width, height) {
        out.push_str(line.text.trim_end());
        out.push('\n');
    }
    out
}

/// Draws the screen for `session` on a terminal of `width` by `height`.
pub fn draw(
    out: &mut impl Write,
    session: &Session,
    width: u16,
    height: u16,
) -> std::io::Result<()> {
    for (row, line) in lines(session, width, height).iter().enumerate() {
        queue!(out, MoveTo(0, row as u16))?;
        match line.look {
            Look::Plain => {}
            Look::Bold => queue!(out, SetAttribute(Attribute::Bold))?,
            Look::Dim => queue!(out, SetAttribute(Attribute::Dim))?,
            Look::Underlined => queue!(out, SetAttribute(Attribute::Underlined))?,
            Look::Reversed => queue!(out, SetAttribute(Attribute::Reverse))?,
        }
        queue!(
            out,
            Print(&line.text),
            SetAttribute(Attribute::Reset),
            Clear(ClearType::UntilNewLine)
        )?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::Key;
    use crate::review::tests::{Memory, SKELETON, fill, open, type_code};

    #[test]
    fn a_skeleton_draws_its_words_readings_origins_and_keys() {
        let session = open(SKELETON);
        let screen = text(&session, 100, 20);
        assert!(screen.contains("sentence 1 of 2"), "{screen}");
        assert!(screen.contains("Run `cargo` now, then stop."), "{screen}");
        assert!(screen.contains("deslag reads"), "{screen}");
        assert!(
            screen.contains("(code)") && screen.contains("(punctuation)"),
            "{screen}"
        );
        assert!(
            screen.contains("Sure") || screen.contains("Unsure"),
            "{screen}"
        );
        assert!(screen.contains("n/p save"), "{screen}");
        assert!(screen.contains("(punctuation)"), "{screen}");
        assert_eq!(screen.lines().count(), 20);
        let second = {
            let mut session = open(SKELETON);
            let mut store = Memory::default();
            fill(&mut session, &mut store);
            session.press(Key::Char('n'), &mut store);
            text(&session, 100, 20)
        };
        assert!(second.contains("Path"), "the origin is shown:\n{second}");
        assert!(
            second.contains("owner*") || second.contains("sentence 2 of 2"),
            "{second}"
        );
    }

    #[test]
    fn the_prompt_and_the_guide_replace_the_notice_and_the_table() {
        let mut session = open(SKELETON);
        let mut store = Memory::default();
        session.cursor = 0;
        session.press(Key::Char('t'), &mut store);
        session.press(Key::Char('n'), &mut store);
        assert!(text(&session, 100, 20).contains("tag> n_"));
        session.press(Key::Char('?'), &mut store);
        let screen = text(&session, 100, 20);
        assert!(screen.contains("common noun"), "{screen}");
        assert!(!screen.contains("deslag reads"), "{screen}");
        session.press(Key::Esc, &mut store);
        type_code(&mut session, &mut store, "d");
        assert!(text(&session, 100, 20).contains("owner*"));
    }

    #[test]
    fn a_narrow_terminal_still_draws() {
        let session = open(SKELETON);
        let screen = text(&session, 40, 12);
        assert!(screen.contains("sentence 1 of 2"), "{screen}");
    }
}
