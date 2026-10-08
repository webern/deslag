//! The runs behind a part or a batch, and the rules about them that `--check-part`, `build` and
//! `silver check` share: `runs.tsv` as `label.py` writes it, set beside the `voters.json` it was
//! made under.
//!
//! A word names its runs in `Runs=`, and `runs.tsv` describes each: the model, the endpoint, the
//! licence that was true when the run was made, the commit of deslag, the hashes of the prompt and
//! the guide, and the sha256 of `voters.json` as it stood when the run began. A batch is checked
//! against these and the `voters.json` it carries in `record/`, never against the checkout.

use std::collections::{BTreeMap, BTreeSet};

use deslag_exam::error::{Error, Place};

use super::table::{Tsv, is_sha256, sha256_hex};

/// The columns of `runs.tsv`, as `RUN_COLUMNS` in `scripts/label/label.py` has them.
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

/// The licences of a labeller whose weights wrote the labels (decision D2): MIT and Apache-2.0.
pub const LABELLER_LICENSES: [&str; 2] = ["MIT", "Apache-2.0"];

/// What `agent.json` says of the processes that answered the adjudicator's requests, as
/// `AGENT_KEYS` in `scripts/label/label.py` has it.
pub const AGENT_KEYS: [&str; 10] = [
    "harness",
    "version",
    "agent_type",
    "model_reported",
    "effort",
    "tools",
    "prompt_sha256",
    "safe_mode",
    "args",
    "cwd",
];

/// The tools the adjudicator's processes have.
pub const AGENT_TOOLS: &str = "Read,Write";

/// The harness of the adjudicator's processes.
pub const AGENT_HARNESS: &str = "claude-code";

/// What kind of process each was.
pub const AGENT_TYPE: &str = "claude -p --safe-mode";

/// Where each process ran, as `CWD_RULE` in `scripts/label/confine.py` has it.
pub const AGENT_CWD: &str = "an empty directory under the system temp directory outside any repository, removed after the call";

/// The argument list of every confined call of the adjudicator's `model`, as `arguments` in
/// `scripts/label/confine.py` gives it.
pub fn confined_args(model: &str) -> Vec<String> {
    [
        "-p",
        "--safe-mode",
        "--model",
        model,
        "--tools",
        AGENT_TOOLS,
        "--strict-mcp-config",
        "--no-session-persistence",
        "--permission-mode",
        "acceptEdits",
        "--output-format",
        "stream-json",
        "--verbose",
    ]
    .map(String::from)
    .to_vec()
}

/// Whether `text` is a version as `claude --version` prints one: numbers joined by dots, then
/// perhaps a suffix with no space, such as `2.1.293` or `2.2.0-beta`.
pub fn is_version(text: &str) -> bool {
    let numbers = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .map_or(text, |at| &text[..at]);
    let parts: Vec<&str> = numbers.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|part| !part.is_empty())
        && !text.chars().any(char::is_whitespace)
}

/// The name of the outside tagger that may vote (decision D3).
pub const EXTERNAL_VOTER: &str = "spacy";

/// The description of a batch's runs, each run once.
#[derive(Debug, Clone)]
pub struct Runs {
    table: Tsv,
}

impl Runs {
    /// Reads `runs.tsv`, which came from `path`. A run that is written twice with the same cells
    /// is one run; with other cells, a problem. The key of a run is its id and its `state_id`.
    pub fn parse(path: &str, text: &str) -> Result<(Runs, Vec<Error>), Error> {
        let table = Tsv::parse(path, text, Some(&RUN_COLUMNS))?;
        Ok(Runs::from_table(path, table))
    }

    /// The runs of several tables as one: a run in two tables must have the same cells in both.
    pub fn union(path: &str, tables: &[&Runs]) -> (Runs, Vec<Error>) {
        let mut all = Tsv {
            header: Vec::new(),
            columns: RUN_COLUMNS.map(String::from).to_vec(),
            rows: Vec::new(),
        };
        for runs in tables {
            all.rows.extend(runs.table.rows.iter().cloned());
        }
        Runs::from_table(path, all)
    }

