//! The code sections `[rust]`, `[toml]` and `[cpp]`: the files each selects, the comments it reads
//! and what the config refuses at load. `ROWS` lists the sections, each rule is one test over it,
//! and a rule only one language has is its own test. A test over `ROWS` passes when a row is
//! missing, so `the_table_has_a_row_for_each_code_section` holds it to the schema, and every
//! expected text is written out in its row. What the lints find in a comment is in `tests/cases`.

mod common;

use std::path::{Path, PathBuf};

use common::config_toml::toml_misfit;
use common::schema::SchemaPaths;
use common::{Repo, code, stderr, stdout};
use deslag::config::schema;
use deslag::{Config, ConfigSource, Error, Lint, check_file};

/// Every banned-character group, for a config that finds an em dash wherever it is read.
const GROUPS: &str = "groups = { dashes = true, arrows = true, ellipsis = true, bullets = true, math = true, checks = true, section = true, box_drawing = true, spaces = true, invisible = true, quotes = true }";

/// What the run prints for the em dashes of a sample file, one on line 1 and one on line 3.
const BOTH_LINES: &str = "  lines 1, 3: U+2014 em dash \"\u{2014}\"; write `-`";

/// The extensions `[cpp]` reads.
const EXTENSIONS: [&str; 8] = ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"];

type S = &'static str;
type L = &'static [S];

/// What a section does. Inputs may come from `name`; every expected text is a literal.
struct Row {
    /// The key, and a sample file: an em dash in a doc comment (line 1), a comment (line 3) and a string; what
    /// the run says of it, and the start of what it finds when only `doc_comment`, or only
    /// `comment`, is read.
    name: S,
    file: S,
    source: S,
    reads: S,
    doc_only: Option<S>,
    comment_only: S,
    /// A glob, an override glob, the files written for them, and what the run says of them.
    glob: S,
    override_glob: S,
    scoped: &'static [(S, S)],
    says: L,
    never_says: L,
    /// Globs refused with `refusal` and `must_end_in`, and globs taken.
    bad_globs: L,
    refusal: S,
    must_end_in: S,
    good_globs: L,
    /// Surface sets tried for every lint; the places and the surfaces a refusal names.
    surface_sets: &'static [L],
    lints_place: S,
    override_place: S,
    reads_surfaces: S,
    /// Whole refusals of a lint no surface feeds, and of an unknown surface.
    max_size_message: S,
    density_message: S,
    variants: S,
    /// The error when `[md]` and the section both select `file`; a path the section is named for,
    /// one it reads elsewhere, ones `[md]` reads, and what a config without the section says of
    /// the first; a file, its text, and the text once fixed; what `explain` says of `file`.
    both_message: S,
    alone: S,
    elsewhere: S,
    md_reads: L,
    no_section_message: S,
    fix: (S, S, S),
    explained: S,
}

