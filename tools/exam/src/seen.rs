//! The sidecar of a gold set: which of its words the sentence's own file writes in code.
//!
//! A word that neither table holds, and that the file it was quoted from also writes in a code span
//! or a fence, has the origin `Seen` (`deslag::tag::CodeWords`). A gold sentence is a few words of
//! a fixture, so the exam cannot find that out from the gold file; `<gold>.seen.tsv`, beside the
//! gold file, says it. The file holds numbers and never a word:
//!
//! ```text
//! # seen_rule = 1
//! # gold_sha256 = <a hash of the sentences' ids and forms>
//! <sent_id> TAB <ID> SPACE <ID> ...
//! ```
//!
//! There is one row for each sentence, in the gold file's order, and each ID is that of a `Word`
//! line whose form is among the code words of the fixture the sentence names in `# source`, or
//! that a manifest names for it (a silver set has no `# source`). The hash covers the `sent_id`s
//! and forms alone, so editing a tag does not stale it; `seen_rule` is the version of the rule
//! that picks the words, and a change to that rule stales every sidecar.
//!
//! Only a `deslag` gold file loads a sidecar. One that quotes a fixture (a `# source` line) or
//! names a silver batch needs one, and a stale one is an error that says how to write it again. A
//! reader that wants only the sentences' text reads the gold file bare, and a stale sidecar does
//! not stop it. The tool that writes it, `deslag-gold seen`, checks quietly that the bytes it reads
//! are the sentence the gold file has, and no error it gives carries a form or a path of a
//! fixture: on a holdout file a sentence is its position.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};

use deslag::document::{Document, TokenKind};
use deslag::tag::{CodeWords, Origin};
use sha2::{Digest, Sha256};

use crate::error::{Error, Place};
use crate::gold::{Gold, GoldSentence, TokenMode, hex};

/// The version of the rule that picks the words of a sidecar: a word of `Word` kind that is among
/// the code words of the sentence's fixture. A change to the rule raises it.
pub const RULE: u32 = 1;

/// The sidecar of the gold file at `gold`: `dev.conllu` has `dev.seen.tsv` beside it.
pub fn path_of(gold: &str) -> PathBuf {
    Path::new(gold).with_extension("seen.tsv")
}

/// The hash a sidecar names for `gold`: its `sent_id`s and the forms of their words, nothing else.
pub fn hash_of(gold: &Gold) -> String {
    let mut hash = Sha256::new();
    for sentence in &gold.sentences {
        hash.update(sentence.sent_id.as_bytes());
        hash.update([0]);
        for word in &sentence.words {
            hash.update(word.form.as_bytes());
            hash.update([1]);
        }
        hash.update(b"\n");
    }
    hex(&hash.finalize())
}

/// Whether `gold` needs a sidecar: it is a `deslag` file that quotes fixtures (a `# source` line,
/// as `dev.conllu`, `holdout.conllu` and `owner.conllu` have), or names a silver batch.
pub fn needs(gold: &Gold) -> bool {
    gold.tokens == TokenMode::Deslag
        && (gold.silver_batch.is_some() || gold.sentences.iter().any(|s| s.source.is_some()))
}

/// The text of the sidecar of `gold`, whose sentences' words have the IDs `ids` (one list each).
pub fn render(gold: &Gold, ids: &[Vec<usize>]) -> String {
    let mut out = format!(
        "# seen_rule = {RULE}\n# gold_sha256 = {}\n\
         # A row: a sentence's sent_id, a tab, and the IDs of its words that its file writes in code.\n",
        hash_of(gold)
    );
    for (sentence, ids) in gold.sentences.iter().zip(ids) {
        let ids: Vec<String> = ids.iter().map(usize::to_string).collect();
        let _ = writeln!(out, "{}\t{}", sentence.sent_id, ids.join(" "));
    }
    out
}

