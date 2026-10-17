//! Reading the commands of a shell block or a code span. It is not a shell: it finds the commands
//! and their words and leaves out whatever it cannot read, which is never a claim.
//!
//! The text is one stream, so a quote spans lines and a trailing `\` joins them. A comment is
//! dropped, and a command ends at a newline, `&&`, `||`, `;`, `|` or `&`. A redirection and its
//! file are dropped, as is the body of a heredoc, whose opener stays. A keyword that starts a
//! command, such as `then` or `{` ([`OPENERS`]), is a command of its own, so the command after it,
//! or a `(`, starts there. A function header (`f()`, `f ()`, `function f`) does the same, and so
//! does `time` or `coproc` for a `(` or a keyword. The head of a `case` and the label of an arm, up
//! to its `)`, are left out, and the commands of the arm start after the `)`. A `(` that opens a
//! command and the `)` that closes it are items of their own, since a subshell keeps its `cd` to
//! itself; the `)` of an arm's label is not one. A block with a `$ ` or `% ` prompt on any line
//! holds commands only on the prompted lines and what they continue; the rest is output.

use std::ops::Range;

/// A word of a command, quotes taken off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Word {
    /// What the word reads as. A command substitution or `$X` stays as written.
    pub text: String,
    /// Its bytes in the text read, quotes included.
    pub range: Range<usize>,
}

/// The keywords that start a command without being one: a command begins after each. Quoted, or
/// after another word, a keyword is an argument.
const OPENERS: [&str; 9] = [
    "if", "elif", "while", "until", "then", "else", "do", "{", "!",
];

/// What the stream holds, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    /// A simple command: its words, redirections left out.
    Command(Vec<Word>),
    /// A `(` that opens a subshell.
    Open,
    /// A `)` that closes one.
    Close,
}

/// A stream, read.
#[derive(Debug, Default)]
pub(super) struct Read {
    /// The commands and subshell edges, in order.
    pub items: Vec<Item>,
    /// Where the quote or substitution was opened that never closes. Nothing after it was read, and
    /// the command it is in is not an item.
    pub unclosed: Option<usize>,
}

/// Reads the commands in `text`.
pub(super) fn read(text: &str) -> Read {
    let mut scanner = Scanner {
        text,
        at: 0,
        prompted: text.lines().any(|line| prompt(line).is_some()),
        items: Vec::new(),
        words: Vec::new(),
        word: None,
        redirect: None,
        heredocs: Vec::new(),
        depth: 0,
        cases: Vec::new(),
        unclosed: None,
    };
    scanner.run();
    Read {
        items: scanner.items,
        unclosed: scanner.unclosed,
    }
}

/// How many bytes of `line` a `$ ` or `% ` prompt takes, indent included.
fn prompt(line: &str) -> Option<usize> {
    let rest = line.trim_start();
    (rest.starts_with("$ ") || rest.starts_with("% ")).then(|| line.len() - rest.len() + 2)
}

/// What the next word is, after a redirection operator.
#[derive(Debug, Clone, Copy)]
enum Redirect {
    /// A file, which is left out.
    File,
    /// The delimiter of a heredoc.
    Heredoc { strip_tabs: bool },
}

/// A heredoc whose body starts on the line after the command.
#[derive(Debug)]
struct Heredoc {
    delimiter: String,
    strip_tabs: bool,
}

/// Where a `case` is, from `case` to `esac`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    /// After `case`, before `in`.
    Head,
    /// In the label of an arm, which ends at an unquoted `)`.
    Label,
    /// In the commands of an arm, which end at `;;`, `;&` or `esac`.
    Body,
}

struct Scanner<'t> {
    text: &'t str,
    at: usize,
    prompted: bool,
    items: Vec<Item>,
    /// The words of the command being read.
    words: Vec<Word>,
    /// The word being read.
    word: Option<Word>,
    redirect: Option<Redirect>,
    heredocs: Vec<Heredoc>,
    /// How many subshells are open.
    depth: usize,
    /// The `case`s open, the innermost last.
    cases: Vec<Case>,
    unclosed: Option<usize>,
}

