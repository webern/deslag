//! One labelled part of a silver draw, read and checked before anything is assembled.
//!
//! A part is the directory a draw dealt (`sample.conllu`, `manifest.tsv`), the `runs.tsv` the
//! labelling wrote beside it, and a merge directory inside it that `finish --trains yes` made
//! (`labelled.conllu`, `voters.tsv`, `worklist.tsv`, `adjudicated.tsv`, `agreement.txt`,
//! `adjudicator.json`). [`Part::load`] is the preflight `silver build --check-part` runs at each
//! gate of the labelling, so a fault shows after one part and not at assembly; `silver build` loads
//! every part the same way. The preflight also holds the part to the lock of the parts of its
//! draw, [`PARTS_LOCK`], which spans the parts.
//!
//! Two kinds of thing go wrong. A fault in what was made (a model's licence, a run at two
//! commits, a word with no provenance, an id of the gold flow) is a problem, and the part is
//! refused. A sentence whose repository was reserved after the draw, or whose text is now a
//! gold text, is paid for already: it is dropped, counted and named in `record/drops.tsv`, and the
//! rule is met by leaving it out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use deslag_exam::conllu::{self, Block};
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::{Tier, kind_name};
use deslag_exam::tagger::Context;

use super::layout;
use super::runs::{self, Facts, Runs, VotersJson};
use super::table::{Machine, Tsv, machine_paths};
use crate::data::{self, Meta, Sample, read_text};
use crate::exclude::{Exclusion, Repos, Reserved, Texts};
use crate::labelling::GOLD_PREFIXES;
use crate::problems::Problems;

/// The fewest model voters a silver word needs (decision D2).
pub const MIN_MODEL_VOTERS: usize = 3;

/// The lock of the parts of a draw, in the directory that holds the part directories: the deslag
/// commit, the draw, the voters, the adjudicator, `min_voters`, each model's prompt and guide
/// hashes and the adjudicator's Claude Code (`agent.version` and `agent.args`), which every part
/// must share. `label.py` writes it at the first run of the first part and adds each field when a
/// step first knows it; `silver build --check-part` holds each part to it.
pub const PARTS_LOCK: &str = "lock.json";

/// A field of the lock of the parts, as its keys from the top, and its value.
pub type LockField = (Vec<String>, serde_json::Value);

/// A part to read: its directory and the name of the merge inside it, `DIR:MERGE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// The part's directory.
    pub dir: PathBuf,
    /// The merge directory's name under it.
    pub merge: String,
}

impl std::str::FromStr for Spec {
    type Err = String;

    fn from_str(text: &str) -> Result<Spec, String> {
        let (dir, merge) = text.rsplit_once(':').ok_or_else(|| {
            format!("`{text}` is not DIR:MERGE, a part's directory and its merge")
        })?;
        if dir.is_empty() || merge.is_empty() || merge.contains('/') || merge.contains('\\') {
            return Err(format!(
                "`{text}` is not DIR:MERGE; MERGE is the name of a directory in DIR, such as `merge`"
            ));
        }
        Ok(Spec {
            dir: PathBuf::from(dir),
            merge: merge.to_string(),
        })
    }
}

/// What a fixture of the corpus says about itself, kept apart from its bytes.
#[derive(Debug, Clone)]
pub struct Source {
    /// The host, `github.com`.
    pub host: String,
    /// `owner/name`.
    pub repo: String,
    /// The commit it was quoted at.
    pub commit: String,
    /// The permalink to the file.
    pub url: String,
    /// The licence expression.
    pub license: String,
    /// The licence files of the repository, as the sidecar names them.
    pub license_files: Vec<String>,
    /// The sha256 of the fixture's bytes.
    pub sha256: String,
    /// The generator a publisher names, or empty.
    pub model: String,
    /// That generator's licence, or empty.
    pub model_license: String,
}

/// What a part is checked against: today's gold, the reserved repositories, the corpus and the
/// `voters.json` the runs were made under.
pub struct Env {
    /// The texts of dev, holdout, owner and the queues.
    pub gold_texts: Texts,
    /// The sentence ids of dev and owner.
    pub gold_ids: BTreeSet<String>,
    /// The reserved repositories.
    pub reserved: Reserved,
    /// The repositories of `tests/corpus/`.
    pub small: Repos,
    /// The exclusion list.
    pub exclusion: Exclusion,
    /// The live fixtures of the corpus, by path.
    pub fixtures: BTreeMap<String, Source>,
    /// `voters.json`.
    pub voters: VotersJson,
    /// The bytes of `voters.json`.
    pub voters_bytes: Vec<u8>,
    /// The directories of this machine, which no word of a part may name.
    pub machine: Machine,
}

/// Where `Env` reads from.
pub struct EnvArgs<'a> {
    /// The corpus, the exclusion list and the gold directory.
    pub pool: &'a crate::Pool,
    /// The small tier.
    pub tests_corpus: &'a Path,
    /// The `voters.json` of this checkout.
    pub voters: &'a Path,
}

