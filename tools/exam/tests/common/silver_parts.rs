//! Synthetic silver parts, made as the labelling makes them: `deslag-gold draw --parts` over a
//! fixture tree, three model voters and spaCy tagging each part with made-up replies, `merge`,
//! the adjudicator's answers, and `finish --trains yes`. The run table, the listings and the
//! adjudicator's record are written as `label.py` writes them. Nothing here needs a model, a key
//! or the network, and nothing opens the real dev, holdout or owner file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::{Digest, Sha256};

use super::draws::{other_small, quiet_gold, wide_tree};

/// The columns of `runs.tsv`, as `label.py` has them.
pub const RUN_COLUMNS: [&str; 31] = [
    "run",
    "state_id",
    "role",
    "name",
    "status",
    "reason",
    "model",
    "provider",
    "endpoint",
    "quantization",
    "price_in_per_m",
    "price_out_per_m",
    "date",
    "prompt_sha256",
    "guide_sha256",
    "calls",
    "retries",
    "prompt_tokens",
    "completion_tokens",
    "reasoning_tokens",
    "cost_usd",
    "seconds",
    "sentences",
    "listing",
    "reply_model",
    "model_version",
    "license",
    "license_checked",
    "voters_sha256",
    "deslag_commit",
    "settings",
];

/// The commit every run was made at.
pub const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The state every run came from.
pub const STATE: &str = "st1";

/// What a run of `deslag-gold` said.
#[derive(Debug)]
pub struct Ran {
    /// The exit code.
    pub code: i32,
    /// Standard output.
    pub out: String,
    /// Standard error.
    pub err: String,
}

