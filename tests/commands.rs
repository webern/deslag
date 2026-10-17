//! Tests for the `repo_commands` lint: what is a claim, where `cd` and `-C` send it, what the
//! Makefile reader finds on disk, and what is never run. The cases in `tests/cases/repo_commands/`
//! pin the report.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::git::{commit, git, hermetic};
use common::{Repo, code, stderr};
use deslag::Document;
use deslag::config::RepoCommands;
use deslag::document::Location;
use deslag::lint::repo_commands::{
    Claim, Over, Reading, Shape, Why, check, marks, read, render, retain,
};
use deslag::lint::{Keep, MarkKind};

/// The claims of `text` as (call, targets) pairs.
fn claims(text: &str) -> Vec<(String, Vec<String>)> {
    read(&Document::markdown(text))
        .claims
        .into_iter()
        .map(|claim| {
            let targets = claim
                .targets
                .into_iter()
                .map(|target| target.name)
                .collect();
            (claim.text, targets)
        })
        .collect()
}

/// `(call, targets)` as [`claims`] gives them.
fn claim(call: &str, targets: &[&str]) -> (String, Vec<String>) {
    (
        call.to_string(),
        targets.iter().map(|target| target.to_string()).collect(),
    )
}

/// A fenced block of shell holding `body`.
fn sh(body: &str) -> String {
    format!("```sh\n{body}\n```\n")
}

/// The reading of `text`.
fn reading(text: &str) -> Reading {
    read(&Document::markdown(text))
}

/// The text that `location` covers in `text`.
fn held<'t>(text: &'t str, location: &Location) -> &'t str {
    &text[location.start..location.end]
}

#[test]
fn only_a_fence_that_names_a_shell_is_read() {
    let mut text = String::new();
    for info in [
        "sh",
        "bash",
        "shell",
        "zsh",
        "console",
        "BASH",
        "bash title=\"x\"",
        "{.sh}",
        "sh,ignore",
        ".zsh",
    ] {
        text.push_str(&format!("```{info}\nmake a\n```\n\n"));
    }
    for info in [
        "",
        "text",
        "fish",
        "powershell",
        "bat",
        "rust",
        "shellsession",
        "make",
        "toml",
    ] {
        text.push_str(&format!("```{info}\nmake b\n```\n\n"));
    }
    text.push_str("    make c\n");
    let reading = reading(&text);
    assert_eq!(reading.blocks, 10);
    assert_eq!(reading.claims.len(), 10);
    assert!(
        reading
            .claims
            .iter()
            .all(|claim| claim.targets[0].name == "a")
    );
    assert!(
        reading
            .claims
            .iter()
            .all(|claim| claim.shape == Shape::Block)
    );
}

#[test]
fn a_claim_is_a_call_of_make_with_a_target() {
    let text = sh("make a b\n\
                   gmake c\n\
                   sudo -E make d\n\
                   CC=gcc make e\n\
                   time -p make f\n\
                   env A=b make g\n\
                   cargo test\n\
                   ./scripts/x.sh\n\
                   make\n\
                   make -n -j4 V=1\n\
                   /usr/bin/make h\n\
                   $MAKE i\n\
                   make --jobs=4 --silent -- -j\n\
                   make a > out.log 2>&1\n\
                   echo make not");
    assert_eq!(
        claims(&text),
        [
            claim("make a b", &["a", "b"]),
            claim("gmake c", &["c"]),
            claim("make d", &["d"]),
            claim("make e", &["e"]),
            claim("make f", &["f"]),
            claim("make g", &["g"]),
            claim("make --jobs=4 --silent -- -j", &["-j"]),
            claim("make a", &["a"]),
        ]
    );
    let reading = reading(&text);
    // Every call of make is counted, with or without a target.
    assert_eq!(reading.calls, 10);
}