/// The `# sent_id` of every sentence of the gold file `path`, none if the file is not there.
fn sent_ids(path: &Path) -> Result<Vec<String>, Error> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let shown = path.display().to_string();
    let blocks = conllu::read(&shown, &read_text(path)?)?;
    Ok(blocks
        .iter()
        .filter_map(|block| block.comment("sent_id"))
        .map(|comment| comment.value.clone())
        .collect())
}

impl Env {
    /// Reads what a part is checked against.
    pub fn read(args: &EnvArgs<'_>) -> Result<Env, Problems> {
        let pool = args.pool;
        let mut gold_ids = BTreeSet::new();
        for name in ["dev.conllu", "owner.conllu"] {
            gold_ids.extend(sent_ids(&pool.gold_dir.join(name))?);
        }
        let shown = pool.exclude.display().to_string();
        let exclusion = Exclusion::parse(&shown, &read_text(&pool.exclude)?)?;
        let (fixtures, _) = crate::corpus_files(&pool.corpus, pool.tree.as_deref(), false, true)?;
        let fixtures = fixtures
            .iter()
            .map(|fixture| {
                let sidecar = &fixture.sidecar;
                let declared = sidecar.declared.as_ref();
                (
                    fixture.path.clone(),
                    Source {
                        host: sidecar.source.host.clone(),
                        repo: sidecar.source.repo.clone(),
                        commit: sidecar.source.commit.clone(),
                        url: sidecar.source.url.clone(),
                        license: sidecar.source.license.clone(),
                        license_files: sidecar.source.license_files.clone(),
                        sha256: sidecar.content.sha256.clone(),
                        model: declared.map(|d| d.model.clone()).unwrap_or_default(),
                        model_license: declared
                            .map(|d| d.model_license.clone())
                            .unwrap_or_default(),
                    },
                )
            })
            .collect();
        let voters_shown = args.voters.display().to_string();
        let voters_bytes = std::fs::read(args.voters).map_err(|source| Error::Io {
            path: voters_shown.clone(),
            source,
        })?;
        Ok(Env {
            gold_texts: Texts::gold(&pool.gold_dir)?,
            gold_ids,
            reserved: Reserved::read(&pool.gold_dir, &[])?,
            small: Repos::tests_corpus(args.tests_corpus)?,
            exclusion,
            fixtures,
            voters: VotersJson::parse(&voters_shown, &voters_bytes)?,
            voters_bytes,
            machine: Machine::here(),
        })
    }
}

/// A sentence taken out of the batch, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drop {
    /// Its id.
    pub id: String,
    /// The part it was in.
    pub part: usize,
    /// The reason: `gold text`, `reserved repository`, `repeat` or `audit rejected`.
    pub reason: &'static str,
}

/// A sentence of the part that is kept: its labels and where it came from.
#[derive(Debug, Clone)]
pub struct Kept {
    /// Its id.
    pub id: String,
    /// What the manifest says of it.
    pub meta: Meta,
    /// Its word lines, tab-separated, one per token, as `finish` wrote them.
    pub lines: Vec<String>,
    /// Its text.
    pub text: String,
    /// Its words, for the repeat check.
    pub toks: Vec<data::Tok>,
}

/// The sentences a merge left out for a word no one settled, counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsettled {
    /// The sentence's tier.
    pub tier: Tier,
    /// Its context.
    pub context: Context,
    /// Sentences.
    pub sentences: usize,
    /// Words.
    pub words: usize,
}

/// One part, read and checked.
pub struct Part {
    /// Its number, from the draw's `part = k of N`.
    pub number: usize,
    /// How many parts the draw was dealt into.
    pub of: usize,
    /// The draw's manifest header, less `part`.
    pub header: Vec<(String, String)>,
    /// The sentences kept, in the order of the draw.
    pub kept: Vec<Kept>,
    /// The sentences dropped.
    pub drops: Vec<Drop>,
    /// The unsettled sentences and words, by tier and context.
    pub unsettled: Vec<Unsettled>,
    /// The sentences the draw dealt to the part.
    pub drawn: usize,
    /// The part's `runs.tsv`.
    pub runs: Runs,
    /// The runs the kept words and the voters name.
    pub used: BTreeSet<String>,
    /// What the rules found out of those runs.
    pub facts: Facts,
    /// The voters in file order: name, whether base only, run.
    pub voters: Vec<(String, bool, String)>,
    /// `min_voters` of the merge.
    pub min_voters: usize,
    merge: MergeFiles,
    /// The listings of the runs, by run id: `listings/<run>.json`.
    pub listings: BTreeMap<String, String>,
}

/// The merge directory's files, as text.
struct MergeFiles {
    voters: Tsv,
    worklist: Option<Tsv>,
    adjudicated: Option<Tsv>,
    agreement: String,
    adjudicator: String,
    unsettled: Vec<(String, usize)>,
}

/// Whether `id` is of the form the gold flow gives its sentences: its prefix and digits.
fn gold_flow_id(id: &str) -> bool {
    GOLD_PREFIXES.iter().any(|prefix| {
        id.strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

/// Each word line of `text`, a CoNLL-U file, in blocks.
fn raw_lines(text: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            open = false;
        } else {
            if !open {
                out.push(Vec::new());
                open = true;
            }
            if !line.starts_with('#') {
                out.last_mut()
                    .expect("a block is open")
                    .push(line.to_string());
            }
        }
    }
    out
}

/// The value of `key` in a MISC column.
pub(super) fn misc_of<'a>(misc: &'a str, key: &str) -> Option<&'a str> {
    conllu::pairs(misc)
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