impl<'t> Scanner<'t> {
    fn run(&mut self) {
        while self.at < self.text.len() && self.unclosed.is_none() {
            if !self.heredocs.is_empty() {
                self.skip_heredocs();
                continue;
            }
            if self.prompted {
                let rest = &self.text[self.at..];
                let line = rest.split('\n').next().unwrap_or_default();
                match prompt(line) {
                    Some(skip) => self.at += skip,
                    None => {
                        self.at += (line.len() + 1).min(rest.len());
                        continue;
                    }
                }
            }
            self.line();
        }
        if self.unclosed.is_some() {
            self.words.clear();
            self.word = None;
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    /// The text `len` bytes after `at`.
    fn after(&self, len: usize) -> &'t str {
        let text = self.text;
        &text[self.at + len..]
    }

    /// The word being read, started if it was not.
    fn word(&mut self) -> &mut Word {
        let at = self.at;
        self.word.get_or_insert_with(|| Word {
            text: String::new(),
            range: at..at,
        })
    }

    fn end_word(&mut self) {
        let Some(mut word) = self.word.take() else {
            return;
        };
        word.range.end = self.at;
        match self.redirect.take() {
            Some(Redirect::File) => {}
            Some(Redirect::Heredoc { strip_tabs }) => self.heredocs.push(Heredoc {
                delimiter: word.text,
                strip_tabs,
            }),
            None if self.is_opener(&word) && self.starts_command() => {
                self.end_command();
                self.items.push(Item::Command(vec![word]));
            }
            None => self.push_word(word),
        }
    }

    /// Adds `word` to the command, or moves the `case` it is a keyword of: `case` opens one, `in`
    /// ends its head, and `esac` closes it.
    fn push_word(&mut self, word: Word) {
        if self.written(&word) {
            let first = self.words.is_empty();
            let top = self.cases.last().copied();
            match (word.text.as_str(), top) {
                ("case", _) if first && top != Some(Case::Label) => self.cases.push(Case::Head),
                ("in", Some(Case::Head)) => {
                    self.words.clear();
                    self.set_case(Case::Label);
                    return;
                }
                ("esac", Some(_)) if first => {
                    self.cases.pop();
                }
                _ => {}
            }
        }
        self.words.push(word);
    }

    /// Whether `word` is as written, not quoted or escaped.
    fn written(&self, word: &Word) -> bool {
        self.text.get(word.range.clone()) == Some(word.text.as_str())
    }

    /// Whether `word` is a keyword as written.
    fn is_opener(&self, word: &Word) -> bool {
        OPENERS.contains(&word.text.as_str()) && self.written(word)
    }

    /// Whether the words so far leave a command to start: there are none, or they are `time` or
    /// `coproc` and the flags of `time`, or a function header (`f()`, `f ()`, `function f`,
    /// `function f ()`). A `(` or a keyword that comes now opens a command of its own.
    fn starts_command(&self) -> bool {
        let is = |word: &Word, text: &str| word.text == text && self.written(word);
        match self.words.as_slice() {
            [] => true,
            [first, flags @ ..] if is(first, "time") || is(first, "coproc") => {
                flags.iter().all(|flag| flag.text.starts_with('-'))
            }
            [name] => name.text.len() > 2 && name.text.ends_with("()") && self.written(name),
            [function, _] if is(function, "function") => true,
            [function, _, parens] if is(function, "function") => is(parens, "()"),
            [_, parens] => is(parens, "()"),
            _ => false,
        }
    }

    /// Whether the word being read, if `()` follows, is a function header: it is the first word, or
    /// the one after `function`.
    fn is_header_name(&self) -> bool {
        let named = self.word.as_ref().is_some_and(|word| !word.text.is_empty());
        named
            && match self.words.as_slice() {
                [] => true,
                [function] => function.text == "function" && self.written(function),
                _ => false,
            }
    }

    /// Whether the word being read is the `esac` that starts a command, which ends the `case`.
    fn ends_case(&self) -> bool {
        self.words.is_empty()
            && self.word.as_ref().is_some_and(|word| {
                word.text == "esac" && &self.text[word.range.start..self.at] == "esac"
            })
    }

    /// Whether the innermost `case` is reading the label of an arm.
    fn in_label(&self) -> bool {
        self.cases.last() == Some(&Case::Label)
    }

    fn set_case(&mut self, case: Case) {
        if let Some(top) = self.cases.last_mut() {
            *top = case;
        }
    }

    /// Ends a command. In the label of an arm it is a pattern, not a command, and is dropped.
    fn end_command(&mut self) {
        self.end_word();
        if self.in_label() {
            self.words.clear();
        } else if !self.words.is_empty() {
            self.items
                .push(Item::Command(std::mem::take(&mut self.words)));
        }
    }

    /// Reads to the end of the line that starts at `at`, or past it where a quote or a
    /// continuation carries the command on.
    fn line(&mut self) {
        while let Some(ch) = self.peek() {
            match ch {
                '\n' => {
                    self.end_command();
                    self.at += 1;
                    return;
                }
                ' ' | '\t' | '\r' => {
                    self.end_word();
                    self.at += 1;
                }
                '\\' => self.escape(),
                '#' if self.word.is_none() => {
                    let rest = &self.text[self.at..];
                    self.at += rest.find('\n').unwrap_or(rest.len());
                }
                '\'' => self.single_quoted(),
                '"' => self.double_quoted(),
                '`' => self.backtick(),
                '$' => self.dollar(),
                '&' if self.after(1).starts_with('>') => self.redirection(),
                ';' | '|' | '&' => self.operator(ch),
                '<' | '>' => self.redirection(),
                // The `(` before the patterns of an arm.
                '(' if self.in_label() && self.word.is_none() && self.words.is_empty() => {
                    self.at += 1;
                }
                // The `()` of `f(){` and `f()(`: the name ends there.
                '(' if self.after(1).starts_with(')') && self.is_header_name() => {
                    self.word().text.push_str("()");
                    self.at += 2;
                    self.end_word();
                }
                '(' if self.word.is_none()
                    && !self.after(1).starts_with(')')
                    && self.starts_command() =>
                {
                    self.end_command();
                    self.at += 1;
                    self.items.push(Item::Open);
                    self.depth += 1;
                }
                // The `)` that ends the label of an arm does not close a subshell. `esac)` is not a label.
                ')' if self.in_label() && !self.ends_case() => {
                    self.word = None;
                    self.words.clear();
                    self.at += 1;
                    self.set_case(Case::Body);
                }
                ')' if self.depth > 0 => {
                    self.end_command();
                    self.at += 1;
                    self.items.push(Item::Close);
                    self.depth -= 1;
                }
                _ => {
                    self.word().text.push(ch);
                    self.at += ch.len_utf8();
                }
            }
            if self.unclosed.is_some() {
                return;
            }
        }
        self.end_command();
    }

    /// A backslash: it quotes the next character, and before a newline it joins the lines.
    fn escape(&mut self) {
        let rest = self.after(1);
        match rest.chars().next() {
            None => self.at += 1,
            Some('\n') => self.at += 2,
            Some('\r') if rest[1..].starts_with('\n') => self.at += 3,
            Some(next) => {
                self.word().text.push(next);
                self.at += 1 + next.len_utf8();
            }
        }
    }

    fn single_quoted(&mut self) {
        let start = self.at;
        let Some(len) = self.after(1).find('\'') else {
            self.unclosed = Some(start);
            return;
        };
        let inner = self.text[start + 1..start + 1 + len].to_string();
        self.word().text.push_str(&inner);
        self.at = start + len + 2;
    }

    fn double_quoted(&mut self) {
        let start = self.at;
        let mut inner = String::new();
        let mut chars = self.after(1).char_indices();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '"' => {
                    self.word().text.push_str(&inner);
                    self.at = start + 1 + i + 1;
                    return;
                }
                '\\' => match chars.next() {
                    Some((_, '\n')) => {}
                    Some((_, next @ ('"' | '\\' | '$' | '`'))) => inner.push(next),
                    Some((_, other)) => {
                        inner.push('\\');
                        inner.push(other);
                    }
                    None => break,
                },
                _ => inner.push(ch),
            }
        }
        self.unclosed = Some(start);
    }

    /// A command substitution in backticks stays in its word as written.
    fn backtick(&mut self) {
        let start = self.at;
        let mut chars = self.after(1).char_indices();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '\\' => {
                    chars.next();
                }
                '`' => {
                    let end = start + 1 + i + 1;
                    let raw = self.text[start..end].to_string();
                    self.word().text.push_str(&raw);
                    self.at = end;
                    return;
                }
                _ => {}
            }
        }
        self.unclosed = Some(start);
    }

    /// `$(...)` and `${...}` stay in their word as written, spaces and all.
    fn dollar(&mut self) {
        let start = self.at;
        let (open, close) = match self.after(1).chars().next() {
            Some('(') => ('(', ')'),
            Some('{') => ('{', '}'),
            _ => {
                self.word().text.push('$');
                self.at += 1;
                return;
            }
        };
        let mut depth = 0usize;
        for (i, ch) in self.after(1).char_indices() {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    let end = start + 1 + i + 1;
                    let raw = self.text[start..end].to_string();
                    self.word().text.push_str(&raw);
                    self.at = end;
                    return;
                }
            }
        }
        self.unclosed = Some(start);
    }

    /// `;`, `|`, `&` and the pairs they make end the command. `;;` and `;&` end the arm of a
    /// `case`.
    fn operator(&mut self, ch: char) {
        self.end_command();
        self.at += 1;
        let next = self.peek();
        let arm = ch == ';' && matches!(next, Some(';' | '&'));
        if arm || next == Some(ch) || (ch == '|' && next == Some('&')) {
            self.at += 1;
        }
        if arm && self.cases.last() == Some(&Case::Body) {
            self.set_case(Case::Label);
        }
    }

    /// `>`, `>>`, `<`, `<<`, `<<-`, `<<<`, `&>`, `2>&1` and the like.
    fn redirection(&mut self) {
        // A word of digits just before it is the descriptor.
        match &self.word {
            Some(word)
                if !word.text.is_empty() && word.text.bytes().all(|b| b.is_ascii_digit()) =>
            {
                self.word = None;
            }
            _ => self.end_word(),
        }
        let start = self.at;
        if self.peek() == Some('&') {
            self.at += 1;
        }
        while matches!(self.peek(), Some('<' | '>')) {
            self.at += 1;
        }
        let operator = &self.text[start..self.at];
        let heredoc = operator == "<<";
        let strip_tabs = heredoc && self.peek() == Some('-');
        if strip_tabs {
            self.at += 1;
        }
        // `>&2` and `>&-` name a descriptor, not a file.
        let mut duplicated = false;
        if !heredoc && self.peek() == Some('&') {
            self.at += 1;
            let digits = self
                .after(0)
                .bytes()
                .take_while(|b| b.is_ascii_digit() || *b == b'-')
                .count();
            self.at += digits;
            duplicated = digits > 0;
        }
        if self.peek() == Some('|') {
            self.at += 1;
        }
        self.redirect = match (heredoc, duplicated) {
            (true, _) => Some(Redirect::Heredoc { strip_tabs }),
            (false, true) => None,
            (false, false) => Some(Redirect::File),
        };
    }

    /// Skips the bodies of the heredocs the line just read opened, each to its delimiter.
    fn skip_heredocs(&mut self) {
        for heredoc in std::mem::take(&mut self.heredocs) {
            while self.at < self.text.len() {
                let rest = &self.text[self.at..];
                let line = rest.split('\n').next().unwrap_or_default();
                self.at += (line.len() + 1).min(rest.len());
                let line = line.trim_end_matches('\r');
                let line = if heredoc.strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line
                };
                if line == heredoc.delimiter {
                    break;
                }
            }
        }
    }
}