    fn from_table(path: &str, mut table: Tsv) -> (Runs, Vec<Error>) {
        let mut problems = Vec::new();
        let mut seen: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut kept = Vec::new();
        for row in std::mem::take(&mut table.rows) {
            let key = (row[0].clone(), row[1].clone());
            match seen.get(&key) {
                Some(earlier) if *earlier == row => continue,
                Some(_) => problems.push(Error::load(
                    path,
                    Place::File,
                    format!(
                        "run {} is written twice under state {} with different rows",
                        key.0, key.1
                    ),
                )),
                None => {
                    seen.insert(key, row.clone());
                    kept.push(row);
                }
            }
        }
        table.rows = kept;
        // `Runs=` names a run by its id alone, so the id must mean one state.
        let mut states: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for row in &table.rows {
            states.entry(&row[0]).or_default().insert(&row[1]);
        }
        for (run, states) in &states {
            if states.len() > 1 {
                problems.push(Error::load(
                    path,
                    Place::File,
                    format!(
                        "run {run} is under {} states, and a word names a run by its id alone",
                        states.len()
                    ),
                ));
            }
        }
        for row in &table.rows {
            if matches!(row[1].trim(), "" | "-") {
                problems.push(Error::load(
                    path,
                    Place::File,
                    format!("run {} has no state_id", row[0]),
                ));
            }
        }
        (Runs { table }, problems)
    }

    /// The row of run `id`.
    pub fn row(&self, id: &str) -> Option<&Vec<String>> {
        self.table.rows.iter().find(|row| row[0] == id)
    }

    /// The cell of run `id` in column `name`.
    pub fn get(&self, id: &str, name: &str) -> &str {
        self.row(id).map_or("", |row| self.table.cell(row, name))
    }

    /// The ids of the runs, in file order.
    pub fn ids(&self) -> Vec<&str> {
        self.table.rows.iter().map(|row| row[0].as_str()).collect()
    }

    /// The table of the runs `ids` name, in file order.
    pub fn only(&self, ids: &BTreeSet<String>) -> Tsv {
        Tsv {
            header: Vec::new(),
            columns: self.table.columns.clone(),
            rows: self
                .table
                .rows
                .iter()
                .filter(|row| ids.contains(&row[0]))
                .cloned()
                .collect(),
        }
    }
}

/// `voters.json`, as much of it as the rules read.
#[derive(Debug, Clone)]
pub struct VotersJson {
    value: serde_json::Value,
    /// The sha256 of the file's bytes.
    pub sha256: String,
}

impl VotersJson {
    /// Reads `bytes`, which came from `path`.
    pub fn parse(path: &str, bytes: &[u8]) -> Result<VotersJson, Error> {
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| Error::load(path, Place::File, format!("not JSON: {error}")))?;
        for key in ["voters", "adjudicator", "models"] {
            if value.get(key).is_none() {
                return Err(Error::load(path, Place::File, format!("it has no `{key}`")));
            }
        }
        Ok(VotersJson {
            value,
            sha256: sha256_hex(bytes),
        })
    }

    /// The entry of model `name`.
    fn model(&self, name: &str) -> Option<&serde_json::Value> {
        self.value.get("models")?.get(name)
    }

    /// The names of the voters.
    pub fn voter_names(&self) -> Vec<String> {
        self.value["voters"]
            .as_array()
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| name.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The name of the adjudicator.
    pub fn adjudicator(&self) -> &str {
        self.value["adjudicator"].as_str().unwrap_or("")
    }

    /// The model id, endpoint tags and quantisations of voter or adjudicator `name`: the tags are
    /// the pinned endpoint and its fallbacks. `None` when `name` is neither.
    pub fn listed(&self, name: &str) -> Option<Listed> {
        if !self.voter_names().iter().any(|voter| voter == name) && self.adjudicator() != name {
            return None;
        }
        let entry = self.model(name)?;
        let mut tags = Vec::new();
        tags.extend(entry["provider"].as_str().map(str::to_string));
        if let Some(more) = entry["provider_fallback"].as_array() {
            tags.extend(
                more.iter()
                    .filter_map(|tag| tag.as_str().map(str::to_string)),
            );
        }
        Some(Listed {
            model: entry["model"].as_str()?.to_string(),
            tags,
            quantizations: entry["quantizations"].as_array().map(|list| {
                list.iter()
                    .filter_map(|q| q.as_str().map(str::to_string))
                    .collect()
            }),
            handoff: entry["transport"].as_str() == Some("handoff"),
        })
    }

    /// The model of the outside tagger `name`.
    pub fn external(&self, name: &str) -> Option<String> {
        self.value
            .get("external")?
            .get(name)?
            .get("model")?
            .as_str()
            .map(str::to_string)
    }
}

