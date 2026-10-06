//! Origin: where a word comes from, read from the token and the tokens beside it.
//!
//! An [`Origin`] is not a tag. `grep` is a command in *grep the logs* and in *run grep*, and what it
//! does in the sentence is for the tag. Origin is the tagger's input, set on each `Word` token by
//! [`origins`] before the tables read it, so every tagger and every import shares it, and the exam
//! finds it from a gold file's own text.
//!
//! Origin is a fact about the word, whatever the tables hold of it and whatever the tagger commits
//! to. The cues, in the order they are tried, and nothing else:
//!
//! 1. **Path**: a file name with an extension from [`EXTENSIONS`] or [`DOCUMENTS`], after a stem
//!    (`main.rs`, `report.pdf`).
//! 2. **Symbol**: a word with an underscore in it, or two lower-case letters straight before a
//!    capital (`foo_bar`, `FrobulatorFactory`, `userId`), `foo::bar`, which the tokenizer splits in
//!    three, and a dotted identifier (`os.path`, `this.setState`). One lower-case letter before a
//!    capital is no cue (`PhD`, `eBook`, `mRNA`, `kHz`). Version labels and digit shapes (`v2`,
//!    `ESP32`) are left to `shape.rs`.
//! 3. **Flag**: a word right after a leading `-` or `--`, and the words a hyphen then joins to it
//!    (`--no-verify`). A dash that follows a word or a number is no leading dash (`pre-commit`).
//! 4. **Command**: a name in [`PROGRAMS`], in lower case or capitalised first in its sentence; a
//!    name in [`HELD`], in lower case; a name in [`SHARED`], which is also an ordinary English word
//!    (`make`, `find`), when it is in the place of a command (after *run*, before a flag); and a git
//!    subcommand in [`GIT`] right after `git`.
//!
//! A word with an apostrophe is English. The names are facts, not a work of anyone's, so no licence
//! travels with the lists; they are written by hand from git's own command list, the GNU
//! coreutils manual and what tools are called in everyday use. They are Rust data, sorted and
//! unique, and nothing reads them from a config or the environment. [`PROGRAMS`] holds no name that
//! a table holds, and [`HELD`] and [`SHARED`] hold only such names (tests check both).
//!
//! **What the tagger commits to** is narrower than the origin, and is set for a word neither table
//! has, which is `Unknown`:
//!
//! - A `Symbol` without a dot, and a `Path` with a source, data or build extension, read as a proper
//!   noun at `Likely`, keeping a noun: the guide tags such names `PN.s`, and dev holds 20 of 20.
//!   A dotted identifier and a document or an image keep the reading `shape.rs` gives them: the
//!   treebank tags an attachment such as *report.pdf* as a noun.
//! - `Command` and `Flag` stay `Unknown`, best guess a proper noun, keeping a noun, and a verb
//!   too for a program that is also an English verb ([`VERBS`]).
//! - A command of that kind that opens an instruction with an object after it (*grep the logs*)
//!   reads as a verb, still `Unknown`, until gold measures it. Otherwise it is a proper noun, as in
//!   *run grep*.
//!
//! A word a table has keeps its table reading whatever its origin, and `Unsure` stays reserved for
//! table words, since the passes treat it as the tables' own.

use super::{Confidence, Features, Origin, Reading, Tag, TagSet, table};
use crate::document::{Token, TokenKind};

/// The extensions of source, configuration, data and build files, lower case, sorted and unique.
/// None that a site ends in (`io`, `co`, `uk`), and no single letter, which an abbreviation (`e.g`)
/// ends in. A file with one is a `Path` the tagger commits to as a name.
const EXTENSIONS: [&str; 55] = [
    "adoc", "bash", "bat", "bin", "cfg", "conf", "cpp", "css", "csv", "dll", "dylib", "env", "exe",
    "gradle", "hpp", "html", "ini", "ipynb", "jar", "java", "js", "json", "jsx", "lock", "log",
    "lua", "md", "mk", "php", "proto", "ps1", "py", "pyi", "rb", "rlib", "rs", "rst", "scss", "sh",
    "sql", "svelte", "tex", "tf", "toml", "ts", "tsv", "tsx", "txt", "vue", "wasm", "whl", "xml",
    "yaml", "yml", "zsh",
];

