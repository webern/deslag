//! Built silver batches: what the assembler writes, what `silver check` finds wrong with an
//! edited one, what `silver standing` holds a live one to, and the audit that a batch carries.
//! The parts are made-up sentences labelled as the pipeline labels them; nothing here opens a
//! holdout, owner or dev gold, and nothing needs a model, a key or the network.

use std::fs;
use std::path::{Path, PathBuf};

mod common;
use common::draws::tree_bytes;
use common::silver_parts::{Made, Ran, reviewed, sha256};

/// Sets cell `column` (from 0) of the row of `id` in the table `name` of `dir`.
fn set_cell(dir: &Path, name: &str, id: &str, column: usize, value: &str) {
    edit(dir, name, |text| {
        text.lines()
            .map(|line| {
                let mut cells: Vec<&str> = line.split('\t').collect();
                if cells[0] == id {
                    cells[column] = value;
                }
                format!("{}\n", cells.join("\t"))
            })
            .collect()
    });
}

/// The fewest sentences an audit holds without the owner's acceptance.
const LEAST_SENTENCES: usize = 50;

/// The name of the batch every test builds.
const NAME: &str = "2026-10-08-fixture";

/// Two parts, built into a batch.
fn built() -> (Made, PathBuf) {
    let made = Made::new();
    made.build(NAME, &[]).ok();
    let dir = made.out(NAME);
    (made, dir)
}

/// Two parts big enough for an audit of 50 sentences, built into a batch.
fn built_big() -> (Made, PathBuf) {
    let made = Made::with_mix(2, "12,6,0,0", 12);
    made.build(NAME, &[]).ok();
    let dir = made.out(NAME);
    (made, dir)
}

/// `silver check --batch DIR`.
fn check(made: &Made, dir: &Path) -> Ran {
    made.gold(&["silver", "check", "--batch", dir.to_str().unwrap()])
}

/// `silver standing`, against the made gold directory, with the live batches in `made.silver`.
fn standing(made: &Made) -> Ran {
    let mut args: Vec<String> = ["silver", "standing"]
        .into_iter()
        .map(str::to_string)
        .collect();
    args.extend(made.pool_args());
    args.extend(made.corpus_args());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    made.gold(&args)
}

/// Copies the directory `from` to `to`.
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Makes the batch `dir` live, as `silver/<name>` of the unpacked image.
fn make_live(made: &Made, dir: &Path) -> PathBuf {
    let live = made.silver.join(dir.file_name().unwrap());
    copy_dir(dir, &live);
    live
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn edit(dir: &Path, name: &str, change: impl Fn(&str) -> String) {
    let text = read(dir, name);
    let changed = change(&text);
    assert_ne!(text, changed, "the edit of {name} changed nothing");
    write(dir, name, &changed);
}

#[test]
fn parts_become_a_batch_with_every_file_of_the_layout_and_it_checks() {
    let (made, dir) = built();
    let files: Vec<String> = tree_bytes(&dir).into_keys().collect();
    let mut wanted: Vec<&str> = vec![
        "DATASHEET.md",
        "manifest.tsv",
        "record/agent.json",
        "record/datasheet.json",
        "record/datasheet.tmpl.md",
        "record/drops.tsv",
        "record/kit.tsv",
        "record/voters.json",
        "runs.tsv",
        "silver.conllu",
        "sources.tsv",
    ];
    for part in ["01", "02"] {
        for name in [
            "adjudicated.tsv",
            "adjudicator.json",
            "agreement.txt",
            "unsettled.tsv",
            "voters.tsv",
            "worklist.tsv",
        ] {
            wanted.push(Box::leak(format!("parts/{part}/{name}").into_boxed_str()));
        }
    }
    for path in &wanted {
        assert!(
            files.iter().any(|file| file == path),
            "no {path} in {files:?}"
        );
    }
    assert!(
        files.iter().any(|f| f == "listings/st1/r1.json"),
        "{files:?}"
    );
    // No audit was given, so none was made up.
    assert!(
        !files.iter().any(|file| file.starts_with("audit/")),
        "{files:?}"
    );

    let said = check(&made, &dir).ok();
    assert!(
        said.out.contains(&format!(
            "silver check: {NAME} passes: 8 sentences, 147 words, 2 parts, 10 runs"
        )),
        "{}",
        said.out
    );

    // The sentences, in id order, with the header that marks them as training labels.
    let silver = read(&dir, "silver.conllu");
    assert!(silver.starts_with(&format!(
        "# exam.tokens = deslag\n# exam.trains = yes\n# silver.batch = {NAME}\n"
    )));
    let ids: Vec<&str> = silver
        .lines()
        .filter_map(|l| l.strip_prefix("# sent_id = "))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
    assert_eq!(ids.len(), 8);
    // Every word has its provenance and its runs.
    for line in silver
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let misc = line.split('\t').nth(9).unwrap();
        assert!(misc.contains("Kind="), "{line}");
        if misc.contains("Kind=Word") {
            assert!(misc.contains("Prov=") && misc.contains("Runs="), "{line}");
        }
    }

    // The manifest has the draw's columns, the split of each repository and the part.
    let manifest = read(&dir, "manifest.tsv");
    let head: Vec<&str> = manifest
        .lines()
        .find(|l| l.starts_with("sent_id"))
        .unwrap()
        .split('\t')
        .collect();
    assert_eq!(head.len(), 14);
    assert_eq!(head[1], "split");
    assert_eq!(head[13], "part");
    for line in manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("sent_id"))
    {
        let cells: Vec<&str> = line.split('\t').collect();
        let first =
            u8::from_str_radix(&sha256(cells[5].to_lowercase().as_bytes())[..2], 16).unwrap();
        let split = if first % 10 == 0 { "tune" } else { "train" };
        assert_eq!(cells[1], split, "{line}");
        assert!(cells[13] == "01" || cells[13] == "02", "{line}");
    }

    // The parts keep no paths and no form of an unsettled word.
    assert_eq!(
        read(&dir, "parts/01/voters.tsv")
            .lines()
            .find(|l| l.starts_with("letter"))
            .unwrap(),
        "letter\tvoter\tbase_only\trun"
    );
    let unsettled = read(&dir, "parts/02/unsettled.tsv");
    assert_eq!(
        unsettled,
        "tier\tcontext\tsentences\twords\nmixed\tprose\t1\t1\n"
    );
    // Only the runs the words and the voters name, one agent record, the voters.json as it was.
    assert_eq!(read(&dir, "runs.tsv").lines().count(), 11);
    let agent: serde_json::Value = serde_json::from_str(&read(&dir, "record/agent.json")).unwrap();
    assert_eq!(agent["safe_mode"], true);
    assert_eq!(
        read(&dir, "record/voters.json"),
        fs::read_to_string(&made.voters).unwrap()
    );
    let kit = read(&dir, "record/kit.tsv");
    assert!(kit.contains("\ncheck_version\t1\n"), "{kit}");
    assert!(
        kit.contains(&format!(
            "\nagent_sha256\t{}\n",
            sha256(agent.to_string().as_bytes())
        )),
        "{kit}"
    );
    assert!(kit.contains("\narchive_sha256\t-\n"), "{kit}");
    // The sheet names the batch and has no unfilled tag.
    let sheet = read(&dir, "DATASHEET.md");
    assert!(sheet.starts_with(&format!("# Silver set {NAME}\n")));
    assert!(!sheet.contains("{{"), "{sheet}");
    // Part 02 left a sentence out unsettled and nothing was dropped: each part's agreement.txt is
    // still its merge's, over the sentence left out too, and the sheet says so.
    assert!(read(&dir, "record/drops.tsv").lines().count() == 1);
    assert!(
        read(&dir, "record/datasheet.json").contains("\"agreement_after_drops\": \"no\""),
        "{}",
        read(&dir, "record/datasheet.json")
    );
    assert!(
        sheet.contains("the dropped and the unsettled ones among them"),
        "{sheet}"
    );
    // The same parts give the same bytes.
    made.build("2026-10-08-again", &[]).ok();
    let again = tree_bytes(&made.out("2026-10-08-again"));
    let first = tree_bytes(&dir);
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        again.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in &first {
        if path == "record/kit.tsv"
            || path == "manifest.tsv"
            || path == "silver.conllu"
            || path == "DATASHEET.md"
            || path == "record/datasheet.json"
        {
            continue; // each names its batch
        }
        assert_eq!(bytes, &again[path], "{path}");
    }
}

