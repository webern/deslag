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
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::guide;
use crate::review::{Mode, Row, Session};

/// The keys, as the footer lists them.
const KEYS: &str =
    "j/k move, t tag, ? guide, a accept, n/p save and go to the next or previous sentence, q quit";

/// The least screen the review draws; under it a message says so.
const MIN_WIDTH: usize = 30;
/// See [`MIN_WIDTH`].
const MIN_HEIGHT: usize = 8;

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

/// `text` with every control character, ESC among them, shown as U+FFFD, so what a file holds
/// can never move the cursor or recolour the screen.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

/// How many bytes of `text` fit in `width` terminal cells. A mark that combines with the
/// character before it stays with it, and a joiner is never left at the end.
fn fit(text: &str, width: usize) -> usize {
    let mut used = 0;
    let mut end = text.len();
    for (at, c) in text.char_indices() {
        let cells = c.width().unwrap_or(0);
        if used + cells > width {
            end = at;
            break;
        }
        used += cells;
    }
    let mut kept = &text[..end];
    while end < text.len() && kept.ends_with('\u{200D}') {
        kept = &kept[..kept.len() - '\u{200D}'.len_utf8()];
    }
    kept.len()
}

/// `text`, cleaned of control characters, cut to `width` terminal cells.
fn clip(text: &str, width: usize) -> String {
    let text = clean(text);
    text[..fit(&text, width)].to_string()
}

/// `text` cut or padded with spaces to exactly `width` terminal cells.
fn cell(text: &str, width: usize) -> String {
    let mut cell = clip(text, width);
    let short = width.saturating_sub(cell.width());
    cell.extend(std::iter::repeat_n(' ', short));
    cell
}