/// The extensions of documents and images, lower case, sorted and unique. A file with one is a
/// `Path` too, but the treebank tags *report.pdf* as a noun, so the tagger keeps its shape reading.
const DOCUMENTS: [&str; 24] = [
    "bmp", "doc", "docx", "epub", "gif", "heic", "ico", "jpeg", "jpg", "odp", "ods", "odt", "pdf",
    "png", "ppt", "pptx", "psd", "rtf", "svg", "tif", "tiff", "webp", "xls", "xlsx",
];

/// The most bytes of an extension in [`EXTENSIONS`] or [`DOCUMENTS`].
const EXTENSION: usize = 6;

/// The last part of a name that is a site, not an identifier (`example.com`), sorted.
const SITES: [&str; 16] = [
    "ai", "app", "ca", "co", "com", "de", "dev", "edu", "fr", "gov", "info", "io", "net", "org",
    "uk", "us",
];

/// Where a name lands in [`FILTER`]: its first, second and last bytes and its length, mixed. A name
/// is lower case or digits, which `| 0x20` leaves as they are, and it turns a capital to lower case.
const fn slot(first: u8, second: u8, last: u8, len: usize) -> usize {
    let mixed = ((first | 0x20) as usize * 0x9E37)
        ^ ((second | 0x20) as usize * 0x85EB)
        ^ (last as usize * 0xC2B3)
        ^ (len * 0x27D5);
    (mixed ^ (mixed >> 7)) & 8191
}

/// One bit for each [`slot`] a name of [`PROGRAMS`], [`HELD`] or [`SHARED`] lands in. A word that
/// lands in none is no program, so most words stop here, at a few instructions, before any other
/// test. A git subcommand is no part of it: it is looked up only right after `git`.
static FILTER: [u64; 128] = add(add(add([0; 128], &PROGRAMS), &HELD), &SHARED);

/// Where the key of a short name lands in [`NAME_FILTER`].
#[inline(always)]
const fn name_slot(key: u64) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 48) as usize
}

/// One bit for each [`name_slot`] of the table key of a name of [`PROGRAMS`], [`HELD`] or [`SHARED`]
/// of up to [`table::SHORT`] bytes. The tables find that key of every word they read anyway, so a
/// word that lands in no bit is no name, for the price of one multiply.
static NAME_FILTER: [u64; 1024] = {
    let mut filter = [0; 1024];
    let lists: [&[&str]; 3] = [&PROGRAMS, &HELD, &SHARED];
    let mut list = 0;
    while list < 3 {
        let mut at = 0;
        while at < lists[list].len() {
            let name = lists[list][at].as_bytes();
            if name.len() <= table::SHORT {
                let slot = name_slot(table::short_key_const(name));
                filter[slot / 64] |= 1 << (slot % 64);
            }
            at += 1;
        }
        list += 1;
    }
    filter
};

/// Whether a word whose lower-cased table key is `key`, of up to [`table::SHORT`] bytes, may be the
/// name of a program.
#[inline(always)]
pub(super) fn maybe_name(key: u64) -> bool {
    let slot = name_slot(key);
    NAME_FILTER[slot / 64] >> (slot % 64) & 1 != 0
}

/// `filter` with a bit set for each of `names`.
const fn add(mut filter: [u64; 128], names: &[&str]) -> [u64; 128] {
    let mut at = 0;
    while at < names.len() {
        let name = names[at].as_bytes();
        let slot = slot(name[0], name[1], name[name.len() - 1], name.len());
        filter[slot / 64] |= 1 << (slot % 64);
        at += 1;
    }
    filter
}

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

/// The programs a table holds, and that no one reads as anything else (`git`, `cargo`, `ls`),
/// sorted. A name of one, in lower case, is a `Command`; its reading stays the table's.
const HELD: [&str; 20] = [
    "apt", "bash", "cargo", "cd", "curl", "cvs", "docker", "emacs", "git", "hg", "ls", "lynx",
    "php", "pip", "sh", "svn", "unzip", "vagrant", "vim", "yarn",
];