/// The licence terms of an expression written `A OR B`.
fn terms(expression: &str) -> impl Iterator<Item = &str> {
    expression.split(" OR ")
}

impl Part {
    /// What the part says of each field of the lock of the parts ([`PARTS_LOCK`]): the commit of
    /// its runs, its draw header without `part`, its model voters in `voters.tsv`'s order, the
    /// name `adjudicator.json` gives, `min_voters`, the prompt and guide hashes of each run it uses
    /// of a voter or the adjudicator, by the run's name, and the version and arguments of the
    /// Claude Code its adjudicator runs record. A field may come twice, from two runs.
    pub fn lock_fields(&self) -> Vec<LockField> {
        let key = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<String>>();
        let mut out: Vec<LockField> = Vec::new();
        if !self.facts.commit.is_empty() {
            out.push((key(&["deslag_commit"]), self.facts.commit.clone().into()));
        }
        for (name, value) in &self.header {
            out.push((key(&["draw", name]), value.clone().into()));
        }
        let voters: Vec<serde_json::Value> = self
            .voters
            .iter()
            .filter(|(_, base_only, _)| !base_only)
            .map(|(name, _, _)| name.clone().into())
            .collect();
        out.push((key(&["voters"]), voters.into()));
        if let Some(name) = serde_json::from_str::<serde_json::Value>(&self.merge.adjudicator)
            .ok()
            .and_then(|value| value["name"].as_str().map(str::to_string))
        {
            out.push((key(&["adjudicator"]), name.into()));
        }
        out.push((key(&["min_voters"]), self.min_voters.into()));
        for run in &self.used {
            if self.runs.row(run).is_none() {
                continue;
            }
            let role = self.runs.get(run, "role");
            let name = self.runs.get(run, "name");
            if role == "voter" || role == "adjudicator" {
                for hash in ["prompt_sha256", "guide_sha256"] {
                    out.push((
                        key(&["models", name, hash]),
                        self.runs.get(run, hash).into(),
                    ));
                }
            }
            if role == "adjudicator" {
                let settings: serde_json::Value =
                    serde_json::from_str(self.runs.get(run, "settings")).unwrap_or_default();
                for field in ["version", "args"] {
                    if let Some(value) = settings.get("agent").and_then(|agent| agent.get(field)) {
                        out.push((key(&["agent", field]), value.clone()));
                    }
                }
            }
        }
        out
    }

