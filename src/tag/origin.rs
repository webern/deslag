//! Origin: where a word comes from, read from the token and the tokens beside it.
//!
//! An [`Origin`] is not a tag. `grep` is a command in *grep the logs* and in *run grep*, and what it
//! does in the sentence is for the tag. Origin is the tagger's input, set on each `Word` token by
//! [`origins`] before the tables read it, so every tagger and every import shares it, and the exam
//! finds it from a gold file's own text.
//!
//! The cues, in the order they are tried, and nothing else:
//!
//! 1. **Path**: a file name with an extension from [`EXTENSIONS`] (`main.rs`), after a stem.
//! 2. **Symbol**: a word with an underscore in it, or a lower-case letter straight before a capital
//!    (`foo_bar`, `FrobulatorFactory`, `userId`), and `foo::bar`, which the tokenizer splits in
//!    three. A word with a dot is never a symbol: `report_final.doc` is no source file's name.
//!    Version labels and digit shapes (`v2`, `ESP32`) are left to `shape.rs`.
//! 3. **Flag**: a word right after a leading `-` or `--`, and the words a hyphen then joins to it
//!    (`--no-verify`). A dash that follows a word or a number is no leading dash (`pre-commit`).
//! 4. **Command**: a name in [`PROGRAMS`], in lower case or capitalised first in its sentence, and a
//!    git subcommand in [`GIT`] right after `git`.
//!
//! A word with an apostrophe is English. The names are facts, not a work of anyone's, so no licence
//! travels with the lists; they are written by hand from git's own command list, the GNU
//! coreutils manual and what tools are called in everyday use. They are Rust data, sorted and
//! unique, and nothing reads them from a config or the environment. [`PROGRAMS`] holds no name that
//! a table holds (`make`, `docker`, `yarn`): those keep the readings the tables give them, and a
//! test fails when one is listed.
//!
//! **What the tagger does with it,** for a word neither table has, which is `Unknown`:
//!
//! - `Symbol` and `Path` read as a proper noun at `Likely`, keeping a noun: the guide tags such
//!   names `PN.s`, and dev holds 20 of 20.
//! - `Command` and `Flag` stay `Unknown`, best guess a proper noun, keeping a noun, and a verb
//!   too for a program that is also an English verb ([`VERBS`]).
//! - A command of that kind that opens an instruction with an object after it (*grep the logs*)
//!   reads as a verb, still `Unknown`, until gold measures it. Otherwise it is a proper noun, as in
//!   *run grep*.
//!
//! A word a table has keeps its table reading whatever its origin, and `Unsure` stays reserved for
//! table words, since the passes treat it as the tables' own.

use super::{Confidence, Features, Origin, Reading, Tag, TagSet, shape};
use crate::document::{Token, TokenKind};

/// The extensions that make a word a file name, lower case, sorted and unique: those of source,
/// configuration, data and build files. None that a site ends in (`io`, `co`, `uk`), no single
/// letter, which an abbreviation (`e.g`) ends in, and no document or image (`pdf`, `doc`, `jpg`):
/// the treebank tags an attachment such as *report.pdf* as a noun, never a name, so such a word
/// stays with `shape.rs`.
const EXTENSIONS: [&str; 55] = [
    "adoc", "bash", "bat", "bin", "cfg", "conf", "cpp", "css", "csv", "dll", "dylib", "env", "exe",
    "gradle", "hpp", "html", "ini", "ipynb", "jar", "java", "js", "json", "jsx", "lock", "log",
    "lua", "md", "mk", "php", "proto", "ps1", "py", "pyi", "rb", "rlib", "rs", "rst", "scss", "sh",
    "sql", "svelte", "tex", "tf", "toml", "ts", "tsv", "tsx", "txt", "vue", "wasm", "whl", "xml",
    "yaml", "yml", "zsh",
];

/// The most bytes of an extension in [`EXTENSIONS`].
const EXTENSION: usize = 6;

/// Where a name lands in [`FILTER`]: its first and last bytes and its length, mixed.
const fn slot(first: u8, last: u8, len: usize) -> usize {
    (first as usize * 31 + last as usize * 7 + len) & 1023
}

/// One bit for each [`slot`] a name of [`PROGRAMS`] lands in. A name that lands in none is no
/// program, so most words never reach the binary search.
const FILTER: [u64; 16] = {
    let mut filter = [0; 16];
    let mut at = 0;
    while at < PROGRAMS.len() {
        let name = PROGRAMS[at].as_bytes();
        let slot = slot(name[0], name[name.len() - 1], name.len());
        filter[slot / 64] |= 1 << (slot % 64);
        at += 1;
    }
    filter
};

