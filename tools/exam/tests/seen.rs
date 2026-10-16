//! The sidecars of the gold sets and the `Seen` origin they give: the checked-in files load, the
//! two ways a set reaches Python give one `Origin=` column, a sidecar error on holdout text names no
//! silver batch, and, with the big tier fetched, each sidecar is what its fixtures give. No failure
//! message here carries a word of a gold set.

use std::path::{Path, PathBuf};

use deslag_exam::error::Error;
use deslag_exam::gold::Gold;
use deslag_exam::{readings, seen, skeleton};

const ROOT: &str = "../..";
const SETS: [&str; 3] = ["dev", "holdout", "owner"];

fn gold_path(name: &str) -> PathBuf {
    Path::new(ROOT)
        .join("tests/gold")
        .join(format!("{name}.conllu"))
}

/// The `Origin=` of each token line of a skeleton or a readings file, in order.
fn origin_column(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let misc = line.split('\t').nth(9).unwrap_or("");
            misc.split('|')
                .find_map(|pair| pair.strip_prefix("Origin="))
                .unwrap_or("")
        })
        .collect()
}

#[test]
fn every_checked_in_sidecar_loads_and_names_words() {
    for name in SETS {
        let gold = Gold::read(&gold_path(name)).unwrap_or_else(|error| panic!("{error}"));
        let with = gold
            .sentences
            .iter()
            .filter(|s| !s.code_ids.is_empty())
            .count();
        assert!(with > 0, "the {name} sidecar names no word");
        for sentence in &gold.sentences {
            assert!(sentence.code_words.len() <= sentence.code_ids.len());
        }
    }
}

#[test]
fn a_skeleton_and_the_gold_give_one_origin_column() {
    for name in ["dev", "owner"] {
        let gold = Gold::read(&gold_path(name)).unwrap();
        let skeleton_text = skeleton::skeleton(&gold);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("skeleton.conllu");
        std::fs::write(&path, &skeleton_text).unwrap();
        let (from_skeleton, sentences) = readings::of_skeleton(&path, false).unwrap();
        let (from_gold, same) = readings::of_gold(&gold).unwrap();
        assert_eq!(sentences, same);
        let (a, b, c) = (
            origin_column(&skeleton_text),
            origin_column(&from_skeleton),
            origin_column(&from_gold),
        );
        assert!(
            a.len() == b.len() && b.len() == c.len(),
            "{name}: token counts differ"
        );
        assert!(
            b == c,
            "{name}: the readings differ in their Origin= column"
        );
        assert!(a == b, "{name}: the skeleton differs in its Origin= column");
        assert!(
            b.contains(&"Seen"),
            "{name}: no word in the readings has Origin=Seen"
        );
    }
}

/// With the big tier fetched, each checked-in sidecar is what its fixtures give. Run by
/// `make test-blobs`.
#[test]
#[ignore = "needs the big tier: make fetch-blobs"]
fn each_sidecar_is_what_the_fixtures_give() {
    let corpus = Path::new(ROOT).join(".blobs/unpacked/corpus");
    for name in SETS {
        let gold = Gold::read_bare(&gold_path(name)).unwrap_or_else(|error| panic!("{error}"));
        let places = seen::locate(&gold, None).unwrap_or_else(|error| panic!("{error}"));
        let ids = seen::compute(&gold, &places, &corpus).unwrap_or_else(|error| panic!("{error}"));
        let held = std::fs::read_to_string(seen::path_of(&gold.path)).unwrap();
        assert!(
            held == seen::render(&gold, &ids),
            "the {name} sidecar is not what its fixtures give: `deslag-gold seen` writes it again"
        );
    }
}

/// A one-sentence `deslag` file that names the silver batch `ZZBATCHNAME`, so it needs a sidecar,
/// with the holdout header when `headed`.
fn silver_gold(headed: bool) -> String {
    let split = if headed {
        "# exam.split = holdout\n# exam.trains = no\n"
    } else {
        ""
    };
    format!(
        "# exam.tokens = deslag\n{split}# silver.batch = ZZBATCHNAME\n# sent_id = a\n# text = x\n\
         1\tx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n"
    )
}

/// A way to read a gold file and its sidecar.
type Read = fn(&Path) -> Result<Gold, Error>;

/// The errors reading a silver gold file from a scratch directory with `read`, once with no
/// sidecar and once with a stale one.
fn sidecar_errors(headed: bool, read: Read) -> [String; 2] {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.conllu");
    std::fs::write(&path, silver_gold(headed)).unwrap();
    let missing = read(&path).expect_err("a missing sidecar is an error");
    std::fs::write(seen::path_of(&path.to_string_lossy()), "# seen_rule = 0\n").unwrap();
    let stale = read(&path).expect_err("a stale sidecar is an error");
    [missing.to_string(), stale.to_string()]
}

/// The command a sidecar error suggests names the silver batch, which is a comment's value, unless
/// the file is holdout text, whether its header says so or its reader does.
#[test]
fn a_holdout_sidecar_error_does_not_name_the_silver_batch() {
    let manifest = |batch: &str| format!("--manifest .blobs/unpacked/silver/{batch}/manifest.tsv");
    for error in sidecar_errors(false, Gold::read) {
        assert!(error.contains(&manifest("ZZBATCHNAME")), "{error}");
    }
    let holdout: [(&str, bool, Read); 3] = [
        ("a header read", true, Gold::read),
        ("a forced read", false, Gold::read_holdout),
        ("a forced read of a headed file", true, Gold::read_holdout),
    ];
    for (what, headed, read) in holdout {
        for error in sidecar_errors(headed, read) {
            assert!(
                !error.contains("ZZBATCHNAME"),
                "{what} echoes the batch: {error}"
            );
            assert!(error.contains(&manifest("<batch>")), "{what}: {error}");
        }
    }
}