    /// Reads the part `spec` names and checks it against `env`.
    pub fn load(spec: &Spec, env: &Env) -> Result<Part, Problems> {
        let dir = &spec.dir;
        let merge_dir = dir.join(&spec.merge);
        data::refuse_holdout(&data::real_path(dir)?)?;
        let sample = Sample::open(dir)?;
        let mut problems: Vec<Error> = Vec::new();
        let shown = |name: &str| dir.join(name).display().to_string();
        let manifest_shown = shown("manifest.tsv");
        let sample_shown = shown("sample.conllu");

        // What kind of draw it is, and how it was dealt.
        if !sample.is_labelling_draw() {
            problems.push(Error::load(
                &manifest_shown,
                Place::File,
                "it is not a draw for labelling",
            ));
        }
        let (number, of) = match sample.manifest.get("part").and_then(parse_part) {
            Some(found) => found,
            None => {
                problems.push(Error::load(
                    &manifest_shown,
                    Place::File,
                    "it has no `# part = k of N`; silver is drawn with `draw --parts`, and the kit has no exemption for an undealt draw",
                ));
                (0, 0)
            }
        };
        let version = deslag::tag::VERSION.to_string();
        match sample.manifest.get("tag_version") {
            Some(drawn) if drawn == version => {}
            Some(drawn) => problems.push(Error::load(
                &manifest_shown,
                Place::File,
                format!(
                    "it was drawn under tag version {drawn}, and this build writes {version}; draw again"
                ),
            )),
            None => problems.push(Error::load(
                &manifest_shown,
                Place::File,
                "it records no tag_version; draw again with this kit",
            )),
        }
        // The tokens are what `deslag-exam tokens` writes at this build.
        let contexts: BTreeMap<&str, Context> = sample
            .manifest
            .rows
            .iter()
            .map(|(id, meta)| (id.as_str(), meta.context))
            .collect();
        let text = read_text(&dir.join("sample.conllu"))?;
        if data::skeleton(&sample.sents, |id| contexts.get(id).copied(), true) != text {
            problems.push(Error::load(
                &sample_shown,
                Place::File,
                "its tokens or origins are not what `deslag-exam tokens` writes for these sentences at this commit; draw again",
            ));
        }
        if let Err(found) = crate::sample::check_with_exam(&sample.sents) {
            problems.extend(found.0);
        }

        // Ids.
        for sent in &sample.sents {
            if gold_flow_id(&sent.id) {
                problems.push(Problems::sentence(
                    &sample_shown,
                    &sent.id,
                    format!(
                        "an id of the gold flow ({}); a silver draw has a prefix of its own",
                        GOLD_PREFIXES.join(", ")
                    ),
                ));
            }
            if env.gold_ids.contains(&sent.id) {
                problems.push(Problems::sentence(
                    &sample_shown,
                    &sent.id,
                    "its id is one of dev.conllu or owner.conllu",
                ));
            }
        }
        let mut seen = BTreeSet::new();
        for sent in &sample.sents {
            if !seen.insert(sent.id.as_str()) {
                problems.push(Problems::sentence(
                    &sample_shown,
                    &sent.id,
                    "the id is used twice",
                ));
            }
        }

        // The merge's files.
        let merge = read_merge(&merge_dir, &mut problems);
        let labelled_path = merge_dir.join("labelled.conllu");
        let labelled_shown = labelled_path.display().to_string();
        let labelled_text = read_text(&labelled_path)?;
        let blocks = conllu::read(&labelled_shown, &labelled_text)?;
        let raw = raw_lines(&labelled_text);
        match blocks
            .first()
            .and_then(|block| block.comment("exam.trains"))
            .map(|comment| comment.value.as_str())
        {
            Some("yes") => {}
            other => problems.push(Error::load(
                &labelled_shown,
                Place::File,
                format!(
                    "its header says `exam.trains = {}`, and silver is made only from labels that say `yes` (`finish --trains yes`)",
                    other.unwrap_or("nothing")
                ),
            )),
        }
        let index = sample.index_of();
        let mut labelled: BTreeMap<&str, (&Block, &Vec<String>)> = BTreeMap::new();
        for (block, lines) in blocks.iter().zip(&raw) {
            let Some(id) = block.comment("sent_id") else {
                continue;
            };
            if labelled.insert(id.value.as_str(), (block, lines)).is_some() {
                problems.push(Problems::sentence(
                    &labelled_shown,
                    &id.value,
                    "the id is used twice",
                ));
            }
        }
        let mut used_runs: BTreeSet<String> = BTreeSet::new();
        let mut candidates: Vec<(usize, Kept)> = Vec::new();
        for (id, (block, lines)) in &labelled {
            let Some(&at) = index.get(id) else {
                problems.push(Problems::sentence(
                    &labelled_shown,
                    id,
                    "the labels hold it and the draw does not",
                ));
                continue;
            };
            let sent = &sample.sents[at];
            let meta = sample
                .meta(id)
                .expect("a draw's manifest has every sentence")
                .clone();
            let before = problems.len();
            if block.lines.len() != sent.toks.len() || lines.len() != sent.toks.len() {
                problems.push(Problems::sentence(
                    &labelled_shown,
                    id,
                    "it has other tokens than the draw",
                ));
                continue;
            }
            for (number, (line, tok)) in block.lines.iter().zip(&sent.toks).enumerate() {
                let at = number + 1;
                let misc = &line.misc;
                let wrong_token = line.form != tok.form
                    || misc_of(misc, "Kind") != Some(kind_name(tok.kind))
                    || misc.split('|').any(|entry| entry == "SpaceAfter=No") != tok.joined;
                if wrong_token {
                    problems.push(Problems::sentence(
                        &labelled_shown,
                        id,
                        format!("token {at} `{}` is not the token the draw holds", line.form),
                    ));
                    continue;
                }
                if tok.is_word() {
                    if line.upos == "_" || line.upos.is_empty() {
                        problems.push(Problems::sentence(
                            &labelled_shown,
                            id,
                            format!("word {at} `{}` has no part of speech", line.form),
                        ));
                    }
                    match misc_of(misc, "Prov") {
                        Some("agree" | "adjudicated") => {}
                        Some(other) => problems.push(Problems::sentence(
                            &labelled_shown,
                            id,
                            format!("word {at} `{}` has `Prov={other}`; a silver word is agreed or adjudicated", line.form),
                        )),
                        None => problems.push(Problems::sentence(
                            &labelled_shown,
                            id,
                            format!("word {at} `{}` has no `Prov=`", line.form),
                        )),
                    }
                    match misc_of(misc, "Runs") {
                        Some(runs) if !runs.is_empty() => {
                            used_runs.extend(runs.split(',').map(str::to_string));
                        }
                        _ => problems.push(Problems::sentence(
                            &labelled_shown,
                            id,
                            format!("word {at} `{}` has no `Runs=`", line.form),
                        )),
                    }
                }
            }
            if problems.len() > before {
                continue;
            }
            candidates.push((
                at,
                Kept {
                    id: (*id).to_string(),
                    meta,
                    lines: (*lines).clone(),
                    text: sent.text(),
                    toks: sent.toks.clone(),
                },
            ));
        }
        candidates.sort_by_key(|(at, _)| *at);

        // Sentences of the draw the labels do not hold are the unsettled ones.
        let mut unsettled: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
        let mut unsettled_words: BTreeMap<&str, usize> = BTreeMap::new();
        for (id, words) in &merge.unsettled {
            *unsettled_words.entry(id.as_str()).or_default() += words;
        }
        for (id, words) in &unsettled_words {
            if labelled.contains_key(id) {
                problems.push(Problems::sentence(
                    &merge_dir.join("unsettled.tsv").display().to_string(),
                    id,
                    "it is listed as unsettled and the labels hold it",
                ));
            } else if !index.contains_key(id) {
                problems.push(Problems::sentence(
                    &merge_dir.join("unsettled.tsv").display().to_string(),
                    id,
                    "the draw does not hold it",
                ));
            } else if let Some(meta) = sample.meta(id) {
                let tier = Tier::ALL
                    .iter()
                    .position(|t| Some(*t) == meta.tier)
                    .unwrap_or(0);
                let context = Context::ALL
                    .iter()
                    .position(|c| *c == meta.context)
                    .unwrap_or(0);
                let entry = unsettled.entry((tier, context)).or_default();
                entry.0 += 1;
                entry.1 += words;
            }
        }
        for sent in &sample.sents {
            if !labelled.contains_key(sent.id.as_str())
                && !unsettled_words.contains_key(sent.id.as_str())
            {
                problems.push(Problems::sentence(
                    &labelled_shown,
                    &sent.id,
                    "the draw holds it, and it is neither labelled nor listed as unsettled",
                ));
            }
        }

        // Drops: a repository reserved since the draw, a text that is gold now.
        let mut kept = Vec::new();
        let mut drops = Vec::new();
        for (_, sentence) in candidates {
            let repo = &sentence.meta.repo;
            let reserved = env.reserved.dev.has(repo)
                || env.reserved.holdout.has(repo)
                || env.reserved.owner.has(repo)
                || env.reserved.queue.has(repo)
                || env.small.has(repo);
            let reason = if reserved {
                Some("reserved repository")
            } else if env.gold_texts.has(&sentence.toks) {
                Some("gold text")
            } else {
                None
            };
            match reason {
                Some(reason) => drops.push(Drop {
                    id: sentence.id,
                    part: number,
                    reason,
                }),
                None => kept.push(sentence),
            }
        }

        // The fixtures the kept sentences quote.
        let mut files: BTreeMap<&str, &Meta> = BTreeMap::new();
        for sentence in &kept {
            files
                .entry(sentence.meta.file.as_str())
                .or_insert(&sentence.meta);
        }
        for (file, meta) in files {
            fixture_rules(&manifest_shown, file, meta, env, &mut problems);
        }

        // Runs, voters and the adjudicator.
        let runs_path = dir.join("runs.tsv");
        let runs_shown = runs_path.display().to_string();
        let (runs, run_problems) = match read_text(&runs_path) {
            Ok(text) => Runs::parse(&runs_shown, &text).map_err(|error| vec![error]),
            Err(error) => Err(vec![error]),
        }
        .unwrap_or_else(|errors| {
            problems.extend(errors);
            (
                Runs::parse(&runs_shown, &format!("{}\n", runs::RUN_COLUMNS.join("\t")))
                    .expect("the columns parse")
                    .0,
                Vec::new(),
            )
        });
        problems.extend(run_problems);
        let voters = voter_rules(&merge, &runs, env, &merge_dir, &mut problems);
        used_runs.extend(voters.0.iter().map(|(_, _, run)| run.clone()));
        let vouchers = Vouchers::new(&merge.voters, voters.1, merge.adjudicated.as_ref());
        for (id, (block, _)) in &labelled {
            for (at, line) in block.lines.iter().enumerate() {
                let (Some(prov), Some(named)) =
                    (misc_of(&line.misc, "Prov"), misc_of(&line.misc, "Runs"))
                else {
                    continue;
                };
                if misc_of(&line.misc, "Kind") != Some("Word") {
                    continue;
                }
                let adjudicator =
                    |run: &str| runs.row(run).is_some() && runs.get(run, "role") == "adjudicator";
                if let Some(why) = vouchers.word(id, at + 1, prov, named, adjudicator) {
                    problems.push(Problems::sentence(
                        &labelled_shown,
                        id,
                        format!("word {} `{}`: {why}", at + 1, line.form),
                    ));
                }
            }
        }
        let facts = match runs::check(&runs_shown, &runs, &used_runs, &env.voters) {
            Ok(facts) => facts,
            Err(found) => {
                problems.extend(found);
                Facts::default()
            }
        };
        let listings = read_listings(dir, &runs, &used_runs, &mut problems);

        let part = Part {
            number,
            of,
            header: sample
                .manifest
                .header
                .iter()
                .filter(|(key, _)| key != "part")
                .cloned()
                .collect(),
            drawn: sample.sents.len(),
            kept,
            drops,
            unsettled: unsettled
                .into_iter()
                .map(|((tier, context), (sentences, words))| Unsettled {
                    tier: Tier::ALL[tier],
                    context: Context::ALL[context],
                    sentences,
                    words,
                })
                .collect(),
            runs,
            used: used_runs,
            facts,
            voters: voters.0,
            min_voters: voters.1,
            merge,
            listings,
        };
        // Nothing the part ships may hold a path of the maker's.
        for (path, text) in part.shipped(&part.kept_ids()) {
            for found in machine_paths(&path, &text, &env.machine) {
                problems.push(Error::load(
                    &path,
                    Place::File,
                    format!("it holds `{found}`, a path of the machine that made it"),
                ));
            }
        }
        for found in machine_paths(
            &runs_shown,
            &part.runs.only(&part.used).render(),
            &env.machine,
        ) {
            problems.push(Error::load(
                &runs_shown,
                Place::File,
                format!("it holds `{found}`, a path of the machine that made it"),
            ));
        }
        for (run, text) in &part.listings {
            let shown = format!("listings/{run}.json");
            for found in machine_paths(&shown, text, &env.machine) {
                problems.push(Error::load(
                    &shown,
                    Place::File,
                    format!("it holds `{found}`, a path of the machine that made it"),
                ));
            }
        }
        Problems::check(problems, part)
    }

