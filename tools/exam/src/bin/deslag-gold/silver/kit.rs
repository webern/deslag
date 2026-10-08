//! `record/kit.tsv`: what the batch says about how it was made, which `silver check` holds it to
//! for as long as the image keeps it.

use std::collections::BTreeMap;

use deslag_exam::error::{Error, Place};

use super::layout::KIT;
use super::table::{Tsv, is_sha256};

/// The version of the rules `silver check` applies. A batch records the version it was built
/// under; `check` refuses a version it does not know, and the rules of a version never change once
/// a batch is live, so a batch that passed once passes for as long as the image holds it.
pub const CHECK_VERSION: u32 = 1;

/// The keys of `kit.tsv`, in the order it writes them.
pub const KEYS: [&str; 15] = [
    "check_version",
    "name",
    "deslag_commit",
    "tag_version",
    "tokens",
    "draw_source",
    "draw_parts",
    "parts",
    "min_voters",
    "voters_json_sha256",
    "template_sha256",
    "agent_sha256",
    "archive_sha256",
    "annotations_license",
    "audit_bar",
];

/// The rows of `kit.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kit {
    rows: BTreeMap<String, String>,
}

impl Kit {
    /// A kit from its rows, which must be exactly [`KEYS`].
    pub fn new(rows: BTreeMap<String, String>) -> Kit {
        debug_assert!(KEYS.iter().all(|key| rows.contains_key(*key)));
        Kit { rows }
    }

    /// Reads `text`, the file `kit.tsv`.
    pub fn parse(text: &str) -> Result<Kit, Error> {
        let table = Tsv::parse(KIT, text, Some(&["key", "value"]))?;
        let mut rows = BTreeMap::new();
        for row in &table.rows {
            if rows.insert(row[0].clone(), row[1].clone()).is_some() {
                return Err(Error::load(
                    KIT,
                    Place::File,
                    format!("`{}` is given twice", row[0]),
                ));
            }
        }
        // The version first: a kit of another version has other keys, and should say so.
        if rows.contains_key("check_version") {
            Kit { rows: rows.clone() }.version()?;
        }
        for key in KEYS {
            if !rows.contains_key(key) {
                return Err(Error::load(KIT, Place::File, format!("it has no `{key}`")));
            }
        }
        if let Some(extra) = rows.keys().find(|key| !KEYS.contains(&key.as_str())) {
            return Err(Error::load(
                KIT,
                Place::File,
                format!("`{extra}` is not a key of check version {CHECK_VERSION}"),
            ));
        }
        Ok(Kit { rows })
    }

    /// The file.
    pub fn render(&self) -> String {
        let mut out = String::from("key\tvalue\n");
        for key in KEYS {
            out.push_str(&format!("{key}\t{}\n", self.rows[key]));
        }
        out
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> &str {
        self.rows.get(key).map_or("", String::as_str)
    }

    /// The rules the batch was built under, or a problem when they are not known.
    pub fn version(&self) -> Result<u32, Error> {
        match self.get("check_version").parse::<u32>() {
            Ok(CHECK_VERSION) => Ok(CHECK_VERSION),
            _ => Err(Error::load(
                KIT,
                Place::File,
                format!(
                    "check_version is `{}`, and this build knows only {CHECK_VERSION}; a batch is checked by the rules it was built under",
                    self.get("check_version")
                ),
            )),
        }
    }

    /// The problems with the values themselves: hashes that are hashes, numbers that are numbers.
    pub fn problems(&self) -> Vec<Error> {
        let mut out = Vec::new();
        let mut bad = |message: String| out.push(Error::load(KIT, Place::File, message));
        for key in ["voters_json_sha256", "template_sha256"] {
            if !is_sha256(self.get(key)) {
                bad(format!("`{key}` is not a sha256"));
            }
        }
        for key in ["agent_sha256", "archive_sha256"] {
            let value = self.get(key);
            if value != "-" && !is_sha256(value) {
                bad(format!("`{key}` is a sha256 or `-`"));
            }
        }
        for key in ["draw_parts", "min_voters"] {
            if self.get(key).parse::<usize>().is_err() {
                bad(format!("`{key}` is not a number"));
            }
        }
        let bar = self.get("audit_bar");
        if bar != "-" && bar.parse::<f64>().is_err() {
            bad("`audit_bar` is a number or `-`".to_string());
        }
        for key in [
            "name",
            "deslag_commit",
            "annotations_license",
            "draw_source",
            "parts",
        ] {
            if matches!(self.get(key), "" | "-") {
                bad(format!("`{key}` is empty"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> Kit {
        let mut rows = BTreeMap::new();
        for key in KEYS {
            rows.insert(key.to_string(), "x".to_string());
        }
        for (key, value) in [
            ("check_version", "1"),
            ("draw_parts", "6"),
            ("min_voters", "3"),
            ("audit_bar", "95.0"),
            ("agent_sha256", "-"),
            ("archive_sha256", "-"),
        ] {
            rows.insert(key.to_string(), value.to_string());
        }
        rows.insert("voters_json_sha256".to_string(), "a".repeat(64));
        rows.insert("template_sha256".to_string(), "b".repeat(64));
        Kit::new(rows)
    }

    #[test]
    fn a_kit_is_written_and_read_back_and_unknown_versions_are_refused() {
        let kit = sample();
        assert_eq!(Kit::parse(&kit.render()).unwrap(), kit);
        assert!(kit.problems().is_empty());
        assert_eq!(kit.version().unwrap(), 1);
        let other = kit.render().replace("check_version\t1", "check_version\t2");
        let error = Kit::parse(&other).unwrap_err().to_string();
        assert!(error.contains("knows only 1"), "{error}");
        // A kit of another version, with other keys, is refused for its version, not its keys.
        let later = format!("{}new_key\t1\n", other.replace("min_voters\t3\n", ""));
        let error = Kit::parse(&later).unwrap_err().to_string();
        assert!(error.contains("knows only 1"), "{error}");
        let short = kit.render().replace("min_voters\t3\n", "");
        assert!(
            Kit::parse(&short)
                .unwrap_err()
                .to_string()
                .contains("no `min_voters`")
        );
        let extra = format!("{}more\t1\n", kit.render());
        assert!(
            Kit::parse(&extra)
                .unwrap_err()
                .to_string()
                .contains("not a key")
        );
        let bad = kit.render().replace(&"a".repeat(64), "short");
        assert!(
            Kit::parse(&bad).unwrap().problems()[0]
                .to_string()
                .contains("voters_json_sha256")
        );
    }
}