/// Reads the sidecar beside `gold` onto its sentences, if it has one or needs one.
pub fn attach(gold: &mut Gold) -> Result<(), Error> {
    if gold.tokens != TokenMode::Deslag {
        return Ok(());
    }
    let path = path_of(&gold.path);
    let shown = path.display().to_string();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if needs(gold) {
                return Err(Error::load(
                    &shown,
                    Place::File,
                    format!("there is none; `{}` writes it", command(gold)),
                ));
            }
            return Ok(());
        }
        Err(source) => {
            return Err(Error::Io {
                path: shown,
                source,
            });
        }
    };
    let writes = command(gold);
    let stale = |what: &str| {
        Error::load(
            &shown,
            Place::File,
            format!("{what}; `{writes}` writes it again"),
        )
    };
    let (mut rule, mut hash) = (None, None);
    let mut rows = Vec::new();
    for line in text.lines() {
        if let Some(comment) = line.strip_prefix('#') {
            match comment.split_once('=') {
                Some((key, value)) if key.trim() == "seen_rule" => {
                    rule = value.trim().parse::<u32>().ok();
                }
                Some((key, value)) if key.trim() == "gold_sha256" => {
                    hash = Some(value.trim().to_string());
                }
                _ => {}
            }
        } else if !line.trim().is_empty() {
            rows.push(line);
        }
    }
    if rule != Some(RULE) {
        return Err(stale("it is of another rule"));
    }
    if hash.as_deref() != Some(&hash_of(gold)) {
        return Err(stale("the gold file's sentences or words changed"));
    }
    if rows.len() != gold.sentences.len() {
        return Err(stale("it has another number of sentences"));
    }
    for (sentence, row) in gold.sentences.iter_mut().zip(rows) {
        let (id, numbers) = row.split_once('\t').unwrap_or((row, ""));
        if id != sentence.sent_id {
            return Err(stale("its sentences are not the gold file's, in order"));
        }
        let mut ids = Vec::new();
        for number in numbers.split_whitespace() {
            let id = number
                .parse::<usize>()
                .ok()
                .filter(|id| (1..=sentence.words.len()).contains(id))
                .ok_or_else(|| stale("a row names a word the sentence lacks"))?;
            ids.push(id);
        }
        let forms = ids.iter().map(|id| sentence.words[id - 1].form.as_str());
        sentence.code_words = CodeWords::of_forms(forms);
        sentence.code_ids = ids;
    }
    Ok(())
}

/// The command that writes the sidecar of `gold`. A silver split has no `# source` lines, so it
/// names the manifest of its batch, which the big tier unpacks beside the batch.
fn command(gold: &Gold) -> String {
    match &gold.silver_batch {
        Some(batch) => format!(
            "deslag-gold seen {} --manifest {SILVER_DIR}/{batch}/manifest.tsv",
            gold.path
        ),
        None => format!("deslag-gold seen {}", gold.path),
    }
}

/// Where `make fetch-blobs` unpacks each silver batch, in a directory named for the batch.
const SILVER_DIR: &str = ".blobs/unpacked/silver";

/// Where a sentence is: the fixture's path under the corpus root, and the bytes in it.
pub type Located = (String, Range<usize>);

/// Where the sentences of `gold` are, from their `# source` comments, or from `manifest` when it
/// is given: its `sent_id`, `file` and `bytes` columns.
pub fn locate(gold: &Gold, manifest: Option<&Path>) -> Result<Vec<Located>, Error> {
    let by_id = match manifest {
        Some(path) => Some(read_manifest(path)?),
        None => None,
    };
    gold.sentences
        .iter()
        .enumerate()
        .map(|(index, sentence)| {
            let found = match &by_id {
                Some(table) => table.get(sentence.sent_id.as_str()).cloned(),
                None => sentence.source.clone(),
            };
            found.ok_or_else(|| {
                Error::load(
                    &gold.path,
                    Place::Sentence(label(gold, index)),
                    "it has no source to look up",
                )
            })
        })
        .collect()
}

/// The `sent_id` -> (`file`, `bytes`) table of the manifest at `path`.
fn read_manifest(path: &Path) -> Result<BTreeMap<String, Located>, Error> {
    let shown = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: shown.clone(),
        source,
    })?;
    let bad = |message: &str| Error::load(&shown, Place::File, message);
    let mut lines = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty());
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| bad("no header row"))?
        .split('\t')
        .collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|found| *found == name)
            .ok_or_else(|| bad("a column is missing"))
    };
    let (id, file, bytes) = (column("sent_id")?, column("file")?, column("bytes")?);
    let mut table = BTreeMap::new();
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let (Some(id), Some(file), Some(bytes)) =
            (cells.get(id), cells.get(file), cells.get(bytes))
        else {
            return Err(bad("a row is short"));
        };
        let range = bytes
            .split_once('-')
            .and_then(|(start, end)| Some(start.parse().ok()?..end.parse().ok()?))
            .ok_or_else(|| bad("a row's bytes are not a range"))?;
        table.insert((*id).to_string(), ((*file).to_string(), range));
    }
    Ok(table)
}