    /// The ids of the kept sentences.
    pub fn kept_ids(&self) -> BTreeSet<String> {
        self.kept
            .iter()
            .map(|sentence| sentence.id.clone())
            .collect()
    }

    /// The part's files as the batch ships them, under `parts/NN/`, with the rows of the
    /// sentences in `keep` only. Unsettled words ship as counts, never with a form.
    pub fn shipped(&self, keep: &BTreeSet<String>) -> BTreeMap<String, String> {
        let dir = layout::part(self.number);
        let mut out = BTreeMap::new();
        // voters.tsv without the file column.
        let mut voters = Tsv {
            header: self.merge.voters.header.clone(),
            columns: ["letter", "voter", "base_only", "run"]
                .map(String::from)
                .to_vec(),
            rows: Vec::new(),
        };
        for row in &self.merge.voters.rows {
            voters.rows.push(row[..4].to_vec());
        }
        out.insert(format!("{dir}/voters.tsv"), voters.render());
        for (name, table) in [
            ("worklist.tsv", &self.merge.worklist),
            ("adjudicated.tsv", &self.merge.adjudicated),
        ] {
            if let Some(table) = table {
                let mut cut = table.clone();
                let at = table
                    .column("sent_id")
                    .expect("read_merge keeps only a table with a `sent_id` column");
                cut.rows.retain(|row| keep.contains(&row[at]));
                out.insert(format!("{dir}/{name}"), cut.render());
            }
        }
        out.insert(format!("{dir}/agreement.txt"), self.merge.agreement.clone());
        out.insert(
            format!("{dir}/adjudicator.json"),
            self.merge.adjudicator.clone(),
        );
        let mut counts = String::from("tier\tcontext\tsentences\twords\n");
        for row in &self.unsettled {
            counts.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                row.tier.name(),
                row.context.name(),
                row.sentences,
                row.words
            ));
        }
        out.insert(format!("{dir}/unsettled.tsv"), counts);
        out
    }
}

