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

/// The signals that end the review from outside, which would leave the terminal raw.
#[cfg(unix)]
struct Signals(std::sync::Arc<std::sync::atomic::AtomicUsize>);

#[cfg(unix)]
impl Signals {
    fn watch() -> std::io::Result<Signals> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for signal in [SIGTERM, SIGHUP, SIGINT] {
            signal_hook::flag::register_usize(signal, seen.clone(), signal as usize)?;
        }
        Ok(Signals(seen))
    }

    /// The signal that arrived, if one did.
    fn arrived(&self) -> Option<usize> {
        match self.0.load(std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            signal => Some(signal),
        }
    }
}

#[cfg(not(unix))]
struct Signals;

#[cfg(not(unix))]
impl Signals {
    fn watch() -> std::io::Result<Signals> {
        Ok(Signals)
    }

    fn arrived(&self) -> Option<usize> {
        None
    }
}

fn stopped(signal: usize) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        format!("stopped by signal {signal}"),
    )
}

/// What a key press asks of the session, or `None` for a key the review ignores. Control and
/// Alt letters are not letters: Ctrl-N must not act as `n`. Ctrl-C asks to quit.
fn key_of(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        return Some(Key::Interrupt);
    }
    match code {
        KeyCode::Char(c) if (modifiers - KeyModifiers::SHIFT).is_empty() => Some(Key::Char(c)),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Esc),
        KeyCode::Backspace => Some(Key::Backspace),
        _ => None,
    }
}

/// The keys in, the screen out, until the owner quits or the process is told to stop.
fn interact(session: &mut Session, store: &mut FileStore) -> std::io::Result<()> {
    let signals = Signals::watch()?;
    enter()?;
    let result = (|| {
        let mut out = std::io::stdout();
        loop {
            let (width, height) = size()?;
            screen::draw(&mut out, session, width, height)?;
            // The wait ends every so often, so a signal is noticed.
            while !event::poll(std::time::Duration::from_millis(100))? {
                if let Some(signal) = signals.arrived() {
                    return Err(stopped(signal));
                }
            }
            if let Some(signal) = signals.arrived() {
                return Err(stopped(signal));
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let Some(key) = key_of(key.code, key.modifiers) else {
                continue;
            };
            if session.press(key, store) == Outcome::Quit {
                return Ok(());
            }
        }
    })();
    leave();
    result
}

/// A file saved by writing beside it and renaming over it. A link is followed, so the file it
/// points to is the one replaced and the link stays a link. The new file has the mode of the old,
/// and the directory is synced after the rename, so a saved sentence survives a power cut.
pub struct FileStore {
    pub path: PathBuf,
}

impl Store for FileStore {
    fn save(&mut self, text: &str) -> Result<Option<String>, String> {
        match write_beside(&self.path, text) {
            Ok(Written::Synced) => Ok(None),
            Ok(Written::NotSynced(error)) => Ok(Some(format!(
                "the file is replaced, but syncing its directory failed: {error}"
            ))),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// How a write ended, once the file was replaced.
enum Written {
    /// The file and its directory are on disk.
    Synced,
    /// The file is replaced, but the directory could not be synced, so a power cut might undo it.
    NotSynced(std::io::Error),
}

/// Replaces the file `path` names (the target of a link) with `text`, all or nothing. An error
/// means the file is as it was; once the rename is done, a failed directory sync is only a
/// warning.
fn write_beside(path: &Path, text: &str) -> std::io::Result<Written> {
    write_beside_with(path, text, sync_directory)
}

/// [`write_beside`], syncing the directory with `sync`.
fn write_beside_with(
    path: &Path,
    text: &str,
    sync: impl Fn(&Path) -> std::io::Result<()>,
) -> std::io::Result<Written> {
    // A file that is not there yet is made, in a directory that is.
    let target = match std::fs::canonicalize(path) {
        Ok(target) => target,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error),
    };
    let mode = std::fs::metadata(&target)
        .ok()
        .map(|meta| meta.permissions());
    let name = target
        .file_name()
        .map_or("review".into(), |name| name.to_string_lossy().into_owned());
    let temporary = target.with_file_name(format!(".{name}.review"));
    let write = || -> std::io::Result<()> {
        let mut file = create_new(&temporary)?;
        if let Some(mode) = &mode {
            file.set_permissions(mode.clone())?;
        }
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, &target)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })?;
    Ok(match sync(&target) {
        Ok(()) => Written::Synced,
        Err(error) => Written::NotSynced(error),
    })
}

/// Creates `path`, which must not exist. A leftover of a crash, or a link planted under the name,
/// is removed first (the link itself, never what it points to), so the file written is new.
fn create_new(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    match options.open(path) {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(path)?;
            options.open(path)
        }
        other => other,
    }
}

/// Makes the rename of `file` durable, where a directory can be opened and synced.
#[cfg(unix)]
fn sync_directory(file: &Path) -> std::io::Result<()> {
    match file.parent() {
        Some(parent) => std::fs::File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn sync_directory(_file: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Today's date in UTC, `YYYY-MM-DD`.
pub fn today() -> String {
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
        // A file that is not there yet is made whole, with no temporary left.
        let fresh = dir.path().join("fresh.conllu");
        FileStore {
            path: fresh.clone(),
        }
        .save("made")
        .unwrap();
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "made");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        let mut gone = FileStore {
            path: dir.path().join("missing/owner.conllu"),
        };
        assert!(gone.save("x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn saving_keeps_the_mode_and_writes_through_a_link_to_its_target() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let target = real.join("owner.conllu");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).unwrap();
        let link = dir.path().join("link.conllu");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut store = FileStore { path: link.clone() };
        store.save("new").unwrap();
        assert!(
            std::fs::symlink_metadata(&link).unwrap().is_symlink(),
            "the link is still a link"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o750);
        let left: Vec<_> = std::fs::read_dir(&real).unwrap().collect();
        assert_eq!(left.len(), 1, "no temporary is left");
        // A plain file keeps a restrictive mode too.
        let plain = dir.path().join("plain.conllu");
        std::fs::write(&plain, "old").unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o600)).unwrap();
        FileStore {
            path: plain.clone(),
        }
        .save("new")
        .unwrap();
        let mode = std::fs::metadata(&plain).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_failed_directory_sync_after_the_rename_is_a_warning_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.conllu");
        std::fs::write(&path, "old").unwrap();
        let written =
            write_beside_with(&path, "new", |_| Err(std::io::Error::other("no sync here")))
                .unwrap();
        assert!(matches!(written, Written::NotSynced(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_link_under_the_temporary_name_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.conllu");
        std::fs::write(&path, "old").unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join(".owner.conllu.review")).unwrap();
        FileStore { path: path.clone() }.save("new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    }

    #[test]
    fn control_letters_are_not_letters() {
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(key_of(KeyCode::Char('n'), none), Some(Key::Char('n')));
        assert_eq!(
            key_of(KeyCode::Char('N'), KeyModifiers::SHIFT),
            Some(Key::Char('N'))
        );
        assert_eq!(key_of(KeyCode::Char('n'), ctrl), None);
        assert_eq!(key_of(KeyCode::Char('q'), ctrl), None);
        assert_eq!(key_of(KeyCode::Char('p'), KeyModifiers::ALT), None);
        assert_eq!(key_of(KeyCode::Char('c'), ctrl), Some(Key::Interrupt));
        assert_eq!(key_of(KeyCode::Enter, none), Some(Key::Enter));
        assert_eq!(key_of(KeyCode::Left, none), None);
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