#[test]
fn silver_conllu_is_read_by_the_exam_loader_and_by_readings() {
    let (made, dir) = built();
    let out = made.root.join("readings.conllu");
    let ran = std::process::Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .args(["readings", "--gold"])
        .arg(dir.join("silver.conllu"))
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert!(String::from_utf8_lossy(&ran.stdout).contains("wrote 8 sentences"));
    let readings = fs::read_to_string(out).unwrap();
    assert!(
        readings.contains("Gold="),
        "deslag's readings carry the labels as Gold="
    );
    // And the gold loader: `score` reads it as a deslag gold file.
    let ran = std::process::Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .args(["score", "--gold"])
        .arg(dir.join("silver.conllu"))
        .args(["--tagger", "deslag"])
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

#[test]
fn a_build_into_a_directory_that_has_files_is_refused() {
    let made = Made::new();
    let out = made.out("2026-10-08-full");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("stale.txt"), "x").unwrap();
    made.build("2026-10-08-full", &[])
        .refused(&["is not empty"]);
    assert_eq!(fs::read_dir(&out).unwrap().count(), 1);
}

#[test]
fn a_build_writes_beside_its_directory_and_renames_it_into_place() {
    let made = Made::new();
    // An empty directory is taken, and a half batch an interrupted build left beside it is not
    // read into this one.
    let out = made.out("2026-10-08-whole");
    fs::create_dir_all(&out).unwrap();
    let partial = out.with_file_name("2026-10-08-whole.partial");
    fs::create_dir_all(partial.join("parts")).unwrap();
    fs::write(partial.join("notes.txt"), "half a batch").unwrap();
    made.build("2026-10-08-whole", &[]).ok();
    assert!(!partial.exists());
    assert!(!out.join("notes.txt").exists());
    check(&made, &out).ok();
}

#[test]
fn a_refused_build_writes_nothing() {
    let made = Made::new();
    made.set_run(2, "r6", "license", "GPL-3.0");
    made.build("2026-10-08-nothing", &[]).refused(&["GPL-3.0"]);
    assert!(!made.out("2026-10-08-nothing").exists());
}

#[test]
fn check_passes_when_there_is_no_silver() {
    let made = Made::new();
    // An image with no silver directory, and one with an empty directory.
    let missing = made.root.join("no-silver");
    let said = made
        .gold(&["silver", "check", "--silver", missing.to_str().unwrap()])
        .ok();
    assert!(said.out.contains("no silver batch"), "{}", said.out);
    let said = made
        .gold(&["silver", "check", "--silver", made.silver.to_str().unwrap()])
        .ok();
    assert!(said.out.contains("nothing to check"), "{}", said.out);
    // Standing too: nothing is live.
    let said = standing(&made).ok();
    assert!(said.out.contains("no live silver batch"), "{}", said.out);
}

#[test]
fn check_finds_every_batch_under_the_silver_root_and_retired_ones_too() {
    let (made, dir) = built();
    let live = make_live(&made, &dir);
    fs::write(
        &made.retired,
        format!("batch\tdate\treason\n{NAME}\t2026-11-02\tretired by hand\n"),
    )
    .unwrap();
    let said = made
        .gold(&["silver", "check", "--silver", made.silver.to_str().unwrap()])
        .ok();
    assert!(
        said.out.contains(&format!("silver check: {NAME} passes")),
        "{}",
        said.out
    );
    // A retired batch is still checked: edit it and the check refuses.
    edit(&live, "runs.tsv", |t| t.replacen("complete", "failed", 1));
    made.gold(&["silver", "check", "--silver", made.silver.to_str().unwrap()])
        .refused(&["runs.tsv"]);
}

#[test]
fn a_changed_checkout_voters_json_does_not_fail_a_batch_built_under_the_old_one() {
    let (made, dir) = built();
    // The checkout's voters.json moves on: another endpoint, another licence date, another voter.
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&made.voters).unwrap()).unwrap();
    value["models"]["deepseek"]["provider"] = "elsewhere/fp8".into();
    value["models"]["qwen"]["license_checked"] = "2027-01-01".into();
    value["voters"] = serde_json::json!(["mistral", "qwen", "hy3"]);
    fs::write(&made.voters, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    let said = check(&made, &dir).ok();
    assert!(said.out.contains("passes"), "{}", said.out);
    // The same file does stop the assembler from taking runs made under the old one.
    made.build("2026-10-08-moved", &[])
        .refused(&["began under a voters.json of sha256"]);
}