/// What vouches for the words of one part: the runs of its voters, `min_voters`, and the run of
/// each answer of its adjudicator. A word of the part is vouched for when
///
/// - `Prov=agree` names in `Runs=` only runs of the part's voters, at least `min_voters` of them of
///   model voters (spaCy votes on the part of speech alone and does not count), and no other run;
/// - `Prov=adjudicated` names in `Runs=` the one run that the part's `adjudicated.tsv` gives for
///   its answer about that word (`sent_id` and `token`), and that run is an adjudicator's.
///
/// So a batch's `voters.tsv`, `adjudicated.tsv` and `runs.tsv` say who made each word, and a
/// word cannot claim agreement it did not have.
pub(super) struct Vouchers {
    /// Each voter's run, and whether it is a model voter's.
    voters: BTreeMap<String, bool>,
    /// The fewest model voters an agreed word needs.
    min_voters: usize,
    /// The run of each answer, by sentence and token; `None` when the table has no `run` column.
    answers: Option<BTreeMap<(String, usize), String>>,
}

impl Vouchers {
    /// From the rows of a `voters.tsv` (`letter`, `voter`, `base_only`, `run` first), its
    /// `min_voters`, and the part's `adjudicated.tsv`, if it has one.
    pub(super) fn new(voters: &Tsv, min_voters: usize, adjudicated: Option<&Tsv>) -> Vouchers {
        let answers = match adjudicated {
            None => Some(BTreeMap::new()),
            Some(table) => match (
                table.column("sent_id"),
                table.column("token"),
                table.column("run"),
            ) {
                (Some(id), Some(token), Some(run)) => Some(
                    table
                        .rows
                        .iter()
                        .filter_map(|row| {
                            Some((
                                (row[id].clone(), row[token].parse().ok()?),
                                row[run].clone(),
                            ))
                        })
                        .collect(),
                ),
                _ => None,
            },
        };
        Vouchers {
            voters: voters
                .rows
                .iter()
                .map(|row| (row[3].clone(), row[2] != "yes"))
                .collect(),
            min_voters,
            answers,
        }
    }

