//! The `[rust]` section: which files it selects, which comments it reads, and what the config
//! refuses at load. What the lints find in a comment is in `tests/cases` and `tests/regions.rs`.

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

/// A repo holding `config` and a Rust file with a banned character in a doc comment, a comment and
/// a string.
fn repo(config: &str) -> Repo {
    let repo = Repo::new();
    repo.write("deslag.toml", &format!("schema_version = 1\n{config}"));
    repo.write(
        "src/lib.rs",
        "/// A doc \u{2014} comment.\npub fn f() {\n    // A plain \u{2014} comment.\n    let _ = \"a \u{2014} string\";\n}\n",
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
fn a_config_without_a_rust_section_reads_no_rust_file() {
    let repo = repo("[md.lints.banned_chars]\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(
        said.contains("README.md has 1 banned character") && !said.contains("src/lib.rs"),
        "{said}"
    );
}

#[test]
fn a_rust_section_with_no_keys_reads_the_doc_comments_and_comments_of_every_rs_file() {
    let repo = repo("[rust.lints.banned_chars]\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(
        said.contains("src/lib.rs has 2 banned characters") && !said.contains("README.md"),
        "{said}"
    );
    assert_eq!(
        found(&output),
        ["  lines 1, 3: U+2014 em dash \"\u{2014}\"; write `-`"]
    );
}

#[test]
fn surfaces_choose_the_comments_that_are_read() {
    for (surfaces, lines) in [
        ("[\"doc_comment\"]", "  line 1: U+2014 em dash"),
        ("[\"comment\"]", "  line 3: U+2014 em dash"),
    ] {
        let repo = repo(&format!(
            "[rust]\nsurfaces = {surfaces}\n[rust.lints.banned_chars]\n"
        ));
        let output = repo.check();
        assert_eq!(code(&output), 1, "{surfaces}");
        assert!(
            found(&output)[0].starts_with(lines),
            "{surfaces}: {:?}",
            found(&output)
        );
    }
    let repo = repo("[rust]\nsurfaces = []\n");
    assert_eq!(code(&repo.check()), 0);
}

#[test]
fn globs_choose_the_rust_files_and_an_override_changes_a_lint_for_some() {
    let repo = repo(
        "[rust]\nglobs = [\"/src/**/*.rs\"]\n[rust.lints.banned_phrases.ban]\n\"delve\" = \"\"\n\
         [[rust.overrides]]\nglobs = [\"/src/other/**\"]\nlints.banned_chars = {}\n",
    );
    repo.write("src/other/mod.rs", "// A \u{2014} dash, and we delve.\n");
    repo.write("build.rs", "// We delve.\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(
        said.contains("src/other/mod.rs has 1 banned character")
            && said.contains("src/other/mod.rs has 1 banned phrase")
            && !said.contains("build.rs")
            && !said.contains("src/lib.rs"),
        "{said}"
    );
}

#[test]
fn an_unknown_surface_is_an_error_that_lists_the_ones_there_are() {
    let said = refused("[rust]\nsurfaces = [\"docstring\"]\n");
    assert!(
        said.contains("unknown variant `docstring`")
            && said.contains("`doc_comment`")
            && said.contains("`comment`"),
        "{said}"
    );
    for text in ["[rust]\nstrings = []\n", "[rust]\nvalues = []\n"] {
        assert!(refused(text).contains("unknown field"), "{text}");
    }
}

#[test]
fn every_glob_of_the_rust_section_ends_in_rs_and_an_override_glob_need_not() {
    for glob in ["docs/**", "src/**", "*", "*.{rs,md}", "a.rs/"] {
        let said = refused(&format!("[rust]\nglobs = [\"/src/*.rs\", \"{glob}\"]\n"));
        assert!(
            said.contains(&format!("rust.globs holds `{glob}`")) && said.contains("end in .rs"),
            "{glob}: {said}"
        );
    }
    toml("[rust]\nglobs = [\"/src/**/*.rs\", \"build.rs\"]\n").expect("globs of Rust files");
    toml("[rust]\n[[rust.overrides]]\nglobs = [\"/src/tag/**\"]\nlints.banned_chars = {}\n")
        .expect("an override glob narrows the section's");
}

#[test]
fn the_md_section_keeps_any_glob() {
    for glob in ["*", "docs/**", "*.txt", "README"] {
        toml(&format!("[md]\nglobs = [\"{glob}\"]\n")).expect(glob);
    }
}

/// Whether a lint can run on the surfaces: the whole file is never given, Markdown blocks only by
/// the doc comments, and prose by either.
fn can_run(lint: Lint, surfaces: &[&str]) -> bool {
    match lint {
        Lint::MaxSizeBytes | Lint::RepoLayout => false,
        Lint::ListGrowth => surfaces.contains(&"doc_comment"),
        Lint::MaxEmphasis
        | Lint::BannedChars
        | Lint::BannedPhrases
        | Lint::Density
        | Lint::VerbsNoNouns => !surfaces.is_empty(),
    }
}

#[test]
fn a_lint_that_the_surfaces_cannot_feed_is_an_error_in_the_section_and_in_an_override() {
    let sets: [&[&str]; 4] = [
        &[],
        &["doc_comment"],
        &["comment"],
        &["doc_comment", "comment"],
    ];
    for surfaces in sets {
        let list = surfaces
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>();
        for lint in Lint::ALL {
            let id = lint.id();
            let table = match lint {
                Lint::RepoLayout => format!("[rust.lints.{id}]\nmin_entries = 1\n"),
                _ => format!("[rust.lints.{id}]\n"),
            };
            let in_override =
                format!("[[rust.overrides]]\nglobs = [\"a.rs\"]\nlints.{id} = {{}}\n");
            for (place, body) in [
                ("rust.lints", table),
                ("rust.overrides[0].lints", in_override),
            ] {
                let text = format!("[rust]\nsurfaces = [{}]\n{body}", list.join(", "));
                let loaded = toml(&text);
                let name = format!("{id} on {surfaces:?} in {place}");
                if can_run(lint, surfaces) {
                    loaded.expect(&name);
                    continue;
                }
                let said = refused(&text);
                assert!(
                    said.contains(&format!("{place}.{id} is on, but it needs"))
                        && said.contains("[rust] reads"),
                    "{name}: {said}"
                );
            }
        }
    }
}

#[test]
fn the_message_names_the_lint_the_surfaces_and_what_would_feed_it() {
    assert_eq!(
        refused("[rust.lints.max_size_bytes]\n"),
        "invalid setting in deslag.toml: rust.lints.max_size_bytes is on, but it needs the whole \
         file, which only [md] reads; [rust] reads the surfaces doc_comment and comment"
    );
    assert_eq!(
        refused("[rust]\nsurfaces = [\"comment\"]\n[rust.lints.list_growth]\n"),
        "invalid setting in deslag.toml: rust.lints.list_growth is on, but it needs the blocks of \
         Markdown, which only the doc_comment surface gives; [rust] reads the comment surface"
    );
    assert_eq!(
        refused("[rust]\nsurfaces = []\n[rust.lints.density]\n"),
        "invalid setting in deslag.toml: rust.lints.density is on, but it needs prose, which the \
         doc_comment and comment surfaces give; [rust] reads no surface"
    );
}

/// `[md]` selects every file, so a Rust file is selected by both sections.
const BOTH: &str = "[md]\nglobs = [\"**\"]\n[rust.lints.banned_chars]\n";

#[test]
fn a_file_that_two_sections_select_is_an_error_that_names_both() {
    let said = "src/lib.rs is selected by both [md] and [rust]; narrow their globs so that one of \
                them selects it\n";
    let repo = repo(BOTH);
    for args in [
        &["check"][..],
        &["fix", "src/lib.rs"],
        &["fix"],
        &["explain", "src/lib.rs"],
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
    // The error is of the file, so a repo with no Rust file in it still runs.
    let no_rust = Repo::new();
    no_rust.write("deslag.toml", &format!("schema_version = 1\n{BOTH}"));
    no_rust.write("a.md", "Text.\n");
    assert_eq!(code(&no_rust.check()), 0);
}

#[test]
fn a_path_named_alone_is_read_by_the_section_for_its_extension() {
    let text = "/// A \u{2014} dash.\n";
    let run = |config: &Config, path: &str| {
        check_file(config, path, text.as_bytes(), Path::new(".")).map(|found| found.len())
    };
    let with = toml(
        "[rust]\nglobs = [\"/src/*.rs\"]\n[rust.lints.banned_chars]\n[md.lints.banned_chars]\n",
    )
    .expect("a config");
    // The section that selects it, then the section for the extension, then `[md]`.
    assert_eq!(run(&with, "src/a.rs").expect("findings"), 1);
    assert_eq!(run(&with, "elsewhere/a.rs").expect("findings"), 1);
    assert_eq!(run(&with, "notes.txt").expect("findings"), 1);

    let without = toml("[md.lints.banned_chars]\n").expect("a config");
    let said = run(&without, "src/a.rs")
        .expect_err("no section reads it")
        .to_string();
    assert_eq!(
        said,
        "invalid setting in deslag.toml: no section reads .rs files; add a [rust] section to the \
         config"
    );
    assert_eq!(run(&without, "notes.txt").expect("findings"), 1);
}

#[test]
fn fix_edits_a_comment_where_the_edit_leaves_it_reading_as_it_did() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[rust.lints.banned_chars]\n",
    );
    repo.write("notes.txt", "Notes.\n");
    repo.write(
        "src/quote.rs",
        "/// Parses the \u{201c}quoted\u{201d} word.\npub fn f() {}\n// A \u{2018}plain\u{2019} one.\n",
    );
    let output = repo.run(&["fix", "src/quote.rs"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(repo.root().join("src/quote.rs")).expect("a file"),
        "/// Parses the \"quoted\" word.\npub fn f() {}\n// A 'plain' one.\n"
    );
    let output = repo.run(&["fix", "notes.txt"]);
    assert_eq!(
        (code(&output), stderr(&output)),
        (
            2,
            "deslag: cannot fix notes.txt: no section selects it\n".to_string()
        )
    );
}

#[test]
fn explain_names_the_rust_section_for_a_rust_file() {
    let repo = repo("[rust.lints.banned_chars]\n");
    let output = repo.run(&["explain", "src/lib.rs"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("# selected by [rust]: yes\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_rust_section_is_read_in_every_language_alike() {
    let toml_text = "schema_version = 1\n[rust]\nglobs = [\"/src/*.rs\"]\nsurfaces = [\"comment\"]\n\
                     [rust.lints.banned_chars]\n[[rust.overrides]]\nglobs = [\"a.rs\"]\n\
                     lints.density = {}\n";
    let yaml = "schema_version: 1\nrust:\n  globs: [\"/src/*.rs\"]\n  surfaces: [comment]\n  \
                lints:\n    banned_chars: {}\n  overrides:\n    - globs: [\"a.rs\"]\n      \
                lints:\n        density: {}\n";
    let json = r#"{"schema_version":1,"rust":{"globs":["/src/*.rs"],"surfaces":["comment"],
        "lints":{"banned_chars":{}},"overrides":[{"globs":["a.rs"],"lints":{"density":{}}}]}}"#;
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
fn the_schema_takes_a_rust_section_and_refuses_an_unknown_surface() {
    let fits = "[rust]\nglobs = [\"/src/*.rs\"]\nsurfaces = [\"doc_comment\", \"comment\"]\n\
                [rust.lints.banned_chars]\n[[rust.overrides]]\nglobs = [\"a.rs\"]\nlints.density = {}\n";
    assert_eq!(toml_misfit(&format!("schema_version = 1\n{fits}")), None);
    let unknown = "schema_version = 1\n[rust]\nsurfaces = [\"docstring\"]\n";
    assert!(toml_misfit(unknown).is_some());
}
