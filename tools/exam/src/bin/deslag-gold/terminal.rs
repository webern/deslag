//! The terminal around the review: reading keys, saving the file, and the date.
//!
//! Nothing here decides what a key does. It turns the terminal's events into [`Key`]s for the
//! [`Session`], draws what the session shows, and writes a sentence's file through a temporary
//! file and a rename, so a crash leaves the file as it was or as it is now, never half-written.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use deslag_exam::error::{Error, Place};

use crate::data::read_text;
use crate::problems::Problems;
use crate::review::{Key, Outcome, Session, Store, refusal, refuses_path};
use crate::screen;

/// Opens `file` for review. With `screen_only` it prints the first screen as text and changes
/// nothing, which is how to see the review with no terminal.
pub fn run(file: &Path, screen_only: bool) -> Result<(), Problems> {
    let shown = file.display().to_string();
    // The path decides before the file is read; so does the canonical path, for a link.
    let canonical = std::fs::canonicalize(file).ok();
    if refuses_path(file) || canonical.as_deref().is_some_and(refuses_path) {
        return Err(refusal(&shown).into());
    }
    let source = read_text(file)?;
    let mut session = Session::open(&shown, source, &today())?;
    if screen_only {
        print!("{}", screen::text(&session, 100, 24));
        return Ok(());
    }
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err(Error::load(
            &shown,
            Place::File,
            "the review needs a terminal; --screen prints one screen as text",
        )
        .into());
    }
    let mut store = FileStore {
        path: file.to_path_buf(),
    };
    let result = interact(&mut session, &mut store);
    let done = session.reviewed();
    eprintln!(
        "{done} of {} sentences reviewed in {shown}",
        session.sentences.len()
    );
    if session.unsaved() {
        eprintln!("the tags set in sentence {} were not saved", session.at + 1);
    }
    result.map_err(|source| {
        Error::Io {
            path: shown,
            source,
        }
        .into()
    })
}

/// Raw mode and the alternate screen on, until [`leave`] puts the terminal back.
fn enter() -> std::io::Result<()> {
    enable_raw_mode()?;
    if let Err(error) = execute!(std::io::stdout(), EnterAlternateScreen, Hide) {
        leave();
        return Err(error);
    }
    // A panic prints on the screen the review is drawn on, which is then lost, so the terminal
    // goes back before the message is printed.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave();
        hook(info);
    }));
    Ok(())
}

/// Puts the terminal back as it was. It may be called twice.
fn leave() {
    let _ = execute!(std::io::stdout(), Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

/// The keys in, the screen out, until the owner quits.
fn interact(session: &mut Session, store: &mut FileStore) -> std::io::Result<()> {
    enter()?;
    let result = (|| {
        let mut out = std::io::stdout();
        loop {
            let (width, height) = size()?;
            screen::draw(&mut out, session, width, height)?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(());
            }
            let key = match key.code {
                KeyCode::Char(c) => Key::Char(c),
                KeyCode::Down => Key::Down,
                KeyCode::Up => Key::Up,
                KeyCode::Right => Key::Right,
                KeyCode::Left => Key::Left,
                KeyCode::Enter => Key::Enter,
                KeyCode::Esc => Key::Esc,
                KeyCode::Backspace => Key::Backspace,
                _ => continue,
            };
            if session.press(key, store) == Outcome::Quit {
                return Ok(());
            }
        }
    })();
    leave();
    result
}

/// A file saved by writing beside it and renaming over it.
struct FileStore {
    path: PathBuf,
}

impl Store for FileStore {
    fn save(&mut self, text: &str) -> Result<(), String> {
        let name = self
            .path
            .file_name()
            .map_or("review".into(), |name| name.to_string_lossy().into_owned());
        let temporary = self.path.with_file_name(format!(".{name}.review"));
        let write = || -> std::io::Result<()> {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)
        };
        write().map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            error.to_string()
        })
    }
}

/// Today's date in UTC, `YYYY-MM-DD`.
fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    date_of_days((seconds / 86_400) as i64)
}

/// The civil date of `days` after 1970-01-01.
fn date_of_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_since_the_epoch_are_dates() {
        assert_eq!(date_of_days(0), "1970-01-01");
        assert_eq!(date_of_days(59), "1970-03-01");
        assert_eq!(date_of_days(11_016), "2000-02-29");
        assert_eq!(date_of_days(20_732), "2026-10-06");
    }

    #[test]
    fn the_file_store_replaces_the_file_and_leaves_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.conllu");
        std::fs::write(&path, "old").unwrap();
        let mut store = FileStore { path: path.clone() };
        store.save("new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        let mut gone = FileStore {
            path: dir.path().join("missing/owner.conllu"),
        };
        assert!(gone.save("x").is_err());
    }

    #[test]
    fn a_refused_path_is_refused_before_it_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("holdout.conllu");
        std::fs::write(&path, "secret words").unwrap();
        let error = run(&path, true).unwrap_err().to_string();
        assert!(error.contains("does not open holdout"), "{error}");
        assert!(!error.contains("secret"), "{error}");
    }

    #[test]
    fn screen_only_prints_a_screen_from_a_file_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.conllu");
        std::fs::write(&path, crate::review::tests::SKELETON).unwrap();
        run(&path, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            crate::review::tests::SKELETON
        );
    }
}
