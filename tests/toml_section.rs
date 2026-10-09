//! The `[toml]` section: which files it selects, which comments it reads, and what the config
//! refuses at load. What the lints find in a comment is in `tests/cases`.

mod common;

use std::path::{Path, PathBuf};

use common::config_toml::toml_misfit;
use common::{Repo, code, stderr, stdout};
use deslag::{Config, ConfigSource, Error, Lint, check_file};

fn load(extension: &str, text: &str) -> Result<Config, Error> {
    let path = PathBuf::from(format!("deslag.{extension}"));
    Config::parse(text, path, ConfigSource::Explicit)
}

fn toml(text: &str) -> Result<Config, Error> {
    load("toml", &format!("schema_version = 1\n{text}"))
}

/// What loading `text` says is wrong with it, with its causes.
fn refused(text: &str) -> String {
    let error = toml(text).expect_err("a config deslag refuses");
    let mut said = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        said.push_str(&format!("\n{cause}"));
        source = cause.source();
    }
    said
}

/// A repo holding `config` and a manifest with a banned character in a comment, in a comment after
/// a key, and in a string.
fn repo(config: &str) -> Repo {
    let repo = Repo::new();
    repo.write("deslag.toml", &format!("schema_version = 1\n{config}"));
    repo.write(
        "Cargo.toml",
        "# A leading \u{2014} comment.\n[package]\nname = \"a\" # A trailing \u{2014} comment.\n\
         description = \"a \u{2014} string\"\n",
    );
    repo.write("README.md", "A readme \u{2014} with a dash.\n");
    repo
}