/// Units written with a capital after a lower-case letter, sorted: `KiB`, `kHz`, `mAh`. They look
/// like camel case and are nouns, as the annotation guide says of a unit (*3.4 GiB*), so they are
/// no symbols.
const UNITS: [&str; 24] = [
    "EiB", "GiB", "KiB", "MiB", "PiB", "TiB", "YiB", "ZiB", "dB", "kB", "kHz", "kPa", "kV", "kW",
    "kWh", "mA", "mAh", "mL", "mV", "mW", "nF", "pF", "uA", "uF",
];

/// The most bytes of a program's name in [`PROGRAMS`], and the fewest.
const NAME: std::ops::RangeInclusive<usize> = 2..=13;

/// The words that may stand before a command that begins an instruction: `then grep the logs`.
const LEAD: [&str; 12] = [
    "and", "can", "first", "must", "next", "or", "please", "should", "then", "to", "will", "would",
];

/// The programs, by name, that no table holds, sorted and unique.
const PROGRAMS: [&str; 220] = [
    "ansible",
    "apk",
    "ar",
    "argocd",
    "aria2c",
    "asdf",
    "autoconf",
    "automake",
    "awk",
    "az",
    "b2sum",
    "base32",
    "base64",
    "basename",
    "bazel",
    "bc",
    "bg",
    "btop",
    "bzip2",
    "chcon",
    "chgrp",
    "chmod",
    "chown",
    "chroot",
    "cksum",
    "clippy",
    "cmake",
    "comm",
    "conda",
    "cp",
    "cpack",
    "cron",
    "crontab",
    "csplit",
    "ctest",
    "dd",
    "deno",
    "df",
    "dir",
    "dircolors",
    "direnv",
    "dirname",
    "dmesg",
    "dnf",
    "dpkg",
    "du",
    "env",
    "eslint",
    "eval",
    "exa",
    "expr",
    "eza",
    "fd",
    "fdisk",
    "ffmpeg",
    "fg",
    "fgrep",
    "flake8",
    "flyctl",
    "fmt",
    "fsck",
    "ftp",
    "fzf",
    "gcc",
    "gcloud",
    "gdb",
    "gh",
    "gofmt",
    "gpg",
    "gradle",
    "grep",
    "gsutil",
    "gunzip",
    "gzip",
    "hexdump",
    "hostid",
    "hostname",
    "htop",
    "iostat",
    "istioctl",
    "javac",
    "journalctl",
    "jq",
    "killall",
    "ksh",
    "kubectl",
    "kustomize",
    "ldd",
    "lldb",
    "ln",
    "logname",
    "lsblk",
    "lsof",
    "ltrace",
    "lz4",
    "magick",
    "meson",
    "mkdir",
    "mkfifo",
    "mknod",
    "mktemp",
    "mv",
    "mvn",
    "mypy",
    "nano",
    "nc",
    "ncdu",
    "netcat",
    "netlify",
    "netstat",
    "nl",
    "nmap",
    "nohup",
    "npm",
    "nproc",
    "npx",
    "nslookup",
    "numfmt",
    "nvim",
    "nvm",
    "objdump",
    "od",
    "openssl",
    "pacman",
    "pandoc",
    "pathchk",
    "pdflatex",
    "perf",
    "pinky",
    "pip3",
    "pipenv",
    "pipx",
    "pkill",
    "pnpm",
    "podman",
    "popd",
    "powershell",
    "pr",
    "printenv",
    "printf",
    "ps",
    "psql",
    "ptx",
    "pulumi",
    "pushd",
    "pwd",
    "pwsh",
    "pyenv",
    "pylint",
    "pytest",
    "python3",
    "rbenv",
    "readlink",
    "realpath",
    "rg",
    "rm",
    "rmdir",
    "rollup",
    "rpm",
    "rsync",
    "rustc",
    "rustfmt",
    "rustup",
    "scp",
    "sed",
    "seq",
    "sftp",
    "sha1sum",
    "sha224sum",
    "sha256sum",
    "sha384sum",
    "sha512sum",
    "shasum",
    "shellcheck",
    "shfmt",
    "shuf",
    "sqlite3",
    "ss",
    "ssh",
    "stat",
    "strace",
    "stty",
    "su",
    "sudo",
    "supabase",
    "systemctl",
    "tac",
    "tcpdump",
    "telnet",
    "terraform",
    "timeout",
    "tmux",
    "tox",
    "tr",
    "traceroute",
    "tsc",
    "tsort",
    "tty",
    "umount",
    "uname",
    "unexpand",
    "uniq",
    "unlink",
    "uptime",
    "uv",
    "vercel",
    "vi",
    "vite",
    "wc",
    "webpack",
    "wget",
    "whereis",
    "whoami",
    "xargs",
    "xxd",
    "xz",
    "yamllint",
    "yq",
    "zsh",
    "zstd",
];