/// How `make` was called: the targets it names, and the directories it was sent to.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Call {
    /// The words that are not flags or `VAR=value`.
    pub targets: Vec<Word>,
    /// The `-C` directories, in order.
    pub dirs: Vec<String>,
}

/// Why a call is not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    /// It names the makefile (`-f`, `--file`, `--makefile`) or adds rules (`-E`, `--eval`).
    Makefile,
    /// It has a `-C` directory the lint cannot read.
    Dir,
}

/// Reads the words of a call of `make` or `gmake`, the first of which is the command.
pub(super) fn call(words: &[Word]) -> Result<Call, Refusal> {
    let mut call = Call::default();
    let mut rest = words.iter().skip(1).peekable();
    let mut options = true;
    while let Some(word) = rest.next() {
        let text = word.text.as_str();
        if options && text == "--" {
            options = false;
        } else if let Some(long) = text.strip_prefix("--").filter(|_| options) {
            let (name, given) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            match name {
                "file" | "makefile" | "eval" => return Err(Refusal::Makefile),
                "directory" => {
                    let dir = given.or_else(|| rest.next().map(|word| word.text.as_str()));
                    call.dirs.push(directory(dir)?);
                }
                "include-dir" | "old-file" | "assume-old" | "new-file" | "assume-new"
                | "what-if"
                    if given.is_none() =>
                {
                    rest.next();
                }
                "jobs" | "load-average" | "max-load"
                    if given.is_none()
                        && rest.peek().is_some_and(|next| starts_with_digit(next)) =>
                {
                    rest.next();
                }
                _ => {}
            }
        } else if options && text.starts_with('-') {
            let cluster = &text[1..];
            for (i, ch) in cluster.char_indices() {
                let attached = &cluster[i + ch.len_utf8()..];
                match ch {
                    'f' | 'E' => return Err(Refusal::Makefile),
                    'C' => {
                        let dir = (!attached.is_empty())
                            .then_some(attached)
                            .or_else(|| rest.next().map(|word| word.text.as_str()));
                        call.dirs.push(directory(dir)?);
                    }
                    'I' | 'o' | 'W' => {
                        if attached.is_empty() {
                            rest.next();
                        }
                    }
                    'j' | 'l' => {
                        if attached.is_empty() && rest.peek().is_some_and(|n| starts_with_digit(n))
                        {
                            rest.next();
                        }
                    }
                    // `-O` takes its argument attached, and so takes the rest of the cluster.
                    'O' => {}
                    _ => continue,
                }
                break;
            }
        } else if !text.is_empty() && !text.contains('=') {
            call.targets.push(word.clone());
        }
    }
    Ok(call)
}