/// `text` broken at spaces into lines of at most `width` terminal cells; a word longer than that
/// is cut.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let word = clean(word);
            let mut word = word.as_str();
            while word.width() > width {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                let first = word.chars().next().map_or(0, char::len_utf8);
                let cut = fit(word, width).max(first);
                out.push(clip(&word[..cut], width));
                word = &word[cut..];
            }
            let joined = current.width() + usize::from(!current.is_empty());
            if !current.is_empty() && joined + word.width() > width {
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
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        let mut out = vec![line(
            clip(
                &format!("terminal too small, needs {MIN_WIDTH}x{MIN_HEIGHT}"),
                width,
            ),
            Look::Plain,
        )];
        out.resize(height, line("", Look::Plain));
        return out;
    }
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
    let keys = wrap(KEYS, width);
    let keys_max = if height >= 16 { 4 } else { 2 };
    let mut text = wrap(&sentence.text, width);
    // The table needs its header and two words; the text and the keys give way, the text down to
    // one line and the keys down to one.
    let spare = height.saturating_sub(2);
    let mut text_lines = text.len().min(4).min((height / 4).max(1));
    let mut key_lines = keys.len().min(keys_max);
    while key_lines + text_lines + 3 > spare {
        if text_lines > 1 {
            text_lines -= 1;
        } else if key_lines > 1 {
            key_lines -= 1;
        } else {
            break;
        }
    }
    if text.len() > text_lines {
        // A sentence that does not fit ends in an ellipsis.
        text.truncate(text_lines);
        if let Some(last) = text.last_mut() {
            *last = format!("{}\u{2026}", clip(last, width.saturating_sub(1)));
        }
    }
    out.extend(text.into_iter().map(|text| line(text, Look::Plain)));
    let keys: Vec<Line> = keys
        .into_iter()
        .take(key_lines)
        .map(|text| line(text, Look::Dim))
        .collect();

    let body = height.saturating_sub(out.len() + 1 + keys.len());
    if let Mode::Guide(base) = &session.mode {
        let entry = guide::entry(*base).unwrap_or_else(|| "the guide has no entry".into());
        let title = "guide; any key returns";
        out.push(line(
            clip(&format!("-- {title} {}", "-".repeat(width)), width),
            Look::Plain,
        ));
        let mut entry = wrap(&entry, width);
        entry.truncate(body.saturating_sub(1));
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
        // The line is cleared first: one as wide as the screen leaves the cursor pending a wrap,
        // where some terminals erase the last cell.
        queue!(out, MoveTo(0, row as u16), Clear(ClearType::UntilNewLine))?;
        match line.look {
            Look::Plain => {}
            Look::Bold => queue!(out, SetAttribute(Attribute::Bold))?,
            Look::Dim => queue!(out, SetAttribute(Attribute::Dim))?,
            Look::Underlined => queue!(out, SetAttribute(Attribute::Underlined))?,
            Look::Reversed => queue!(out, SetAttribute(Attribute::Reverse))?,
        }
        queue!(out, Print(&line.text), SetAttribute(Attribute::Reset))?;
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

    /// A skeleton of one sentence of `words` words, `w1 w2 ...`.
    fn long(words: usize) -> String {
        let text: Vec<String> = (1..=words).map(|n| format!("w{n}")).collect();
        let mut out = format!(
            "# exam.tokens = deslag\n# sent_id = s1\n# exam.context = prose\n# text = {}\n",
            text.join(" ")
        );
        for (at, word) in text.iter().enumerate() {
            out.push_str(&format!(
                "{}\t{word}\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n",
                at + 1
            ));
        }
        out
    }

    #[test]
    fn control_characters_are_shown_as_replacements_never_written() {
        let mut source = SKELETON.replace("Run `cargo` now", "Run \u{1b}]0;PWNED\u{7} now");
        source = source.replace("\t`cargo`\t", "\t`ca\u{1b}[2Jrgo`\t");
        let mut session = open(&source);
        let mut store = Memory::default();
        session.press(Key::Char('t'), &mut store);
        session.press(Key::Char('\u{1b}'), &mut store);
        for height in [12, 24] {
            let lines = lines(&session, 100, height);
            for line in &lines {
                assert!(!line.text.chars().any(char::is_control), "{:?}", line.text);
            }
        }
        let screen = text(&session, 100, 24);
        assert!(screen.contains("\u{FFFD}]0;PWNED\u{FFFD}"), "{screen}");
        assert!(screen.contains("`ca\u{FFFD}[2Jrgo`"), "{screen}");
        let mut raw = Vec::new();
        draw(&mut raw, &session, 100, 24).unwrap();
        let raw = String::from_utf8(raw).unwrap();
        assert!(!raw.contains('\u{7}'), "{raw:?}");
        assert!(
            !raw.contains("\u{1b}[2J") && !raw.contains("\u{1b}]"),
            "{raw:?}"
        );
    }

    #[test]
    fn columns_count_terminal_cells() {
        assert_eq!(cell("日本", 6), "日本  ");
        assert_eq!(cell("日本語", 5), "日本 ");
        assert_eq!(cell("e\u{301}\u{301}", 3).width(), 3);
        assert_eq!(clip("e\u{301}x", 1), "e\u{301}");
        assert_eq!(clip("a\u{1F468}\u{200D}\u{1F469}", 3), "a\u{1F468}");
        for line in wrap("日本語のテスト 日本語のテスト ab", 7) {
            assert!(line.width() <= 7, "{line:?}");
        }
        assert!(wrap("日", 1).iter().all(|line| line.width() <= 1));
        let row = table_row('>', &[("日本語", 6), ("x", 2)], 40);
        assert_eq!(row, ">日本語 x");
    }

    #[test]
    fn a_small_terminal_keeps_the_cursor_row_in_view() {
        let mut session = open(&long(40));
        let mut store = Memory::default();
        for _ in 0..25 {
            session.press(Key::Down, &mut store);
        }
        assert_eq!(session.cursor, 25);
        for (width, height) in [(60, 8), (60, 10), (60, 12), (30, 9), (100, 24), (45, 40)] {
            let screen = text(&session, width, height);
            assert_eq!(screen.lines().count(), usize::from(height), "{screen}");
            let marked: Vec<&str> = screen.lines().filter(|l| l.starts_with('>')).collect();
            assert_eq!(marked.len(), 1, "{width}x{height}\n{screen}");
            assert!(marked[0].contains("26"), "{width}x{height}\n{screen}");
            assert!(
                screen.lines().next().unwrap().starts_with("sentence 1"),
                "{screen}"
            );
        }
    }

    #[test]
    fn a_terminal_under_the_least_says_so() {
        let session = open(SKELETON);
        let screen = text(&session, 20, 6);
        assert!(screen.contains("terminal too small"), "{screen}");
        assert_eq!(screen.lines().count(), 6);
    }

    #[test]
    fn a_narrow_terminal_still_draws() {
        let session = open(SKELETON);
        let screen = text(&session, 40, 12);
        assert!(screen.contains("sentence 1 of 2"), "{screen}");
    }
}