/// What `voters.json` lists for a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The model id.
    pub model: String,
    /// The endpoint tags a run may be at.
    pub tags: Vec<String>,
    /// The quantisations, when the file pins them.
    pub quantizations: Option<Vec<String>>,
    /// Whether a person's own harness makes the calls.
    pub handoff: bool,
}

/// Whether `text` is a date written `YYYY-MM-DD`.
pub fn is_date(text: &str) -> bool {
    super::live::is_date(text)
}

/// What the rules found out about the runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The one commit of deslag every run was made at.
    pub commit: String,
    /// The sha256 of the one adjudicator record, `-` when no adjudicator run is named.
    pub agent_sha256: String,
    /// The model of each voter, by name.
    pub models: BTreeMap<String, String>,
}

/// The agent record of an adjudicator run: the `agent` object of its `settings`.
fn agent_of(settings: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(settings).ok()?;
    let agent = value.get("agent")?;
    agent.is_object().then(|| agent.clone())
}

/// The sha256 of a record, as its keys sorted and written compactly.
fn record_sha256(record: &serde_json::Value) -> String {
    // serde_json's maps are sorted by key, so the compact text is canonical.
    sha256_hex(record.to_string().as_bytes())
}

/// Checks the runs `used` (named by a word's `Runs=` or by a `voters.tsv`) of `runs` against
/// `voters`, the `voters.json` the batch carries. `path` is what the problems call `runs.tsv`.
///
/// Every named run must be described and complete, made at one commit of deslag that is not dirty,
/// with one prompt hash and one guide hash for each model. A voter's run must be at an endpoint
/// the file lists for its model, with a licence of MIT or Apache-2.0 that was read on a date, and
/// begin with the file's sha256. The outside tagger must be spaCy, under the same licence rule.
/// Every run, of every role, begins with the file's sha256. An adjudicator run must be the file's
/// adjudicator, answered by the confined processes of one agent record that says the harness, the
/// tools, the argument list and the working directory of the confined call.
pub fn check(
    path: &str,
    runs: &Runs,
    used: &BTreeSet<String>,
    voters: &VotersJson,
) -> Result<Facts, Vec<Error>> {
    let mut problems = Vec::new();
    let mut bad = |message: String| problems.push(Error::load(path, Place::File, message));
    let mut facts = Facts {
        agent_sha256: "-".to_string(),
        ..Facts::default()
    };
    let mut commits: BTreeSet<String> = BTreeSet::new();
    let mut hashes: BTreeMap<(String, &str), BTreeSet<String>> = BTreeMap::new();
    let mut agents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for run in used {
        let Some(_) = runs.row(run) else {
            bad(format!(
                "run {run} is named and runs.tsv does not describe it"
            ));
            continue;
        };
        let get = |column: &str| runs.get(run, column);
        if get("status") != "complete" {
            bad(format!(
                "run {run} is `{}`, and only a complete run may vouch for a word",
                get("status")
            ));
        }
        let commit = get("deslag_commit");
        if matches!(commit, "" | "-") || commit.ends_with("-dirty") {
            bad(format!(
                "run {run} was made at `{commit}`; a run is at a clean commit of deslag"
            ));
        }
        commits.insert(commit.to_string());
        let model = get("model").to_string();
        for column in ["prompt_sha256", "guide_sha256"] {
            hashes
                .entry((model.clone(), column))
                .or_default()
                .insert(get(column).to_string());
        }
        let role = get("role");
        let name = get("name");
        if matches!(role, "voter" | "external") {
            licence_rule(&mut bad, run, get("license"), get("license_checked"));
        }
        if get("voters_sha256") != voters.sha256 {
            bad(format!(
                "run {run} began under a voters.json of sha256 {}, and the record holds {}",
                get("voters_sha256"),
                voters.sha256
            ));
        }
        match role {
            "voter" => match voters.listed(name) {
                None => bad(format!(
                    "run {run} is of `{name}`, which voters.json does not list as a voter"
                )),
                Some(listed) => {
                    if listed.handoff {
                        bad(format!("run {run}: voter `{name}` is a handoff model"));
                    }
                    if get("model") != listed.model {
                        bad(format!(
                            "run {run} is of model {}, and voters.json has {} for `{name}`",
                            get("model"),
                            listed.model
                        ));
                    }
                    if !listed.tags.iter().any(|tag| tag == get("endpoint")) {
                        bad(format!(
                            "run {run} was at endpoint {}, which voters.json does not list for `{name}`",
                            get("endpoint")
                        ));
                    }
                    if let Some(known) = &listed.quantizations {
                        if !known.iter().any(|q| q == get("quantization")) {
                            bad(format!(
                                "run {run} was at quantization {}, which voters.json does not list for `{name}`",
                                get("quantization")
                            ));
                        }
                    }
                    facts.models.insert(name.to_string(), listed.model);
                }
            },
            "external" => {
                if name != EXTERNAL_VOTER {
                    bad(format!(
                        "run {run} is of the outside tagger `{name}`; only {EXTERNAL_VOTER} may vote (D3)"
                    ));
                } else if voters.external(name).as_deref() != Some(get("model")) {
                    bad(format!(
                        "run {run} is of model {}, which voters.json does not give for `{name}`",
                        get("model")
                    ));
                }
                facts
                    .models
                    .insert(name.to_string(), get("model").to_string());
            }
            "adjudicator" => {
                let listed = voters.listed(name);
                if name != voters.adjudicator() || !listed.as_ref().is_some_and(|l| l.handoff) {
                    bad(format!(
                        "run {run} is of adjudicator `{name}`, and the batch's adjudicator is the confined `{}`",
                        voters.adjudicator()
                    ));
                } else if listed.is_some_and(|l| get("model") != l.model) {
                    bad(format!(
                        "run {run} is of model {}, not the adjudicator's",
                        get("model")
                    ));
                }
                match agent_of(get("settings")) {
                    Some(agent) => {
                        agents
                            .entry(record_sha256(&agent))
                            .or_default()
                            .push(run.clone());
                        agent_rules(&mut bad, run, get("model"), &agent);
                    }
                    None => bad(format!(
                        "run {run} records no agent in its settings; its replies were not made by a confined process"
                    )),
                }
            }
            other => bad(format!("run {run} has the role `{other}`")),
        }
    }
    if commits.len() > 1 {
        bad(format!(
            "the runs were made at {} commits of deslag ({}); a batch is made at one",
            commits.len(),
            commits.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    for ((model, column), values) in &hashes {
        if values.len() > 1 {
            bad(format!(
                "model {model} has {} {column} values across its runs; a batch uses one",
                values.len()
            ));
        }
    }
    if agents.len() > 1 {
        bad(format!(
            "the adjudicator runs record {} different agents; a batch has one agent.json",
            agents.len()
        ));
    }
    facts.commit = commits.into_iter().next().unwrap_or_default();
    if let Some(sha) = agents.keys().next() {
        facts.agent_sha256 = sha.clone();
    }
    if problems.is_empty() {
        Ok(facts)
    } else {
        Err(problems)
    }
}

/// The licence of a voter's run: MIT or Apache-2.0, read on a date.
fn licence_rule(bad: &mut impl FnMut(String), run: &str, license: &str, checked: &str) {
    if license.is_empty() || license == "-" {
        bad(format!(
            "run {run} records no licence (`-`); a labeller's licence is read before its run"
        ));
    } else if !LABELLER_LICENSES.contains(&license) {
        bad(format!(
            "run {run} records the licence `{license}`; a labeller is under {} (D2)",
            LABELLER_LICENSES.join(" or ")
        ));
    }
    if !is_date(checked) {
        bad(format!(
            "run {run} records `{checked}` as the date its licence was read; it needs a date, YYYY-MM-DD"
        ));
    }
}

/// What one agent record of a run of `model` must say.
fn agent_rules(bad: &mut impl FnMut(String), run: &str, model: &str, agent: &serde_json::Value) {
    let missing: Vec<&str> = AGENT_KEYS
        .iter()
        .filter(|key| {
            agent.get(**key).is_none_or(|value| {
                value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty())
            })
        })
        .copied()
        .collect();
    if !missing.is_empty() {
        bad(format!(
            "run {run}: its agent record lacks {}",
            missing.join(", ")
        ));
        return;
    }
    if agent["safe_mode"] != serde_json::Value::Bool(true) {
        bad(format!(
            "run {run}: its agent record does not say safe_mode true"
        ));
    }
    if agent["tools"].as_str() != Some(AGENT_TOOLS) {
        bad(format!(
            "run {run}: its agent has other tools than {AGENT_TOOLS}"
        ));
    }
    if !agent["prompt_sha256"].as_str().is_some_and(is_sha256) {
        bad(format!("run {run}: its agent record has no prompt sha256"));
    }
    for (key, wanted) in [
        ("harness", AGENT_HARNESS),
        ("agent_type", AGENT_TYPE),
        ("cwd", AGENT_CWD),
    ] {
        if agent[key].as_str() != Some(wanted) {
            bad(format!(
                "run {run}: its agent record says {key} `{}`, and a confined call's is `{wanted}`",
                shown(&agent[key])
            ));
        }
    }
    if !agent["version"].as_str().is_some_and(is_version) {
        bad(format!(
            "run {run}: its agent record says version `{}`, which is not a version of Claude Code",
            shown(&agent["version"])
        ));
    }
    let args = confined_args(model);
    if agent["args"] != serde_json::json!(args) {
        bad(format!(
            "run {run}: its agent record gives the arguments `{}`, and a confined call of {model} is `{}`",
            shown(&agent["args"]),
            args.join(" ")
        ));
    }
}

/// A JSON value as a message shows it: a string bare, a list of strings joined by spaces.
fn shown(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) if items.iter().all(serde_json::Value::is_string) => items
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `voters.json` with three voters, spaCy and a confined adjudicator.
    pub fn voters_json() -> String {
        r#"{
  "voters": ["deepseek", "qwen", "gemma"],
  "adjudicator": "opus",
  "models": {
    "deepseek": {"model": "vendor-a/model-a", "provider": "host-a/fp8", "provider_fallback": ["host-b/fp8"], "quantizations": ["fp8"], "license": "MIT"},
    "qwen": {"model": "vendor-b/model-b", "provider": "host-c/bf16", "quantizations": ["bf16"], "license": "Apache-2.0"},
    "gemma": {"model": "vendor-c/model-c", "provider": "host-d/fp8", "quantizations": ["fp8"], "license": "Apache-2.0"},
    "opus": {"model": "vendor-d/judge", "provider": "claude-code", "transport": "handoff"}
  },
  "external": {"spacy": {"model": "en-core-web-trf", "license": "MIT"}}
}"#
        .to_string()
    }

    /// The `runs.tsv` row of a run with the cells `set` changes.
    pub fn row(run: &str, set: &[(&str, &str)]) -> String {
        let mut cells: BTreeMap<&str, String> = RUN_COLUMNS
            .iter()
            .map(|column| (*column, "-".to_string()))
            .collect();
        let agent = format!(
            r#"{{"request":{{}},"agent":{{"harness":"claude-code","version":"2.1.293","agent_type":"claude -p --safe-mode","model_reported":"vendor-d/judge","effort":"default","tools":"Read,Write","prompt_sha256":"{}","safe_mode":true,"args":{},"cwd":"{}"}}}}"#,
            "a".repeat(64),
            serde_json::json!(confined_args("vendor-d/judge")),
            AGENT_CWD
        );
        let (name, role, model, endpoint, quantization) = match run {
            "r1" => ("deepseek", "voter", "vendor-a/model-a", "host-a/fp8", "fp8"),
            "r2" => ("qwen", "voter", "vendor-b/model-b", "host-c/bf16", "bf16"),
            "r3" => ("gemma", "voter", "vendor-c/model-c", "host-d/fp8", "fp8"),
            "r4" => ("spacy", "external", "en-core-web-trf", "local", "3.8"),
            _ => ("opus", "adjudicator", "vendor-d/judge", "claude-code", "-"),
        };
        for (key, value) in [
            ("run", run),
            ("state_id", "s1"),
            ("role", role),
            ("name", name),
            ("status", "complete"),
            ("model", model),
            ("endpoint", endpoint),
            ("quantization", quantization),
            ("prompt_sha256", if role == "voter" { "p1" } else { "-" }),
            ("guide_sha256", "g1"),
            (
                "license",
                if role == "adjudicator" {
                    "terms"
                } else {
                    "MIT"
                },
            ),
            ("license_checked", "2026-10-04"),
            ("deslag_commit", "abc1234"),
        ] {
            cells.insert(key, value.to_string());
        }
        if role == "adjudicator" {
            cells.insert("settings", agent);
        }
        for (key, value) in set {
            cells.insert(key, (*value).to_string());
        }
        RUN_COLUMNS
            .iter()
            .map(|column| cells[column].clone())
            .collect::<Vec<_>>()
            .join("\t")
    }

    /// A `runs.tsv` of runs `r1` to `r5` with the voters' hash filled in.
    pub fn runs_text(voters_sha: &str, set: &[(&str, &[(&str, &str)])]) -> String {
        let mut out = format!("{}\n", RUN_COLUMNS.join("\t"));
        for id in ["r1", "r2", "r3", "r4", "r5"] {
            let mut cells: Vec<(&str, &str)> = vec![("voters_sha256", voters_sha)];
            if let Some((_, more)) = set.iter().find(|(run, _)| *run == id) {
                cells.extend(more.iter().copied());
            }
            out.push_str(&row(id, &cells));
            out.push('\n');
        }
        out
    }

    fn all() -> BTreeSet<String> {
        ["r1", "r2", "r3", "r4", "r5"].map(String::from).into()
    }

    fn checked(set: &[(&str, &[(&str, &str)])]) -> Result<Facts, Vec<Error>> {
        let voters = VotersJson::parse("voters.json", voters_json().as_bytes()).unwrap();
        let (runs, problems) = Runs::parse("runs.tsv", &runs_text(&voters.sha256, set)).unwrap();
        assert!(problems.is_empty());
        check("runs.tsv", &runs, &all(), &voters)
    }

    fn says(set: &[(&str, &[(&str, &str)])], part: &str) {
        let problems = checked(set).unwrap_err();
        let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert!(
            text.iter().any(|line| line.contains(part)),
            "{part}: {text:?}"
        );
    }

    #[test]
    fn runs_made_as_the_rules_want_pass_and_give_the_facts() {
        let facts = checked(&[]).unwrap();
        assert_eq!(facts.commit, "abc1234");
        assert_eq!(facts.agent_sha256.len(), 64);
        assert_eq!(facts.models["spacy"], "en-core-web-trf");
        assert_eq!(facts.models["gemma"], "vendor-c/model-c");
    }

    #[test]
    fn every_rule_about_the_runs_has_its_refusal() {
        says(&[("r1", &[("status", "failed")])], "only a complete run");
        says(
            &[("r2", &[("deslag_commit", "abc1234-dirty")])],
            "clean commit",
        );
        says(&[("r2", &[("deslag_commit", "-")])], "clean commit");
        says(&[("r2", &[("deslag_commit", "def5678")])], "2 commits");
        // Two prompts for one model.
        says(
            &[
                ("r1", &[("prompt_sha256", "p2")]),
                ("r2", &[("model", "vendor-a/model-a")]),
            ],
            "model vendor-a/model-a has 2 prompt_sha256 values",
        );
        says(&[("r1", &[("license", "-")])], "records no licence");
        says(&[("r4", &[("license", "-")])], "records no licence");
        says(&[("r3", &[("license", "GPL-3.0")])], "(D2)");
        says(
            &[("r3", &[("license_checked", "-")])],
            "date its licence was read",
        );
        says(
            &[("r3", &[("license_checked", "2026-13-01")])],
            "date its licence was read",
        );
        says(
            &[("r1", &[("voters_sha256", &"f".repeat(64))])],
            "began under a voters.json",
        );
        says(
            &[("r1", &[("endpoint", "host-z/fp8")])],
            "does not list for `deepseek`",
        );
        says(&[("r1", &[("quantization", "int4")])], "quantization int4");
        says(
            &[("r1", &[("model", "vendor-z/other")])],
            "voters.json has vendor-a/model-a",
        );
        says(&[("r4", &[("name", "harper")])], "only spacy may vote");
        says(
            &[("r4", &[("model", "other-model")])],
            "does not give for `spacy`",
        );
        says(&[("r5", &[("name", "claude")])], "confined `opus`");
        says(&[("r5", &[("role", "judge")])], "has the role `judge`");
        says(&[("r5", &[("settings", "-")])], "records no agent");
        let unsafe_agent = row("r5", &[]).replace("\"safe_mode\":true", "\"safe_mode\":false");
        let cells: Vec<&str> = unsafe_agent.split('\t').collect();
        says(&[("r5", &[("settings", cells[30])])], "safe_mode true");
        let other_tools =
            row("r5", &[]).replace("\"tools\":\"Read,Write\"", "\"tools\":\"Read,Write,Bash\"");
        let cells: Vec<&str> = other_tools.split('\t').collect();
        says(&[("r5", &[("settings", cells[30])])], "other tools");
        says(
            &[("r5", &[("voters_sha256", &"f".repeat(64))])],
            "run r5 began under a voters.json",
        );
        // The agent record is held by value: its arguments, version, working directory and kind.
        for (from, to, part) in [
            (
                "\"--strict-mcp-config\",",
                "",
                "a confined call of vendor-d/judge is `-p --safe-mode",
            ),
            (
                "\"--verbose\"",
                "\"--verbose\",\"--add-dir\",\"/\"",
                "--add-dir",
            ),
            (
                "\"version\":\"2.1.293\"",
                "\"version\":\"latest\"",
                "version `latest`",
            ),
            (
                "\"version\":\"2.1.293\"",
                "\"version\":\"2.1 .293\"",
                "not a version",
            ),
            ("outside any repository", "anywhere", "says cwd"),
            (
                "\"claude -p --safe-mode\"",
                "\"claude -p\"",
                "says agent_type `claude -p`",
            ),
            (
                "\"harness\":\"claude-code\"",
                "\"harness\":\"other\"",
                "says harness `other`",
            ),
        ] {
            let changed = row("r5", &[]).replacen(from, to, 1);
            assert_ne!(changed, row("r5", &[]), "{from}");
            let cells: Vec<&str> = changed.split('\t').collect();
            says(&[("r5", &[("settings", cells[30])])], part);
        }
    }

    #[test]
    fn a_version_is_numbers_joined_by_dots_and_a_suffix() {
        for good in ["2.1.293", "2.1", "10.0.0-beta.1", "2.1.293+build"] {
            assert!(is_version(good), "{good}");
        }
        for bad in [
            "",
            "2",
            "latest",
            "2..1",
            ".2.1",
            "2.1 .3",
            "v2.1.293",
            "2.1.293 (Claude Code)",
        ] {
            assert!(!is_version(bad), "{bad}");
        }
    }

    #[test]
    fn a_run_that_is_named_and_not_described_is_refused_and_one_agent_is_required() {
        let voters = VotersJson::parse("voters.json", voters_json().as_bytes()).unwrap();
        let (runs, _) = Runs::parse("runs.tsv", &runs_text(&voters.sha256, &[])).unwrap();
        let mut used = all();
        used.insert("r9".to_string());
        let problems = check("runs.tsv", &runs, &used, &voters).unwrap_err();
        assert!(
            problems[0]
                .to_string()
                .contains("run r9 is named and runs.tsv does not describe")
        );
        // A second adjudicator run by another agent version.
        let mut text = runs_text(&voters.sha256, &[]);
        let second = row("r5", &[("run", "r6"), ("voters_sha256", &voters.sha256)])
            .replace("2.1.293", "2.1.294");
        text.push_str(&second);
        text.push('\n');
        let (runs, _) = Runs::parse("runs.tsv", &text).unwrap();
        let mut used = all();
        used.insert("r6".to_string());
        let problems = check("runs.tsv", &runs, &used, &voters).unwrap_err();
        assert!(
            problems
                .iter()
                .any(|p| p.to_string().contains("2 different agents"))
        );
    }

    #[test]
    fn a_run_written_twice_is_one_run_when_the_rows_agree_and_a_problem_when_not() {
        let voters = VotersJson::parse("voters.json", voters_json().as_bytes()).unwrap();
        let base = runs_text(&voters.sha256, &[]);
        let first = base.lines().nth(1).unwrap().to_string();
        let (runs, problems) = Runs::parse("runs.tsv", &format!("{base}{first}\n")).unwrap();
        assert!(problems.is_empty());
        assert_eq!(runs.ids().len(), 5);
        let moved = first.replace("host-a/fp8", "host-b/fp8");
        let (_, problems) = Runs::parse("runs.tsv", &format!("{base}{moved}\n")).unwrap();
        assert!(
            problems[0]
                .to_string()
                .contains("run r1 is written twice under state s1")
        );
        let other_state = first.replacen("\ts1\t", "\ts2\t", 1);
        let (_, problems) = Runs::parse("runs.tsv", &format!("{base}{other_state}\n")).unwrap();
        assert!(problems[0].to_string().contains("run r1 is under 2 states"));
        let none = first.replacen("\ts1\t", "\t-\t", 1);
        let (_, problems) =
            Runs::parse("runs.tsv", &format!("{}\n{none}\n", RUN_COLUMNS.join("\t"))).unwrap();
        assert!(problems[0].to_string().contains("run r1 has no state_id"));
        // Other columns are another kit's.
        assert!(Runs::parse("runs.tsv", "run\tstate_id\n").is_err());
    }
}
