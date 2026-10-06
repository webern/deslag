//! What the review looks like: a [`Session`] drawn on a ratatui frame.
//!
//! Drawing reads the session and decides nothing. [`text`] draws one screen on a backend that
//! holds a grid and no terminal, so a screen can be tested and shown as text.

use deslag_exam::gold::kind_name;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Wrap};

use crate::guide;
use crate::review::{Mode, Row, Session};

/// The keys, as the footer lists them.
const KEYS: &str = "j/k move  t tag  ? guide  a accept  n/p save, next/previous sentence  q quit";

/// Draws `session` on `frame`.
pub fn draw(frame: &mut Frame<'_>, session: &Session) {
    let sentence = session.sentence();
    let area = frame.area();
    let text_lines = (sentence.text.chars().count() as u16 / area.width.max(1) + 1).clamp(1, 4);
    let [head, text, body, notice, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(text_lines),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(area);

    let reviewed = sentence
        .reviewed
        .as_deref()
        .map_or("not reviewed".to_string(), |date| {
            format!("reviewed {date}")
        });
    frame.render_widget(
        Paragraph::new(format!(
            "sentence {} of {}  {}  {}  ({} of {} done)",
            session.at + 1,
            session.sentences.len(),
            sentence.context.name(),
            reviewed,
            session.reviewed(),
            session.sentences.len()
        ))
        .style(Style::new().add_modifier(Modifier::BOLD)),
        head,
    );
    frame.render_widget(
        Paragraph::new(sentence.text.as_str()).wrap(Wrap { trim: true }),
        text,
    );

    if let Mode::Guide(base) = &session.mode {
        let entry = guide::entry(*base).unwrap_or_else(|| "the guide has no entry".into());
        frame.render_widget(
            Paragraph::new(entry).wrap(Wrap { trim: true }).block(
                Block::new()
                    .borders(Borders::TOP)
                    .title("guide; any key returns"),
            ),
            body,
        );
    } else {
        let rows = sentence.rows.iter().enumerate().map(|(at, row)| match row {
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
                let style = if word.tag.is_none() {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                };
                TableRow::new([
                    Cell::from((at + 1).to_string()),
                    Cell::from(word.form.clone()),
                    Cell::from(tag),
                    Cell::from(by),
                    Cell::from(reading),
                    Cell::from(origin),
                ])
                .style(style)
            }
            Row::Other(other) => TableRow::new([
                Cell::from((at + 1).to_string()),
                Cell::from(other.form.clone()),
                Cell::from(format!("({})", kind_name(other.kind).to_lowercase())),
            ])
            .style(Style::new().add_modifier(Modifier::DIM)),
        });
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(18),
                Constraint::Length(13),
                Constraint::Length(8),
                Constraint::Min(20),
                Constraint::Length(8),
            ],
        )
        .header(
            TableRow::new(["#", "word", "tag", "by", "deslag reads", "origin"])
                .style(Style::new().add_modifier(Modifier::UNDERLINED)),
        )
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol(">");
        let mut state = TableState::default().with_selected(Some(session.cursor));
        frame.render_stateful_widget(table, body, &mut state);
    }

    let line = match &session.mode {
        Mode::Prompt(typed) => format!(
            "tag> {typed}_   (a base, then .feature as in n.s or v.pp; Enter sets, Esc cancels)"
        ),
        _ => session.notice.clone(),
    };
    frame.render_widget(Paragraph::new(line), notice);
    frame.render_widget(
        Paragraph::new(KEYS)
            .wrap(Wrap { trim: true })
            .style(Style::new().add_modifier(Modifier::DIM)),
        keys,
    );
}

/// One screen of `session`, `width` columns by `height` rows, as text.
pub fn text(session: &Session, width: u16, height: u16) -> String {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test backend cannot fail");
    terminal
        .draw(|frame| draw(frame, session))
        .expect("a test backend cannot fail");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buffer[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
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
