//! Tests for `deslag fix`: which edits `Document::apply` proves and which it refuses, and what the
//! command writes, prints and exits with. The cases under `tests/cases/banned_chars/fix_*` pin its
//! report; these pin the bytes it writes.

mod common;

use std::fs;

use common::{Repo, code, stderr, stdout};
use deslag::Document;
use deslag::document::{Edit, Refusal};

/// A config that turns on `banned_chars` with its default groups.
const CONFIG: &str = "schema_version = 1\n\n[md.lints.banned_chars]\n";

/// Replaces every `from` in `text` with `to` through [`Document::apply`]. Returns the text made
/// and why each edit was refused.
fn replace(text: &str, from: &str, to: &str) -> (String, Vec<Option<Refusal>>) {
    let edits: Vec<Edit> = text
        .match_indices(from)
        .map(|(at, found)| Edit {
            range: at..at + found.len(),
            replacement: to.to_string(),
        })
        .collect();
    assert!(!edits.is_empty(), "{from:?} is not in {text:?}");
    let applied = Document::markdown(text)
        .apply(&edits)
        .expect("edits apart and on characters");
    (applied.text, applied.refused)
}

/// Why each edit that replaces a `from` in `text` with `to` is refused.
fn refused(text: &str, from: &str, to: &str) -> Vec<Option<Refusal>> {
    replace(text, from, to).1
}

/// A repo with the default config and `README.md` holding `text`.
fn repo_with(text: &str) -> Repo {
    let repo = Repo::new();
    repo.write("deslag.toml", CONFIG);
    repo.write("README.md", text);
    repo
}

/// The bytes of `relative` in `repo`.
fn read(repo: &Repo, relative: &str) -> Vec<u8> {
    fs::read(repo.root().join(relative)).expect("a readable file")
}

#[test]
fn an_edit_in_prose_is_made() {
    let text = "# A \u{2014} b\n\nC \u{2014} d, *e \u{2014} f* and [g \u{2014} h](https://x.org).\n\n\
                - i \u{2014} j\n\n> k \u{2014} l\n\n| m \u{2014} n |\n| - |\n| o \u{2014} p |\n";
    let (made, refused) = replace(text, "\u{2014}", "-");
    assert_eq!(made, text.replace('\u{2014}', "-"));
    assert!(refused.iter().all(Option::is_none), "{refused:?}");
}

#[test]
fn an_edit_outside_prose_is_refused() {
    for (text, from, want) in [
        (
            "---\ntitle: a \u{2014} b\n---\n\n# T\n",
            "\u{2014}",
            Refusal::Frontmatter,
        ),
        ("<div>\na \u{2014} b\n</div>\n", "\u{2014}", Refusal::Html),
        (
            "A <span title=\"a\u{2014}b\">c</span>.\n",
            "\u{2014}",
            Refusal::Html,
        ),
        (
            "See https://x.org/a\u{2014}b now.\n",
            "\u{2014}",
            Refusal::Url,
        ),
        (
            "See <https://x.org/a\u{2014}b> now.\n",
            "\u{2014}",
            Refusal::Url,
        ),
        ("A `b \u{2014} c` span.\n", "\u{2014}", Refusal::Markup),
        ("```\na \u{2014} b\n```\n", "\u{2014}", Refusal::Markup),
        ("An &mdash; entity.\n", "&mdash;", Refusal::Markup),
        (
            "[a](https://x.org/b\u{2014}c)\n",
            "\u{2014}",
            Refusal::Markup,
        ),
    ] {
        assert_eq!(refused(text, from, "-"), [Some(want)], "{text:?}");
    }
}

#[test]
fn an_edit_that_leaves_part_of_a_grapheme_is_refused() {
    let text = "Go \u{27A1}\u{FE0F} there.\n";
    assert_eq!(refused(text, "\u{27A1}", "->"), [Some(Refusal::Grapheme)]);
    assert_eq!(
        replace(text, "\u{27A1}\u{FE0F}", "->"),
        ("Go -> there.\n".to_string(), vec![None])
    );

    // Two edits that together cover the whole cluster stand together.
    let at = text.find('\u{27A1}').expect("the arrow");
    let selector = at + '\u{27A1}'.len_utf8();
    let edits = [
        Edit {
            range: at..selector,
            replacement: "->".to_string(),
        },
        Edit {
            range: selector..selector + '\u{FE0F}'.len_utf8(),
            replacement: String::new(),
        },
    ];
    let applied = Document::markdown(text).apply(&edits).expect("edits apart");
    assert_eq!(applied.text, "Go -> there.\n");
    assert_eq!(applied.refused, [None, None]);
}

#[test]
fn a_replacement_with_a_control_character_is_refused() {
    for to in ["\n", "\t", "a\u{7}"] {
        assert_eq!(
            refused("A \u{2014} b.\n", "\u{2014}", to),
            [Some(Refusal::Control)],
            "{to:?}"
        );
    }
}