/// The programs that are also English verbs, sorted: `grep the logs`.
const VERBS: [&str; 23] = [
    "awk", "chgrp", "chmod", "chown", "cp", "fgrep", "fsck", "grep", "gunzip", "gzip", "killall",
    "ln", "mkdir", "mv", "pkill", "rm", "rmdir", "rsync", "scp", "sed", "ssh", "sudo", "wget",
];

/// Git's subcommands, sorted, which are commands right after `git` whatever the tables say of them.
const GIT: [&str; 61] = [
    "add",
    "am",
    "annotate",
    "apply",
    "archive",
    "bisect",
    "blame",
    "branch",
    "bugreport",
    "bundle",
    "checkout",
    "cherry",
    "clean",
    "clone",
    "commit",
    "config",
    "describe",
    "diagnose",
    "diff",
    "difftool",
    "fetch",
    "fsck",
    "gc",
    "gitk",
    "grep",
    "gui",
    "help",
    "init",
    "instaweb",
    "log",
    "maintenance",
    "merge",
    "mergetool",
    "mktree",
    "mv",
    "notes",
    "prune",
    "pull",
    "push",
    "rebase",
    "reflog",
    "remote",
    "repack",
    "replace",
    "rerere",
    "reset",
    "restore",
    "revert",
    "rm",
    "scalar",
    "shortlog",
    "show",
    "stage",
    "stash",
    "status",
    "submodule",
    "subtree",
    "switch",
    "tag",
    "whatchanged",
    "worktree",
];