    /// Why word `token` (from 1) of sentence `id`, of provenance `prov` and runs `runs`, is not
    /// vouched for, if it is not. `adjudicator` says whether a run is an adjudicator's.
    pub(super) fn word(
        &self,
        id: &str,
        token: usize,
        prov: &str,
        runs: &str,
        adjudicator: impl Fn(&str) -> bool,
    ) -> Option<String> {
        let named: Vec<&str> = runs.split(',').collect();
        match prov {
            "agree" => {
                if let Some(other) = named.iter().find(|run| !self.voters.contains_key(**run)) {
                    return Some(format!(
                        "it is agreed and names run {other}, which is not a voter's run of its part"
                    ));
                }
                // A run named twice is one voter.
                let models = named
                    .iter()
                    .filter(|run| self.voters[**run])
                    .collect::<BTreeSet<_>>()
                    .len();
                (models < self.min_voters).then(|| {
                    format!(
                        "it is agreed by the runs of {models} model voters, and min_voters is {}",
                        self.min_voters
                    )
                })
            }
            "adjudicated" => {
                let Some(answers) = &self.answers else {
                    return Some(
                        "it is adjudicated, and its part's adjudicated.tsv has no `run` column to say by whom"
                            .to_string(),
                    );
                };
                let Some(run) = answers.get(&(id.to_string(), token)) else {
                    return Some(format!(
                        "it is adjudicated, and its part's adjudicated.tsv has no answer for {id}.{token}"
                    ));
                };
                if named != [run.as_str()] {
                    return Some(format!(
                        "it is adjudicated and names runs {runs}, and the answer for {id}.{token} is run {run}'s"
                    ));
                }
                (!adjudicator(run)).then(|| {
                    format!("it is adjudicated by run {run}, which is not an adjudicator's")
                })
            }
            _ => None,
        }
    }
}

/// `k of N` as `(k, N)`.
fn parse_part(text: &str) -> Option<(usize, usize)> {
    let (k, n) = text.split_once(" of ")?;
    let (k, n): (usize, usize) = (k.trim().parse().ok()?, n.trim().parse().ok()?);
    (k >= 1 && k <= n).then_some((k, n))
}

/// What the corpus and the exclusion list say of the fixture `file` that `meta` quotes.
fn fixture_rules(manifest: &str, file: &str, meta: &Meta, env: &Env, problems: &mut Vec<Error>) {
    let mut bad = |message: String| {
        problems.push(Error::load(
            manifest,
            Place::File,
            format!("fixture {file}: {message}"),
        ));
    };
    let Some(source) = env.fixtures.get(file) else {
        bad(
            "the pinned image does not hold it (it was never there, or a batch excludes it)"
                .to_string(),
        );
        return;
    };
    let from = meta.provenance.clone().unwrap_or_default();
    if source.sha256 != from.sha256 {
        bad("its content is not what the draw quoted (sha256 differs)".to_string());
    }
    if source.commit != from.commit {
        bad("its commit is not the one the draw quoted".to_string());
    }
    if source.url != from.url {
        bad("its URL is not the one the draw quoted".to_string());
    }
    if source.repo.to_lowercase() != meta.repo.to_lowercase() {
        bad("its repository is not the one the manifest names".to_string());
    }
    if source.license != meta.license {
        bad(format!(
            "its sidecar licence `{}` is not the manifest's `{}`",
            source.license, meta.license
        ));
    }
    let outside: Vec<&str> = terms(&source.license)
        .filter(|term| !deslag_corpus::load::LICENSES.contains(term))
        .collect();
    if !outside.is_empty() {
        bad(format!(
            "its licence `{}` is not one the corpus accepts",
            source.license
        ));
    }
    if env.exclusion.lists(file, &source.sha256) {
        bad("the exclusion list names it".to_string());
    }
    if source.model != from.model || source.model_license != from.model_license {
        bad("the generator its sidecar declares is not the one the draw quoted".to_string());
    }
    if let Some(family) = crate::labelling::banned_generator(&source.model, &source.model_license) {
        bad(format!(
            "its declared generator is of the banned family `{family}`"
        ));
    }
}