/// The lines that name a banned character, which say where the run found one.
fn found(output: &std::process::Output) -> Vec<String> {
    stderr(output)
        .lines()
        .filter(|line| line.contains("U+2014"))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_config_without_a_toml_section_reads_no_toml_file() {
    let repo = repo("[md.lints.banned_chars]\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(
        said.contains("README.md has 1 banned character") && !said.contains("Cargo.toml"),
        "{said}"
    );
}

#[test]
fn a_toml_section_with_no_keys_reads_the_comments_of_every_toml_file_and_no_value() {
    let repo = repo("[toml.lints.banned_chars]\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(
        said.contains("Cargo.toml has 2 banned characters") && !said.contains("README.md"),
        "{said}"
    );
    assert_eq!(
        found(&output),
        ["  lines 1, 3: U+2014 em dash \"\u{2014}\"; write `-`"]
    );

    repo.write("rust-toolchain.toml", "# A \u{2014} dash.\n");
    repo.write("src/deep/a.toml", "# A \u{2014} dash.\n");
    repo.write("Cargo.toml.orig", "# A \u{2014} dash.\n");
    repo.write("Cargo.lock", "# A \u{2014} dash.\n");
    let said = stderr(&repo.check());
    assert!(
        said.contains("rust-toolchain.toml has 1 banned")
            && said.contains("src/deep/a.toml has 1 banned"),
        "{said}"
    );
    assert!(
        !said.contains("Cargo.toml.orig") && !said.contains("Cargo.lock"),
        "{said}"
    );
}

#[test]
fn the_comment_surface_reads_the_comments_and_none_reads_nothing() {
    let repo = repo("[toml]\nsurfaces = [\"comment\"]\n[toml.lints.banned_chars]\n");
    assert_eq!(code(&repo.check()), 1);
    let repo = self::repo("[toml]\nsurfaces = []\n");
    assert_eq!(code(&repo.check()), 0);
}

#[test]
fn globs_choose_the_files_and_an_override_changes_a_lint_for_some() {
    let repo = repo(
        "[toml]\nglobs = [\"/crates/**/*.toml\"]\n[toml.lints.banned_phrases.ban]\n\"delve\" = \"\"\n\
         [[toml.overrides]]\nglobs = [\"/crates/other/**\"]\nlints.banned_chars = {}\n",
    );
    repo.write(
        "crates/other/Cargo.toml",
        "# A \u{2014} dash, and we delve.\n",
    );
    repo.write("crates/other/notes.txt", "# We delve.\n");
    repo.write("rust-toolchain.toml", "# We delve.\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(
        said.contains("crates/other/Cargo.toml has 1 banned character")
            && said.contains("crates/other/Cargo.toml has 1 banned phrase")
            && !said.contains("rust-toolchain.toml")
            && !said.contains("notes.txt")
            && !said.contains("README.md"),
        "{said}"
    );
}

#[test]
fn a_doc_comment_surface_is_refused_because_a_toml_file_has_none() {
    let said = refused("[toml]\nsurfaces = [\"doc_comment\"]\n");
    assert!(
        said.contains("unknown variant `doc_comment`, expected `comment`"),
        "{said}"
    );
    let said = refused("[toml]\nsurfaces = [\"comment\", \"docstring\"]\n");
    assert!(said.contains("unknown variant `docstring`"), "{said}");
    for text in ["[toml]\nstrings = []\n", "[toml]\nvalues = []\n"] {
        assert!(refused(text).contains("unknown field"), "{text}");
    }
}

#[test]
fn every_glob_of_the_toml_section_ends_in_toml_and_an_override_glob_need_not() {
    for glob in [
        "Cargo.lock",
        "Cargo.toml.orig",
        "docs/**",
        "*",
        "*.{toml,md}",
        "a.toml/",
        "*.rs",
    ] {
        let said = refused(&format!("[toml]\nglobs = [\"/src/*.toml\", \"{glob}\"]\n"));
        assert!(
            said.contains(&format!("toml.globs holds `{glob}`")) && said.contains("end in .toml"),
            "{glob}: {said}"
        );
    }
    toml("[toml]\nglobs = [\"/src/**/*.toml\", \"Cargo.toml\"]\n").expect("globs of TOML files");
    toml("[toml]\n[[toml.overrides]]\nglobs = [\"/src/tag/**\"]\nlints.banned_chars = {}\n")
        .expect("an override glob narrows the section's");
}

/// Whether a lint can run on the surfaces: the whole file is never given, Markdown blocks are not
/// either, since no TOML comment has them, and prose by the one surface.
fn can_run(lint: Lint, surfaces: &[&str]) -> bool {
    match lint {
        Lint::MaxSizeBytes | Lint::RepoLayout | Lint::ListGrowth => false,
        Lint::MaxEmphasis
        | Lint::BannedChars
        | Lint::BannedPhrases
        | Lint::Density
        | Lint::VerbsNoNouns => !surfaces.is_empty(),
    }
}

#[test]
fn a_lint_that_the_surfaces_cannot_feed_is_an_error_in_the_section_and_in_an_override() {
    let sets: [&[&str]; 2] = [&[], &["comment"]];
    for surfaces in sets {
        let list = surfaces
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>();
        for lint in Lint::ALL {
            let id = lint.id();
            let table = match lint {
                Lint::RepoLayout => format!("[toml.lints.{id}]\nmin_entries = 1\n"),
                _ => format!("[toml.lints.{id}]\n"),
            };
            let in_override =
                format!("[[toml.overrides]]\nglobs = [\"a.toml\"]\nlints.{id} = {{}}\n");
            for (place, body) in [
                ("toml.lints", table),
                ("toml.overrides[0].lints", in_override),
            ] {
                let text = format!("[toml]\nsurfaces = [{}]\n{body}", list.join(", "));
                let loaded = toml(&text);
                let name = format!("{id} on {surfaces:?} in {place}");
                if can_run(lint, surfaces) {
                    loaded.expect(&name);
                    continue;
                }
                let said = refused(&text);
                assert!(
                    said.contains(&format!("{place}.{id} is on, but it needs"))
                        && said.contains("[toml] reads"),
                    "{name}: {said}"
                );
            }
        }
    }
}

#[test]
fn the_message_names_the_lint_the_surface_and_what_would_feed_it() {
    assert_eq!(
        refused("[toml.lints.max_size_bytes]\n"),
        "invalid setting in deslag.toml: toml.lints.max_size_bytes is on, but it needs the whole \
         file, which only [md] reads; [toml] reads the comment surface"
    );
    assert_eq!(
        refused("[toml.lints.list_growth]\n"),
        "invalid setting in deslag.toml: toml.lints.list_growth is on, but it needs the blocks of \
         Markdown, which no surface of [toml] has; [toml] reads the comment surface"
    );
    assert_eq!(
        refused("[[toml.overrides]]\nglobs = [\"a.toml\"]\nlints.list_growth = {}\n"),
        "invalid setting in deslag.toml: toml.overrides[0].lints.list_growth is on, but it needs \
         the blocks of Markdown, which no surface of [toml] has; [toml] reads the comment surface"
    );
    assert_eq!(
        refused("[toml]\nsurfaces = []\n[toml.lints.density]\n"),
        "invalid setting in deslag.toml: toml.lints.density is on, but it needs prose, which the \
         comment surface gives; [toml] reads no surface"
    );
}

/// `[md]` selects every file, so a TOML file is selected by both sections.
const BOTH: &str = "[md]\nglobs = [\"**\"]\n[toml.lints.banned_chars]\n";

#[test]
fn a_file_that_two_sections_select_is_an_error_that_names_both() {
    let said = "Cargo.toml is selected by both [md] and [toml]; narrow their globs so that one of \
                them selects it\n";
    let repo = repo(BOTH);
    for args in [
        &["check"][..],
        &["fix", "Cargo.toml"],
        &["fix"],
        &["explain", "Cargo.toml"],
    ] {
        let output = repo.run(args);
        let printed = stderr(&output);
        assert_eq!(code(&output), 2, "{args:?}: {printed}");
        assert!(
            printed.starts_with("deslag: invalid setting in ")
                && printed.ends_with(&format!("deslag.toml: {said}")),
            "{args:?}: {printed}"
        );
    }
    // The error is of the file, so a repo with no TOML file in it still runs.
    let no_toml = Repo::new();
    no_toml.write(
        "deslag.json",
        r#"{"schema_version":1,"md":{"globs":["**"]},"toml":{"lints":{"banned_chars":{}}}}"#,
    );
    no_toml.write("a.md", "Text.\n");
    assert_eq!(code(&no_toml.check()), 0);
}

#[test]
fn the_rust_cpp_and_toml_sections_never_select_one_file() {
    let repo =
        repo("[rust.lints.banned_chars]\n[cpp.lints.banned_chars]\n[toml.lints.banned_chars]\n");
    repo.write("src/lib.rs", "// A \u{2014} dash.\n");
    repo.write("src/lib.c", "// A \u{2014} dash.\n");
    let output = repo.check();
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    let said = stderr(&output);
    assert!(
        said.contains("Cargo.toml has 2 banned characters")
            && said.contains("src/lib.rs has 1 banned character")
            && said.contains("src/lib.c has 1 banned character"),
        "{said}"
    );
}

#[test]
fn a_path_named_alone_is_read_by_the_section_for_its_extension() {
    let text = "# A \u{2014} dash.\n";
    let run = |config: &Config, path: &str| {
        check_file(config, path, text.as_bytes(), Path::new(".")).map(|found| found.len())
    };
    let with = toml(
        "[toml]\nglobs = [\"/src/*.toml\"]\n[toml.lints.banned_chars]\n[md.lints.banned_chars]\n",
    )
    .expect("a config");
    // The section that selects it, then the section for the extension, then `[md]`.
    assert_eq!(run(&with, "src/a.toml").expect("findings"), 1);
    assert_eq!(run(&with, "elsewhere/Cargo.toml").expect("findings"), 1);
    assert_eq!(run(&with, "notes.txt").expect("findings"), 1);
    assert_eq!(run(&with, "Cargo.lock").expect("findings"), 1);

    let without = toml("[md.lints.banned_chars]\n").expect("a config");
    let said = run(&without, "src/a.toml")
        .expect_err("no section reads it")
        .to_string();
    assert_eq!(
        said,
        "invalid setting in deslag.toml: no section reads .toml files; add a [toml] section to \
         the config"
    );
    assert_eq!(run(&without, "notes.txt").expect("findings"), 1);
}

#[test]
fn fix_edits_a_comment_and_leaves_a_string_alone_and_keeps_the_line_breaks() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[toml.lints.banned_chars]\n",
    );
    repo.write(
        "quote.toml",
        "# A \u{2018}plain\u{2019} one.\r\nname = \"\u{201c}q\u{201d}\" # Parses the \u{201c}quoted\u{201d} \
         word.\r\nnote = \"# \u{2014} not a comment\"\r\n",
    );
    let output = repo.run(&["fix", "quote.toml"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(repo.root().join("quote.toml")).expect("a file"),
        "# A 'plain' one.\r\nname = \"\u{201c}q\u{201d}\" # Parses the \"quoted\" word.\r\n\
         note = \"# \u{2014} not a comment\"\r\n"
    );
}