/// The checkout these tests run in, as its links resolve.
fn checkout() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

#[test]
fn a_reason_may_quote_a_path_of_the_corpus_but_the_preflight_refuses_one_of_this_machine() {
    let made = Made::new();
    let quoted = "part of the path /etc/hosts";
    made.edit(1, "merge/adjudicated.tsv", |text| {
        text.replacen("the guide says so", quoted, 1)
    });
    made.check_part(1).ok();
    made.build(NAME, &[]).ok();
    let dir = made.out(NAME);
    assert!(read(&dir, "parts/01/adjudicated.tsv").contains(quoted));
    check(&made, &dir).ok();

    // The checkout's own path is refused at the preflight, where the part is made. The check of a
    // built batch reads nothing of the machine, so it passes the same words, here and elsewhere.
    let leaked = format!("{}/tests/gold/dev.conllu", checkout());
    made.edit(1, "merge/adjudicated.tsv", |text| {
        text.replacen(quoted, &leaked, 1)
    });
    made.check_part(1)
        .refused(&[&leaked, "a path of the machine that made it"]);
    edit(&dir, "parts/01/adjudicated.tsv", |text| {
        text.replacen(quoted, &format!("it read {leaked}"), 1)
    });
    check(&made, &dir).ok();

    // A handoff's working directory is refused by the check on every machine.
    edit(&dir, "parts/01/adjudicated.tsv", |text| {
        text.replacen(
            &format!("it read {leaked}"),
            "it read deslag-handoff-x/request.json",
            1,
        )
    });
    check(&made, &dir).refused(&[
        "parts/01/adjudicated.tsv",
        "deslag-handoff-x/request.json",
        "a path of the machine that made it",
    ]);
}

/// An edit of a batch and what `silver check` must say of it: a name, the edit, what stderr holds.
type Case = (&'static str, Box<dyn Fn(&Path)>, Vec<&'static str>);