impl Ran {
    fn of(output: &Output) -> Ran {
        Ran {
            code: output.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&output.stdout).into_owned(),
            err: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// Fails unless the run exited 0.
    pub fn ok(self) -> Ran {
        assert_eq!(self.code, 0, "stdout:\n{}\nstderr:\n{}", self.out, self.err);
        self
    }

    /// Fails unless the run exited 2, and every one of `needles` is on stderr.
    pub fn refused(self, needles: &[&str]) -> Ran {
        assert_eq!(self.code, 2, "stdout:\n{}\nstderr:\n{}", self.out, self.err);
        for needle in needles {
            assert!(
                self.err.contains(needle),
                "stderr lacks `{needle}`:\n{}",
                self.err
            );
        }
        self
    }
}

/// The sha256 of `bytes`, in hex.
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A tree for the pool, a small tier, a gold directory that reserves nothing, and `parts` parts of
/// a draw that were labelled: with one merge, `merge`, that spaCy votes in, or with two as the run
/// makes them, `merge` of the model voters and `merge-spacy` settled from it.
pub struct Made {
    _tmp: tempfile::TempDir,
    /// Where everything is; the commands run here.
    pub root: PathBuf,
    /// The pool's fixtures.
    pub tree: PathBuf,
    /// The small tier, in other repositories.
    pub small: PathBuf,
    /// The gold directory.
    pub gold: PathBuf,
    /// The exclusion list.
    pub exclude: PathBuf,
    /// The silver root of an image; empty.
    pub silver: PathBuf,
    /// The retired list; empty.
    pub retired: PathBuf,
    /// A copy of this checkout's `voters.json`.
    pub voters: PathBuf,
    /// The part directories, in order.
    pub parts: Vec<PathBuf>,
    /// The merge directory of each part that the assembler reads.
    merge: &'static str,
    /// The runs of each part: five, or six with a second adjudicator run.
    stride: usize,
}

/// A sentence of a skeleton: its id, and for each token its kind.
struct Skeleton {
    id: String,
    kinds: Vec<String>,
    lines: Vec<Vec<String>>,
}

fn skeleton(path: &Path) -> Vec<Skeleton> {
    let mut out: Vec<Skeleton> = Vec::new();
    for line in fs::read_to_string(path).unwrap().lines() {
        if let Some(id) = line.strip_prefix("# sent_id = ") {
            out.push(Skeleton {
                id: id.to_string(),
                kinds: Vec::new(),
                lines: Vec::new(),
            });
        } else if !line.is_empty() && !line.starts_with('#') {
            let cells: Vec<String> = line.split('\t').map(str::to_string).collect();
            let kind = cells[9]
                .split('|')
                .find_map(|entry| entry.strip_prefix("Kind="))
                .unwrap()
                .to_string();
            let last = out.last_mut().unwrap();
            last.kinds.push(kind);
            last.lines.push(cells);
        }
    }
    out
}

/// The codes the voters agree on, word by word round a sentence, and spaCy's UPOS and FEATS of
/// each: several tags, so that a batch and its audit are not all nouns. The first word of a
/// sentence is a singular noun.
const CYCLE: [(&str, &str, &str); 7] = [
    ("N.s", "NOUN", "Number=Sing"),
    ("V.in", "VERB", "VerbForm=Inf"),
    ("J", "ADJ", "_"),
    ("N.p", "NOUN", "Number=Plur"),
    ("D", "DET", "_"),
    ("R", "ADV", "_"),
    ("P", "ADP", "_"),
];

/// The compact lines of a voter that tags the words round [`CYCLE`], except the words `change`
/// names (sentence index, word number from 1) which it tags `code`.
fn compact(sentences: &[Skeleton], change: &[(usize, usize)], code: &str) -> String {
    let mut out = String::new();
    for (at, sentence) in sentences.iter().enumerate() {
        let mut word = 0;
        let codes: Vec<&str> = sentence
            .kinds
            .iter()
            .map(|kind| {
                if kind == "Word" {
                    word += 1;
                    if change.contains(&(at, word)) {
                        code
                    } else {
                        CYCLE[(word - 1) % CYCLE.len()].0
                    }
                } else {
                    "_"
                }
            })
            .collect();
        out.push_str(&format!("{}: {}\n", sentence.id, codes.join(" ")));
    }
    out
}

/// spaCy's CoNLL-U of the sentences, with its run, except that it takes each word `change` names
/// (sentence index, word number from 1) for a singular noun.
fn outside(sentences: &[Skeleton], run: &str, change: &[(usize, usize)]) -> String {
    let mut out = String::new();
    for (at, sentence) in sentences.iter().enumerate() {
        out.push_str(&format!("# sent_id = {}\n", sentence.id));
        let mut word = 0;
        for cells in &sentence.lines {
            let kind = cells[9]
                .split('|')
                .find_map(|entry| entry.strip_prefix("Kind="))
                .unwrap();
            let (upos, feats) = match kind {
                "Word" => {
                    word += 1;
                    let (_, upos, feats) = if change.contains(&(at, word)) {
                        CYCLE[0]
                    } else {
                        CYCLE[(word - 1) % CYCLE.len()]
                    };
                    (upos, feats)
                }
                "Punctuation" => ("PUNCT", "_"),
                "Symbol" => ("SYM", "_"),
                _ => ("X", "_"),
            };
            out.push_str(&format!(
                "{}\t{}\t_\t{upos}\t_\t{feats}\t_\t_\t_\t{}|Runs={run}\n",
                cells[0], cells[1], cells[9]
            ));
        }
        out.push('\n');
    }
    out
}

/// The token number of word `word` (from 1) of the sentence.
fn token_of(sentence: &Skeleton, word: usize) -> usize {
    sentence
        .kinds
        .iter()
        .enumerate()
        .filter(|(_, kind)| *kind == "Word")
        .nth(word - 1)
        .map(|(at, _)| at + 1)
        .unwrap()
}

/// A row of `runs.tsv`: every cell `-` but the ones `set` gives.
pub fn run_row(set: &[(&str, &str)]) -> String {
    let mut cells: Vec<String> = RUN_COLUMNS.iter().map(|_| "-".to_string()).collect();
    for (key, value) in set {
        let at = RUN_COLUMNS
            .iter()
            .position(|column| column == key)
            .unwrap_or_else(|| panic!("no column {key}"));
        cells[at] = (*value).to_string();
    }
    cells.join("\t")
}

/// The agent record of the adjudicator's confined processes.
pub fn agent() -> serde_json::Value {
    serde_json::json!({
        "harness": "claude-code",
        "version": "2.1.293",
        "agent_type": "claude -p --safe-mode",
        "model_reported": "claude-opus-5-5",
        "effort": "default",
        "tools": "Read,Write",
        "prompt_sha256": "a".repeat(64),
        "safe_mode": true,
        "args": [
            "-p", "--safe-mode", "--model", "claude-opus-5-5", "--tools", "Read,Write",
            "--strict-mcp-config", "--no-session-persistence", "--permission-mode", "acceptEdits",
            "--output-format", "stream-json", "--verbose",
        ],
        "cwd": "an empty directory under the system temp directory outside any repository, removed after the call",
    })
}

/// The rows of the runs of one part: three voters, spaCy and the adjudicator, with ids from
/// `first`, and with `two` merges the adjudicator's second run. Each is `(set of cells)`.
fn part_runs(
    voters_sha: &str,
    voters: &serde_json::Value,
    first: usize,
    two: bool,
) -> Vec<Vec<(String, String)>> {
    let run = |offset: usize| format!("r{}", first + offset);
    let model = |name: &str| {
        voters["models"][name]["model"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut rows: Vec<Vec<(String, String)>> = Vec::new();
    let common = |run: String,
                  role: &str,
                  name: &str,
                  model: String,
                  endpoint: &str,
                  quantization: &str,
                  license: &str| {
        let mut cells: Vec<(String, String)> = [
            ("run", run.clone()),
            ("state_id", STATE.to_string()),
            ("role", role.to_string()),
            ("name", name.to_string()),
            ("status", "complete".to_string()),
            ("model", model),
            ("endpoint", endpoint.to_string()),
            ("quantization", quantization.to_string()),
            ("date", "2026-10-08".to_string()),
            ("guide_sha256", "9".repeat(64)),
            ("sentences", "5".to_string()),
            ("license", license.to_string()),
            ("license_checked", "2026-10-04".to_string()),
            ("deslag_commit", COMMIT.to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        cells.push(("voters_sha256".to_string(), voters_sha.to_string()));
        if role == "voter" {
            cells.push(("prompt_sha256".to_string(), "8".repeat(64)));
            cells.push(("listing".to_string(), format!("listings/{run}.json")));
        }
        cells
    };
    rows.push(common(
        run(0),
        "voter",
        "deepseek",
        model("deepseek"),
        "gmicloud/fp8",
        "fp8",
        "MIT",
    ));
    rows.push(common(
        run(1),
        "voter",
        "qwen",
        model("qwen"),
        "deepinfra/bf16",
        "bf16",
        "Apache-2.0",
    ));
    rows.push(common(
        run(2),
        "voter",
        "gemma",
        model("gemma"),
        "parasail/fp8",
        "fp8",
        "Apache-2.0",
    ));
    rows.push(common(
        run(3),
        "external",
        "spacy",
        "en-core-web-trf".to_string(),
        "local",
        "-",
        "MIT",
    ));
    let mut judge = common(
        run(4),
        "adjudicator",
        "opus",
        "claude-opus-5-5".to_string(),
        "claude-code",
        "-",
        "Anthropic Commercial Terms (adjudicator, D2a)",
    );
    judge.push(("prompt_sha256".to_string(), "7".repeat(64)));
    judge.push((
        "settings".to_string(),
        serde_json::json!({"agent": agent()}).to_string(),
    ));
    if two {
        let mut again = judge.clone();
        again[0].1 = run(5);
        rows.push(judge);
        rows.push(again);
    } else {
        rows.push(judge);
    }
    rows
}

impl Made {
    /// Two parts, labelled.
    pub fn new() -> Made {
        Made::with(2)
    }

    /// `parts` parts, labelled. The first word of the first sentence of each is one a voter
    /// disagrees on, which the adjudicator answers; the last part also has a disputed word in its
    /// last sentence that the adjudicator never answers, so that sentence is left out unsettled.
    pub fn with(parts: usize) -> Made {
        Made::with_mix(parts, "2,1,0,0", 6)
    }

    /// [Made::with], drawing `mix` from `per_tier` fixtures of each tier.
    pub fn with_mix(parts: usize, mix: &str, per_tier: usize) -> Made {
        Made::build_with(parts, mix, per_tier, false)
    }

    /// [Made::with], each part judged as the run judges it: into `merge` by the model voters, then
    /// into `merge-spacy`, which spaCy votes in and which is settled from `merge`. spaCy takes
    /// the second word of the first sentence for a noun where the models agree on a verb, so the
    /// adjudicator is asked a second time, in a run of its own, and the assembler reads
    /// `merge-spacy`.
    pub fn with_two_merges(parts: usize) -> Made {
        Made::build_with(parts, "2,1,0,0", 6, true)
    }

    fn build_with(parts: usize, mix: &str, per_tier: usize, two: bool) -> Made {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let tree = root.join("tree");
        wide_tree(&tree, per_tier);
        let small = other_small(&root);
        let gold = quiet_gold(&root);
        let exclude = root.join("exclude.tsv");
        fs::write(&exclude, format!("{}\n", "0".repeat(64))).unwrap();
        let silver = root.join("silver");
        fs::create_dir_all(&silver).unwrap();
        let retired = root.join("retired.tsv");
        fs::write(&retired, "batch\tdate\treason\n").unwrap();
        let voters = root.join("voters.json");
        let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/label/voters.json");
        fs::copy(real, &voters).unwrap();
        let made = Made {
            _tmp: tmp,
            root: root.clone(),
            tree,
            small,
            gold,
            exclude,
            silver,
            retired,
            voters,
            parts: Vec::new(),
            merge: if two { "merge-spacy" } else { "merge" },
            stride: if two { 6 } else { 5 },
        };
        let label = root.join(".label").join("silver");
        let out = Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
            .current_dir(&root)
            .arg("draw")
            .args(["--prefix", "s"])
            .args(made.pool_args())
            .args(["--tests-corpus"])
            .arg(&made.small)
            .args(["--mix", mix, "--parts", &parts.to_string(), "--dir"])
            .arg(&label)
            .output()
            .unwrap();
        Ran::of(&out).ok();
        let mut made = made;
        let voters_bytes = fs::read(&made.voters).unwrap();
        let voters_sha = sha256(&voters_bytes);
        let voters_json: serde_json::Value = serde_json::from_slice(&voters_bytes).unwrap();
        for number in 1..=parts {
            let dir = label.join(format!("part-{number:02}"));
            made.label(&dir, number, parts, &voters_sha, &voters_json, two);
            made.parts.push(dir);
        }
        made
    }

    /// The pool arguments every command that reads the corpus takes.
    pub fn pool_args(&self) -> Vec<String> {
        let path = |path: &Path| path.to_str().unwrap().to_string();
        vec![
            "--tree".to_string(),
            path(&self.tree),
            "--exclude".to_string(),
            path(&self.exclude),
            "--gold-dir".to_string(),
            path(&self.gold),
            "--silver".to_string(),
            path(&self.silver),
            "--silver-retired".to_string(),
            path(&self.retired),
            "--silver-parts".to_string(),
            path(&self.root.join(".label").join("silver")),
        ]
    }

    /// `deslag-gold ARGS` in the root, reading the made gold directory.
    pub fn gold(&self, args: &[&str]) -> Ran {
        self.gold_with(args, &[])
    }

    /// [Made::gold] with more environment.
    pub fn gold_with(&self, args: &[&str], env: &[(&str, &str)]) -> Ran {
        let mut command = Command::new(env!("CARGO_BIN_EXE_deslag-gold"));
        command
            .current_dir(&self.root)
            .env("DESLAG_GOLD_DIR", &self.gold)
            .env_remove("OPENROUTER_API_KEY")
            .args(args);
        for (key, value) in env {
            command.env(key, value);
        }
        Ran::of(&command.output().unwrap())
    }

    /// `deslag-gold --dir DIR ARGS`.
    fn gold_in(&self, dir: &Path, args: &[&str]) -> Ran {
        let mut all = vec!["--dir", dir.to_str().unwrap()];
        all.extend(args);
        self.gold(&all)
    }

    /// Labels the part in `dir`, which is part `number` of `of`.
    fn label(
        &self,
        dir: &Path,
        number: usize,
        of: usize,
        voters_sha: &str,
        voters: &serde_json::Value,
        two: bool,
    ) {
        let sentences = skeleton(&dir.join("sample.conllu"));
        assert!(sentences.len() >= 3, "a part needs three sentences");
        let first_run = 1 + (number - 1) * self.stride;
        let runs = part_runs(voters_sha, voters, first_run, two);
        let spacy_differs = if two { vec![(0, 2)] } else { Vec::new() };
        // The voters. The third model voter calls the first word of the first sentence a verb,
        // and in the last part the first word of the last sentence too.
        let mut changes = vec![(0, 1)];
        if number == of {
            changes.push((sentences.len() - 1, 1));
        }
        self.gold_in(dir, &["batches"]).ok();
        for (index, name) in ["deepseek", "qwen", "gemma"].into_iter().enumerate() {
            let text = if name == "gemma" {
                compact(&sentences, &changes, "V.fi")
            } else {
                compact(&sentences, &[], "N.s")
            };
            let file = dir.join(format!("{name}.reply.txt"));
            fs::write(&file, text).unwrap();
            let run = format!("r{}", first_run + index);
            self.gold_in(
                dir,
                &[
                    "read-tags",
                    "--check",
                    "--lines",
                    file.to_str().unwrap(),
                    "--prov",
                    name,
                    "--run",
                    &run,
                ],
            )
            .ok();
        }
        fs::write(
            dir.join("tags").join("spacy.conllu"),
            outside(&sentences, &format!("r{}", first_run + 3), &spacy_differs),
        )
        .unwrap();
        // With two merges the first has the model voters alone; spaCy votes in the second.
        let mut first_merge = vec![
            "merge", "--voter", "deepseek", "--voter", "qwen", "--voter", "gemma",
        ];
        if !two {
            first_merge.extend(["--voter", "spacy", "--base-only", "spacy"]);
        }
        self.gold_in(dir, &first_merge).ok();
        // The adjudicator answers the first item, and the last part's second one is left open.
        let judge = format!("r{}", first_run + 4);
        let item = format!("{}.{}", sentences[0].id, token_of(&sentences[0], 1));
        let answers = dir.join("answers.txt");
        fs::write(&answers, format!("{item}: N.s | the guide says so\n")).unwrap();
        self.gold_in(
            dir,
            &[
                "read-answers",
                "--check",
                "--answers",
                answers.to_str().unwrap(),
                "--run",
                &judge,
            ],
        )
        .ok();
        // The second merge settles from the first what the adjudicator answered there, and asks it
        // for the one word spaCy disputes that the models agreed on.
        if two {
            self.gold_in(
                dir,
                &[
                    "merge",
                    "--into",
                    "merge-spacy",
                    "--settled",
                    dir.join("merge").join("adjudicated.tsv").to_str().unwrap(),
                    "--voter",
                    "deepseek",
                    "--voter",
                    "qwen",
                    "--voter",
                    "gemma",
                    "--voter",
                    "spacy",
                    "--base-only",
                    "spacy",
                ],
            )
            .ok();
            let spacy_item = format!("{}.{}", sentences[0].id, token_of(&sentences[0], 2));
            let again = dir.join("answers-spacy.txt");
            fs::write(
                &again,
                format!("{spacy_item}: V.in | the models agree it is a verb\n"),
            )
            .unwrap();
            self.gold_in(
                dir,
                &[
                    "read-answers",
                    "--check",
                    "--into",
                    "merge-spacy",
                    "--answers",
                    again.to_str().unwrap(),
                    "--run",
                    &format!("r{}", first_run + 5),
                ],
            )
            .ok();
        }
        // The run table and the listings of the voters, then the finish.
        let mut table = format!("{}\n", RUN_COLUMNS.join("\t"));
        for cells in &runs {
            let set: Vec<(&str, &str)> = cells
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            table.push_str(&run_row(&set));
            table.push('\n');
        }
        fs::write(dir.join("runs.tsv"), table).unwrap();
        fs::create_dir_all(dir.join("listings")).unwrap();
        for cells in &runs {
            let get = |key: &str| {
                cells
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
                    .unwrap()
            };
            if get("role") == "voter" {
                fs::write(
                    dir.join("listings").join(format!("{}.json", get("run"))),
                    format!(
                        "{{\"model\": \"{}\", \"endpoints\": [{{\"tag\": \"{}\", \"quantization\": \"{}\"}}]}}\n",
                        get("model"),
                        get("endpoint"),
                        get("quantization")
                    ),
                )
                .unwrap();
            }
        }
        let merges: &[&str] = if two {
            &["merge", "merge-spacy"]
        } else {
            &["merge"]
        };
        for merge in merges {
            self.gold_in(
                dir,
                &[
                    "finish",
                    "--into",
                    merge,
                    "--trains",
                    "yes",
                    "--runs",
                    dir.join("runs.tsv").to_str().unwrap(),
                    "--leave-open",
                ],
            )
            .ok();
            fs::write(
                dir.join(merge).join("adjudicator.json"),
                "{\n  \"name\": \"opus\",\n  \"model\": \"claude-opus-5-5\"\n}\n",
            )
            .unwrap();
        }
    }

    /// The part `number`'s directory, from 1.
    pub fn part(&self, number: usize) -> &Path {
        &self.parts[number - 1]
    }

    /// `DIR:merge` for the part `number`, or `DIR:merge-spacy` when the parts were judged twice.
    pub fn spec(&self, number: usize) -> String {
        format!("{}:{}", self.part(number).display(), self.merge)
    }

    /// The text of `name` in part `number`.
    pub fn read(&self, number: usize, name: &str) -> String {
        fs::read_to_string(self.part(number).join(name)).unwrap()
    }

    /// Replaces the text of `name` in part `number`.
    pub fn write(&self, number: usize, name: &str, text: &str) {
        let path = self.part(number).join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Changes the text of `name` in part `number` by `change`.
    pub fn edit(&self, number: usize, name: &str, change: impl Fn(&str) -> String) {
        let text = self.read(number, name);
        let changed = change(&text);
        assert_ne!(text, changed, "the edit of {name} changed nothing");
        self.write(number, name, &changed);
    }

    /// Sets `column` of run `run` in part `number`'s `runs.tsv`.
    pub fn set_run(&self, number: usize, run: &str, column: &str, value: &str) {
        let at = RUN_COLUMNS.iter().position(|c| *c == column).unwrap();
        self.edit(number, "runs.tsv", |text| {
            text.lines()
                .map(|line| {
                    let mut cells: Vec<&str> = line.split('\t').collect();
                    if cells[0] == run {
                        cells[at] = value;
                    }
                    format!("{}\n", cells.join("\t"))
                })
                .collect()
        });
    }

    /// The id of run `n` (0 voter deepseek, 1 qwen, 2 gemma, 3 spaCy, 4 the adjudicator) of part
    /// `number`.
    pub fn run(&self, number: usize, n: usize) -> String {
        format!("r{}", 1 + (number - 1) * self.stride + n)
    }

    /// The ids of the sentences of part `number`, in the draw's order.
    pub fn ids(&self, number: usize) -> Vec<String> {
        skeleton(&self.part(number).join("sample.conllu"))
            .into_iter()
            .map(|sentence| sentence.id)
            .collect()
    }

    /// The cell `column` of sentence `id` in part `number`'s draw manifest.
    pub fn cell(&self, number: usize, id: &str, column: &str) -> String {
        let text = self.read(number, "manifest.tsv");
        let mut lines = text.lines().filter(|line| !line.starts_with('#'));
        let head: Vec<&str> = lines.next().unwrap().split('\t').collect();
        let at = head.iter().position(|name| *name == column).unwrap();
        lines
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .find(|cells| cells[0] == id)
            .map(|cells| cells[at].to_string())
            .unwrap_or_else(|| panic!("no sentence {id} in part {number}"))
    }

    /// `silver build --check-part` for part `number`.
    pub fn check_part(&self, number: usize) -> Ran {
        self.check_spec(&self.spec(number))
    }

    /// `silver build --check-part SPEC`.
    pub fn check_spec(&self, spec: &str) -> Ran {
        self.check_spec_with(spec, &[])
    }

    /// [Made::check_part] with more environment, such as the `HOME` the check takes for its own.
    pub fn check_part_with(&self, number: usize, env: &[(&str, &str)]) -> Ran {
        self.check_spec_with(&self.spec(number), env)
    }

    fn check_spec_with(&self, spec: &str, env: &[(&str, &str)]) -> Ran {
        let mut args: Vec<String> = ["silver", "build", "--check-part", spec]
            .into_iter()
            .map(str::to_string)
            .collect();
        args.extend(self.pool_args());
        args.extend(self.corpus_args());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.gold_with(&args, env)
    }

    /// The small tier and the `voters.json` to check against.
    pub fn corpus_args(&self) -> Vec<String> {
        vec![
            "--tests-corpus".to_string(),
            self.small.to_str().unwrap().to_string(),
            "--voters".to_string(),
            self.voters.to_str().unwrap().to_string(),
        ]
    }

    /// The template of this checkout.
    pub fn template() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/label/silver-datasheet.md")
    }

    /// Where batch `name` is built.
    pub fn out(&self, name: &str) -> PathBuf {
        self.root.join("out").join(name)
    }

    /// `silver build` of every part into batch `name`, with `extra` arguments.
    pub fn build(&self, name: &str, extra: &[&str]) -> Ran {
        let parts: Vec<String> = (1..=self.parts.len()).map(|n| self.spec(n)).collect();
        self.build_parts(name, &parts, extra)
    }

    /// [Made::build] with more environment, such as the `HOME` of the machine that builds.
    pub fn build_env(&self, name: &str, extra: &[&str], env: &[(&str, &str)]) -> Ran {
        let parts: Vec<String> = (1..=self.parts.len()).map(|n| self.spec(n)).collect();
        self.build_parts_with(name, &parts, extra, env)
    }

    /// `silver check --batch DIR` of the batch built as `name`, with more environment, such as the
    /// `HOME` of the machine that checks.
    pub fn check_batch_with(&self, name: &str, env: &[(&str, &str)]) -> Ran {
        let out = self.out(name);
        self.gold_with(&["silver", "check", "--batch", out.to_str().unwrap()], env)
    }

    /// `silver build` of the given parts.
    pub fn build_parts(&self, name: &str, parts: &[String], extra: &[&str]) -> Ran {
        self.build_parts_with(name, parts, extra, &[])
    }

    fn build_parts_with(
        &self,
        name: &str,
        parts: &[String],
        extra: &[&str],
        env: &[(&str, &str)],
    ) -> Ran {
        let out = self.out(name);
        let mut args: Vec<String> = ["silver", "build", "--name", name, "--out"]
            .into_iter()
            .map(str::to_string)
            .collect();
        args.push(out.to_str().unwrap().to_string());
        for part in parts {
            args.push("--part".to_string());
            args.push(part.clone());
        }
        args.extend(
            ["--annotations-license", "CC-BY-4.0", "--template"]
                .into_iter()
                .map(str::to_string),
        );
        args.push(Made::template().to_str().unwrap().to_string());
        args.extend(self.pool_args());
        args.extend(self.corpus_args());
        args.extend(extra.iter().map(|arg| arg.to_string()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.gold_with(&args, env)
    }
}

/// `text` with every sentence id of the form `FROM` and four digits renamed to `TO` and the same
/// digits: `rename_ids("s0001\tx", "s", "t")` is `t0001\tx`.
pub fn rename_ids(text: &str, from: &str, to: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        let wanted: Vec<char> = from.chars().collect();
        let here = chars[at..].starts_with(&wanted)
            && chars.len() >= at + wanted.len() + 4
            && chars[at + wanted.len()..at + wanted.len() + 4]
                .iter()
                .all(char::is_ascii_digit)
            && (at == 0 || !chars[at - 1].is_alphanumeric())
            && chars
                .get(at + wanted.len() + 4)
                .is_none_or(|next| !next.is_alphanumeric());
        if here {
            out.push_str(to);
            at += wanted.len();
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// The owner's review of a blind queue: the labels of `labels` copied in as his own, every
/// sentence reviewed, and those in `reject` rejected.
pub fn reviewed(queue: &str, labels: &str, reject: &[&str]) -> String {
    let mut codes = std::collections::BTreeMap::new();
    let mut id = String::new();
    for line in labels.lines() {
        if let Some(found) = line.strip_prefix("# sent_id = ") {
            id = found.to_string();
        } else if !line.starts_with('#') && !line.is_empty() {
            let cells: Vec<&str> = line.split('\t').collect();
            codes.insert(
                (id.clone(), cells[0].to_string()),
                (cells[3].to_string(), cells[5].to_string()),
            );
        }
    }
    let mut out = String::new();
    let mut id = String::new();
    for line in queue.lines() {
        if let Some(found) = line.strip_prefix("# sent_id = ") {
            id = found.to_string();
            out.push_str(line);
            out.push('\n');
            let mark = if reject.contains(&id.as_str()) {
                "owner_rejected"
            } else {
                "owner_reviewed"
            };
            out.push_str(&format!("# {mark} = 2026-10-20\n"));
        } else if !line.starts_with('#') && !line.is_empty() {
            let mut cells: Vec<String> = line.split('\t').map(str::to_string).collect();
            let (upos, feats) = codes[&(id.clone(), cells[0].clone())].clone();
            cells[3] = upos;
            cells[5] = feats;
            cells[9] = cells[9].replace("Kind=", "Prov=owner|Kind=");
            out.push_str(&cells.join("\t"));
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