fn starts_with_digit(word: &Word) -> bool {
    word.text.starts_with(|ch: char| ch.is_ascii_digit())
}

/// A `-C` directory, if the lint can read it.
fn directory(value: Option<&str>) -> Result<String, Refusal> {
    match value {
        Some(dir) if !dir.is_empty() && !unreadable(dir) => Ok(dir.to_string()),
        _ => Err(Refusal::Dir),
    }
}

/// Whether `word` holds what only a shell or a reader can fill in: a variable, a command
/// substitution, a glob, a placeholder such as `<name>`, `[name]` or `{name}`, or an ellipsis,
/// `...` or `…`.
pub(super) fn unreadable(word: &str) -> bool {
    word.contains(['$', '`', '<', '>', '[', ']', '{', '}', '*', '?', '…']) || word.contains("...")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words of each command of `text`, with `(` and `)` as themselves.
    fn commands(text: &str) -> Vec<Vec<String>> {
        let read = read(text);
        assert_eq!(read.unclosed, None, "{text:?}");
        read.items
            .into_iter()
            .map(|item| match item {
                Item::Command(words) => words.into_iter().map(|word| word.text).collect(),
                Item::Open => vec!["(".to_string()],
                Item::Close => vec![")".to_string()],
            })
            .collect()
    }

    fn lines(commands: &[&[&str]]) -> Vec<Vec<String>> {
        commands
            .iter()
            .map(|words| words.iter().map(|word| word.to_string()).collect())
            .collect()
    }

    #[test]
    fn commands_end_at_newlines_and_operators() {
        let text = "make a && make b || make c; make d | tee x & make e\nmake f\n";
        let expected = lines(&[
            &["make", "a"],
            &["make", "b"],
            &["make", "c"],
            &["make", "d"],
            &["tee", "x"],
            &["make", "e"],
            &["make", "f"],
        ]);
        assert_eq!(commands(text), expected);
    }

    #[test]
    fn a_comment_ends_a_line_but_a_hash_inside_a_word_does_not() {
        assert_eq!(
            commands("# make a\nmake b # make c\nmake d#e\n"),
            lines(&[&["make", "b"], &["make", "d#e"]])
        );
    }

    #[test]
    fn quotes_join_words_and_span_lines() {
        let text = "make 'a b' \"c d\" e\"f\"g\nmake \"x\ny\" z\n";
        assert_eq!(
            commands(text),
            lines(&[&["make", "a b", "c d", "efg"], &["make", "x\ny", "z"]])
        );
        assert_eq!(
            commands("make \"a\\\"b\" 'c\\d' \\$x\n"),
            lines(&[&["make", "a\"b", "c\\d", "$x"]])
        );
    }

    #[test]
    fn an_unclosed_quote_ends_the_reading_and_drops_its_command() {
        for text in ["make a\nmake 'b\nmake c\n", "make a\nmake \"b\nmake c\n"] {
            let read = read(text);
            assert_eq!(
                read.items,
                [Item::Command(vec![
                    Word {
                        text: "make".into(),
                        range: 0..4
                    },
                    Word {
                        text: "a".into(),
                        range: 5..6
                    }
                ])],
                "{text:?}"
            );
            assert_eq!(read.unclosed, Some(text.find(['\'', '"']).unwrap()));
        }
        assert_eq!(read("make $(x\n").unclosed, Some(5));
        assert_eq!(read("make `x\n").unclosed, Some(5));
    }

    #[test]
    fn a_trailing_backslash_joins_the_next_line() {
        assert_eq!(
            commands("make a \\\n  b \\\r\n  c\nmake d\n"),
            lines(&[&["make", "a", "b", "c"], &["make", "d"]])
        );
    }

    #[test]
    fn substitutions_stay_in_their_word() {
        assert_eq!(
            commands("make -j$(nproc --all) `echo a b` ${T:-x y} \"$(echo c d)\"\n"),
            lines(&[&[
                "make",
                "-j$(nproc --all)",
                "`echo a b`",
                "${T:-x y}",
                "$(echo c d)"
            ]])
        );
    }

    #[test]
    fn redirections_and_their_files_are_left_out() {
        let text =
            "make a > out.log 2>&1\nmake b 2>/dev/null >>log\nmake c &> all < in\nmake d >&2\n";
        assert_eq!(
            commands(text),
            lines(&[
                &["make", "a"],
                &["make", "b"],
                &["make", "c"],
                &["make", "d"]
            ])
        );
    }

    #[test]
    fn a_heredoc_keeps_its_opener_and_loses_its_body() {
        let text = "cat <<EOF > f\nmake nope\nEOF\nmake a\ncat <<-'X'\n\tmake no\n\tX\nmake b\n";
        assert_eq!(
            commands(text),
            lines(&[&["cat"], &["make", "a"], &["cat"], &["make", "b"]])
        );
        // Never closed: the rest is the body.
        assert_eq!(
            commands("make a\ncat <<EOF\nmake b\n"),
            lines(&[&["make", "a"], &["cat"]])
        );
    }

    #[test]
    fn a_subshell_is_an_open_and_a_close() {
        assert_eq!(
            commands("(cd sub && make a) && make b\n"),
            lines(&[
                &["("],
                &["cd", "sub"],
                &["make", "a"],
                &[")"],
                &["make", "b"]
            ])
        );
        // A parenthesis in a word, or with no subshell open, is the word's.
        assert_eq!(
            commands("make a) f(x)\n"),
            lines(&[&["make", "a)", "f(x)"]])
        );
    }

    #[test]
    fn a_keyword_that_starts_a_command_is_a_command_of_its_own() {
        let text = "if [ -d sub ]; then cd sub; make a; fi\n\
                    for d in x; do cd sub; make b; done\n\
                    { cd sub && make c; }\n\
                    ! make d\n";
        assert_eq!(
            commands(text),
            lines(&[
                &["if"],
                &["[", "-d", "sub", "]"],
                &["then"],
                &["cd", "sub"],
                &["make", "a"],
                &["fi"],
                &["for", "d", "in", "x"],
                &["do"],
                &["cd", "sub"],
                &["make", "b"],
                &["done"],
                &["{"],
                &["cd", "sub"],
                &["make", "c"],
                &["}"],
                &["!"],
                &["make", "d"],
            ])
        );
        // After another word, or quoted or escaped, a keyword is an argument.
        assert_eq!(
            commands("echo then cd x\n\"do\" cd x\n\\{ cd x\n"),
            lines(&[
                &["echo", "then", "cd", "x"],
                &["do", "cd", "x"],
                &["{", "cd", "x"]
            ])
        );
    }

    #[test]
    fn a_subshell_opens_after_a_keyword_and_closes_before_the_target_is_read() {
        assert_eq!(
            commands("for d in sub other; do (cd sub && make sub-only); done\n"),
            lines(&[
                &["for", "d", "in", "sub", "other"],
                &["do"],
                &["("],
                &["cd", "sub"],
                &["make", "sub-only"],
                &[")"],
                &["done"],
            ])
        );
        assert_eq!(
            commands("if true; then (cd sub && make x); fi\n"),
            lines(&[
                &["if"],
                &["true"],
                &["then"],
                &["("],
                &["cd", "sub"],
                &["make", "x"],
                &[")"],
                &["fi"],
            ])
        );
        // In the middle of a command, a parenthesis is the word's.
        assert_eq!(commands("make (a) b\n"), lines(&[&["make", "(a)", "b"]]));
    }

    #[test]
    fn a_function_header_is_a_command_of_its_own() {
        let body = lines(&[&["{"], &["cd", "sub"], &["make", "a"], &["}"]]);
        for (text, header) in [
            ("f() { cd sub; make a; }", &["f()"][..]),
            ("f(){ cd sub; make a; }", &["f()"]),
            ("f () { cd sub; make a; }", &["f", "()"]),
            ("function f { cd sub; make a; }", &["function", "f"]),
            ("function f() { cd sub; make a; }", &["function", "f()"]),
            (
                "function f () { cd sub; make a; }",
                &["function", "f", "()"],
            ),
        ] {
            let mut expected = lines(&[header]);
            expected.extend(body.clone());
            assert_eq!(commands(text), expected, "{text}");
        }
        // A header with a subshell for a body.
        assert_eq!(
            commands("f() ( cd sub; make a )\ng()(make b)\n"),
            lines(&[
                &["f()"],
                &["("],
                &["cd", "sub"],
                &["make", "a"],
                &[")"],
                &["g()"],
                &["("],
                &["make", "b"],
                &[")"],
            ])
        );
        // After another word, or quoted, a header is an argument.
        assert_eq!(
            commands("echo f() { cd x\n\"f()\" { cd x\necho function f { cd x\n"),
            lines(&[
                &["echo", "f()", "{", "cd", "x"],
                &["f()", "{", "cd", "x"],
                &["echo", "function", "f", "{", "cd", "x"],
            ])
        );
    }

    #[test]
    fn time_and_coproc_leave_a_command_to_start() {
        assert_eq!(
            commands("time (cd sub && make a)\ntime -p (make b)\ncoproc (make c)\n"),
            lines(&[
                &["time"],
                &["("],
                &["cd", "sub"],
                &["make", "a"],
                &[")"],
                &["time", "-p"],
                &["("],
                &["make", "b"],
                &[")"],
                &["coproc"],
                &["("],
                &["make", "c"],
                &[")"],
            ])
        );
        assert_eq!(
            commands("time { cd sub; make a; }\n"),
            lines(&[&["time"], &["{"], &["cd", "sub"], &["make", "a"], &["}"]])
        );
        // After another word, quoted, or after an argument, `time` and `(` are words.
        assert_eq!(
            commands("echo time (a)\n\"time\" (a)\ntime x (a)\n"),
            lines(&[
                &["echo", "time", "(a)"],
                &["time", "(a)"],
                &["time", "x", "(a)"]
            ])
        );
    }

    #[test]
    fn a_case_head_and_a_case_arm_label_are_left_out_and_the_arm_starts_a_command() {
        let arm = lines(&[&["cd", "sub"], &["make", "a"], &["esac"]]);
        for text in [
            "case $x in a) cd sub; make a ;; esac\n",
            "case $x in a|b) cd sub; make a ;; esac\n",
            "case $x in (a) cd sub; make a ;; esac\n",
            "case $x in (a | 'b)' | \"c)\") cd sub; make a ;& esac\n",
            "case $x in\n  a) cd sub\n     make a\n     ;;\nesac\n",
            "case \"$x\" in\n  a) cd sub; make a;;\n  *) ;;\nesac\n",
        ] {
            assert_eq!(commands(text), arm, "{text:?}");
        }
        // A label named like a command, or like a keyword, is no command.
        assert_eq!(
            commands("case $x in cd|make) make a ;; then) true ;; esac\n"),
            lines(&[&["make", "a"], &["true"], &["esac"]])
        );
        // Each arm after `;;`, `;&` and `;;&` starts at its label, and a case in an arm ends alone.
        assert_eq!(
            commands(
                "case $x in a) case $y in b) make 1 ;; esac ;; c) make 2 ;;& d) make 3 ;; esac\n"
            ),
            lines(&[
                &["make", "1"],
                &["esac"],
                &["make", "2"],
                &["make", "3"],
                &["esac"]
            ])
        );
        // The word `case` after another word is an argument.
        assert_eq!(
            commands("echo case x in a) y\n"),
            lines(&[&["echo", "case", "x", "in", "a)", "y"]])
        );
    }

    #[test]
    fn the_parenthesis_of_a_case_arm_label_does_not_close_a_subshell() {
        assert_eq!(
            commands("(cd sub; case x in a) true;; esac; make a)\n"),
            lines(&[
                &["("],
                &["cd", "sub"],
                &["true"],
                &["esac"],
                &["make", "a"],
                &[")"],
            ])
        );
        // An `esac)` closes the case and the subshell; a subshell in an arm closes at its own `)`.
        assert_eq!(
            commands("(case x in a) (make a);; b) true;; esac) && make b\n"),
            lines(&[
                &["("],
                &["("],
                &["make", "a"],
                &[")"],
                &["true"],
                &["esac"],
                &[")"],
                &["make", "b"],
            ])
        );
        // Without a case, or after its `esac`, a `)` closes the subshell it finds.
        assert_eq!(
            commands("(case x in a) true;; esac) && (make b)\n"),
            lines(&[
                &["("],
                &["true"],
                &["esac"],
                &[")"],
                &["("],
                &["make", "b"],
                &[")"],
            ])
        );
    }

    #[test]
    fn a_prompt_makes_the_other_lines_output() {
        let text = "$ make a\nmake: *** No rule to make target 'a'.  Stop.\n  % make b \\\n    c\nmake d\n";
        assert_eq!(
            commands(text),
            lines(&[&["make", "a"], &["make", "b", "c"]])
        );
        // No prompt anywhere: every line is a command.
        assert_eq!(
            commands("make a\nmake b\n"),
            lines(&[&["make", "a"], &["make", "b"]])
        );
    }

    #[test]
    fn word_ranges_cover_the_word_as_written() {
        let text = "x \"a b\" c\\ d\n";
        let Item::Command(words) = &read(text).items[0] else {
            panic!("a command");
        };
        let ranges: Vec<&str> = words.iter().map(|word| &text[word.range.clone()]).collect();
        assert_eq!(ranges, ["x", "\"a b\"", "c\\ d"]);
    }

    fn words(text: &str) -> Vec<Word> {
        match read(text).items.remove(0) {
            Item::Command(words) => words,
            other => panic!("{other:?}"),
        }
    }

    fn targets(text: &str) -> Vec<String> {
        call(&words(text))
            .unwrap()
            .targets
            .into_iter()
            .map(|word| word.text)
            .collect()
    }

    #[test]
    fn a_call_names_its_targets_and_leaves_out_flags_and_assignments() {
        assert_eq!(targets("make ci ci-fast"), ["ci", "ci-fast"]);
        assert_eq!(targets("make -j8 -k CC=gcc \"A=b c\" build V=1"), ["build"]);
        assert_eq!(targets("make -j 8 all"), ["all"]);
        assert_eq!(targets("make -j all"), ["all"]);
        assert_eq!(targets("make --jobs 4 --jobs=2 --silent all"), ["all"]);
        assert_eq!(targets("make -I inc -o old -W new a"), ["a"]);
        assert_eq!(targets("make -kj4 -Otarget a"), ["a"]);
        assert_eq!(targets("make a -- -b c=d"), ["a", "-b"]);
        assert_eq!(targets("make"), Vec::<String>::new());
        assert_eq!(targets("make -n"), Vec::<String>::new());
    }

    #[test]
    fn a_call_collects_its_directories() {
        let call = call(&words("make -C a -Cb --directory c --directory=d x")).unwrap();
        assert_eq!(call.dirs, ["a", "b", "c", "d"]);
        assert_eq!(call.targets.len(), 1);
    }

    #[test]
    fn a_call_that_names_its_makefile_or_an_unreadable_directory_is_refused() {
        for text in [
            "make -f other all",
            "make -fother all",
            "make --file=x all",
            "make --makefile x all",
            "make -E 'a:' all",
            "make --eval=a all",
            "make -sf x all",
        ] {
            assert_eq!(call(&words(text)), Err(Refusal::Makefile), "{text}");
        }
        for text in [
            "make -C $DIR all",
            "make -C",
            "make -C '' all",
            "make -C d/* a",
        ] {
            assert_eq!(call(&words(text)), Err(Refusal::Dir), "{text}");
        }
    }

    #[test]
    fn what_a_shell_or_a_reader_fills_in_is_unreadable() {
        for word in [
            "$(T)", "$T", "${T}", "`t`", "<target>", "[target]", "{a,b}", "t*", "t?", "a...", "…",
            "a…",
        ] {
            assert!(unreadable(word), "{word}");
        }
        for word in ["ci-fast", "test_all", "a/b.o", "..", ".venv", "a.b", "#1"] {
            assert!(!unreadable(word), "{word}");
        }
    }
}