/// The programs a table holds as ordinary English words (`make`, `find`), sorted. One is a `Command`
/// only in the place of one: after a word of [`RUN`] or a `$`, or right before a flag.
const SHARED: [&str; 25] = [
    "alias", "brew", "cat", "clear", "cut", "dig", "echo", "find", "head", "kill", "locate",
    "make", "mount", "paste", "ping", "rev", "sleep", "sort", "strings", "sync", "tail", "tar",
    "touch", "tree", "zip",
];

/// The words that put a [`SHARED`] name in the place of a command: *run make*.
const RUN: [&str; 6] = ["execute", "invoke", "ran", "run", "running", "runs"];

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
            origin_of(&work, at, false)
        } else {
            Origin::English
        };
    }
    work.iter().map(|token| token.origin).collect()
}

/// Sets the origin of the token at `at` of a sentence, whose word the tables have read, and reads a
/// word they lack by its origin (see the module docs). `name` says the word may be a program's name (see `table::read_shaped`). `after_git` holds whether the word before
/// this one is `git`, and is set for the next. `plain` says the tables' keys found the word
/// to be ASCII letters alone, lower case or with a capital first; false is no claim. The origins before `at` are set already.
/// The origin does not depend on what the tables hold. Returns whether it is `Command`.
///
/// Debug builds run this on every word, so it keeps to plain loops and tests with no closure.
#[inline(always)]
pub(super) fn mark(
    tokens: &mut [Token<'_>],
    at: usize,
    plain: bool,
    name: bool,
    after_git: &mut bool,
) -> bool {
    let bytes = tokens[at].text.as_bytes();
    let git_before = *after_git;
    *after_git = name && bytes.len() == 3 && bytes.eq_ignore_ascii_case(b"git");
    // A plain word is English unless a dash or a colon is beside it, `git` is before it or its name
    // is on a list. Only a punctuation token is looked into.
    let mut near = git_before | (name && (bytes.len() <= table::SHORT || in_filter(bytes)));
    if at > 0 && tokens[at - 1].kind == TokenKind::Punctuation {
        near |= matches!(first_byte(&tokens[at - 1]), b'-' | b':');
    }
    if at + 1 < tokens.len() && tokens[at + 1].kind == TokenKind::Punctuation {
        near |= first_byte(&tokens[at + 1]) == b':';
    }
    let origin = if plain && !near {
        Origin::English
    } else {
        origin_of(tokens, at, plain)
    };
    let token = &mut tokens[at];
    token.origin = origin;
    if origin == Origin::English {
        return false;
    }
    if let Some(reading) = token.reading
        && reading.confidence == Confidence::Unknown
    {
        token.reading = Some(read(origin, &token.text, reading));
    }
    origin == Origin::Command
}

/// The origin of the word at `at`, given the origins `tokens` already hold for the words before it.
///
/// It runs on every word of every document, so a plain lower-case word, which most are, is read
/// once as bytes and goes no further than the neighbour and name checks, and it looks at a
/// neighbour's kind, which lives in the token, before its text, which does not.
#[inline(never)]
fn origin_of(tokens: &[Token<'_>], at: usize, plain: bool) -> Origin {
    let text: &str = &tokens[at].text;
    let bytes = text.as_bytes();
    if !(plain || is_plain(bytes))
        && let Some(origin) = by_marks(text)
    {
        // `:arrows_clockwise:` is an emoji, no name from code.
        return if origin == Origin::Symbol && is_shortcode(tokens, at) {
            Origin::English
        } else {
            origin
        };
    }
    // A dash or a colon beside the word is where a flag or a `::` starts, and a word has none.
    let before = if at > 0 {
        first_byte(&tokens[at - 1])
    } else {
        0
    };
    let after = if at + 1 < tokens.len() {
        first_byte(&tokens[at + 1])
    } else {
        0
    };
    if matches!(before, b'-' | b':') || after == b':' {
        if joins_double_colon(tokens, at) {
            return Origin::Symbol;
        }
        if before == b'-' && is_flag(tokens, at) {
            return Origin::Flag;
        }
    }
    if at > 0 && is_git(&tokens[at - 1]) && GIT.binary_search(&text).is_ok() {
        return Origin::Command;
    }
    if in_filter(bytes) && is_command(tokens, at) {
        Origin::Command
    } else {
        Origin::English
    }
}

/// The first byte of the token's text, or 0 if it has none.
#[inline(always)]
fn first_byte(token: &Token<'_>) -> u8 {
    match token.text.as_bytes().first() {
        Some(byte) => *byte,
        None => 0,
    }
}

/// Whether the token is the word `git`.
fn is_git(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Word && token.text.len() == 3 && token.text.eq_ignore_ascii_case("git")
}

/// Whether the word is ASCII letters alone, all lower case or with a capital first, which most
/// words are. No such word has a mark that makes it a symbol, a path or an English contraction.
///
/// The letters after the first are tested eight at a time as the tables' keys are, with no branch
/// on the length, which a text makes unpredictable; a word of over nine bytes is rare.
fn is_plain(bytes: &[u8]) -> bool {
    let len = bytes.len();
    if len == 0 || (bytes[0] | 0x20).wrapping_sub(b'a') >= 26 {
        return false;
    }
    let mut at = 1;
    while at < len {
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
    if marks.dot {
        if is_path(text, &EXTENSIONS) || is_path(text, &DOCUMENTS) {
            Some(Origin::Path)
        } else if is_dotted_identifier(text) {
            Some(Origin::Symbol)
        } else {
            None
        }
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
    /// An ASCII capital after at least two ASCII lower-case letters.
    camel: bool,
}

impl Marks {
    fn of(text: &str) -> Marks {
        let mut marks = Marks::default();
        let mut lower = 0;
        for byte in text.bytes() {
            match byte {
                b'a'..=b'z' => {
                    lower += 1;
                    continue;
                }
                b'A'..=b'Z' => marks.camel |= lower >= 2,
                b'\'' => marks.apostrophe = true,
                b'.' => marks.dot = true,
                b'_' => marks.underscore = true,
                0x80.. => marks.non_ascii = true,
                _ => {}
            }
            marks.other = true;
            lower = 0;
        }
        marks
    }

    /// Whether `text`, whose marks these are and which has no dot, is written as a name from code:
    /// an underscore among its letters (`0001_initial` too), or camel or Pascal case. A word of
    /// lower-case letters alone has no such cue, and a camel shape that starts with a digit
    /// (`1Password`) is left to `shape.rs`.
    fn is_symbol(&self, text: &str) -> bool {
        if !self.other {
            return false;
        }
        if self.non_ascii {
            return (text.contains('_') && text.chars().any(char::is_alphabetic))
                || (!text.starts_with(|c: char| c.is_ascii_digit()) && has_camel(text));
        }
        (self.underscore && text.bytes().any(|byte| byte.is_ascii_alphabetic()))
            || (self.camel && !text.starts_with(|c: char| c.is_ascii_digit()))
    }
}

/// Whether a capital stands after two or more lower-case letters: `userId`, `PowerShell`,
/// `macOS`. One letter before it is no cue: `PhD`, `eBook`, `mRNA`, `iPhone`, `kHz` are English.
fn has_camel(text: &str) -> bool {
    let mut lower = 0;
    for ch in text.chars() {
        if ch.is_uppercase() && lower >= 2 {
            return true;
        }
        lower = if ch.is_lowercase() { lower + 1 } else { 0 };
    }
    false
}

/// Whether `text`, which has a dot, is a dotted identifier (`os.path`, `foo.bar`,
/// `user.first_name`): two or more parts of two or more letters, digits or underscores, each with
/// a letter, the first starting with one; the last is no site's ending (`example.com`), and none
/// after the first is a capitalised word (`works.Then`, `St.Louis`), so a missing space after a
/// full stop is no identifier.
fn is_dotted_identifier(text: &str) -> bool {
    let first = text.as_bytes()[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    let mut parts = 0;
    let mut last = "";
    for part in text.split('.') {
        let bytes = part.as_bytes();
        let capitalised = bytes.first().is_some_and(u8::is_ascii_uppercase)
            && bytes[1..].iter().all(u8::is_ascii_lowercase);
        if bytes.len() < 2
            || !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
            || !bytes.iter().any(u8::is_ascii_alphabetic)
            || (parts > 0 && capitalised)
        {
            return false;
        }
        parts += 1;
        last = part;
    }
    parts >= 2
        && !text.starts_with("www.")
        && SITES
            .binary_search(&last.to_ascii_lowercase().as_str())
            .is_err()
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

/// Whether `text` is a file name after a stem, whose extension is in `list`.
fn is_path(text: &str, list: &[&str]) -> bool {
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
        .is_ok_and(|extension| list.binary_search(&extension).is_ok())
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

/// Whether [`FILTER`] holds the slot of this word, which any name of the lists must.
#[inline(always)]
fn in_filter(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    let slot = slot(bytes[0], bytes[1], bytes[bytes.len() - 1], bytes.len());
    FILTER[slot / 64] >> (slot % 64) & 1 != 0
}

/// Whether the word at `at` names a program: a name in [`PROGRAMS`] in lower case, or capitalised
/// when no word comes before it in the sentence, a name in [`HELD`] in lower case, or one in
/// [`SHARED`] in the place of a command. A git subcommand is [`origin_of`]'s to find.
fn is_command(tokens: &[Token<'_>], at: usize) -> bool {
    let text: &str = &tokens[at].text;
    if !NAME.contains(&text.len()) || !text.is_ascii() {
        return false;
    }
    let bytes = text.as_bytes();
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return PROGRAMS.binary_search(&text).is_ok()
            || HELD.binary_search(&text).is_ok()
            || (SHARED.binary_search(&text).is_ok() && is_in_command_place(tokens, at));
    }
    let first = !tokens[..at]
        .iter()
        .any(|token| token.kind == TokenKind::Word);
    first
        && bytes[0].is_ascii_uppercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && PROGRAMS
            .binary_search(&text.to_ascii_lowercase().as_str())
            .is_ok()
}

/// Whether the word at `at` stands where a command does: after a word of [`RUN`] or a `$`, or
/// before a flag that a space parts from it (`make -j`).
fn is_in_command_place(tokens: &[Token<'_>], at: usize) -> bool {
    if at > 0 {
        let before = &tokens[at - 1];
        if (before.kind == TokenKind::Word && RUN.contains(&before.folded().as_str()))
            || (before.kind == TokenKind::Symbol && before.text == "$")
        {
            return true;
        }
    }
    at + 2 < tokens.len()
        && is_dash(&tokens[at + 1])
        && tokens[at].range.end < tokens[at + 1].range.start
        && tokens[at + 1].range.end == tokens[at + 2].range.start
        && tokens[at + 2].kind == TokenKind::Word
}

/// Whether `text` names a program that is also an English verb.
fn is_verb(text: &str) -> bool {
    VERBS
        .binary_search(&text.to_ascii_lowercase().as_str())
        .is_ok()
}

/// Whether the tagger commits to a word of this origin as a name, which is narrower than the
/// origin. A dotted identifier is no name the guide's rule covers, and a document or an image is
/// tagged as the noun it is; see the module docs.
fn commits(origin: Origin, text: &str) -> bool {
    match origin {
        Origin::Symbol => !text.contains('.'),
        Origin::Path => is_path(text, &EXTENSIONS),
        _ => false,
    }
}

/// The reading of a word that no table has and whose origin is `origin`, given its reading by
/// shape, which stands for `English`. See the module docs.
pub(super) fn read(origin: Origin, text: &str, by_shape: Reading) -> Reading {
    let names = TagSet::of(Tag::Noun).with(Tag::ProperNoun);
    let (confidence, kept) = match origin {
        Origin::English => return by_shape,
        Origin::Symbol | Origin::Path if !commits(origin, text) => return by_shape,
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
            ("DOCUMENTS", &DOCUMENTS[..]),
            ("SITES", &SITES[..]),
            ("PROGRAMS", &PROGRAMS[..]),
            ("HELD", &HELD[..]),
            ("SHARED", &SHARED[..]),
            ("RUN", &RUN[..]),
            ("VERBS", &VERBS[..]),
            ("GIT", &GIT[..]),
        ] {
            assert!(
                list.windows(2).all(|pair| pair[0] < pair[1]),
                "{name} is not sorted and unique"
            );
            assert!(
                list.iter()
                    .all(|word| word.is_ascii() && **word == word.to_ascii_lowercase()),
                "{name} holds a word that is not lower case ASCII"
            );
        }
        for name in PROGRAMS.iter().chain(&HELD).chain(&SHARED) {
            assert!(NAME.contains(&name.len()), "{name} is outside NAME");
        }
        for name in HELD.iter().chain(&SHARED) {
            assert!(
                PROGRAMS.binary_search(name).is_err(),
                "{name} is in two lists"
            );
        }
        for name in SHARED {
            assert!(HELD.binary_search(&name).is_err(), "{name} is in two lists");
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
                .chain(&DOCUMENTS)
                .all(|e| e.len() <= EXTENSION && e.len() > 1)
        );
        assert!(
            DOCUMENTS
                .iter()
                .all(|e| EXTENSIONS.binary_search(e).is_err()),
            "a document extension is also a source one"
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
    fn the_programs_the_tables_hold_are_listed_apart() {
        let lacking: Vec<&str> = HELD
            .iter()
            .chain(&SHARED)
            .copied()
            .filter(|name| !in_a_table(name))
            .collect();
        assert!(
            lacking.is_empty(),
            "the tables lack these, so they belong in PROGRAMS: {lacking:?}"
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
        for (words, at, word) in [
            ("Run make now.", 1, "make"),
            ("Run cargo build.", 1, "cargo"),
        ] {
            let mut tokens = Token::split(words);
            sentence(&mut tokens, Context::Prose);
            assert_eq!(tokens[at].origin, Origin::Command, "{words}");
            let by_table = crate::tag::read(word);
            let reading = tokens[at].reading.unwrap();
            assert_ne!(reading.confidence, Confidence::Unknown, "{words}");
            assert_eq!(
                reading.possible().intersection(by_table.possible()),
                reading.possible(),
                "{words}"
            );
        }
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
    fn two_lower_case_letters_before_a_capital_make_camel_case() {
        for text in [
            "userId",
            "macOS",
            "GitHub",
            "fooBar",
            "xmlHTTPRequest",
            "naïveFoo",
        ] {
            assert_eq!(
                only(&format!("Use {text} here.")),
                marked(text, Origin::Symbol),
                "{text}"
            );
        }
        for text in [
            "Ph", "PhD", "eBook", "mRNA", "iOS", "xDS", "pH", "AbC", "éD",
        ] {
            assert!(only(&format!("Use {text} here.")).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_dotted_identifier_is_a_symbol_the_tagger_does_not_commit_to() {
        for text in [
            "os.path",
            "foo.bar",
            "user.first_name",
            "this.setState",
            "react.useState",
        ] {
            assert_eq!(
                only(&format!("Call {text} now.")),
                marked(text, Origin::Symbol),
                "{text}"
            );
            let words = format!("Call {text} now.");
            let mut tokens = Token::split(&words);
            sentence(&mut tokens, Context::Prose);
            let reading = tokens[1].reading.unwrap();
            let by_shape = crate::tag::read(text);
            assert_eq!(reading, by_shape, "the tag is the shape's: {text}");
        }
        for text in [
            "example.com",
            "node.io",
            "www.foo.bar",
            "works.Then",
            "St.Louis",
            "e.g",
            "U.S",
            "p.m",
            "v1.2",
            "3.14",
            "1.2.3",
            "x.y",
        ] {
            assert!(only(&format!("See {text} now.")).is_empty(), "{text}");
        }
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
            "PhD",
            "eBook",
            "mRNA",
            "iPhone",
            "pH",
            "kHz",
            "KiB",
            "mAh",
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
            "report.pdf",
            "Lisa_resume.doc",
            "UnleadedStocks.pdf",
            "logo.PNG",
            "photo.jpeg",
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
            "www.adobe.pdf",
        ] {
            assert!(only(&format!("See {text} now.")).is_empty(), "{text}");
        }
    }

    #[test]
    fn the_tagger_commits_to_a_source_file_but_keeps_the_shape_of_a_document() {
        for text in ["main.rs", "AGENTS.md", "foo.test.js"] {
            let words = format!("Edit {text} now.");
            let mut tokens = Token::split(&words);
            sentence(&mut tokens, Context::Prose);
            let reading = tokens[1].reading.unwrap();
            assert_eq!(
                (reading.tag, reading.confidence),
                (Tag::ProperNoun, Confidence::Likely),
                "{text}"
            );
        }
        for text in ["report.pdf", "lisa_resume.doc", "logo.png", "photo.jpeg"] {
            let words = format!("See {text} now.");
            let mut tokens = Token::split(&words);
            sentence(&mut tokens, Context::Prose);
            assert_eq!(tokens[1].origin, Origin::Path, "{text}");
            assert_eq!(tokens[1].reading.unwrap(), crate::tag::read(text), "{text}");
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
    fn a_program_a_table_holds_is_a_command() {
        for (text, word) in [
            ("Run cargo build now.", "cargo"),
            ("Use git now.", "git"),
            ("Run ls now.", "ls"),
            ("Run make now.", "make"),
            ("Then run make -j4.", "make"),
            ("Pipe it to sort -u.", "sort"),
            ("Type $ cat now.", "cat"),
        ] {
            let found: Vec<_> = only(text).into_iter().filter(|(w, _)| w == word).collect();
            assert_eq!(found, marked(word, Origin::Command), "{text}");
        }
        for text in [
            "Git is a tool.",
            "Cargo ships today.",
            "We make it.",
            "Find the file.",
            "They sort of agree.",
            "A cat sat.",
            "A pre-make step.",
            "Make-believe worlds.",
        ] {
            assert!(only(text).is_empty(), "{text}: {:?}", only(text));
        }
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
    fn origins_and_the_tagger_agree_over_the_core_corpus() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/core");
        let (mut files, mut sentences, mut others) = (0, 0, 0);
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let document = crate::document::Document::markdown(&source);
            files += 1;
            for (block, _) in document.walk() {
                for found in document.sentences_of(block) {
                    let tokens = &document.tokens_in(found.range.clone());
                    // Origins from the kind, text and place alone, on tokens the tagger has set.
                    let found = origins(tokens);
                    for (token, origin) in tokens.iter().zip(found) {
                        assert_eq!(token.origin, origin, "{}: {}", path.display(), token.text);
                        sentences += 1;
                        others += usize::from(origin != Origin::English);
                    }
                }
            }
        }
        assert!(
            files > 20 && sentences > 1000 && others > 20,
            "{files} {sentences} {others}"
        );
    }

    #[test]
    fn the_name_filter_holds_every_short_name_in_any_case() {
        for name in PROGRAMS.iter().chain(&HELD).chain(&SHARED) {
            if name.len() > table::SHORT {
                continue;
            }
            assert_eq!(
                table::short_key(name.as_bytes()),
                table::short_key_const(name.as_bytes()),
                "{name}"
            );
            for text in [name.to_string(), name.to_uppercase(), {
                let mut capital = name.to_string();
                capital[..1].make_ascii_uppercase();
                capital
            }] {
                assert!(table::read_shaped(&text).2, "{text}");
            }
        }
    }

    #[test]
    fn a_word_the_keys_call_plain_has_no_mark_of_a_name() {
        // Every word of up to nine bytes over a few letters of both cases and some marks.
        let alphabet = ['a', 'b', 'Z', 'Q', '\'', '_', '1'];
        let (mut plain, mut checked) = (0, 0);
        let mut words = vec![String::new()];
        for _ in 0..9 {
            let mut longer = Vec::new();
            for word in &words {
                for letter in alphabet {
                    let mut next = word.clone();
                    next.push(letter);
                    let (_, said, _) = crate::tag::table::read_shaped(&next);
                    checked += 1;
                    if said {
                        plain += 1;
                        assert_eq!(by_marks(&next), None, "{next}");
                    }
                    longer.push(next);
                }
            }
            words = if longer.len() > 300_000 {
                longer.into_iter().step_by(7).collect()
            } else {
                longer
            };
        }
        assert!(checked > 100_000 && plain > 1000, "{checked} {plain}");
        for word in [
            "API", "APIs", "PhD", "iOS", "NASA", "README", "Hello", "the", "The", "A", "I",
        ] {
            assert!(crate::tag::table::read_shaped(word).1, "{word}");
        }
        for word in [
            "GitHub", "userId", "userIds", "macOS", "don't", "foo_bar", "abcdEfgh",
        ] {
            assert!(!crate::tag::table::read_shaped(word).1, "{word}");
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