/// The origin of each token of one sentence, in order: `English` for every token that is no word,
/// and for a word no cue marks. It reads only the tokens' kind, text and place, so the exam and
/// the tagger agree on it.
pub fn origins(tokens: &[Token<'_>]) -> Vec<Origin> {
    // A copy, so that each word finds the origins before it where `mark` finds them: in the tokens.
    let mut work = tokens.to_vec();
    for at in 0..work.len() {
        work[at].origin = if work[at].kind == TokenKind::Word {
            origin_of(&work, at, true)
        } else {
            Origin::English
        };
    }
    work.iter().map(|token| token.origin).collect()
}

/// Sets the origin of the token at `at` of a sentence, whose word the tables have read, and reads a
/// word they lack by its origin (see the module docs). The origins before `at` are set already.
/// A name from [`PROGRAMS`] is looked up only for such a word, as no table holds one, which is what
/// [`origins`] would find too. Returns whether the origin is `Command`.
///
/// Debug builds run this on every word, so it keeps to plain loops and tests with no closure.
pub(super) fn mark(tokens: &mut [Token<'_>], at: usize) -> bool {
    if tokens[at].kind != TokenKind::Word {
        tokens[at].origin = Origin::English;
        return false;
    }
    let lacks = match tokens[at].reading {
        Some(reading) => reading.confidence == Confidence::Unknown,
        None => false,
    };
    let origin = origin_of(tokens, at, lacks);
    let token = &mut tokens[at];
    token.origin = origin;
    if lacks && let Some(reading) = token.reading {
        token.reading = Some(read(origin, &token.text, reading));
    }
    origin == Origin::Command
}

/// The origin of the word at `at`, given the origins `tokens` already hold for the words before it,
/// and whether a name in [`PROGRAMS`] counts, which it does for a word no table has.
///
/// It runs on every word of every document, so it reads a word's bytes once, and looks at a
/// neighbour's kind, which lives in the token, before its text, which does not.
fn origin_of(tokens: &[Token<'_>], at: usize, programs: bool) -> Origin {
    let text: &str = &tokens[at].text;
    if !is_plain(text.as_bytes())
        && let Some(origin) = by_marks(text)
    {
        // `:arrows_clockwise:` is an emoji, no name from code.
        return if origin == Origin::Symbol && is_shortcode(tokens, at) {
            Origin::English
        } else {
            origin
        };
    }
    // Dashes and colons are punctuation, so a word with none beside it has neither cue.
    let after_punctuation = at > 0 && tokens[at - 1].kind == TokenKind::Punctuation;
    let before_punctuation = at + 1 < tokens.len() && tokens[at + 1].kind == TokenKind::Punctuation;
    if after_punctuation || before_punctuation {
        if joins_double_colon(tokens, at) {
            return Origin::Symbol;
        }
        if after_punctuation && is_flag(tokens, at) {
            return Origin::Flag;
        }
    }
    // A name of PROGRAMS, which only a word the tables lack can be, or a git subcommand.
    let after_git = at > 0 && is_git(&tokens[at - 1]);
    if (programs || after_git) && is_command(tokens, at, programs) {
        Origin::Command
    } else {
        Origin::English
    }
}

/// Whether `token` is the word `git`.
fn is_git(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Word && token.text.len() == 3 && token.text.eq_ignore_ascii_case("git")
}

/// Whether every byte is a lower-case ASCII letter, which most words are.
fn is_plain(bytes: &[u8]) -> bool {
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].wrapping_sub(b'a') >= 26 {
            return false;
        }
        at += 1;
    }
    true
}

/// What the bytes of a word that is not lower-case letters alone say of its origin: `English` for
/// an apostrophe, `Path`, `Symbol`, or `None` when they say nothing.
fn by_marks(text: &str) -> Option<Origin> {
    let marks = Marks::of(text);
    if marks.non_ascii {
        // Rare, and read the slow way: a curly apostrophe is one.
        if text.contains(['\'', '\u{2019}']) {
            return Some(Origin::English);
        }
    } else if marks.apostrophe {
        return Some(Origin::English);
    }
    if marks.dot && is_path(text) {
        Some(Origin::Path)
    } else if marks.is_symbol(text) {
        Some(Origin::Symbol)
    } else {
        None
    }
}

/// What one pass over a word's bytes finds in it.
#[derive(Default)]
struct Marks {
    /// Any byte that is not a lower-case ASCII letter.
    other: bool,
    /// A byte outside ASCII.
    non_ascii: bool,
    apostrophe: bool,
    dot: bool,
    underscore: bool,
    /// An ASCII lower-case letter straight before an ASCII capital.
    camel: bool,
}

impl Marks {
    fn of(text: &str) -> Marks {
        let mut marks = Marks::default();
        let mut after_lower = false;
        for byte in text.bytes() {
            match byte {
                b'a'..=b'z' => {
                    after_lower = true;
                    continue;
                }
                b'A'..=b'Z' => marks.camel |= after_lower,
                b'\'' => marks.apostrophe = true,
                b'.' => marks.dot = true,
                b'_' => marks.underscore = true,
                0x80.. => marks.non_ascii = true,
                _ => {}
            }
            marks.other = true;
            after_lower = false;
        }
        marks
    }

    /// Whether `text`, whose marks these are, is written as a name from code: an underscore among
    /// its letters (`0001_initial` too), or camel or Pascal case. A word of lower-case letters
    /// alone has no such cue, one with a dot is no symbol, and a camel shape that starts with a
    /// digit (`1Password`) is left to `shape.rs`.
    fn is_symbol(&self, text: &str) -> bool {
        if !self.other || self.dot || UNITS.binary_search(&text).is_ok() {
            return false;
        }
        if self.non_ascii {
            return (text.contains('_') && text.chars().any(char::is_alphabetic))
                || (!text.starts_with(|c: char| c.is_ascii_digit()) && shape::is_camel_case(text));
        }
        (self.underscore && text.bytes().any(|byte| byte.is_ascii_alphabetic()))
            || (self.camel && !text.starts_with(|c: char| c.is_ascii_digit()))
    }
}

/// Whether the word at `at` is an emoji shortcode, `:rocket:`: a single colon straight before and
/// after it. The guide tags its word `X`.
fn is_shortcode(tokens: &[Token<'_>], at: usize) -> bool {
    let colon = |i: Option<usize>| {
        i.and_then(|i| tokens.get(i))
            .is_some_and(|token| token.kind == TokenKind::Punctuation && token.text == ":")
    };
    let joined = |a: usize, b: usize| tokens[a].range.end == tokens[b].range.start;
    at > 0
        && colon(Some(at - 1))
        && colon(Some(at + 1))
        && joined(at - 1, at)
        && joined(at, at + 1)
        && !colon(at.checked_sub(2))
        && !colon(Some(at + 2))
}

/// Whether the word at `at` is a part of `foo::bar`, which the tokenizer splits at the colons: a
/// word with `::` right before it and a word before that, or `::` right after it and a word after.
fn joins_double_colon(tokens: &[Token<'_>], at: usize) -> bool {
    let colon_at = |i: usize| tokens.get(i).is_some_and(|t| t.text == ":");
    if !(colon_at(at + 1) || at > 0 && colon_at(at - 1)) {
        return false;
    }
    let colon = |i: usize| {
        tokens
            .get(i)
            .is_some_and(|t| t.text == ":" && t.kind == TokenKind::Punctuation)
    };
    let word = |i: usize| tokens.get(i).is_some_and(|t| t.kind == TokenKind::Word);
    let joined = |a: usize, b: usize| tokens[a].range.end == tokens[b].range.start;
    let before = at >= 3
        && colon(at - 1)
        && colon(at - 2)
        && word(at - 3)
        && joined(at - 3, at - 2)
        && joined(at - 2, at - 1)
        && joined(at - 1, at);
    let after = colon(at + 1)
        && colon(at + 2)
        && word(at + 3)
        && joined(at, at + 1)
        && joined(at + 1, at + 2)
        && joined(at + 2, at + 3);
    before || after
}

/// Whether `text` is a file name with an extension of [`EXTENSIONS`] after a stem.
fn is_path(text: &str) -> bool {
    let Some((stem, extension)) = text.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty()
        || extension.len() > EXTENSION
        || stem
            .get(..4)
            .is_some_and(|s| s.eq_ignore_ascii_case("www."))
    {
        return false;
    }
    let mut lower = [0u8; EXTENSION];
    let bytes = extension.as_bytes();
    lower[..bytes.len()].copy_from_slice(bytes);
    lower[..bytes.len()].make_ascii_lowercase();
    std::str::from_utf8(&lower[..bytes.len()])
        .is_ok_and(|extension| EXTENSIONS.binary_search(&extension).is_ok())
}

/// Whether the token is a hyphen, `-`.
fn is_dash(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Punctuation && token.text == "-"
}

/// Whether the word at `at` is a flag: one or two dashes straight before it that lead, or a word
/// a hyphen joins to a flag (`verify` in `--no-verify`). the tokens before `at` hold their origins.
fn is_flag(tokens: &[Token<'_>], at: usize) -> bool {
    let joined = |a: usize, b: usize| tokens[a].range.end == tokens[b].range.start;
    let mut first = at;
    while first > 0 && at - first < 2 && is_dash(&tokens[first - 1]) && joined(first - 1, first) {
        first -= 1;
    }
    let dashes = at - first;
    if dashes == 0 {
        return false;
    }
    let Some(before) = first.checked_sub(1).filter(|before| joined(*before, first)) else {
        return true;
    };
    match tokens[before].kind {
        TokenKind::Word => dashes == 1 && tokens[before].origin == Origin::Flag,
        TokenKind::Number | TokenKind::Code | TokenKind::Url => false,
        _ => !is_dash(&tokens[before]),
    }
}

/// Whether the word at `at` names a program: a git subcommand right after `git`, or, if `programs`
/// holds, a name in [`PROGRAMS`] in lower case, or capitalised when no word comes before it in the
/// sentence.
fn is_command(tokens: &[Token<'_>], at: usize, programs: bool) -> bool {
    let text: &str = &tokens[at].text;
    if at > 0 && is_git(&tokens[at - 1]) && GIT.binary_search(&text).is_ok() {
        return true;
    }
    if !programs || !NAME.contains(&text.len()) || !text.is_ascii() {
        return false;
    }
    let bytes = text.as_bytes();
    let slot = slot(
        bytes[0].to_ascii_lowercase(),
        bytes[bytes.len() - 1],
        bytes.len(),
    );
    if FILTER[slot / 64] >> (slot % 64) & 1 == 0 {
        return false;
    }
    if text
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return PROGRAMS.binary_search(&text).is_ok();
    }
    let first = !tokens[..at]
        .iter()
        .any(|token| token.kind == TokenKind::Word);
    first
        && text.starts_with(|c: char| c.is_ascii_uppercase())
        && text[1..]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && PROGRAMS
            .binary_search(&text.to_ascii_lowercase().as_str())
            .is_ok()
}

/// Whether `text` names a program that is also an English verb.
fn is_verb(text: &str) -> bool {
    VERBS
        .binary_search(&text.to_ascii_lowercase().as_str())
        .is_ok()
}

/// The reading of a word that no table has and whose origin is `origin`, given its reading by
/// shape, which stands for `English`. See the module docs.
pub(super) fn read(origin: Origin, text: &str, by_shape: Reading) -> Reading {
    let names = TagSet::of(Tag::Noun).with(Tag::ProperNoun);
    let (confidence, kept) = match origin {
        Origin::English => return by_shape,
        Origin::Symbol | Origin::Path => (Confidence::Likely, names),
        Origin::Command if is_verb(text) => (Confidence::Unknown, names.with(Tag::Verb)),
        Origin::Command | Origin::Flag => (Confidence::Unknown, names),
    };
    Reading {
        tag: Tag::ProperNoun,
        features: Features::SINGULAR,
        confidence,
        kept,
    }
}

/// Reads as verbs the commands that open an instruction with an object after them (*grep the
/// logs*): the program is an English verb too, no word comes before it in the sentence or one of
/// [`LEAD`] does, and the next word can only be a determiner or a pronoun. The reading stays
/// `Unknown`; dev holds no such command, so nothing measures it yet.
pub(super) fn verbs(tokens: &mut [Token<'_>]) {
    for at in 0..tokens.len().saturating_sub(1) {
        let token = &tokens[at];
        let open = token.origin == Origin::Command
            && token.reading.is_some_and(|reading| {
                reading.confidence == Confidence::Unknown && reading.possible().contains(Tag::Verb)
            });
        if !open
            || !tokens[..at]
                .iter()
                .rev()
                .find(|t| t.kind == TokenKind::Word)
                .is_none_or(|t| LEAD.contains(&t.folded().as_str()))
        {
            continue;
        }
        let objects = TagSet::of(Tag::Determiner).with(Tag::Pronoun);
        let object = tokens[at + 1].reading.is_some_and(|next| {
            next.possible().intersection(objects) == next.possible() && !next.possible().is_empty()
        });
        if object && let Some(reading) = &mut tokens[at].reading {
            reading.tag = Tag::Verb;
            reading.features = Features::INFINITIVE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{Context, closed, fold, lexicon, sentence};

    /// Each word of `text` with its origin.
    fn of(text: &str) -> Vec<(String, Origin)> {
        let tokens = Token::split(text);
        let found = origins(&tokens);
        tokens
            .iter()
            .zip(found)
            .filter(|(token, _)| token.kind == TokenKind::Word)
            .map(|(token, origin)| (token.text.to_string(), origin))
            .collect()
    }

    /// The origin of the one word of `text` that is not `English`, which must be the only one.
    fn only(text: &str) -> Vec<(String, Origin)> {
        of(text)
            .into_iter()
            .filter(|(_, origin)| *origin != Origin::English)
            .collect()
    }

    fn marked(text: &str, origin: Origin) -> Vec<(String, Origin)> {
        vec![(text.to_string(), origin)]
    }

    fn in_a_table(text: &str) -> bool {
        let mut buf = [0; crate::tag::LONGEST];
        fold(text, &mut buf).is_some_and(|word| {
            closed::lookup(word)
                .or_else(|| lexicon::lookup(word))
                .is_some()
        })
    }

    #[test]
    fn the_lists_are_sorted_and_unique() {
        for (name, list) in [
            ("EXTENSIONS", &EXTENSIONS[..]),
            ("PROGRAMS", &PROGRAMS[..]),
            ("VERBS", &VERBS[..]),
            ("GIT", &GIT[..]),
            ("UNITS", &UNITS[..]),
        ] {
            assert!(
                list.windows(2).all(|pair| pair[0] < pair[1]),
                "{name} is not sorted and unique"
            );
            assert!(
                list.iter().all(|word| word.is_ascii()
                    && (name == "UNITS" || **word == word.to_ascii_lowercase())),
                "{name} holds a word that is not lower case ASCII"
            );
        }
        for name in PROGRAMS {
            assert!(NAME.contains(&name.len()), "{name} is outside NAME");
        }
        for verb in VERBS {
            assert!(
                PROGRAMS.binary_search(&verb).is_ok(),
                "{verb} is a verb but no program"
            );
        }
        assert!(
            EXTENSIONS
                .iter()
                .all(|e| e.len() <= EXTENSION && e.len() > 1)
        );
    }

    #[test]
    fn no_program_is_a_word_a_table_holds() {
        let held: Vec<&str> = PROGRAMS
            .iter()
            .copied()
            .filter(|name| in_a_table(name))
            .collect();
        assert!(
            held.is_empty(),
            "the tables hold these programs, which keep their table readings, so drop them: {held:?}"
        );
    }

    #[test]
    fn a_table_word_keeps_its_reading_whatever_its_origin() {
        // After a git, `commit` and `push` are commands; their readings are the tables', and only
        // a pass may narrow them.
        for text in ["commit", "push", "stash", "add"] {
            let words = format!("git {text}");
            let mut tokens = Token::split(&words);
            sentence(&mut tokens, Context::Prose);
            assert_eq!(tokens[1].origin, Origin::Command, "{text}");
            let reading = tokens[1].reading.unwrap();
            let by_table = crate::tag::read(text);
            assert_ne!(reading.confidence, Confidence::Unknown, "{text}");
            assert_eq!(
                reading.possible().intersection(by_table.possible()),
                reading.possible(),
                "{text}"
            );
        }
        let mut tokens = Token::split("Run make now.");
        sentence(&mut tokens, Context::Prose);
        assert_eq!(tokens[1].origin, Origin::English);
        let make = crate::tag::read("make");
        assert_eq!(tokens[1].reading.unwrap().possible(), make.possible());
    }

    #[test]
    fn an_identifier_is_a_symbol() {
        for text in [
            "foo_bar",
            "FrobulatorFactory",
            "userId",
            "_private",
            "x86_64",
            "snake_case_name",
        ] {
            assert_eq!(
                only(&format!("Use {text} here.")),
                marked(text, Origin::Symbol),
                "{text}"
            );
        }
        assert_eq!(
            only("The std::io module."),
            vec![
                ("std".to_string(), Origin::Symbol),
                ("io".to_string(), Origin::Symbol)
            ]
        );
    }

    #[test]
    fn a_shortcode_is_no_symbol_but_a_path_of_colons_is() {
        assert!(only("Added :arrows_clockwise: here.").is_empty());
        assert_eq!(
            only("Use std::io::Read now."),
            ["std", "io", "Read"].map(|w| (w.to_string(), Origin::Symbol))
        );
    }

    #[test]
    fn digit_shapes_and_plain_words_are_english() {
        for text in [
            "v2",
            "v1.2",
            "ESP32",
            "4th",
            "1990s",
            "400k",
            "NASA",
            "APIs",
            "Frobnitz",
            "e.g",
            "U.S",
            "www.adobe.com",
            "don't",
            "Matt's",
        ] {
            assert!(only(&format!("Use {text} here.")).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_file_name_with_an_extension_is_a_path() {
        for text in [
            "main.rs",
            "AGENTS.md",
            "REPORT.JSON",
            "config.toml",
            "FooBar.rs",
            "foo.test.js",
        ] {
            assert_eq!(
                only(&format!("Edit {text} now.")),
                marked(text, Origin::Path),
                "{text}"
            );
        }
        for text in [
            "e.g",
            "i.e",
            "example.com",
            "works.Then",
            "node.io",
            "p.m",
            "report.pdf",
            "Lisa_resume.doc",
            "UnleadedStocks.pdf",
            "logo.jpg",
            "KiB",
            "kHz",
            "mAh",
        ] {
            assert!(only(&format!("See {text} now.")).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_word_joined_to_a_leading_dash_is_a_flag() {
        assert_eq!(only("Pass --locked now."), marked("locked", Origin::Flag));
        assert_eq!(only("Pass -v now."), marked("v", Origin::Flag));
        assert_eq!(
            only("Pass (--release) now."),
            marked("release", Origin::Flag)
        );
        assert_eq!(
            only("Pass --no-verify now."),
            vec![
                ("no".to_string(), Origin::Flag),
                ("verify".to_string(), Origin::Flag)
            ]
        );
        for text in [
            "a pre-commit hook",
            "a - b",
            "the well--known fact",
            "- item",
            "an --- rule",
            "foo -- bar",
        ] {
            assert!(only(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_listed_program_or_a_git_subcommand_is_a_command() {
        assert_eq!(only("Run grep now."), marked("grep", Origin::Command));
        assert_eq!(only("Run rustfmt now."), marked("rustfmt", Origin::Command));
        assert_eq!(only("Grep the logs."), marked("Grep", Origin::Command));
        assert!(
            only("Run Grep now.").is_empty(),
            "capitalised mid-sentence is a name for the passes"
        );
        assert!(
            only("Run git stash now.")
                .iter()
                .any(|(word, origin)| word == "stash" && *origin == Origin::Command)
        );
        assert!(
            only("A stash of money.").is_empty(),
            "a subcommand needs git before it"
        );
    }

    #[test]
    fn a_symbol_or_a_path_the_tables_lack_is_a_likely_proper_noun_that_may_be_a_noun() {
        let mut tokens = Token::split("Open main.rs and FrobulatorFactory and foo_bar.");
        sentence(&mut tokens, Context::Prose);
        for at in [1, 3, 5] {
            let reading = tokens[at].reading.unwrap();
            assert_eq!(reading.tag, Tag::ProperNoun, "{}", tokens[at].text);
            assert_eq!(reading.confidence, Confidence::Likely);
            assert_eq!(reading.kept, TagSet::of(Tag::Noun).with(Tag::ProperNoun));
            assert_eq!(reading.features, Features::SINGULAR);
        }
    }

    #[test]
    fn a_command_or_flag_the_tables_lack_stays_unknown_as_a_proper_noun() {
        let mut tokens = Token::split("Run rustfmt and grep and --frobnicate.");
        sentence(&mut tokens, Context::Prose);
        let reading = |text: &str| {
            tokens
                .iter()
                .find(|t| t.text == text)
                .unwrap()
                .reading
                .unwrap()
        };
        for (text, kept) in [
            ("rustfmt", TagSet::of(Tag::Noun).with(Tag::ProperNoun)),
            (
                "grep",
                TagSet::of(Tag::Noun).with(Tag::ProperNoun).with(Tag::Verb),
            ),
            ("frobnicate", TagSet::of(Tag::Noun).with(Tag::ProperNoun)),
        ] {
            let reading = reading(text);
            assert_eq!(
                (reading.tag, reading.confidence, reading.kept),
                (Tag::ProperNoun, Confidence::Unknown, kept),
                "{text}"
            );
        }
    }

    #[test]
    fn a_command_opening_an_instruction_with_an_object_reads_as_a_verb_below_likely() {
        for text in [
            "grep the logs",
            "Grep the logs",
            "grep your logs",
            "then grep the logs",
            "to grep the logs",
        ] {
            let mut tokens = Token::split(text);
            sentence(&mut tokens, Context::Prose);
            let at = tokens
                .iter()
                .position(|t| t.text.eq_ignore_ascii_case("grep"))
                .unwrap();
            let reading = tokens[at].reading.unwrap();
            assert_eq!(
                (reading.tag, reading.confidence),
                (Tag::Verb, Confidence::Unknown),
                "{text}"
            );
            assert!(reading.kept.contains(Tag::ProperNoun), "{text}");
        }
        for text in [
            "run grep the logs",
            "run grep",
            "grep foo",
            "the grep command",
            "grep",
        ] {
            let mut tokens = Token::split(text);
            sentence(&mut tokens, Context::Prose);
            let at = tokens
                .iter()
                .position(|t| t.text.eq_ignore_ascii_case("grep"))
                .unwrap();
            let reading = tokens[at].reading.unwrap();
            assert_eq!(
                (reading.tag, reading.confidence),
                (Tag::ProperNoun, Confidence::Unknown),
                "{text}"
            );
        }
    }

    #[test]
    fn no_command_reaches_likely() {
        for text in [
            "grep the logs",
            "run grep",
            "Run rustfmt on it.",
            "git stash pop",
            "run cargo build",
            "ssh the box",
        ] {
            let mut tokens = Token::split(text);
            sentence(&mut tokens, Context::Prose);
            for token in &tokens {
                if token.origin == Origin::Command && !in_a_table(&token.text) {
                    assert_eq!(
                        token.reading.unwrap().confidence,
                        Confidence::Unknown,
                        "{text}: {}",
                        token.text
                    );
                }
            }
        }
    }

    #[test]
    fn a_token_that_is_no_word_is_english() {
        let tokens = Token::split("Pass --locked, 42 and https://example.com/foo_bar now.");
        for (token, origin) in tokens.iter().zip(origins(&tokens)) {
            if token.kind != TokenKind::Word {
                assert_eq!(origin, Origin::English, "{}", token.text);
            }
        }
    }
}