#[test]
fn an_edit_that_changes_how_the_file_reads_is_refused() {
    for (text, from, to) in [
        // A wrapped line comes to start a list.
        (
            "A line that ends\n\u{2014} and starts with a dash.\n",
            "\u{2014}",
            "-",
        ),
        ("> \u{2014} @someone\n", "\u{2014}", "-"),
        // A tab comes to start the line, which makes a code block.
        ("\u{200B}\tindented\n", "\u{200B}", ""),
        // Two spaces come to end the line, which makes a hard break.
        ("Hard break\u{00A0} \nnext line.\n", "\u{00A0}", " "),
        // A backslash comes to escape the replacement, which then reads without it.
        ("Escaped \\\u{2014} dash.\n", "\u{2014}", "-"),
        // A line of dashes comes to underline the one above as a heading.
        (
            "A line\n\u{2014}\u{2014}\u{2014}\n",
            "\u{2014}\u{2014}\u{2014}",
            "---",
        ),
    ] {
        assert_eq!(
            refused(text, from, to),
            [Some(Refusal::Structure)],
            "{text:?}"
        );
    }

    // A link's text no longer matches its definition, which the edit cannot reach.
    assert_eq!(
        refused(
            "See [a\u{2026}b] here.\n\n[a\u{2026}b]: https://x.org\n",
            "\u{2026}",
            "..."
        ),
        [Some(Refusal::Structure), Some(Refusal::Markup)]
    );
}

#[test]
fn edits_are_tried_one_at_a_time_when_together_they_fail() {
    let text = "A \u{2014} b.\n\u{2014} c.\n\nD \u{2014} e.\n";
    assert_eq!(
        replace(text, "\u{2014}", "-"),
        (
            "A - b.\n\u{2014} c.\n\nD - e.\n".to_string(),
            vec![None, Some(Refusal::Structure), None]
        )
    );
}

#[test]
fn an_edit_a_caller_should_not_make_is_an_error() {
    let text = "A \u{2014} b.\n";
    let document = Document::markdown(text);
    let edit = |range: std::ops::Range<usize>| Edit {
        range,
        replacement: "-".to_string(),
    };
    for (edits, problem) in [
        (vec![edit(2..2)], "covers nothing"),
        (vec![edit(2..3)], "does not start and end on characters"),
        (vec![edit(2..100)], "does not start and end on characters"),
        (vec![edit(2..5), edit(1..5)], "overlaps the one before it"),
    ] {
        let error = document.apply(&edits).expect_err("an error");
        assert!(error.contains(problem), "{error}");
    }
}