/// The edits `silver check` must refuse. Each is made to a fresh copy of the batch.
fn edits() -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    macro_rules! case {
        ($name:expr, |$dir:ident| $body:expr, [$($needle:expr),*]) => {
            cases.push(($name, Box::new(|$dir: &Path| $body), vec![$($needle),*]));
        };
    }
    case!(
        "a stray file",
        |d| write(d, "notes.txt", "x"),
        ["notes.txt", "a batch holds no such file"]
    );
    case!(
        "a missing file",
        |d| fs::remove_file(d.join("sources.tsv")).unwrap(),
        ["sources.tsv", "the batch has no such file"]
    );
    case!(
        "a missing part file",
        |d| fs::remove_file(d.join("parts/02/voters.tsv")).unwrap(),
        ["parts/02/voters.tsv"]
    );
    case!(
        "a half audit",
        |d| write(d, "audit/score.tsv", "x\n"),
        ["the audit has some of"]
    );
    case!(
        "a kit version it does not know",
        |d| edit(d, "record/kit.tsv", |t| t
            .replace("check_version\t1", "check_version\t2")),
        ["check_version", "2"]
    );
    case!(
        "a kit of another name",
        |d| edit(d, "record/kit.tsv", |t| t
            .replace(&format!("name\t{NAME}"), "name\tother")),
        ["the batch says it is `other`"]
    );
    case!(
        "a changed voters.json",
        |d| edit(d, "record/voters.json", |t| t.replacen(
            "batch_size",
            "batch_sizes",
            1
        )),
        ["record/voters.json", "its sha256 is not the kit's"]
    );
    case!(
        "a changed template",
        |d| edit(d, "record/datasheet.tmpl.md", |t| format!("{t}\nmore\n")),
        ["record/datasheet.tmpl.md", "its sha256 is not the kit's"]
    );
    case!(
        "a header that names another batch",
        |d| edit(d, "silver.conllu", |t| t.replace(
            &format!("silver.batch = {NAME}"),
            "silver.batch = other"
        )),
        ["silver.batch"]
    );
    case!(
        "labels that do not train",
        |d| edit(d, "silver.conllu", |t| t
            .replace("exam.trains = yes", "exam.trains = no")),
        ["exam.trains"]
    );
    case!(
        "a word with no provenance",
        |d| edit(d, "silver.conllu", |t| t.replacen("Prov=agree|", "", 1)),
        ["has no `Prov=`"]
    );
    case!(
        "a word with another provenance",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "Prov=agree",
            "Prov=blind",
            1
        )),
        ["Prov=blind"]
    );
    case!(
        "a word with no run",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "|Runs=r1,r2,r3,r4",
            "",
            1
        )),
        ["has no `Runs=`"]
    );
    case!(
        "an agreed word that names spaCy's run alone",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "Prov=agree|Runs=r1,r2,r3,r4",
            "Prov=agree|Runs=r4",
            1
        )),
        [
            "silver.conllu",
            "it is agreed by the runs of 0 model voters, and min_voters is 3"
        ]
    );
    case!(
        "an agreed word that names another part's runs",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "Prov=agree|Runs=r1,r2,r3,r4",
            "Prov=agree|Runs=r6,r7,r8,r9",
            1
        )),
        ["names run r6, which is not a voter's run of its part"]
    );
    case!(
        "an adjudicated word that says it was agreed",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "Prov=adjudicated|Runs=r5",
            "Prov=agree|Runs=r5",
            1
        )),
        ["names run r5, which is not a voter's run of its part"]
    );
    case!(
        "an agreed word that says it was adjudicated",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "Prov=agree|Runs=r1,r2,r3,r4",
            "Prov=adjudicated|Runs=r5",
            1
        )),
        ["it is adjudicated, and its part's adjudicated.tsv has no answer for s0001.2"]
    );
    case!(
        "an adjudicated word that names a voter's run",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "Prov=adjudicated|Runs=r5",
            "Prov=adjudicated|Runs=r1",
            1
        )),
        ["it is adjudicated and names runs r1, and the answer for s0001.1 is run r5's"]
    );
    case!(
        "an answer whose run is a voter's",
        |d| {
            edit(d, "parts/01/adjudicated.tsv", |t| {
                t.replace("\tr5\n", "\tr1\n")
            });
            edit(d, "silver.conllu", |t| {
                t.replacen("Prov=adjudicated|Runs=r5", "Prov=adjudicated|Runs=r1", 1)
            });
        },
        ["it is adjudicated by run r1, which is not an adjudicator's"]
    );
    case!(
        "a min_voters below three",
        |d| {
            edit(d, "record/kit.tsv", |t| {
                t.replace("min_voters\t3", "min_voters\t2")
            });
            for part in ["01", "02"] {
                edit(d, &format!("parts/{part}/voters.tsv"), |t| {
                    t.replace("# min_voters = 3", "# min_voters = 2")
                });
            }
        },
        [
            "record/kit.tsv",
            "min_voters is 2; silver needs a word agreed by at least 3 model voters"
        ]
    );
    case!(
        "an adjudicator run under another voters.json",
        |d| edit(d, "runs.tsv", |t| {
            let sha = sha256(read(d, "record/voters.json").as_bytes());
            let row = t
                .lines()
                .find(|l| l.starts_with("r5\t"))
                .unwrap()
                .to_string();
            t.replacen(&row, &row.replacen(&sha, &"0".repeat(64), 1), 1)
        }),
        ["run r5 began under a voters.json of sha256 000"]
    );
    case!(
        "a word with an illegal code",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "\tNOUN\t_\tNumber=Sing",
            "\tFOO\t_\tNumber=Sing",
            1
        )),
        ["word"]
    );
    case!(
        "a sentence out of the manifest",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "# sent_id = s0003",
            "# sent_id = s0099",
            1
        )),
        ["s0099"]
    );
    case!(
        "a context that is not the manifest's",
        |d| edit(d, "silver.conllu", |t| t.replacen(
            "# exam.context = prose",
            "# exam.context = heading",
            1
        )),
        ["its context"]
    );
    case!(
        "a split that is not the repository's",
        |d| edit(d, "manifest.tsv", |t| t
            .replacen("\ttrain\t", "\tx\t", 1)
            .replacen("\ttune\t", "\ttrain\t", 0)),
        ["split"]
    );
    case!(
        "a content hash that is not one",
        |d| set_cell(d, "manifest.tsv", "s0001", 10, "xyz"),
        ["content_sha256 is not a sha256"]
    );
    case!(
        "a part that is not in the batch",
        |d| edit(d, "manifest.tsv", |t| t.replacen("\t02\n", "\t07\n", 1)),
        ["is not a part of the batch"]
    );
    case!(
        "sources that are not the manifest's",
        |d| edit(d, "sources.tsv", |t| t.replacen("\t1\t", "\t9\t", 1)),
        ["sources.tsv"]
    );
    case!(
        "a run table that lacks a run",
        |d| edit(d, "runs.tsv", |t| t
            .lines()
            .filter(|l| !l.starts_with("r1\t"))
            .map(|l| format!("{l}\n"))
            .collect()),
        ["run r1"]
    );
    case!(
        "a run that nothing names",
        |d| edit(d, "runs.tsv", |t| {
            let row = t
                .lines()
                .find(|l| l.starts_with("r1\t"))
                .unwrap()
                .replacen("r1\t", "r99\t", 1);
            format!("{t}{row}\n")
        }),
        ["run r99 is described and no word or voter names it"]
    );
    case!(
        "a licence the rules refuse",
        |d| edit(d, "runs.tsv", |t| t.replacen("\tMIT\t", "\tGPL-3.0\t", 1)),
        ["GPL-3.0"]
    );
    case!(
        "an endpoint voters.json does not list",
        |d| edit(d, "runs.tsv", |t| t.replacen(
            "gmicloud/fp8",
            "elsewhere/fp8",
            1
        )),
        ["endpoint elsewhere/fp8"]
    );
    case!(
        "a run at another commit",
        |d| edit(d, "runs.tsv", |t| t.replacen(
            "0123456789abcdef0123456789abcdef01234567",
            "fedcba9876543210fedcba9876543210fedcba98",
            1
        )),
        ["commits of deslag"]
    );
    case!(
        "a kit commit that is not the runs'",
        |d| edit(d, "record/kit.tsv", |t| t.replacen(
            "deslag_commit\t0123",
            "deslag_commit\t4567",
            1
        )),
        ["deslag_commit is"]
    );
    case!(
        "an agent hash that is not the runs'",
        |d| edit(d, "record/kit.tsv", |t| t
            .replace("agent_sha256\t", "agent_sha256\t0")),
        ["agent_sha256"]
    );
    case!(
        "an agent record that is not the kit's",
        |d| edit(d, "record/agent.json", |t| t.replace("2.1.293", "2.1.294")),
        ["record/agent.json"]
    );
    case!(
        "an agent record that is not there",
        |d| fs::remove_file(d.join("record/agent.json")).unwrap(),
        ["record/agent.json"]
    );
    case!(
        "a listing that is not there",
        |d| fs::remove_file(d.join("listings/st1/r1.json")).unwrap(),
        ["listings"]
    );
    case!(
        "a listing no run has",
        |d| write(d, "listings/st1/r77.json", "{}\n"),
        ["no run of the batch has this listing"]
    );
    case!(
        "a part table of a sentence the manifest lacks",
        |d| edit(d, "parts/01/worklist.tsv", |t| t
            .replacen("s0001", "s0098", 2)),
        ["s0098"]
    );
    case!(
        "a part's adjudicator that voters.json does not name",
        |d| write(d, "parts/01/adjudicator.json", "{\"name\": \"claude\"}\n"),
        ["its adjudicator is not voters.json's"]
    );
    case!(
        "a voter run the table does not describe",
        |d| edit(d, "parts/01/voters.tsv", |t| t
            .replacen("\tr1\n", "\tr98\n", 1)),
        ["r98"]
    );
    case!(
        "an unsettled table with a form",
        |d| write(
            d,
            "parts/02/unsettled.tsv",
            "tier\tcontext\tsentences\twords\tform\nmixed\tprose\t1\t1\tcats\n"
        ),
        ["parts/02/unsettled.tsv"]
    );
    case!(
        "a drop that is not a reason",
        |d| write(
            d,
            "record/drops.tsv",
            "sent_id\tpart\treason\ns0099\t01\ttired\n"
        ),
        ["`tired` is not a reason"]
    );
    case!(
        "a dropped sentence the manifest holds",
        |d| write(
            d,
            "record/drops.tsv",
            "sent_id\tpart\treason\ns0001\t01\trepeat\n"
        ),
        ["it is dropped and the manifest holds it"]
    );
    case!(
        "a path of the maker's machine",
        |d| edit(d, "parts/01/agreement.txt", |t| format!(
            "{t}\n/home/someone/.label/silver/part-01\n"
        )),
        ["a path of the machine that made it"]
    );
    case!(
        "a path in a part's JSON",
        |d| write(
            d,
            "parts/01/adjudicator.json",
            "{\"name\": \"opus\", \"model\": \"claude-opus-5-5\", \"dir\": \"/tmp/x\"}\n"
        ),
        ["/tmp/x"]
    );
    case!(
        "a path in the middle of the adjudicator's reason",
        |d| edit(d, "parts/01/adjudicated.tsv", |t| t.replace(
            "\tthe guide says so\t",
            "\tI read /tmp/deslag-handoff-x/request.json and the guide says so\t"
        )),
        [
            "parts/01/adjudicated.tsv",
            "/tmp/deslag-handoff-x/request.json"
        ]
    );
    case!(
        "numbers that are not the files'",
        |d| edit(d, "record/datasheet.json", |t| t.replacen(
            "\"sentences\": 8",
            "\"sentences\": 9",
            1
        )),
        [
            "record/datasheet.json",
            "not what the batch's files compute"
        ]
    );
    case!(
        "a datasheet that is not the template's",
        |d| edit(d, "DATASHEET.md", |t| format!(
            "{t}\nAnd it is a perfect set.\n"
        )),
        ["DATASHEET.md", "not what record/'s template renders"]
    );
    case!(
        "a calibration table with forms",
        |d| write(d, "noise/dev.tsv", "form\tn\ncats\t1\n"),
        ["a calibration table holds numbers"]
    );
    cases
}