const ROWS: [Row; 3] = [
    Row {
        name: "rust",
        file: "src/lib.rs",
        reads: "src/lib.rs has 2 banned characters",
        source: "/// A doc \u{2014} comment.\npub fn f() {\n    // A plain \u{2014} comment.\n    let _ = \"a \u{2014} string\";\n}\n",
        doc_only: Some("  line 1: U+2014 em dash"),
        comment_only: "  line 3: U+2014 em dash",
        glob: "/src/**/*.rs",
        override_glob: "/src/other/**",
        scoped: &[
            ("src/other/mod.rs", "// A \u{2014} dash, and we delve.\n"),
            ("build.rs", "// We delve.\n"),
        ],
        says: &[
            "src/other/mod.rs has 1 banned character",
            "src/other/mod.rs has 1 banned phrase",
        ],
        never_says: &["build.rs", "src/lib.rs"],
        bad_globs: &["docs/**", "src/**", "*", "*.{rs,md}", "a.rs/"],
        refusal: "rust.globs holds",
        must_end_in: "end in .rs",
        good_globs: &["/src/**/*.rs", "build.rs"],
        surface_sets: &[
            &[],
            &["doc_comment"],
            &["comment"],
            &["doc_comment", "comment"],
        ],
        lints_place: "rust.lints",
        override_place: "rust.overrides[0].lints",
        reads_surfaces: "[rust] reads",
        max_size_message: "invalid setting in deslag.toml: rust.lints.max_size_bytes is on, but it needs the whole file, which only [md] reads; [rust] reads the surfaces doc_comment and comment",
        density_message: "invalid setting in deslag.toml: rust.lints.density is on, but it needs prose, which the doc_comment and comment surfaces give; [rust] reads no surface",
        variants: "unknown variant `docstring`, expected `doc_comment` or `comment`",
        both_message: "src/lib.rs is selected by both [md] and [rust]; narrow their globs so that one of them selects it, or exclude it from one of them\n",
        alone: "src/a.rs",
        elsewhere: "elsewhere/a.rs",
        md_reads: &["notes.txt"],
        no_section_message: "invalid setting in deslag.toml: no section reads .rs files; add a [rust] section to the config",
        fix: (
            "src/quote.rs",
            "/// Parses the \u{201c}quoted\u{201d} word.\npub fn f() {}\n// A \u{2018}plain\u{2019} one.\n",
            "/// Parses the \"quoted\" word.\npub fn f() {}\n// A 'plain' one.\n",
        ),
        explained: "# selected by [rust]: yes\n",
    },
    Row {
        name: "toml",
        file: "Cargo.toml",
        reads: "Cargo.toml has 2 banned characters",
        source: "# A leading \u{2014} comment.\n[package]\nname = \"a\" # A trailing \u{2014} comment.\ndescription = \"a \u{2014} string\"\n",
        doc_only: None,
        comment_only: BOTH_LINES,
        glob: "/crates/**/*.toml",
        override_glob: "/crates/other/**",
        scoped: &[
            (
                "crates/other/Cargo.toml",
                "# A \u{2014} dash, and we delve.\n",
            ),
            ("crates/other/notes.txt", "# We delve.\n"),
            ("rust-toolchain.toml", "# We delve.\n"),
        ],
        says: &[
            "crates/other/Cargo.toml has 1 banned character",
            "crates/other/Cargo.toml has 1 banned phrase",
        ],
        never_says: &["rust-toolchain.toml", "notes.txt", "README.md"],
        bad_globs: &[
            "Cargo.lock",
            "Cargo.toml.orig",
            "docs/**",
            "*",
            "*.{toml,md}",
            "a.toml/",
            "*.rs",
        ],
        refusal: "toml.globs holds",
        must_end_in: "end in .toml",
        good_globs: &["/src/**/*.toml", "Cargo.toml"],
        surface_sets: &[&[], &["comment"]],
        lints_place: "toml.lints",
        override_place: "toml.overrides[0].lints",
        reads_surfaces: "[toml] reads",
        max_size_message: "invalid setting in deslag.toml: toml.lints.max_size_bytes is on, but it needs the whole file, which only [md] reads; [toml] reads the comment surface",
        density_message: "invalid setting in deslag.toml: toml.lints.density is on, but it needs prose, which the comment surface gives; [toml] reads no surface",
        variants: "unknown variant `docstring`, expected `comment`",
        both_message: "Cargo.toml is selected by both [md] and [toml]; narrow their globs so that one of them selects it, or exclude it from one of them\n",
        alone: "src/a.toml",
        elsewhere: "elsewhere/Cargo.toml",
        md_reads: &["notes.txt", "Cargo.lock"],
        no_section_message: "invalid setting in deslag.toml: no section reads .toml files; add a [toml] section to the config",
        fix: (
            "quote.toml",
            "# A \u{2018}plain\u{2019} one.\n",
            "# A 'plain' one.\n",
        ),
        explained: "# selected by [toml]: yes\n",
    },
    Row {
        name: "cpp",
        file: "src/lib.c",
        reads: "src/lib.c has 2 banned characters",
        source: "/// A doc \u{2014} comment.\nint f(void) {\n    // A plain \u{2014} comment.\n    return puts(\"a \u{2014} string\");\n}\n",
        doc_only: Some("  line 1: U+2014 em dash"),
        comment_only: "  line 3: U+2014 em dash",
        glob: "/src/**/*.c",
        override_glob: "/src/other/**",
        scoped: &[
            ("src/other/mod.c", "// A \u{2014} dash, and we delve.\n"),
            ("src/other/mod.h", "// A \u{2014} dash, and we delve.\n"),
        ],
        says: &[
            "src/other/mod.c has 1 banned character",
            "src/other/mod.c has 1 banned phrase",
        ],
        never_says: &["mod.h", "src/lib.c"],
        bad_globs: &[
            "docs/**", "src/**", "*", "*.{c,h}", "*.rs", "a.c/", "*.md", "*.tcc",
        ],
        refusal: "cpp.globs holds",
        must_end_in: "end in .c, .h, .cc, .cpp, .cxx, .hpp, .hh or .hxx",
        good_globs: &["/src/**/*.cpp", "a.hxx"],
        surface_sets: &[
            &[],
            &["doc_comment"],
            &["comment"],
            &["doc_comment", "comment"],
        ],
        lints_place: "cpp.lints",
        override_place: "cpp.overrides[0].lints",
        reads_surfaces: "[cpp] reads",
        max_size_message: "invalid setting in deslag.toml: cpp.lints.max_size_bytes is on, but it needs the whole file, which only [md] reads; [cpp] reads the surfaces doc_comment and comment",
        density_message: "invalid setting in deslag.toml: cpp.lints.density is on, but it needs prose, which the doc_comment and comment surfaces give; [cpp] reads no surface",
        variants: "unknown variant `docstring`, expected `doc_comment` or `comment`",
        both_message: "src/lib.c is selected by both [md] and [cpp]; narrow their globs so that one of them selects it, or exclude it from one of them\n",
        alone: "src/a.c",
        elsewhere: "elsewhere/a.cc",
        md_reads: &["notes.txt", "a.tcc", "Makefile"],
        no_section_message: "invalid setting in deslag.toml: no section reads .c files; add a [cpp] section to the config",
        fix: (
            "src/quote.c",
            "// A \u{2018}plain\u{2019} one.\nint f(void);\n/**\n * Parses the \u{201c}quoted\u{201d} word.\n */\nint g(void);\n",
            "// A 'plain' one.\nint f(void);\n/**\n * Parses the \"quoted\" word.\n */\nint g(void);\n",
        ),
        explained: "# selected by [cpp]: yes\n",
    },
];