#[test]
fn a_code_span_is_a_claim_only_when_it_is_exactly_one_call() {
    let yes = [
        "`make ci`",
        "`make ci-fast check-release`",
        "` gmake x `",
        "`make -j4 -C sub build`",
        "`CC=gcc make x`",
        "``make x``",
        "`$ make x`",
        "[`make x`](docs/x.md)",
    ];
    for span in yes {
        let text = format!("# Title\n\nRun {span} now.\n");
        let found = reading(&text);
        assert_eq!(found.claims.len(), 1, "{span}");
        assert_eq!(found.claims[0].shape, Shape::Span, "{span}");
        assert_eq!(found.blocks, 0, "{span}");
    }
    let no = [
        "`make`",
        "`make -j4`",
        "`cd a && make x`",
        "`make x; make y`",
        "`make x || make y`",
        "`cargo make x`",
        "`makes x`",
        "`./make x`",
        "`make 'x`",
        "`ninja x`",
        "`x`",
        "make x",
        "**make x**",
    ];
    for span in no {
        let text = format!("Run {span} now.\n");
        assert_eq!(claims(&text), [], "{span}");
    }
}

#[test]
fn a_span_claim_is_at_its_whole_span_and_a_block_claim_is_at_its_words() {
    let text = "# T\n\n- step:\n\n  ```sh\n  cd sub\n  make one   two\n  ```\n\n> ```bash\n> make three\n> ```\n\nUse `make four`.\n";
    let found = reading(text);
    let seen: Vec<(Shape, &str, Vec<&str>, usize)> = found
        .claims
        .iter()
        .map(|claim| {
            let targets = claim
                .targets
                .iter()
                .map(|target| held(text, &target.location))
                .collect();
            (
                claim.shape,
                held(text, &claim.call),
                targets,
                claim.call.line,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (Shape::Block, "make one   two", vec!["one", "two"], 7),
            (Shape::Block, "make three", vec!["three"], 11),
            (Shape::Span, "`make four`", vec!["`make four`"], 14),
        ]
    );
    assert_eq!(found.claims[0].cds, ["sub"]);
    assert_eq!(found.claims[0].targets[1].location.line, 7);
}

#[test]
fn a_block_is_one_stream() {
    let text = sh("make a \\\n   b # make c\n\
                   echo \"x\n\
                   make d\" && make e\n\
                   cat <<EOF\n\
                   make f\n\
                   EOF\n\
                   make g");
    assert_eq!(
        claims(&text),
        [
            claim("make a b", &["a", "b"]),
            claim("make e", &["e"]),
            claim("make g", &["g"])
        ]
    );
}

#[test]
fn a_prompt_makes_every_other_line_output() {
    let text = "```console\n$ make a\nmake: *** No rule to make target 'a'.  Stop.\n$ make b \\\n    c\nmake d\n```\n\n\
                ```sh\nmake e\nmake f\n```\n";
    assert_eq!(
        claims(text),
        [
            claim("make a", &["a"]),
            claim("make b c", &["b", "c"]),
            claim("make e", &["e"]),
            claim("make f", &["f"]),
        ]
    );
}

#[test]
fn an_unclosed_quote_ends_the_claims_of_its_block_and_no_other() {
    let text = format!("{}\n{}", sh("make a\necho 'oops\nmake b"), sh("make c"));
    let found = reading(&text);
    let names: Vec<&str> = found
        .claims
        .iter()
        .map(|claim| claim.targets[0].name.as_str())
        .collect();
    assert_eq!(names, ["a", "c"]);
    assert_eq!(
        found
            .refused
            .iter()
            .map(|refused| refused.why)
            .collect::<Vec<_>>(),
        [Why::Unclosed]
    );
    assert_eq!(held(&text, &found.refused[0].location), "'");
}

#[test]
fn what_a_shell_or_a_reader_fills_in_is_not_a_target() {
    let text = sh("make $(T) a `t` b $T c [t] d {x,y} e t* f ... g h");
    let found = reading(&text);
    assert_eq!(found.claims.len(), 1);
    let names: Vec<&str> = found.claims[0]
        .targets
        .iter()
        .map(|target| target.name.as_str())
        .collect();
    assert_eq!(names, ["a", "b", "c", "d", "e", "f", "g", "h"]);
    assert_eq!(
        found
            .refused
            .iter()
            .filter(|r| r.why == Why::Target)
            .count(),
        7
    );
    // A call of nothing else claims nothing, and `<t>` is a redirection to a shell.
    assert_eq!(claims(&sh("make $(T) $U")), []);
    assert_eq!(claims("Run `make <target>`.\n"), []);
    assert_eq!(claims(&sh("make <target>")), []);
}

#[test]
fn a_call_that_names_its_makefile_or_an_unreadable_directory_is_left_alone() {
    let text = sh("make -f other a\nmake -C $D b\nmake --eval=x c\nmake d");
    let found = reading(&text);
    assert_eq!(found.claims.len(), 1);
    assert_eq!(
        found
            .refused
            .iter()
            .map(|refused| refused.why)
            .collect::<Vec<_>>(),
        [Why::Makefile, Why::Dir, Why::Makefile]
    );
    assert_eq!(held(&text, &found.refused[0].location), "make -f other a");
}

#[test]
fn a_cd_before_a_call_applies_in_order_and_a_subshell_keeps_its_own() {
    let text = sh("make a\n\
                   cd one\n\
                   make b\n\
                   (cd two && make c)\n\
                   make d\n\
                   cd -P three/four\n\
                   make e\n\
                   cd -- ..\n\
                   make f");
    let found = reading(&text);
    let cds: Vec<(&str, Vec<&str>)> = found
        .claims
        .iter()
        .map(|claim| {
            (
                claim.targets[0].name.as_str(),
                claim.cds.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        cds,
        [
            ("a", vec![]),
            ("b", vec!["one"]),
            ("c", vec!["one", "two"]),
            ("d", vec!["one"]),
            ("e", vec!["one", "three/four"]),
            ("f", vec!["one", "three/four", ".."]),
        ]
    );
}

#[test]
fn a_cd_that_cannot_be_followed_ends_the_claims_of_its_block() {
    for cd in [
        "cd",
        "cd -",
        "cd ~",
        "cd ~/x",
        "cd $HOME",
        "cd $(pwd)",
        "cd a b",
        "cd ../*",
        "pushd sub",
        "popd",
    ] {
        let text = sh(&format!("make a\n{cd}\nmake b\ncd c\nmake d"));
        let found = reading(&text);
        assert_eq!(found.claims.len(), 1, "{cd}");
        assert_eq!(found.calls, 3, "{cd}");
        let why = if cd.starts_with("pushd") || cd == "popd" {
            Why::Pushd
        } else {
            Why::Cd
        };
        assert_eq!(
            found
                .refused
                .iter()
                .map(|refused| refused.why)
                .collect::<Vec<_>>(),
            [why],
            "{cd}"
        );
        assert_eq!(held(&text, &found.refused[0].location), cd);
    }
    // The next block starts over.
    let text = format!("{}\n{}", sh("cd $X\nmake a"), sh("make b"));
    assert_eq!(claims(&text), [claim("make b", &["b"])]);
}

#[test]
fn a_c_flag_is_kept_as_written() {
    let found = reading(&sh("make -C a -Cb --directory=c --directory d x"));
    assert_eq!(found.claims[0].dirs, ["a", "b", "c", "d"]);
}

// On disk.

fn settings() -> RepoCommands {
    RepoCommands::default()
}

/// The targets of `text`, a file at `relative` in `repo`, that no Makefile defines.
fn missing(repo: &Repo, relative: &str, text: &str) -> Vec<String> {
    let dir = repo.root().join(relative);
    let dir = dir.parent().expect("a directory").to_path_buf();
    check(
        &Document::markdown(text),
        &dir,
        Some(repo.root()),
        Some(&settings()),
    )
    .map(|over| {
        over.missing
            .into_iter()
            .map(|missing| missing.target)
            .collect()
    })
    .unwrap_or_default()
}

fn repo_with(makefile: &str) -> Repo {
    let repo = Repo::new();
    repo.write("Makefile", makefile);
    repo
}

#[test]
fn a_target_the_makefile_lacks_is_missing_and_one_it_has_is_not() {
    let repo = repo_with("ci:\n\t@true\n.PHONY: test\n%.o: %.c\n");
    let text =
        "`make ci` `make cii` `make test` `make a.o` `make .o`\n\n```sh\nmake ci tset\n```\n";
    assert_eq!(missing(&repo, "README.md", text), ["cii", ".o", "tset"]);
}

#[test]
fn a_file_with_nothing_to_check_or_no_settings_is_not_checked() {
    let repo = repo_with("ci:\n");
    let document = Document::markdown("`make nope`\n");
    assert_eq!(check(&document, repo.root(), Some(repo.root()), None), None);
    let plain = Document::markdown("Nothing here.\n");
    assert_eq!(
        check(&plain, repo.root(), Some(repo.root()), Some(&settings())),
        None
    );
}

#[test]
fn a_repo_without_a_makefile_is_silent() {
    let repo = Repo::new();
    assert_eq!(
        missing(&repo, "README.md", "`make nope`\n"),
        Vec::<String>::new()
    );
    // A directory named Makefile is not one.
    fs::create_dir(repo.root().join("Makefile")).expect("a directory");
    assert_eq!(
        missing(&repo, "README.md", "`make nope`\n"),
        Vec::<String>::new()
    );
}

#[test]
fn make_reads_gnumakefile_before_makefile() {
    let repo = repo_with("m:\n");
    repo.write("GNUmakefile", "g:\n");
    assert_eq!(missing(&repo, "README.md", "`make g` `make m`\n"), ["m"]);

    let lower = Repo::new();
    lower.write("makefile", "l:\n");
    assert_eq!(missing(&lower, "README.md", "`make l` `make m`\n"), ["m"]);
}

#[test]
fn an_included_file_under_the_root_defines_targets() {
    let repo = repo_with("include mk/rules.mk\n-include more.mk\nall:\n");
    repo.write("mk/rules.mk", "extra:\ninclude mk/deeper.mk\n");
    repo.write("mk/deeper.mk", "deep:\n");
    repo.write("more.mk", "more:\n");
    let text = "`make all` `make extra` `make deep` `make more` `make zz`\n";
    assert_eq!(missing(&repo, "README.md", text), ["zz"]);
}

#[test]
fn an_include_that_cannot_be_read_leaves_the_makefile_open() {
    let closed = repo_with("all:\n");
    assert_eq!(missing(&closed, "README.md", "`make zz`\n"), ["zz"]);
    for line in [
        "include missing.mk",
        "-include missing.mk",
        "include $(DIR)/x.mk",
        "include mk/*.mk",
    ] {
        let repo = repo_with(&format!("{line}\nall:\n"));
        assert_eq!(
            missing(&repo, "README.md", "`make all` `make zz`\n"),
            Vec::<String>::new(),
            "{line}"
        );
    }
    // An include of itself ends.
    let looped = repo_with("include Makefile\nall:\n");
    assert_eq!(missing(&looped, "README.md", "`make zz`\n"), ["zz"]);
}

#[test]
fn a_file_outside_the_root_is_not_read() {
    let outside = Repo::new();
    outside.write("evil.mk", "evil:\n");
    outside.write("Makefile", "evil:\n");
    let included = repo_with(&format!(
        "include {}\nall:\n",
        outside.root().join("evil.mk").display()
    ));
    // Not read, so the Makefile cannot be told, and the call passes.
    assert_eq!(
        missing(&included, "README.md", "`make zz`\n"),
        Vec::<String>::new()
    );
    #[cfg(unix)]
    {
        let repo = Repo::new();
        std::os::unix::fs::symlink(
            outside.root().join("Makefile"),
            repo.root().join("Makefile"),
        )
        .expect("a symlink");
        assert_eq!(
            missing(&repo, "README.md", "`make zz`\n"),
            Vec::<String>::new()
        );
        // A symlink inside the root is read.
        let inside = Repo::new();
        inside.write("real.mk", "all:\n");
        std::os::unix::fs::symlink(
            inside.root().join("real.mk"),
            inside.root().join("Makefile"),
        )
        .expect("a symlink");
        assert_eq!(
            missing(&inside, "README.md", "`make all` `make zz`\n"),
            ["zz"]
        );
    }
}

#[test]
fn an_open_makefile_passes_a_target_it_lacks_and_not_one_it_has_wrong() {
    for makefile in [
        "$(eval $(call rule,a))\nall:\n",
        "$(TARGETS): x\nall:\n",
        ".PHONY: $(T)\nall:\n",
        "$(call rules)\nall:\n",
    ] {
        let repo = repo_with(makefile);
        assert_eq!(
            missing(&repo, "README.md", "`make all` `make zz`\n"),
            Vec::<String>::new(),
            "{makefile}"
        );
    }
    // An open Makefile in one place does not open the others.
    let repo = repo_with("all:\n");
    repo.write("sub/Makefile", "$(eval x)\n");
    assert_eq!(
        missing(&repo, "README.md", "`make -C sub zz`\n"),
        Vec::<String>::new()
    );
    assert_eq!(missing(&repo, "README.md", "`make zz`\n"), ["zz"]);
}

#[test]
fn a_call_passes_if_any_place_it_may_run_defines_the_target() {
    let repo = repo_with("root:\n");
    repo.write("docs/Makefile", "docs:\n");
    repo.write("docs/guide.md", "");
    repo.write("other/Makefile", "other:\n");
    let text = "`make root` `make docs` `make other` `make zz`\n";
    assert_eq!(missing(&repo, "docs/guide.md", text), ["other", "zz"]);
    assert_eq!(missing(&repo, "README.md", text), ["docs", "other", "zz"]);
}

#[test]
fn c_applies_to_each_place_and_stays_under_the_root() {
    let repo = repo_with("root:\n");
    repo.write("sub/Makefile", "s:\n");
    repo.write("sub/deeper/Makefile", "d:\n");
    repo.write("docs/Makefile", "docs:\n");
    repo.write("docs/sub/Makefile", "ds:\n");
    let text = "`make -C sub s` `make -C sub root` `make -C sub -C deeper d` `make -Csub d` `make --directory sub s`\n";
    assert_eq!(missing(&repo, "README.md", text), ["root", "d"]);
    // From docs/, `-C sub` is docs/sub as well as sub.
    assert_eq!(
        missing(
            &repo,
            "docs/guide.md",
            "`make -C sub s` `make -C sub ds` `make -C sub zz`\n"
        ),
        ["zz"]
    );
    // A directory that is not there, or is above the root, leaves nothing to check.
    for call in ["make -C nowhere zz", "make -C .. zz", "make -C /tmp zz"] {
        assert_eq!(
            missing(&repo, "README.md", &format!("`{call}`\n")),
            Vec::<String>::new(),
            "{call}"
        );
    }
}

#[test]
fn a_cd_moves_the_block_to_a_directory_under_the_root() {
    let repo = repo_with("root:\n");
    repo.write("sub/Makefile", "s:\n");
    repo.write("sub/more/Makefile", "m:\n");
    let in_block = |body: &str| missing(&repo, "README.md", &sh(body));
    assert_eq!(in_block("cd sub\nmake s root zz"), ["zz"]);
    assert_eq!(in_block("cd sub && cd more && make m s zz"), ["s", "zz"]);
    assert_eq!(in_block("cd sub/more\nmake m"), Vec::<String>::new());
    assert_eq!(in_block("(cd sub && make s)\nmake s"), ["s"]);
    assert_eq!(in_block("cd sub\ncd ..\nmake s"), ["s"]);
    assert_eq!(in_block("cd ./sub/\nmake s"), Vec::<String>::new());
}

#[test]
fn a_cd_to_a_directory_that_is_missing_or_not_under_the_root_leaves_the_rest_of_the_block_silent() {
    let outer = Repo::new();
    outer.write("Makefile", "up:\n");
    outer.write("inner/Makefile", "in:\n");
    outer.write("inner/README.md", "");
    let root = outer.root().join("inner");
    let run = |body: &str| {
        let text = sh(body);
        let document = Document::markdown(&text);
        check(&document, &root, Some(&root), Some(&settings()))
            .map(|over| over.missing.len())
            .unwrap_or_default()
    };
    // Followed.
    assert_eq!(run("make zz"), 1);
    // Not there, because the block makes it, or because the doc is wrong.
    assert_eq!(run("git clone x\ncd x\nmake zz"), 0);
    assert_eq!(run("mkdir -p out && cd out && make zz"), 0);
    // Above the root, and an absolute path elsewhere: never followed.
    assert_eq!(run("cd ..\nmake zz"), 0);
    assert_eq!(run("cd ../..\nmake up"), 0);
    assert_eq!(run(&format!("cd {}\nmake zz", outer.root().display())), 0);
    assert_eq!(run("cd /\nmake zz"), 0);
    // Calls before it are still checked.
    assert_eq!(run("make zz\ncd ..\nmake zz"), 1);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outer.root(), root.join("link")).expect("a symlink");
        assert_eq!(run("cd link\nmake zz"), 0);
    }
}

#[test]
fn a_run_on_one_file_goes_no_further_than_its_directory() {
    let repo = repo_with("root:\n");
    repo.write("docs/Makefile", "docs:\n");
    repo.write("docs/sub/Makefile", "ds:\n");
    let dir = repo.root().join("docs");
    let found = |text: &str| {
        check(&Document::markdown(text), &dir, None, Some(&settings()))
            .map(|over| {
                over.missing
                    .into_iter()
                    .map(|m| m.target)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        found("`make docs` `make root` `make -C sub ds`\n"),
        ["root"]
    );
    assert_eq!(found(&sh("cd ..\nmake zz")), Vec::<String>::new());
    assert_eq!(found(&sh("cd sub\nmake ds zz")), ["zz"]);
}

#[test]
fn a_makefile_is_read_and_never_run() {
    let repo = repo_with(
        "X := $(shell touch ran-shell)\n\
         Y != touch ran-bang\n\
         $(info $(shell touch ran-info))\n\
         include $(shell touch ran-include)\n\
         all:\n\t@touch ran-recipe\n\
         ran-target:\n\t@touch ran-target\n",
    );
    let text = "`make all` `make -n all` `make ran-target`\n\n```sh\nmake all\nmake zz\n```\n";
    let before = missing(&repo, "README.md", text);
    let mut ran: Vec<String> = fs::read_dir(repo.root())
        .expect("a directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.starts_with("ran-"))
        .collect();
    ran.sort();
    assert_eq!(ran, Vec::<String>::new());
    // The include names a variable, so the Makefile is open, and zz passes.
    assert_eq!(before, Vec::<String>::new());
}

#[test]
fn the_report_gives_the_file_the_line_the_call_and_the_target() {
    let repo = repo_with("ci:\n");
    let text = "# T\n\nRun `make ci-fats` first.\n\n```sh\nmake ci\nmake -j4 ci-fast ci\n```\n";
    let document = Document::markdown(text);
    let over =
        check(&document, repo.root(), Some(repo.root()), Some(&settings())).expect("a finding");
    let report = render("docs/RUN.md", &over);
    let expected = "\
ERROR: deslag detected a make target that does not exist!

docs/RUN.md names 2 make targets that no Makefile defines.

";
    assert!(report.starts_with(expected), "{report}");
    assert!(
        report.ends_with(
            "The calls:\n  line 3: `make ci-fats`: no Makefile defines ci-fats\n  \
             line 7: `make -j4 ci-fast ci`: no Makefile defines ci-fast"
        ),
        "{report}"
    );
    assert!(report.contains("never runs make"), "{report}");
    assert!(
        report.contains("Do not change the language of a code block"),
        "{report}"
    );
    // It does not list what exists.
    assert!(!report.contains("ci:"), "{report}");
}

#[test]
fn a_message_replaces_the_advice() {
    let repo = repo_with("ci:\n");
    let settings = RepoCommands {
        message: Some("Fix {path}.".to_string()),
    };
    let document = Document::markdown("`make nope`\n");
    let over =
        check(&document, repo.root(), Some(repo.root()), Some(&settings)).expect("a finding");
    assert_eq!(
        render("A.md", &over),
        "ERROR: deslag detected a make target that does not exist!\n\n\
         A.md names 1 make target that no Makefile defines.\n\n\
         Fix A.md.\n\n\
         The calls:\n  line 1: `make nope`: no Makefile defines nope"
    );
}

/// Keeps the occurrences on `lines`.
struct Lines(&'static [usize]);

impl Keep for Lines {
    fn occurrence(&self, location: &Location) -> bool {
        self.0.contains(&location.line)
    }

    fn verdict(&self) -> bool {
        false
    }
}

#[test]
fn each_target_is_an_occurrence_and_a_change_keeps_those_on_its_lines() {
    let repo = repo_with("ci:\n");
    let text = "`make a`\n\n```sh\nmake ci b c\n```\n";
    let document = Document::markdown(text);
    let over =
        check(&document, repo.root(), Some(repo.root()), Some(&settings())).expect("a finding");

    let marks = marks(&over);
    assert_eq!(marks.len(), 3);
    assert!(marks.iter().all(|mark| mark.kind == MarkKind::Occurrence));
    let notes: Vec<&str> = marks.iter().map(|mark| mark.note.as_str()).collect();
    assert_eq!(
        notes,
        [
            "`make a`: no Makefile defines a",
            "`make ci b c`: no Makefile defines b",
            "`make ci b c`: no Makefile defines c",
        ]
    );
    let lines: Vec<usize> = marks.iter().map(|mark| mark.location.line).collect();
    assert_eq!(lines, [1, 4, 4]);

    let kept: Over = retain(&over, &Lines(&[4])).expect("targets kept");
    assert_eq!(kept.missing.len(), 2);
    assert_eq!(retain(&over, &Lines(&[2])), None);
}

#[test]
fn it_is_a_lint_with_no_edits_and_a_retain_that_matches_its_marks() {
    use deslag::Violation;
    let repo = repo_with("ci:\n");
    let document = Document::markdown("`make a`\n");
    let over =
        check(&document, repo.root(), Some(repo.root()), Some(&settings())).expect("a finding");
    let violation = Violation::RepoCommands(over);
    assert!(violation.edits().is_empty());
    assert_eq!(violation.marks().len(), 1);
    assert_eq!(violation.retain(&Lines(&[1])), Some(violation.clone()));
    assert_eq!(violation.retain(&Lines(&[])), None);
}

fn claim_is_block(claim: &Claim) -> bool {
    claim.shape == Shape::Block
}

#[test]
fn a_block_claim_and_a_span_claim_are_told_apart() {
    let found = reading("`make a`\n\n```sh\nmake b\n```\n");
    assert_eq!(
        found.claims.iter().map(claim_is_block).collect::<Vec<_>>(),
        [false, true]
    );
}

// The limit of `--diff`.

/// Runs deslag with `args` in `dir`, and returns its exit code and what it printed on stderr.
fn deslag(dir: &Path, args: &[&str]) -> (i32, String) {
    let output = hermetic(
        Command::new(env!("CARGO_BIN_EXE_deslag"))
            .args(args)
            .current_dir(dir),
    )
    .output()
    .expect("deslag runs");
    (code(&output), stderr(&output))
}

#[test]
fn diff_misses_a_target_renamed_in_the_makefile_alone() {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write(
        "deslag.toml",
        "schema_version = 1\n\n[md.lints.repo_commands]\n",
    );
    repo.write("Makefile", "build:\n\t@true\n");
    repo.write("README.md", "# T\n\nRun `make build`.\n");
    commit(repo.root(), "base");
    assert_eq!(deslag(repo.root(), &["check"]).0, 0);

    // The Makefile lies about nothing it had to, and the doc is untouched.
    repo.write("Makefile", "compile:\n\t@true\n");
    commit(repo.root(), "rename the target");
    let (full, report) = deslag(repo.root(), &["check"]);
    assert_eq!(full, 1, "{report}");
    assert!(report.contains("README.md names 1 make target"), "{report}");
    let (diff, report) = deslag(repo.root(), &["check", "--diff", "HEAD~1"]);
    assert_eq!(diff, 0, "{report}");

    // Touching the doc's line brings the finding back.
    repo.write("README.md", "# T\n\nRun `make build` now.\n");
    let (diff, report) = deslag(repo.root(), &["check", "--diff", "HEAD~1"]);
    assert_eq!(diff, 1, "{report}");
}
