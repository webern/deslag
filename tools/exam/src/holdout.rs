//! What runs on holdout text: the boundary every command that reads it stays inside, and `score
//! --holdout`.
//!
//! A panic message can quote the text it choked on, as a bad `str` slice does, and a load error
//! can echo a comment. So a command that reads holdout text does all of its work inside
//! [`withholding`], which catches a panic and says one fixed line in its place, and reads the file
//! with [`Gold::read_holdout`], whose errors name no key and no value. `gate` runs its holdout set
//! this way (`crate::gate`), and so does `score --holdout` ([`score_report`]).

use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use crate::align::align_all;
use crate::disputes::Disputes;
use crate::error::Error;
use crate::gold::{Gold, Split};
use crate::report;
use crate::saved::SavedRun;
use crate::score::Input;

/// What the shipped holdout line says when reading or tagging a set panics.
pub const PANIC: &str = "holdout: reading or tagging the set panicked; the message is withheld because it may quote the text";

/// Runs `work`, which reads and scores holdout text, with its panic caught and silent. The hook
/// that prints a panic is replaced for the run and put back after. It is process-wide: call this
/// once at a time.
pub fn withholding<T>(work: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let caught = panic::catch_unwind(AssertUnwindSafe(work));
    panic::set_hook(hook);
    match caught {
        Ok(result) => result,
        Err(_) => Err(Error::Cannot(PANIC.to_string())),
    }
}

/// The aggregate report of `input` on the holdout gold at `gold`, as `score --holdout` prints it,
/// after writing the run to `save` if given. The disputes, the gold, the import, the alignment,
/// the scoring, the save and the report are all inside [`withholding`], so a panic anywhere is the
/// one fixed line, and no error names a word or a comment.
///
/// The gold may say `exam.split = holdout` or have no header, as the treebank's test file has none;
/// one that names another split is the wrong file, and is refused.
pub fn score_report(
    gold: &Path,
    disputes: Option<&Path>,
    input: &Input<'_>,
    save: Option<&Path>,
    words: usize,
) -> Result<String, Error> {
    withholding(|| {
        let disputes = Disputes::read(gold, disputes)?;
        let read = Gold::read_holdout(gold)?;
        if let Some(split) = read.split
            && split != Split::Holdout
        {
            return Err(Error::Cannot(format!(
                "{}: --holdout reads a holdout gold, and this one says `exam.split = {}`",
                gold.display(),
                split.name()
            )));
        }
        let aligned = align_all(&read);
        let scoring = input.score(&read, &aligned, false)?;
        // The run is saved before the report is returned, as `score` does.
        if let Some(path) = save {
            SavedRun::of(&read, &scoring).write(path)?;
        }
        Ok(report::render(
            &read, &aligned, &disputes, &scoring, false, words,
        ))
    })
}

/// The panic hook is process-wide, and [`withholding`] swaps it: a test that runs it or reads the
/// hook holds this.
#[cfg(test)]
static HOOK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Takes [`HOOK`], whether or not a failed test poisoned it.
#[cfg(test)]
pub(crate) fn alone() -> std::sync::MutexGuard<'static, ()> {
    HOOK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn a_panic_in_the_boundary_is_withheld_every_time() {
        let _alone = alone();
        let sentinel = "secret-sentinel";
        for _ in 0..2 {
            let error =
                withholding(|| -> Result<(), Error> { panic!("cannot slice `{sentinel}`") });
            let Err(Error::Cannot(message)) = error else {
                panic!("a panic is Error::Cannot, not {error:?}");
            };
            assert_eq!(message, PANIC);
            assert!(!message.contains(sentinel), "{message}");
        }
        // An error and a count cross the boundary as they are.
        assert_eq!(withholding(|| Ok(vec![1, 2])).unwrap(), [1, 2]);
        let error = withholding(|| -> Result<(), Error> { Err(Error::Cannot("kept".to_string())) });
        assert_eq!(error.unwrap_err().to_string(), "kept");
    }

    #[test]
    fn the_hook_in_place_before_the_boundary_is_in_place_after_it() {
        let _alone = alone();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let before = panic::take_hook();
        let marker = Arc::clone(&heard);
        panic::set_hook(Box::new(move |info| {
            // Only the panics of this test count: others run beside it and may panic too.
            if let Some(message) = info.payload().downcast_ref::<&str>()
                && message.starts_with("marker:")
            {
                marker.lock().unwrap().push(message.to_string());
            }
        }));
        let inside = withholding(|| -> Result<(), Error> { panic!("marker: inside") });
        let _ = panic::catch_unwind(|| panic!("marker: after"));
        // Put the hook back first, so a failed assertion below is printed.
        panic::set_hook(before);
        assert!(inside.is_err());
        assert_eq!(
            *heard.lock().unwrap(),
            ["marker: after"],
            "the boundary silences the hook inside it, and puts the one it found back"
        );
    }
}