#[test]
fn explain_names_the_toml_section_for_a_toml_file() {
    let repo = repo("[toml.lints.banned_chars]\n");
    let output = repo.run(&["explain", "Cargo.toml"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("# selected by [toml]: yes\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_toml_section_is_read_in_every_language_alike() {
    let toml_text = "schema_version = 1\n[toml]\nglobs = [\"/src/*.toml\"]\nsurfaces = [\"comment\"]\n\
                     [toml.lints.banned_chars]\n[[toml.overrides]]\nglobs = [\"a.toml\"]\n\
                     lints.density = {}\n";
    let yaml = "schema_version: 1\ntoml:\n  globs: [\"/src/*.toml\"]\n  surfaces: [comment]\n  \
                lints:\n    banned_chars: {}\n  overrides:\n    - globs: [\"a.toml\"]\n      \
                lints:\n        density: {}\n";
    let json = r#"{"schema_version":1,"toml":{"globs":["/src/*.toml"],"surfaces":["comment"],
        "lints":{"banned_chars":{}},"overrides":[{"globs":["a.toml"],"lints":{"density":{}}}]}}"#;
    let compiled: Vec<String> = [("toml", toml_text), ("yaml", yaml), ("json", json)]
        .into_iter()
        .map(|(extension, text)| {
            let config = load(extension, text).unwrap_or_else(|e| panic!("{extension}: {e:#?}"));
            assert_eq!(config.sections().len(), 2, "{extension}");
            format!("{:?}", config.sections())
        })
        .collect();
    assert_eq!(compiled[0], compiled[1]);
    assert_eq!(compiled[0], compiled[2]);
}

#[test]
fn the_schema_takes_a_toml_section_and_refuses_a_doc_comment_surface() {
    let fits = "[toml]\nglobs = [\"/src/*.toml\"]\nsurfaces = [\"comment\"]\n\
                [toml.lints.banned_chars]\n[[toml.overrides]]\nglobs = [\"a.toml\"]\nlints.density = {}\n";
    assert_eq!(toml_misfit(&format!("schema_version = 1\n{fits}")), None);
    let unknown = "schema_version = 1\n[toml]\nsurfaces = [\"doc_comment\"]\n";
    assert!(toml_misfit(unknown).is_some());
}