fn load(extension: &str, text: &str) -> Result<Config, Error> {
    let path = PathBuf::from(format!("deslag.{extension}"));
    Config::parse(text, path, ConfigSource::Explicit)
}

fn toml(text: &str) -> Result<Config, Error> {
    load("toml", &format!("schema_version = 2\n{text}"))
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

/// The table of banned characters of the section `name`, with every group on.
fn chars(name: &str) -> String {
    format!("[{name}.lints.banned_chars]\n{GROUPS}\n")
}

/// A repo holding `config`, the row's sample file and a readme with a banned character.
fn repo(row: &Row, config: &str) -> Repo {
    let repo = Repo::new();
    repo.write("deslag.toml", &format!("schema_version = 2\n{config}"));
    repo.write(row.file, row.source);
    repo.write("README.md", "A readme \u{2014} with a dash.\n");
    repo
}

/// How many findings the config has in `text` as the file `path`.
fn findings(config: &Config, path: &str, text: &str) -> Result<usize, Error> {
    check_file(config, path, text.as_bytes(), Path::new(".")).map(|found| found.len())
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
fn the_table_has_a_row_for_each_code_section() {
    let schema = schema();
    let paths = SchemaPaths::of(&schema);
    let mut named = paths.sections();
    named.retain(|section| *section != "md");
    let mut rows: Vec<&str> = ROWS.iter().map(|row| row.name).collect();
    named.sort_unstable();
    rows.sort_unstable();
    assert_eq!(
        rows, named,
        "the table's rows against the schema's sections"
    );
}

#[test]
fn a_section_with_no_keys_reads_the_comments_of_its_files_and_no_string() {
    for row @ &Row { name, .. } in &ROWS {
        // With no section for them, the files are not read.
        let said = stderr(&repo(row, &chars("md")).check());
        assert!(
            said.contains("README.md has 1 banned") && !said.contains(row.file),
            "{name}: {said}"
        );
        let repo = repo(row, &chars(name));
        let output = repo.check();
        assert_eq!(code(&output), 1, "{name}");
        let said = stderr(&output);
        assert!(
            said.contains(row.reads) && !said.contains("README.md"),
            "{name}: {said}"
        );
        assert_eq!(found(&output), [BOTH_LINES], "{name}");
    }
}

#[test]
fn surfaces_choose_the_comments_that_are_read() {
    for row @ &Row { name, .. } in &ROWS {
        let only = |surfaces: &str| {
            let config = format!("[{name}]\nsurfaces = {surfaces}\n{}", chars(name));
            let output = repo(row, &config).check();
            assert_eq!(code(&output), 1, "{name} {surfaces}");
            found(&output)
        };
        if let Some(doc) = row.doc_only {
            assert!(only("[\"doc_comment\"]")[0].starts_with(doc), "{name}");
        }
        assert!(
            only("[\"comment\"]")[0].starts_with(row.comment_only),
            "{name}"
        );
        let none = repo(row, &format!("[{name}]\nsurfaces = []\n"));
        assert_eq!(code(&none.check()), 0, "{name}");
    }
}

#[test]
fn globs_choose_the_files_and_an_override_changes_a_lint_for_some() {
    for row @ &Row { name, .. } in &ROWS {
        let config = format!(
            "[{name}]\nglobs = [\"{glob}\"]\n[{name}.lints.banned_phrases.ban]\n\"delve\" = \"\"\n\
             [[{name}.overrides]]\nglobs = [\"{over}\"]\nlints.banned_chars = {{ {GROUPS} }}\n",
            glob = row.glob,
            over = row.override_glob,
        );
        let repo = repo(row, &config);
        for (path, text) in row.scoped {
            repo.write(path, text);
        }
        let output = repo.check();
        assert_eq!(code(&output), 1, "{name}");
        let said = stderr(&output);
        for want in row.says {
            assert!(said.contains(want), "{name}: {want}: {said}");
        }
        for never in row.never_says {
            assert!(!said.contains(never), "{name}: {never}: {said}");
        }
    }
}

#[test]
fn an_unknown_surface_is_an_error_that_lists_the_ones_there_are() {
    for row @ &Row { name, .. } in &ROWS {
        let said = refused(&format!("[{name}]\nsurfaces = [\"docstring\"]\n"));
        assert!(said.contains(row.variants), "{name}: {said}");
        for key in ["strings", "values"] {
            let said = refused(&format!("[{name}]\n{key} = []\n"));
            assert!(said.contains("unknown field"), "{name} {key}");
        }
    }
}

#[test]
fn every_glob_of_a_section_ends_in_an_extension_it_reads_and_an_override_glob_need_not() {
    for row @ &Row { name, .. } in &ROWS {
        let first = row.good_globs[0];
        for glob in row.bad_globs {
            let said = refused(&format!("[{name}]\nglobs = [\"{first}\", \"{glob}\"]\n"));
            assert!(
                said.contains(&format!("{} `{glob}`", row.refusal))
                    && said.contains(row.must_end_in),
                "{name}: {glob}: {said}"
            );
        }
        let globs = row
            .good_globs
            .iter()
            .map(|glob| format!("\"{glob}\""))
            .collect::<Vec<_>>()
            .join(", ");
        toml(&format!("[{name}]\nglobs = [{globs}]\n")).expect("globs of its files");
        let narrowed = format!(
            "[{name}]\n[[{name}.overrides]]\nglobs = [\"/src/tag/**\"]\nlints.banned_chars = {{ {GROUPS} }}\n"
        );
        toml(&narrowed).expect("an override glob narrows the section's");
    }
    // The `[md]` section keeps any glob.
    for glob in ["*", "docs/**", "*.txt", "README"] {
        toml(&format!("[md]\nglobs = [\"{glob}\"]\n")).expect(glob);
    }
}

/// Whether a lint can run on the surfaces: the whole file is never given, and prose by any.
fn can_run(lint: Lint, surfaces: &[&str]) -> bool {
    match lint {
        Lint::MaxSizeBytes | Lint::RepoLayout => false,
        Lint::MaxEmphasis
        | Lint::BannedChars
        | Lint::BannedPhrases
        | Lint::Density
        | Lint::VerbsNoNouns
        | Lint::NotAtAll
        | Lint::VerbsNothing
        | Lint::PossessiveOwn => !surfaces.is_empty(),
    }
}

#[test]
fn a_lint_that_the_surfaces_cannot_feed_is_an_error_in_the_section_and_in_an_override() {
    for row @ &Row { name, .. } in &ROWS {
        for surfaces in row.surface_sets {
            for lint in Lint::ALL {
                let id = lint.id();
                let table = match lint {
                    Lint::RepoLayout => format!("[{name}.lints.{id}]\nmin_entries = 1\n"),
                    _ => format!("[{name}.lints.{id}]\n"),
                };
                let in_override = format!(
                    "[[{name}.overrides]]\nglobs = [\"{}\"]\nlints.{id} = {{}}\n",
                    row.alone
                );
                for (place, body) in [(row.lints_place, table), (row.override_place, in_override)] {
                    let text = format!("[{name}]\nsurfaces = {surfaces:?}\n{body}");
                    let loaded = toml(&text);
                    let case = format!("{name}: {id} on {surfaces:?} in {place}");
                    if can_run(lint, surfaces) {
                        loaded.expect(&case);
                        continue;
                    }
                    let said = refused(&text);
                    assert!(
                        said.contains(&format!("{place}.{id} is on, but it needs"))
                            && said.contains(row.reads_surfaces),
                        "{case}: {said}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_message_names_the_lint_the_surfaces_and_what_would_feed_it() {
    for row @ &Row { name, .. } in &ROWS {
        assert_eq!(
            refused(&format!("[{name}.lints.max_size_bytes]\n")),
            row.max_size_message,
            "{name}"
        );
        assert_eq!(
            refused(&format!(
                "[{name}]\nsurfaces = []\n[{name}.lints.density]\n"
            )),
            row.density_message,
            "{name}"
        );
    }
}

#[test]
fn a_file_that_two_sections_select_is_an_error_that_names_both() {
    for row @ &Row { name, .. } in &ROWS {
        let config = format!("[md]\nglobs = [\"**\"]\n{}", chars(name));
        let repo = repo(row, &config);
        for args in [
            &["check"][..],
            &["fix", row.file],
            &["fix"],
            &["explain", row.file],
        ] {
            let output = repo.run(args);
            let printed = stderr(&output);
            assert_eq!(code(&output), 2, "{name}: {args:?}: {printed}");
            assert!(
                printed.starts_with("deslag: invalid setting in ")
                    && printed.ends_with(&format!("deslag.toml: {}", row.both_message)),
                "{name}: {args:?}: {printed}"
            );
        }
        // The error is of the file, so a repo with none of the section's files still runs. The
        // config is JSON, so that it is not a TOML file the `[toml]` section selects.
        let bare = Repo::new();
        let json = format!(r#"{{"schema_version":2,"md":{{"globs":["**"]}},"{name}":{{}}}}"#);
        bare.write("deslag.json", &json);
        bare.write("a.md", "Text.\n");
        assert_eq!(code(&bare.check()), 0, "{name}");
    }
}

#[test]
fn the_rust_cpp_and_toml_sections_never_select_one_file() {
    let config = ["rust", "cpp", "toml"].map(chars).concat();
    let repo = repo(&ROWS[1], &config);
    for path in ["src/lib.rs", "src/lib.c"] {
        repo.write(path, "// A \u{2014} dash.\n");
    }
    let output = repo.check();
    let said = stderr(&output);
    assert_eq!(code(&output), 1, "{said}");
    assert!(
        said.contains("Cargo.toml has 2 banned characters")
            && said.contains("src/lib.rs has 1 banned character")
            && said.contains("src/lib.c has 1 banned character"),
        "{said}"
    );
}

#[test]
fn a_path_named_alone_is_read_by_the_section_for_its_extension() {
    for row @ &Row { name, .. } in &ROWS {
        let text = row.scoped[0].1;
        let run = |config: &Config, path: &str| findings(config, path, text);
        let with = toml(&format!(
            "[{name}]\nglobs = [\"{glob}\"]\n{}{}",
            chars(name),
            chars("md"),
            glob = row.glob,
        ))
        .expect("a config");
        // The section that selects it, then the section for the extension, then `[md]`.
        assert_eq!(run(&with, row.alone).expect("findings"), 1, "{name}");
        assert_eq!(run(&with, row.elsewhere).expect("findings"), 1, "{name}");
        let without = toml(&chars("md")).expect("a config");
        let said = run(&without, row.alone)
            .expect_err("no section reads it")
            .to_string();
        assert_eq!(said, row.no_section_message, "{name}");
        for path in row.md_reads {
            assert_eq!(run(&with, path).expect("findings"), 1, "{path}");
            assert_eq!(run(&without, path).expect("findings"), 1, "{path}");
        }
    }
}

#[test]
fn fix_edits_a_comment_where_the_edit_leaves_it_reading_as_it_did() {
    for row @ &Row { name, .. } in &ROWS {
        let (path, before, after) = row.fix;
        let repo = Repo::new();
        repo.write(
            "deslag.toml",
            &format!("schema_version = 2\n{}", chars(name)),
        );
        repo.write("notes.txt", "Notes.\n");
        repo.write(path, before);
        let output = repo.run(&["fix", path]);
        assert_eq!(code(&output), 0, "{name}: {}", stderr(&output));
        assert_eq!(
            std::fs::read_to_string(repo.root().join(path)).expect("a file"),
            after,
            "{name}"
        );
        let output = repo.run(&["fix", "notes.txt"]);
        assert_eq!(
            (code(&output), stderr(&output)),
            (
                2,
                "deslag: cannot fix notes.txt: no section selects it\n".to_string()
            ),
            "{name}"
        );
    }
}

#[test]
fn explain_names_the_section_for_its_file() {
    for row @ &Row { name, .. } in &ROWS {
        let repo = repo(row, &chars(name));
        let output = repo.run(&["explain", row.file]);
        assert_eq!(code(&output), 0, "{name}: {}", stderr(&output));
        assert!(
            stdout(&output).contains(row.explained),
            "{name}: {}",
            stdout(&output)
        );
    }
}

#[test]
fn a_section_is_read_in_every_language_alike_and_the_schema_takes_it() {
    for row @ &Row { name, .. } in &ROWS {
        let (glob, over) = (row.glob, row.alone);
        let toml_text = format!(
            "schema_version = 2\n[{name}]\nglobs = [\"{glob}\"]\nsurfaces = [\"comment\"]\n\
             [{name}.lints.density]\n[[{name}.overrides]]\nglobs = [\"{over}\"]\nlints.density = {{}}\n"
        );
        let yaml = format!(
            "schema_version: 2\n{name}:\n  globs: [\"{glob}\"]\n  surfaces: [comment]\n  lints:\n    \
             density: {{}}\n  overrides:\n    - globs: [\"{over}\"]\n      lints:\n        density: {{}}\n"
        );
        let json = format!(
            r#"{{"schema_version":2,"{name}":{{"globs":["{glob}"],"surfaces":["comment"],"lints":{{"density":{{}}}},"overrides":[{{"globs":["{over}"],"lints":{{"density":{{}}}}}}]}}}}"#
        );
        assert_eq!(toml_misfit(&toml_text), None, "{name}");
        let unknown = "schema_version = 2\n[{name}]\nsurfaces = [\"docstring\"]\n";
        assert!(
            toml_misfit(&unknown.replace("{name}", name)).is_some(),
            "{name}"
        );
        let compiled: Vec<String> = [("toml", toml_text), ("yaml", yaml), ("json", json)]
            .into_iter()
            .map(|(extension, text)| {
                let config =
                    load(extension, &text).unwrap_or_else(|e| panic!("{name} {extension}: {e:#?}"));
                assert_eq!(config.sections().len(), 2, "{name} {extension}");
                format!("{:?}", config.sections())
            })
            .collect();
        assert_eq!(compiled[0], compiled[1], "{name}");
        assert_eq!(compiled[0], compiled[2], "{name}");
    }
}

#[test]
fn a_toml_file_has_no_doc_comment_so_the_surface_is_refused() {
    let said = refused("[toml]\nsurfaces = [\"doc_comment\"]\n");
    assert!(
        said.contains("unknown variant `doc_comment`, expected `comment`"),
        "{said}"
    );
    assert!(toml_misfit("schema_version = 2\n[toml]\nsurfaces = [\"doc_comment\"]\n").is_some());
}

#[test]
fn the_toml_section_reads_files_ending_in_toml_and_no_other() {
    let repo = repo(&ROWS[1], &chars("toml"));
    for path in [
        "rust-toolchain.toml",
        "src/deep/a.toml",
        "Cargo.toml.orig",
        "Cargo.lock",
    ] {
        repo.write(path, "# A \u{2014} dash.\n");
    }
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
fn fix_edits_a_toml_comment_and_leaves_a_string_alone_and_keeps_the_line_breaks() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        &["schema_version = 2\n", &chars("toml")].concat(),
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
fn the_cpp_section_reads_the_eight_extensions_and_no_other() {
    let repo = repo(&ROWS[2], &chars("cpp"));
    for extension in EXTENSIONS.into_iter().chain(["rs", "tcc"]) {
        repo.write(&format!("src/a.{extension}"), "// A \u{2014} dash.\n");
    }
    let said = stderr(&repo.check());
    for extension in EXTENSIONS {
        assert!(
            said.contains(&format!("src/a.{extension} has 1 banned")),
            "{said}"
        );
    }
    assert!(
        !said.contains("src/a.rs") && !said.contains("src/a.tcc"),
        "{said}"
    );
    let with = toml(&format!("[cpp]\nglobs = [\"/src/*.c\"]\n{}", chars("cpp"))).expect("a config");
    let without = toml(&chars("md")).expect("a config");
    let run = |config: &Config, path: &str| findings(config, path, "// A \u{2014} dash.\n");
    for extension in EXTENSIONS {
        let path = format!("elsewhere/a.{extension}");
        assert_eq!(run(&with, &path).expect("findings"), 1, "{path}");
        let said = run(&without, &path)
            .expect_err("no section reads it")
            .to_string();
        assert_eq!(
            said,
            format!(
                "invalid setting in deslag.toml: no section reads .{extension} files; add a \
                 [cpp] section to the config"
            )
        );
    }
}
