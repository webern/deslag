//! `deslag init`: it writes the config with everything on where the config search finds none, and
//! refuses, writing nothing, where it finds one.
//!
//! `tests/instructions.rs` holds what the config says to the lints pages; these tests run the
//! command.

use crate::common;

use common::{Repo, code, raw_stderr};
use deslag::config::SCHEMA_VERSION;
use deslag::{Config, ConfigSource};

/// The commands a refusal sends the agent to.
const POINTERS: [&str; 3] = [
    "`deslag instructions lints`",
    "`deslag explain <PATH>`",
    "`deslag update`",
];

/// What `init` wrote at `relative` in `repo`.
fn read(repo: &Repo, relative: &str) -> String {
    std::fs::read_to_string(repo.root().join(relative)).expect("a config")
}

#[test]
fn it_writes_deslag_toml_which_check_passes_and_a_second_run_refuses() {
    let repo = Repo::new();
    let output = repo.run(&["init"]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert!(
        raw_stderr(&output).starts_with("deslag: wrote deslag.toml, "),
        "{}",
        raw_stderr(&output)
    );
    let text = read(&repo, "deslag.toml");
    let config = Config::parse(&text, "deslag.toml".into(), ConfigSource::Canonical)
        .expect("the config loads");
    assert_eq!(config.schema_version(), SCHEMA_VERSION);
    assert_eq!(
        config.stamp().map(ToString::to_string).as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );

    let check = repo.check();
    assert_eq!(code(&check), 0, "{}", raw_stderr(&check));
    assert_eq!(raw_stderr(&check), "");

    let again = repo.run(&["init"]);
    assert_eq!(code(&again), 2);
    assert!(raw_stderr(&again).contains("found the config deslag.toml"));
    assert_eq!(read(&repo, "deslag.toml"), text);
}

#[test]
fn it_refuses_where_a_config_is_in_any_language_and_names_it() {
    for relative in [".agents/deslag.toml", "deslag.yaml", "config/deslag.json"] {
        let repo = Repo::new();
        repo.write(relative, "schema_version = 2\n");
        let output = repo.run(&["init"]);
        assert_eq!(code(&output), 2, "{relative}");
        let said = raw_stderr(&output);
        assert!(
            said.contains(&format!("found the config {relative},")),
            "{said}"
        );
        for pointer in POINTERS {
            assert!(said.contains(pointer), "{said}");
        }
        assert!(!repo.root().join("deslag.toml").exists(), "{relative}");
    }
}

#[test]
fn it_refuses_two_configs_at_one_location() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 2\n");
    repo.write("deslag.json", "{}");
    let output = repo.run(&["init"]);
    assert_eq!(code(&output), 2);
    let said = raw_stderr(&output);
    assert!(
        said.contains("deslag.toml") && said.contains("deslag.json"),
        "{said}"
    );
    assert_eq!(read(&repo, "deslag.toml"), "schema_version = 2\n");
}

#[test]
fn config_path_names_the_file_it_writes_which_must_be_toml() {
    let repo = Repo::new();
    let output = repo.run(&["init", "--config-path", "lint.toml"]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert!(!repo.root().join("deslag.toml").exists());
    let check = repo.run(&["check", "--config-path", "lint.toml"]);
    assert_eq!(code(&check), 0, "{}", raw_stderr(&check));

    let again = repo.run(&["init", "--config-path", "lint.toml"]);
    assert_eq!(code(&again), 2);
    assert!(raw_stderr(&again).contains("found the config lint.toml"));

    let yaml = repo.run(&["init", "--config-path", "lint.yaml"]);
    assert_eq!(code(&yaml), 2);
    assert!(raw_stderr(&yaml).contains("deslag init writes TOML"));
    assert!(!repo.root().join("lint.yaml").exists());
}