#[test]
fn fix_writes_each_replacement_and_nothing_else() {
    let repo = repo_with(
        "\u{FEFF}# Notes\r\n\r\nThe plan \u{2014} in short \u{2014} is \u{201C}simple\u{201D}.\
         \u{00A0}Done\u{2026}\r\n",
    );
    let output = repo.run(&["fix"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        String::from_utf8(read(&repo, "README.md")).expect("UTF-8"),
        "\u{FEFF}# Notes\r\n\r\nThe plan - in short - is \"simple\". Done...\r\n"
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn fix_says_a_deletion_takes_out_only_the_character() {
    let repo = repo_with("# Notes\n\nA zero\u{200B} width space.\n");
    let output = repo.run(&["fix"]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        read(&repo, "README.md"),
        b"# Notes\n\nA zero width space.\n"
    );
    assert_eq!(
        stderr(&output),
        "deslag fixed README.md:\n  line 3: U+200B zero width space; delete it\nA deletion took \
         out the character and nothing else, so check the spaces around it.\n\n"
    );
}

#[test]
fn fix_leaves_what_it_cannot_prove_and_exits_as_check_does() {
    let text = "# Notes\n\nA line that ends\n\u{2014} and starts with a dash.\n";
    let repo = repo_with(text);
    let output = repo.run(&["fix"]);
    assert_eq!(code(&output), 1);
    assert_eq!(read(&repo, "README.md"), text.as_bytes());
    assert!(
        stderr(&output).starts_with(
            "deslag cannot fix these in README.md:\n  line 4: U+2014 em dash \"\u{2014}\"; write \
             `-`\n    the edit changes how the file reads"
        ),
        "{}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("ERROR: deslag detected banned characters!"));
}

#[test]
fn fix_leaves_a_file_that_is_not_utf8() {
    let bytes = b"# Notes\n\nA \xe2\x80\x94 b \xff.\n";
    let repo = Repo::new();
    repo.write("deslag.toml", CONFIG);
    repo.write_bytes("README.md", bytes);
    let output = repo.run(&["fix"]);
    assert_eq!(code(&output), 1);
    assert_eq!(read(&repo, "README.md"), bytes);
    assert!(
        stderr(&output).starts_with(
            "deslag did not fix README.md: it is not valid UTF-8, so its findings do not point at \
             its bytes.\n\n"
        ),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_dry_run_writes_nothing_and_prints_check_as_it_stands() {
    let text = "# Notes\n\nA \u{2014} b.\n";
    let repo = repo_with(text);
    let output = repo.run(&["fix", "--dry-run"]);
    assert_eq!(code(&output), 1);
    assert_eq!(read(&repo, "README.md"), text.as_bytes());
    let printed = stderr(&output);
    assert!(
        printed.starts_with("deslag would fix README.md:\n  line 3: U+2014 em dash"),
        "{printed}"
    );
    assert!(printed.ends_with(&stderr(&repo.check())), "{printed}");
}

#[test]
fn format_json_prints_what_check_prints_after() {
    let repo = repo_with("# Notes\n\nA \u{2014} b.\n\nhttps://x.org/a\u{2014}b\n");
    let fixed = repo.run(&["fix", "--format", "json"]);
    assert_eq!(code(&fixed), 1);
    let checked = repo.run(&["check", "--format", "json"]);
    assert_eq!(stdout(&fixed), stdout(&checked));
    assert!(stdout(&fixed).contains("\"line\": 5"), "{}", stdout(&fixed));
}

#[cfg(unix)]
#[test]
fn a_file_with_nothing_to_fix_is_not_written() {
    use std::os::unix::fs::MetadataExt;

    let repo = repo_with("# Notes\n\nA \u{2014} b.\n");
    let inode = || {
        fs::metadata(repo.root().join("README.md"))
            .expect("metadata")
            .ino()
    };
    let before = inode();
    assert_eq!(code(&repo.run(&["fix"])), 0);
    let fixed = inode();
    assert_ne!(before, fixed, "a fixed file takes the place of the old one");
    let again = repo.run(&["fix"]);
    assert_eq!((code(&again), stderr(&again)), (0, String::new()));
    assert_eq!(inode(), fixed);
}

#[cfg(unix)]
#[test]
fn fix_keeps_permissions_and_leaves_no_temp_file() {
    use std::os::unix::fs::PermissionsExt;

    let repo = repo_with("# Notes\n\nA \u{2014} b.\n");
    let path = repo.root().join("README.md");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("permissions");
    assert_eq!(code(&repo.run(&["fix"])), 0);
    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
    let mut names: Vec<String> = fs::read_dir(repo.root())
        .expect("a readable root")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["README.md", "deslag.toml"]);
}

#[test]
fn a_ban_value_is_written_unless_it_holds_a_letter_or_digit() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n\n[md.lints.banned_chars]\nban = { \"\\u2014\" = \", \", \"\\u00ae\" \
         = \"(R)\" }\n",
    );
    repo.write("README.md", "# Notes\n\nA \u{2014} b\u{00AE}.\n");
    let output = repo.run(&["fix"]);
    assert_eq!(code(&output), 1);
    assert_eq!(
        String::from_utf8(read(&repo, "README.md")).expect("UTF-8"),
        "# Notes\n\nA ,  b\u{00AE}.\n"
    );
    assert!(
        stderr(&output).contains(
            "U+00AE \"\u{00AE}\"; write `(R)`\n    the replacement holds letters or digits"
        ),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_ban_value_the_file_bans_stops_fix_before_it_writes() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n\n[md.lints.banned_chars]\nban = { \"\\u2014\" = \"\\u2013\" }\n",
    );
    let text = "# Notes\n\nA \u{2014} b \u{201C}c\u{201D}.\n";
    repo.write("README.md", text);
    let output = repo.run(&["fix"]);
    assert_eq!(code(&output), 2);
    assert!(
        stderr(&output).contains("which holds U+2013, a character it bans"),
        "{}",
        stderr(&output)
    );
    assert_eq!(read(&repo, "README.md"), text.as_bytes());
}

#[test]
fn fix_fixes_only_the_files_it_is_given() {
    let text = "# Notes\n\nA \u{2014} b.\n";
    let repo = repo_with(text);
    repo.write("docs/guide.md", text);
    let output = repo.run(&["fix", "docs/../docs/guide.md"]);
    assert_eq!(code(&output), 1);
    assert_eq!(read(&repo, "docs/guide.md"), b"# Notes\n\nA - b.\n");
    assert_eq!(read(&repo, "README.md"), text.as_bytes());
    assert!(
        stderr(&output).starts_with("deslag fixed docs/guide.md:\n"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_path_check_does_not_read_is_an_error_and_nothing_is_written() {
    let text = "# Notes\n\nA \u{2014} b.\n";
    let repo = repo_with(text);
    repo.write(".gitignore", "ignored.md\n");
    repo.write("ignored.md", text);
    repo.write("notes.txt", text);
    repo.write("docs/note.md", "# Note\n");
    let elsewhere = Repo::new();
    let outside = elsewhere.write("outside.md", text);
    let outside = outside.to_str().expect("a UTF-8 path");
    for (path, problem) in [
        ("missing.md", "it does not exist"),
        ("docs", "it is not a file"),
        (outside, "it is outside the repo"),
        (
            "ignored.md",
            "it is ignored, so deslag check never reads it",
        ),
        ("notes.txt", "[md] does not select it"),
    ] {
        let output = repo.run(&["fix", "README.md", path]);
        assert_eq!(
            (code(&output), stderr(&output)),
            (2, format!("deslag: cannot fix {path}: {problem}\n")),
        );
        assert_eq!(read(&repo, "README.md"), text.as_bytes());
    }
    assert_eq!(read(&elsewhere, "outside.md"), text.as_bytes());
}
