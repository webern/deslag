//! The sweep end to end: through the library with a scanner defined here, and through the binary
//! with none, over the fixtures in `tests/fixtures/`.
//!
//! The golden files hold what the sweep prints, lock digest included, so they change whenever
//! `Cargo.lock` does. Rerun with `BLESS=1` to rewrite them, and read the diff.

use std::path::{Path, PathBuf};
use std::process::Command;

use deslag_sweep::lang::Lang;
use deslag_sweep::lexer::{Kind, Lexed, Lexer, Span};
use deslag_sweep::sweep;

/// A scanner that is wrong the obvious way: every `//` starts a comment that runs to the end of
/// the line, and there are no block comments, strings or chars.
struct Naive;

impl Lexer for Naive {
    fn lex(&mut self, src: &str) -> Lexed {
        let mut spans = Vec::new();
        let mut line_start = 0;
        for line in src.split_inclusive('\n') {
            if let Some(at) = line.find("//") {
                let text = line.trim_end_matches(['\n', '\r']);
                spans.push(Span {
                    range: line_start + at..line_start + text.len(),
                    kind: Kind::Comment,
                });
            }
            line_start += line.len();
        }
        Lexed {
            spans,
            blind: Vec::new(),
            clean: true,
        }
    }
}

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures").join(name)
}

/// Compares `actual` with the golden file `name`, or rewrites it when `BLESS` is set.
fn assert_golden(name: &str, actual: &str) {
    let path = manifest_dir().join("tests/golden").join(name);
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        actual, expected,
        "{name} differs from what the sweep prints; rerun with BLESS=1 to rewrite it"
    );
}

#[test]
fn a_naive_scanner_differs_from_the_rust_oracle() {
    let report = sweep(Lang::Rust, &[fixtures("rust")], Some(Box::new(Naive))).unwrap();
    assert_eq!(report.exit_code(), 1);
    let toml = report.to_toml();
    assert_golden("rust-naive.toml", &toml);
    // It finds `//` inside strings, and misses what is not a line comment.
    assert!(toml.contains("only_scanner = "));
    assert!(!toml.contains("[clean.comment]\noracle = 0"));
    assert!(
        report
            .samples_text()
            .contains("clean.comment.only_scanner ")
    );
    assert!(report.samples_text().contains("clean.str.only_oracle "));
}

#[test]
fn a_naive_scanner_differs_from_the_c_oracle() {
    let report = sweep(Lang::C, &[fixtures("c")], Some(Box::new(Naive))).unwrap();
    assert_eq!(report.exit_code(), 1);
    assert_golden("c-naive.toml", &report.to_toml());
}

#[test]
fn an_oracle_checked_against_itself_agrees() {
    // The oracle as the scanner: every span agrees, whatever the file, so exit 0.
    for (lang, root) in [(Lang::Rust, "rust"), (Lang::C, "c")] {
        let scanner = lang.oracle().unwrap();
        let report = sweep(lang, &[fixtures(root)], Some(scanner)).unwrap();
        assert_eq!(report.exit_code(), 0, "{root}");
        assert!(report.samples_text().is_empty(), "{root}");
    }
}

#[test]
fn the_c_define_bodies_are_counted_apart_from_the_misses() {
    // A scanner that reads the oracle's spans and adds a string inside a `#define` body, where the
    // oracle cannot look. Those are `oracle_blind` and not `only_scanner`, and exit 0.
    struct WithDefineStrings(Box<dyn Lexer>);
    impl Lexer for WithDefineStrings {
        fn lex(&mut self, src: &str) -> Lexed {
            let mut lexed = self.0.lex(src);
            for (at, _) in src.match_indices("\"text\"") {
                lexed.spans.push(Span {
                    range: at..at + 6,
                    kind: Kind::Str,
                });
            }
            lexed.blind.clear();
            lexed
        }
    }
    let scanner = Box::new(WithDefineStrings(Lang::C.oracle().unwrap()));
    let report = sweep(Lang::C, &[fixtures("c")], Some(scanner)).unwrap();
    assert!(report.to_toml().contains("oracle_blind = 1"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn the_rust_oracle_alone_matches_its_golden_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_deslag-sweep"))
        .current_dir(manifest_dir())
        .args(["rust", "tests/fixtures/rust"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_golden("rust.toml", std::str::from_utf8(&output.stdout).unwrap());
}

#[test]
fn the_c_oracle_alone_matches_its_golden_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_deslag-sweep"))
        .current_dir(manifest_dir())
        .args(["c", "tests/fixtures/c"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_golden("c.toml", std::str::from_utf8(&output.stdout).unwrap());
}

#[test]
fn stdout_names_no_absolute_path_and_is_the_same_from_anywhere() {
    let sweep_rust = |current_dir: &Path, root: &Path| {
        let output = Command::new(env!("CARGO_BIN_EXE_deslag-sweep"))
            .current_dir(current_dir)
            .arg("rust")
            .arg(root)
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap()
    };
    let relative = sweep_rust(manifest_dir(), Path::new("tests/fixtures/rust"));
    let absolute = sweep_rust(Path::new("/"), &fixtures("rust"));
    assert!(!absolute.contains(env!("CARGO_MANIFEST_DIR")));
    assert_eq!(relative, absolute);
}

#[test]
fn a_run_that_cannot_happen_exits_2() {
    let bin = env!("CARGO_BIN_EXE_deslag-sweep");
    for args in [
        &[][..],
        &["rust"][..],
        &["go", "tests/fixtures/rust"][..],
        &["rust", "tests/fixtures/no-such-directory"][..],
        &["rust", "tests/fixtures/rust/empty.rs"][..],
    ] {
        let output = Command::new(bin)
            .current_dir(manifest_dir())
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}