#[test]
fn check_refuses_each_edit_of_a_built_batch() {
    let (made, dir) = built();
    let mut failures: Vec<String> = Vec::new();
    for (name, change, needles) in edits() {
        // The copy keeps the batch's name as its directory's, as an image's would.
        let _ = fs::remove_dir_all(made.root.join("tampered"));
        let copy = made.root.join("tampered").join(NAME);
        copy_dir(&dir, &copy);
        change(&copy);
        let ran = check(&made, &copy);
        if ran.code != 2 {
            failures.push(format!(
                "{name}: exit {} and not 2\n{}{}",
                ran.code, ran.out, ran.err
            ));
            continue;
        }
        for needle in needles {
            if !ran.err.contains(needle) {
                failures.push(format!("{name}: stderr lacks `{needle}`:\n{}", ran.err));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The owner's audit of the batch `dir`: a blind queue of every sentence, reviewed by copying
/// silver's labels (so he agrees on every word) except where `disagree` turns his nouns to verbs,
/// with the sentences in `reject` rejected. Returns the directory `silver build --audit` reads.
fn audit_of(made: &Made, dir: &Path, reject: &[&str], disagree: bool) -> PathBuf {
    let out = made.root.join("audit");
    let _ = fs::remove_dir_all(&out);
    let held = read(dir, "silver.conllu").matches("# sent_id").count();
    let count = held.min(LEAST_SENTENCES).to_string();
    made.gold(&[
        "audit",
        "--blind",
        "--from",
        dir.join("silver.conllu").to_str().unwrap(),
        "--count",
        &count,
        "--out",
        out.to_str().unwrap(),
    ])
    .ok();
    let queue = read(&out, "queue.conllu");
    let labels = read(&out, "labels.conllu");
    // Blind: the queue holds no label.
    assert!(
        !queue.contains("Prov=agree") && !queue.contains("Runs="),
        "{queue}"
    );
    assert!(
        labels.contains("Prov=agree") && labels.contains("Runs="),
        "{labels}"
    );
    let mut owner = reviewed(&queue, &labels, reject);
    if disagree {
        owner = owner
            .lines()
            .map(|line| {
                if line.starts_with('#') {
                    line.to_string()
                } else {
                    line.replacen("\tNOUN\t_\tNumber=Sing", "\tVERB\t_\tVerbForm=Fin", 1)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
    }
    write(&out, "queue.conllu", &owner);
    out
}

/// The CoNLL-U `text` without the sentence `id`.
fn without_sentence(text: &str, id: &str) -> String {
    let header = format!("# sent_id = {id}\n");
    let mut out: String = text
        .split_inclusive("\n\n")
        .filter(|block| !block.contains(&header))
        .collect();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The archive hash every audited build is given.
fn archive() -> String {
    sha256(b"the archive of what stays on the machine")
}

/// Builds `name` with the audit `audit`.
fn build_audited(made: &Made, name: &str, audit: &Path, extra: &[&str]) -> Ran {
    let archive = archive();
    let mut args = vec![
        "--audit",
        audit.to_str().unwrap(),
        "--archive-sha256",
        archive.as_str(),
    ];
    args.extend(extra);
    made.build(name, &args)
}

#[test]
fn an_audit_is_scored_into_the_batch_and_the_owners_rejections_come_out() {
    let (made, draft) = built_big();
    let audit = audit_of(&made, &draft, &["s0004"], false);
    let said = build_audited(&made, "2026-10-08-audited", &audit, &[]).ok();
    assert!(
        said.out.contains("dropped 1, 1 audit rejected"),
        "{}",
        said.out
    );
    assert!(
        said.out.contains("the audit met its bar of 95.0: yes"),
        "{}",
        said.out
    );
    let dir = made.out("2026-10-08-audited");
    // The rejected sentence is out of the batch, its tables and the audit, and is in the drops.
    assert!(read(&dir, "record/drops.tsv").contains("s0004\t"));
    assert!(!read(&dir, "silver.conllu").contains("# sent_id = s0004"));
    assert!(!read(&dir, "audit/queue.conllu").contains("s0004"));
    assert!(!read(&dir, "audit/labels.conllu").contains("s0004"));
    assert_eq!(
        read(&dir, "audit/queue.conllu")
            .matches("# sent_id")
            .count(),
        LEAST_SENTENCES - 1
    );
    // Silver's labels of those sentences are silver's own words.
    let silver = read(&dir, "silver.conllu");
    for line in read(&dir, "audit/labels.conllu")
        .lines()
        .filter(|l| l.contains("Prov="))
    {
        assert!(silver.contains(line), "{line}");
    }
    let score = read(&dir, "audit/score.tsv");
    assert!(score.contains("# met = yes\n"), "{score}");
    let kit = read(&dir, "record/kit.tsv");
    assert!(
        kit.contains(&format!("\narchive_sha256\t{}\n", archive())),
        "{kit}"
    );
    assert!(kit.contains("\naudit_bar\t95.0\n"), "{kit}");
    let sheet = read(&dir, "DATASHEET.md");
    assert!(sheet.contains("The owner reviewed 49 sentences"), "{sheet}");
    // It checks, and the check scores the audit again.
    let said = check(&made, &dir).ok();
    assert!(
        said.out
            .contains("52 sentences, 617 words, 2 parts, 9 runs, audited"),
        "{}",
        said.out
    );
}

#[test]
fn check_scores_the_audit_again_and_matches_its_labels_to_silver() {
    let (made, draft) = built_big();
    let audit = audit_of(&made, &draft, &[], false);
    build_audited(&made, "2026-10-08-audited", &audit, &[]).ok();
    let dir = made.out("2026-10-08-audited");
    let cases: Vec<Case> = vec![
        (
            "an owner's label changed",
            Box::new(|d| {
                edit(d, "audit/queue.conllu", |t| {
                    t.replacen("\tNOUN\t_\tNumber=Sing", "\tVERB\t_\tVerbForm=Fin", 1)
                })
            }),
            vec!["audit/score.tsv"],
        ),
        (
            "a score edited",
            Box::new(|d| {
                edit(d, "audit/score.tsv", |t| {
                    t.replace("# met = yes", "# met = no")
                })
            }),
            vec!["audit/score.tsv"],
        ),
        (
            "labels that are not silver's",
            Box::new(|d| {
                edit(d, "audit/labels.conllu", |t| {
                    t.replacen("\tNOUN\t_\tNumber=Sing", "\tVERB\t_\tVerbForm=Fin", 1)
                })
            }),
            vec!["audit/labels.conllu", "its words are not silver.conllu's"],
        ),
        (
            "a queue sentence the manifest lacks",
            Box::new(|d| {
                edit(d, "audit/queue.conllu", |t| {
                    t.replacen("# sent_id = s0003", "# sent_id = s0097", 1)
                });
                edit(d, "audit/labels.conllu", |t| {
                    t.replacen("# sent_id = s0003", "# sent_id = s0097", 1)
                });
            }),
            vec!["s0097", "the manifest does not hold it"],
        ),
        (
            "a bar under 95.0 with no acceptance",
            Box::new(|d| {
                edit(d, "record/kit.tsv", |t| {
                    t.replace("audit_bar\t95.0", "audit_bar\t90.0")
                })
            }),
            vec![
                "its bar is 90.0, under the 95.0",
                "holds no acceptance by the owner",
            ],
        ),
        (
            "an audit of 49 sentences with no acceptance",
            Box::new(|d| {
                // The last, since the first block holds the file's header too.
                let last = read(d, "audit/queue.conllu")
                    .lines()
                    .rev()
                    .find_map(|line| line.strip_prefix("# sent_id = ").map(str::to_string))
                    .unwrap();
                for name in ["audit/queue.conllu", "audit/labels.conllu"] {
                    edit(d, name, |t| without_sentence(t, &last));
                }
            }),
            vec![
                "it holds 49 sentences, reviewed and rejected, fewer than the 50",
                "holds no acceptance by the owner",
            ],
        ),
        (
            "an audit_bar that is gone",
            Box::new(|d| {
                edit(d, "record/kit.tsv", |t| {
                    t.replace("audit_bar\t95.0", "audit_bar\t-")
                })
            }),
            vec!["a batch with an audit records its audit_bar"],
        ),
    ];
    for (name, change, needles) in cases {
        let _ = fs::remove_dir_all(made.root.join("tampered"));
        let copy = made.root.join("tampered").join("2026-10-08-audited");
        copy_dir(&dir, &copy);
        change(&copy);
        let ran = check(&made, &copy);
        assert_eq!(ran.code, 2, "{name}: {}{}", ran.out, ran.err);
        for needle in needles {
            assert!(
                ran.err.contains(needle),
                "{name}: stderr lacks `{needle}`:\n{}",
                ran.err
            );
        }
    }
}

#[test]
fn an_audit_that_met_its_bar_keeps_a_live_batch_standing() {
    let (made, draft) = built_big();
    let audit = audit_of(&made, &draft, &[], false);
    build_audited(&made, "2026-10-08-audited", &audit, &[]).ok();
    make_live(&made, &made.out("2026-10-08-audited"));
    let said = standing(&made).ok();
    assert!(
        said.out
            .contains("1 live batches hold to today's gold and corpus (0 retired, skipped)"),
        "{}",
        said.out
    );
}

#[test]
fn a_live_batch_with_no_audit_does_not_stand_and_the_message_names_the_ways_out() {
    let (made, dir) = built();
    make_live(&made, &dir);
    let ran = standing(&made).refused(&[
        "it has no audit score",
        "retire the batch",
        "silver-retired.tsv",
        "undo the change",
    ]);
    assert!(ran.err.contains(NAME), "{}", ran.err);
}

#[test]
fn an_audit_below_its_bar_needs_the_owners_acceptance() {
    let (made, draft) = built_big();
    let audit = audit_of(&made, &draft, &[], true);
    let said = build_audited(&made, "2026-10-08-low", &audit, &[]).ok();
    assert!(
        said.out.contains("the audit met its bar of 95.0: no"),
        "{}",
        said.out
    );
    assert!(
        said.out
            .contains("`silver standing` will refuse it as live"),
        "{}",
        said.out
    );
    let low = made.out("2026-10-08-low");
    assert!(read(&low, "audit/score.tsv").contains("# met = no\n"));
    assert!(!low.join("record/audit-accepted.txt").exists());
    // The check passes, since it is the batch's own record; standing does not.
    check(&made, &low).ok();
    make_live(&made, &low);
    standing(&made).refused(&[
        "its audit is below the bar",
        "holds no acceptance by the owner",
    ]);
    // With the owner's words in record/, the same audit stands.
    let audit = audit_of(&made, &draft, &[], true);
    let said = build_audited(
        &made,
        "2026-10-08-accepted",
        &audit,
        &[
            "--accept-below-bar",
            "I accept this score, for a first batch. Matt, 2026-10-09",
        ],
    )
    .ok();
    assert!(
        said.out.contains("the audit met its bar of 95.0: no"),
        "{}",
        said.out
    );
    let accepted = made.out("2026-10-08-accepted");
    assert!(read(&accepted, "record/audit-accepted.txt").contains("I accept this score"));
    check(&made, &accepted).ok();
    fs::remove_dir_all(made.silver.join("2026-10-08-low")).unwrap();
    make_live(&made, &accepted);
    standing(&made).ok();
    // The datasheet says so.
    assert!(read(&accepted, "DATASHEET.md").contains("accepted an audit that falls short"));
}

#[test]
fn a_small_audit_or_a_low_bar_needs_the_owners_acceptance() {
    // Eight sentences, all agreed with: the audit meets its bar but is too small to vouch alone.
    let (made, draft) = built();
    let audit = audit_of(&made, &draft, &[], false);
    build_audited(&made, "2026-10-08-small", &audit, &[]).refused(&[
        "it holds 8 sentences, reviewed and rejected, fewer than the 50",
        "holds no acceptance by the owner",
    ]);
    build_audited(&made, "2026-10-08-low", &audit, &["--bar", "90"]).refused(&[
        "its bar is 90.0, under the 95.0",
        "holds no acceptance by the owner",
    ]);
    // The owner's words let it through check and standing.
    build_audited(
        &made,
        "2026-10-08-small",
        &audit,
        &[
            "--accept-below-bar",
            "A small first audit. Matt, 2026-10-09",
        ],
    )
    .ok();
    let small = made.out("2026-10-08-small");
    check(&made, &small).ok();
    let live = make_live(&made, &small);
    standing(&made).ok();
    // An empty acceptance accepts nothing: build refuses one, and check and standing an emptied file.
    build_audited(
        &made,
        "2026-10-08-empty",
        &audit,
        &["--accept-below-bar", " "],
    )
    .refused(&["--accept-below-bar", "it is empty"]);
    fs::write(live.join("record/audit-accepted.txt"), "\n").unwrap();
    check(&made, &live).refused(&["audit-accepted.txt", "it is empty"]);
    standing(&made).refused(&[
        "its audit falls short: it holds 8 sentences",
        "holds no acceptance by the owner",
    ]);
    // Without them, standing refuses it too, whatever check would say.
    fs::remove_file(live.join("record/audit-accepted.txt")).unwrap();
    standing(&made).refused(&[
        "its audit falls short: it holds 8 sentences",
        "holds no acceptance by the owner",
    ]);
}

#[test]
fn an_acceptance_without_an_audit_is_refused() {
    let (made, _) = built();
    made.build("2026-10-08-x", &["--accept-below-bar", "yes"])
        .refused(&["--accept-below-bar"]);
}

/// A live, audited batch, and the made parts it came from.
fn live_audited() -> (Made, PathBuf) {
    let (made, draft) = built_big();
    let audit = audit_of(&made, &draft, &[], false);
    build_audited(&made, "2026-10-08-audited", &audit, &[]).ok();
    let live = make_live(&made, &made.out("2026-10-08-audited"));
    standing(&made).ok();
    (made, live)
}

#[test]
fn a_repository_that_became_reserved_stops_a_live_batch_standing() {
    let (made, live) = live_audited();
    let manifest = read(&live, "manifest.tsv");
    let repo = manifest
        .lines()
        .find(|l| l.starts_with("s0001\t"))
        .unwrap()
        .split('\t')
        .nth(5)
        .unwrap()
        .to_string();
    let dev = made.gold.join("dev.manifest.tsv");
    let mut text = fs::read_to_string(&dev).unwrap();
    text.push_str(&format!("g2\tdev\thuman\tprose\tx.md\t{repo}\tMIT\t0-1\n"));
    fs::write(&dev, text).unwrap();
    standing(&made).refused(&[
        "are of repositories that today's gold or tests/corpus reserve",
        "s0001",
        "retire the batch",
    ]);
}

#[test]
fn a_text_that_became_gold_stops_a_live_batch_standing() {
    let (made, live) = live_audited();
    let silver = read(&live, "silver.conllu");
    let block = silver
        .split("\n\n")
        .find(|b| b.contains("# sent_id = s0003"))
        .unwrap()
        .to_string();
    let mut gold = block
        .lines()
        .filter(|l| l.starts_with("# text") || !l.starts_with('#'))
        .map(|line| {
            if line.starts_with('#') {
                return line.to_string();
            }
            let mut cells: Vec<String> = line.split('\t').map(str::to_string).collect();
            cells[9] = cells[9]
                .split('|')
                .filter(|e| !e.starts_with("Runs="))
                .collect::<Vec<_>>()
                .join("|");
            cells.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n");
    gold.insert_str(0, "# sent_id = g0900\n");
    let dev = made.gold.join("dev.conllu");
    let mut text = fs::read_to_string(&dev).unwrap();
    text.push_str(&format!("{gold}\n\n"));
    fs::write(&dev, text).unwrap();
    standing(&made).refused(&["have the text of a sentence of today's gold", "s0003"]);
}

#[test]
fn a_fixture_that_became_excluded_stops_a_live_batch_standing() {
    let (made, live) = live_audited();
    let manifest = read(&live, "manifest.tsv");
    let sha = manifest
        .lines()
        .find(|l| l.starts_with("s0001\t"))
        .unwrap()
        .split('\t')
        .nth(10)
        .unwrap()
        .to_string();
    fs::write(&made.exclude, format!("{}\n{sha}\n", "0".repeat(64))).unwrap();
    standing(&made).refused(&["fixtures it quotes", "are on the exclusion list"]);
}

#[test]
fn a_fixture_the_corpus_no_longer_holds_stops_a_live_batch_standing() {
    let (made, live) = live_audited();
    let manifest = read(&live, "manifest.tsv");
    let file = manifest
        .lines()
        .find(|l| l.starts_with("s0001\t"))
        .unwrap()
        .split('\t')
        .nth(4)
        .unwrap()
        .to_string();
    fs::remove_file(made.tree.join(&file)).unwrap();
    fs::remove_file(made.tree.join(&file).with_extension("json")).unwrap();
    standing(&made).refused(&["are no longer in the corpus: a later batch excludes them"]);
}

#[test]
fn a_repository_the_small_tier_gained_stops_a_live_batch_standing() {
    let (made, live) = live_audited();
    let manifest = read(&live, "manifest.tsv");
    let row = manifest
        .lines()
        .find(|l| l.starts_with("s0001\t"))
        .unwrap()
        .to_string();
    let file = row.split('\t').nth(4).unwrap().to_string();
    let tier = row.split('\t').nth(2).unwrap().to_string();
    let md = made.tree.join(&file);
    let dir = made.small.join(tier).join("added");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(&md, dir.join(md.file_name().unwrap())).unwrap();
    fs::copy(
        md.with_extension("json"),
        dir.join(md.with_extension("json").file_name().unwrap()),
    )
    .unwrap();
    standing(&made).refused(&["are of repositories that today's gold or tests/corpus reserve"]);
}

#[test]
fn a_retired_batch_is_skipped_by_standing_and_still_checked() {
    let (made, live) = live_audited();
    // The gold now reserves its repository: the batch does not stand, so it is retired.
    let manifest = read(&live, "manifest.tsv");
    let repo = manifest
        .lines()
        .find(|l| l.starts_with("s0001\t"))
        .unwrap()
        .split('\t')
        .nth(5)
        .unwrap()
        .to_string();
    let dev = made.gold.join("dev.manifest.tsv");
    let mut text = fs::read_to_string(&dev).unwrap();
    text.push_str(&format!("g2\tdev\thuman\tprose\tx.md\t{repo}\tMIT\t0-1\n"));
    fs::write(&dev, text).unwrap();
    standing(&made).refused(&["reserve"]);
    let name = live.file_name().unwrap().to_str().unwrap().to_string();
    fs::write(
        &made.retired,
        format!("batch\tdate\treason\n{name}\t2026-11-02\tits repository went to dev\n"),
    )
    .unwrap();
    let said = standing(&made).ok();
    assert!(
        said.out.contains("no live silver batch") && said.out.contains("(1 retired)"),
        "{}",
        said.out
    );
    // The check still runs on it, and finds it sound.
    let said = made
        .gold(&["silver", "check", "--silver", made.silver.to_str().unwrap()])
        .ok();
    assert!(
        said.out.contains(&format!("silver check: {name} passes")),
        "{}",
        said.out
    );
    edit(&live, "runs.tsv", |t| t.replacen("complete", "failed", 1));
    made.gold(&["silver", "check", "--silver", made.silver.to_str().unwrap()])
        .refused(&["runs.tsv"]);
}

#[test]
fn a_live_batch_with_no_retired_list_is_an_error_and_not_a_pass() {
    let (made, live) = live_audited();
    let _ = live;
    fs::remove_file(&made.retired).unwrap();
    standing(&made).refused(&["retired.tsv"]);
}

/// Where the committed fixture batch is.
const FIXTURE: &str = "tests/silver-fixture/2026-01-01-fixture";

/// Writes the committed fixture batch again: `DESLAG_REGENERATE_SILVER_FIXTURE=1 cargo test -p
/// deslag-exam --test silver_batch regenerate_the_committed_fixture_batch -- --ignored`. The unit
/// test `the_committed_batch_still_passes_the_frozen_rules` in `silver/check.rs` reads it. Do this
/// only when a new check version is added, so that version 1 is still held to the batch it passed
/// once.
#[test]
#[ignore = "writes the committed fixture; run it by name with --ignored"]
fn regenerate_the_committed_fixture_batch() {
    assert!(
        std::env::var_os("DESLAG_REGENERATE_SILVER_FIXTURE").is_some(),
        "set DESLAG_REGENERATE_SILVER_FIXTURE=1 to write the committed fixture"
    );
    let (made, draft) = built();
    // The owner rejects a sentence and differs from silver on some words, so the score has real
    // intervals.
    let audit = audit_of(&made, &draft, &["s0004"], true);
    // After the draw, the gold took the text of one sentence and the repository of another, so
    // the batch drops sentences for three reasons.
    let rows = read(&draft, "manifest.tsv");
    let repo = rows
        .lines()
        .find(|line| line.starts_with("s0007\t"))
        .and_then(|line| line.split('\t').nth(5))
        .unwrap()
        .to_string();
    let manifest = made.gold.join("dev.manifest.tsv");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str(&format!("g2\tdev\thuman\tprose\tx.md\t{repo}\tMIT\t0-1\n"));
    fs::write(&manifest, text).unwrap();
    let block: String = read(&draft, "silver.conllu")
        .split("\n\n")
        .find(|block| block.contains("# sent_id = s0006\n"))
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("# text") || !line.starts_with('#'))
        .map(|line| {
            if line.starts_with('#') {
                return line.to_string();
            }
            let mut cells: Vec<&str> = line.split('\t').collect();
            let misc: Vec<&str> = cells[9]
                .split('|')
                .filter(|entry| !entry.starts_with("Runs="))
                .collect();
            let misc = misc.join("|");
            cells[9] = &misc;
            cells.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dev = made.gold.join("dev.conllu");
    let mut text = fs::read_to_string(&dev).unwrap();
    text.push_str(&format!("# sent_id = g0900\n{block}\n\n"));
    fs::write(&dev, text).unwrap();
    let noise = made.root.join("dev.tsv");
    fs::write(
        &noise,
        "name\tpos\tpos_low\tpos_high\twords\npipeline\t97.1\t96.2\t97.9\t1210\n",
    )
    .unwrap();
    let archive = archive();
    let name = "2026-01-01-fixture";
    made.build(
        name,
        &[
            "--audit",
            audit.to_str().unwrap(),
            "--archive-sha256",
            &archive,
            "--noise",
            &format!("dev={}", noise.display()),
            "--accept-below-bar",
            "A small audit under its bar, accepted for the fixture.",
        ],
    )
    .ok();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let _ = fs::remove_dir_all(&fixture);
    copy_dir(&made.out(name), &fixture);
    check(&made, &fixture).ok();
}
