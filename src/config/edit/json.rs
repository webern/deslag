//! The keys of a JSON config and their places, found by a scanner of its own.
//!
//! `serde_json` does not give a position for a member that parsed, and writing a tree back out
//! sorts the keys and rewrites escapes and big numbers, so the text is read here and edited by
//! span. The scanner is for text that already loaded as a config: it knows JSON's grammar and
//! nothing past it.

use super::{Member, Part, Scan};

/// How deep a file may nest before the scanner gives up, so that a hostile file cannot overflow the
/// stack. A config is a few levels deep.
const MAX_DEPTH: usize = 64;

/// Reads the keys of `text`.
pub(super) fn scan(text: &str) -> Result<Scan, String> {
    let mut scanner = Scanner {
        text,
        at: 0,
        members: Vec::new(),
    };
    scanner.space();
    if scanner.peek() != Some(b'{') {
        return Err("the top level is not an object".to_string());
    }
    scanner.value(&[], 0)?;
    Ok(Scan {
        members: scanner.members,
        braced: true,
        aliased: false,
    })
}

struct Scanner<'a> {
    text: &'a str,
    at: usize,
    members: Vec<Member>,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", byte as char, self.at))
        }
    }

    /// Reads the string at the cursor and returns its bytes, quotes included.
    fn string(&mut self) -> Result<std::ops::Range<usize>, String> {
        let start = self.at;
        self.expect(b'"')?;
        loop {
            match self.peek() {
                None => return Err("a string is not closed".to_string()),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(start..self.at);
                }
                Some(b'\\') => self.at += 2,
                Some(_) => self.at += 1,
            }
        }
    }

    /// Reads the value at the cursor, whose place is `path`, and returns its bytes when it is a
    /// scalar.
    fn value(
        &mut self,
        path: &[Part],
        depth: usize,
    ) -> Result<Option<std::ops::Range<usize>>, String> {
        if depth > MAX_DEPTH {
            return Err("it nests too deeply".to_string());
        }
        self.space();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                self.space();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(None);
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    let name: String = serde_json::from_str(&self.text[key.clone()])
                        .map_err(|error| error.to_string())?;
                    self.space();
                    self.expect(b':')?;
                    let mut member_path = path.to_vec();
                    member_path.push(Part::Key(name));
                    let line = self.text[..key.start].matches('\n').count() + 1;
                    let index = self.members.len();
                    self.members.push(Member {
                        path: member_path.clone(),
                        end: key.end,
                        key,
                        line,
                        value: None,
                        empty: false,
                        flow: true,
                        decorated: false,
                        raw: false,
                    });
                    let value = self.value(&member_path, depth + 1)?;
                    self.members[index].value = value;
                    self.members[index].end = self.at;
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(None);
                        }
                        _ => return Err(format!("expected `,` or `}}` at byte {}", self.at)),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                self.space();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(None);
                }
                for index in 0.. {
                    let mut item_path = path.to_vec();
                    item_path.push(Part::Index(index));
                    self.value(&item_path, depth + 1)?;
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(None);
                        }
                        _ => return Err(format!("expected `,` or `]` at byte {}", self.at)),
                    }
                }
                unreachable!("the loop returns")
            }
            Some(b'"') => self.string().map(Some),
            Some(_) => {
                let start = self.at;
                while !matches!(
                    self.peek(),
                    None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
                ) {
                    self.at += 1;
                }
                if start == self.at {
                    return Err(format!("expected a value at byte {start}"));
                }
                Ok(Some(start..self.at))
            }
            None => Err("the text ends where a value should be".to_string()),
        }
    }
}