/// How an error names the sentence at `index`: by `sent_id`, or by position in a holdout file.
fn label(gold: &Gold, index: usize) -> String {
    if gold.holdout() {
        (index + 1).to_string()
    } else {
        gold.sentences[index].sent_id.clone()
    }
}

/// Whitespace runs as one space, the ends trimmed: a form as the gold flow writes a token.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The IDs of the words of each sentence of `gold` that its fixture writes in code, `places` being
/// where the fixtures are (see [`locate`]) under `corpus`. Each sentence is looked up in its
/// fixture and its tokens checked against the gold file's lines; a sentence that is not where it is
/// said to be is an error that names it, by position on a holdout file, and nothing else.
pub fn compute(gold: &Gold, places: &[Located], corpus: &Path) -> Result<Vec<Vec<usize>>, Error> {
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, (file, _)) in places.iter().enumerate() {
        by_file.entry(file.as_str()).or_default().push(index);
    }
    let mut ids = vec![Vec::new(); gold.sentences.len()];
    for (file, sentences) in by_file {
        let first = sentences[0];
        let fail = |index: usize, message: &str| {
            Error::load(&gold.path, Place::Sentence(label(gold, index)), message)
        };
        let source = std::fs::read_to_string(corpus.join(file)).map_err(|_| {
            fail(
                first,
                "its fixture cannot be read; `make fetch-blobs` unpacks them",
            )
        })?;
        let document = Document::markdown(&source);
        let code = CodeWords::of(&document);
        for index in sentences {
            let sentence = &gold.sentences[index];
            let range = &places[index].1;
            let found = document
                .walk()
                .flat_map(|(block, _)| document.sentences_of(block))
                .find(|found| found.range == *range)
                .ok_or_else(|| fail(index, "its bytes are no sentence of its fixture"))?;
            let tokens = &document.tokens[found.tokens.clone()];
            if !matches_tokens(sentence, tokens) {
                return Err(fail(
                    index,
                    "its fixture's bytes are not the sentence the gold file has",
                ));
            }
            ids[index] = tokens
                .iter()
                .enumerate()
                .filter(|(_, token)| token.kind == TokenKind::Word && code.contains(&token.text))
                .map(|(at, _)| at + 1)
                .collect();
        }
    }
    Ok(ids)
}

/// How many words of `gold` have the origin `Seen`, given `ids` from [`compute`]: what the exam
/// finds from the sidecar, and fewer than the IDs, since a table word or a symbol a file writes in
/// code is not `Seen`.
pub fn seen_words(gold: &Gold, ids: &[Vec<usize>]) -> usize {
    gold.sentences
        .iter()
        .zip(ids)
        .map(|(sentence, ids)| {
            let code = CodeWords::of_forms(ids.iter().map(|id| &sentence.words[id - 1].form));
            deslag::tag::origins_seen(&sentence.tokens(), &code)
                .into_iter()
                .filter(|origin| *origin == Origin::Seen)
                .count()
        })
        .sum()
}