/// Reads the merge directory's files, collecting what is missing or misshapen.
fn read_merge(dir: &Path, problems: &mut Vec<Error>) -> MergeFiles {
    let mut read = |name: &str, needed: bool| -> Option<String> {
        let path = dir.join(name);
        if !path.exists() {
            if needed {
                problems.push(Error::load(
                    &path.display().to_string(),
                    Place::File,
                    "the merge has no such file; run `finish --trains yes` after the adjudication",
                ));
            }
            return None;
        }
        match read_text(&path) {
            Ok(text) => Some(text),
            Err(error) => {
                problems.push(error);
                None
            }
        }
    };
    let voters_text = read("voters.tsv", true);
    let worklist = read("worklist.tsv", true);
    let adjudicated = read("adjudicated.tsv", false);
    let agreement = read("agreement.txt", true);
    let adjudicator = read("adjudicator.json", true);
    let unsettled = read("unsettled.tsv", false);
    let table = |name: &str, text: Option<String>, problems: &mut Vec<Error>| -> Option<Tsv> {
        let text = text?;
        let shown = dir.join(name).display().to_string();
        match Tsv::parse(&shown, &text, None) {
            Ok(table) if table.columns.len() >= 2 && table.column("sent_id").is_some() => {
                Some(table)
            }
            Ok(_) => {
                problems.push(Error::load(
                    &shown,
                    Place::File,
                    "it has no `sent_id` column",
                ));
                None
            }
            Err(error) => {
                problems.push(error);
                None
            }
        }
    };
    let voters_shown = dir.join("voters.tsv").display().to_string();
    let voters = voters_text
        .and_then(|text| {
            match Tsv::parse(
                &voters_shown,
                &text,
                Some(&["letter", "voter", "base_only", "run", "file"]),
            ) {
                Ok(table) => Some(table),
                Err(error) => {
                    problems.push(error);
                    None
                }
            }
        })
        .unwrap_or_else(|| Tsv {
            header: Vec::new(),
            columns: ["letter", "voter", "base_only", "run", "file"]
                .map(String::from)
                .to_vec(),
            rows: Vec::new(),
        });
    let mut unsettled_rows = Vec::new();
    if let Some(text) = unsettled {
        let shown = dir.join("unsettled.tsv").display().to_string();
        match Tsv::parse(&shown, &text, Some(&["sent_id", "token", "form"])) {
            Ok(table) => {
                for row in &table.rows {
                    unsettled_rows.push((row[0].clone(), 1));
                }
            }
            Err(error) => problems.push(error),
        }
    }
    MergeFiles {
        voters,
        worklist: table("worklist.tsv", worklist, problems),
        adjudicated: table("adjudicated.tsv", adjudicated, problems),
        agreement: agreement.unwrap_or_default(),
        adjudicator: adjudicator.unwrap_or_default(),
        unsettled: unsettled_rows,
    }
}

/// The merge's voters against the rules: at least three model voters, `min_voters` at least
/// three, spaCy among them, each with a run that `runs.tsv` describes under the same name.
/// Returns the voters and `min_voters`.
fn voter_rules(
    merge: &MergeFiles,
    runs: &Runs,
    env: &Env,
    dir: &Path,
    problems: &mut Vec<Error>,
) -> (Vec<(String, bool, String)>, usize) {
    let shown = dir.join("voters.tsv").display().to_string();
    let mut bad = |message: String| problems.push(Error::load(&shown, Place::File, message));
    let min_voters: usize = merge
        .voters
        .head("min_voters")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    if min_voters < MIN_MODEL_VOTERS {
        bad(format!(
            "min_voters is {min_voters}; silver needs a word agreed by at least {MIN_MODEL_VOTERS} model voters"
        ));
    }
    let voters: Vec<(String, bool, String)> = merge
        .voters
        .rows
        .iter()
        .map(|row| (row[1].clone(), row[2] == "yes", row[3].clone()))
        .collect();
    let model_voters = voters.iter().filter(|(_, base, _)| !base).count();
    if model_voters < MIN_MODEL_VOTERS {
        bad(format!(
            "the merge has {model_voters} model voters; silver needs at least {MIN_MODEL_VOTERS}"
        ));
    }
    if !voters
        .iter()
        .any(|(name, base, _)| name == runs::EXTERNAL_VOTER && *base)
    {
        bad(format!(
            "the merge has no {} among its voters; the pipeline's voters are the models and {}",
            runs::EXTERNAL_VOTER,
            runs::EXTERNAL_VOTER
        ));
    }
    for (name, base, run) in &voters {
        if run == "-" || run.is_empty() {
            bad(format!("voter {name} has no run recorded"));
            continue;
        }
        if runs.row(run).is_some() {
            if runs.get(run, "name") != name {
                bad(format!(
                    "voter {name} names run {run}, which runs.tsv has as a run of `{}`",
                    runs.get(run, "name")
                ));
            }
            let role = runs.get(run, "role");
            if (role == "external") != *base {
                bad(format!(
                    "voter {name}: base_only does not match run {run}'s role `{role}`"
                ));
            }
        }
        if !*base && env.voters.listed(name).is_none() {
            bad(format!("voter {name} is not a voter of voters.json"));
        }
    }
    let adjudicator = serde_json::from_str::<serde_json::Value>(&merge.adjudicator);
    match adjudicator {
        Ok(value) => {
            if value["name"].as_str() != Some(env.voters.adjudicator()) {
                bad(format!(
                    "adjudicator.json names `{}`, and voters.json has `{}`",
                    value["name"].as_str().unwrap_or("nothing"),
                    env.voters.adjudicator()
                ));
            }
        }
        Err(error) => bad(format!("adjudicator.json is not JSON: {error}")),
    }
    (voters, min_voters)
}

/// The listings of the runs `used` that have one, read from `dir/listings/<run>.json`.
fn read_listings(
    dir: &Path,
    runs: &Runs,
    used: &BTreeSet<String>,
    problems: &mut Vec<Error>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for run in used {
        if runs.row(run).is_none() || runs.get(run, "listing") == "-" {
            continue;
        }
        let path = dir.join("listings").join(format!("{run}.json"));
        match read_text(&path) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(_) => {
                    out.insert(run.clone(), text);
                }
                Err(error) => problems.push(Error::load(
                    &path.display().to_string(),
                    Place::File,
                    format!("not JSON: {error}"),
                )),
            },
            Err(error) => problems.push(error),
        }
    }
    out
}