/// Whether `tokens` are the lines of `sentence`: as many, with the same forms.
fn matches_tokens(sentence: &GoldSentence, tokens: &[deslag::document::Token<'_>]) -> bool {
    sentence.words.len() == tokens.len()
        && sentence
            .words
            .iter()
            .zip(tokens)
            .all(|(word, token)| word.form == tidy(&token.text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "Use `kubectl` here.\n\nThe kubectl tool works.\n";

    /// A gold file of the second sentence of `FILE`, as the gold flow writes it.
    fn gold_text(second: &str) -> String {
        let start = FILE.find("The kubectl").unwrap();
        let end = start + "The kubectl tool works.".len();
        let mut out = format!(
            "# exam.tokens = deslag\n# exam.split = dev\n# exam.trains = no\n# sent_id = d1\n# source = f.md bytes {start}-{end}\n# text = x\n"
        );
        for (index, (form, kind)) in [
            ("The", "Word"),
            ("kubectl", "Word"),
            ("tool", "Word"),
            ("works", "Word"),
            (".", "Punctuation"),
        ]
        .iter()
        .enumerate()
        {
            let form = if index == 1 { second } else { form };
            let upos = if *kind == "Word" { "NOUN" } else { "PUNCT" };
            out.push_str(&format!(
                "{}\t{form}\t_\t{upos}\t_\t_\t_\t_\t_\tKind={kind}|Prov=agree\n",
                index + 1
            ));
        }
        out.push('\n');
        out
    }

    #[test]
    fn a_sidecar_names_the_words_the_fixture_writes_in_code() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.md"), FILE).unwrap();
        let gold = Gold::parse("dev.conllu", "dev.conllu", &gold_text("kubectl")).unwrap();
        let places = locate(&gold, None).unwrap();
        let ids = compute(&gold, &places, dir.path()).unwrap();
        assert_eq!(ids, vec![vec![2]]);
        assert!(render(&gold, &ids).contains("d1\t2\n"));
        // A sentence whose forms are not the fixture's is refused, naming the sentence only.
        let other = Gold::parse("dev.conllu", "dev.conllu", &gold_text("kubectl2")).unwrap();
        let error = compute(&other, &places, dir.path())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("sentence d1") && !error.contains("kubectl"),
            "{error}"
        );
    }

    #[test]
    fn the_hash_ignores_tags_and_a_sidecar_loads_and_goes_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dev.conllu");
        let text = gold_text("kubectl");
        std::fs::write(&path, &text).unwrap();
        let gold = Gold::read_bare(&path).unwrap();
        assert!(needs(&gold));
        let error = Gold::read(&path).unwrap_err().to_string();
        assert!(error.contains("deslag-gold seen"), "{error}");
        std::fs::write(path_of(&gold.path), render(&gold, &[vec![2]])).unwrap();
        let loaded = Gold::read(&path).unwrap();
        assert_eq!(loaded.sentences[0].code_ids, [2]);
        assert!(loaded.sentences[0].code_words.contains("kubectl"));
        // A tag edit leaves it fresh.
        std::fs::write(&path, text.replace("NOUN", "PROPN")).unwrap();
        assert!(Gold::read(&path).is_ok());
        // A form edit stales it.
        std::fs::write(&path, text.replace("tool", "tools")).unwrap();
        let error = Gold::read(&path).unwrap_err().to_string();
        assert!(error.contains("changed"), "{error}");
    }

    #[test]
    fn a_file_that_quotes_fixtures_needs_one_without_a_split() {
        // As `owner.conllu` is: `# source` lines, and no `exam.split`.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.conllu");
        std::fs::write(
            &path,
            gold_text("kubectl").replace("# exam.split = dev\n", ""),
        )
        .unwrap();
        let gold = Gold::read_bare(&path).unwrap();
        assert!(gold.split.is_none() && needs(&gold));
        let error = Gold::read(&path).unwrap_err().to_string();
        assert!(error.contains("there is none"), "{error}");
        std::fs::write(path_of(&gold.path), render(&gold, &[vec![2]])).unwrap();
        assert_eq!(Gold::read(&path).unwrap().sentences[0].code_ids, [2]);
    }

    #[test]
    fn the_advice_for_a_silver_split_names_its_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silver-train.conllu");
        let text = gold_text("kubectl")
            .replace("# exam.split = dev\n", "# silver.batch = b1\n")
            .replace("# source = ", "# note = ");
        std::fs::write(&path, &text).unwrap();
        let error = Gold::read(&path).unwrap_err().to_string();
        assert!(
            error.contains("seen.tsv")
                && error.contains("--manifest .blobs/unpacked/silver/b1/manifest.tsv"),
            "{error}"
        );
        let gold = Gold::read_bare(&path).unwrap();
        std::fs::write(
            path_of(&gold.path),
            render(&gold, &[vec![2]]).replace("= 1", "= 0"),
        )
        .unwrap();
        let error = Gold::read(&path).unwrap_err().to_string();
        assert!(
            error.contains("another rule") && error.contains("--manifest"),
            "{error}"
        );
    }

    #[test]
    fn a_seen_word_is_a_code_word_that_no_table_holds() {
        let gold = Gold::parse("dev.conllu", "dev.conllu", &gold_text("frobnozz")).unwrap();
        // `tool` is a table word: written in code, it is not `Seen`.
        assert_eq!(seen_words(&gold, &[vec![2, 3]]), 1);
        assert_eq!(seen_words(&gold, &[vec![3]]), 0);
    }

    #[test]
    fn a_ud_file_has_none_and_a_file_that_quotes_nothing_needs_none() {
        let ud = Gold::parse(
            "f.conllu",
            "f.conllu",
            "# sent_id = a\n# text = Hi\n1\tHi\t_\tINTJ\t_\t_\t_\t_\t_\t_\n",
        )
        .unwrap();
        assert!(!needs(&ud));
        let plain = gold_text("kubectl").replace("# source = ", "# note = ");
        let plain = Gold::parse("p.conllu", "p.conllu", &plain).unwrap();
        assert!(!needs(&plain));
    }
}
