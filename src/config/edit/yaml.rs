//! The keys of a YAML config and their places, read from the parser's events.
//!
//! The parser is `granit-parser`, which `serde-saphyr` re-exports: it gives a span for every scalar,
//! keys included, which `serde-saphyr` itself does not. A key reached through an alias or a `<<`
//! merge does not have a place of its own and is not listed.

use serde_saphyr::granit_parser::{Event, Parser, ScalarStyle, StructureStyle};

use super::{Member, Part, Scan};

/// Where a collection is, and what the next scalar or collection in it is.
enum Frame {
    /// A map, and the member whose value comes next, if a key was just read.
    Map { path: Vec<Part>, key: Option<usize> },
    /// A list, and the index of its next item.
    List { path: Vec<Part>, next: usize },
}

/// Reads the keys of `text`.
pub(super) fn scan(text: &str) -> Result<Scan, String> {
    let mut members: Vec<Member> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut braced = false;
    let mut root: Option<bool> = None;
    let mut parser = Parser::new_from_str(text);

    for event in &mut parser {
        let (event, span) = event.map_err(|error| error.to_string())?;
        let bytes = |marker: &serde_saphyr::granit_parser::Marker| {
            marker
                .byte_offset()
                .ok_or_else(|| "the parser gave no byte offset".to_string())
        };
        match event {
            Event::Scalar(_, style, anchor, tag) => {
                let range = bytes(&span.start)?..bytes(&span.end)?;
                match stack.last_mut() {
                    Some(Frame::Map { path, key }) if key.is_none() => {
                        // A key. Its text is read back from the file, so an escape in it is read.
                        let name = key_text(&text[range.clone()], style);
                        let mut member_path = path.clone();
                        member_path.push(Part::Key(name));
                        members.push(Member {
                            path: member_path,
                            key: range,
                            line: span.start.line(),
                            value: None,
                        });
                        *key = Some(members.len() - 1);
                    }
                    Some(Frame::Map { key, .. }) => {
                        let replaceable = anchor == 0
                            && tag.is_none()
                            && matches!(
                                style,
                                ScalarStyle::Plain
                                    | ScalarStyle::SingleQuoted
                                    | ScalarStyle::DoubleQuoted
                            );
                        if let (Some(index), true) = (key.take(), replaceable) {
                            members[index].value = Some(range);
                        }
                    }
                    Some(Frame::List { next, .. }) => *next += 1,
                    None => return Err("the top level is a scalar".to_string()),
                }
            }
            Event::Alias(_) => done(&mut stack),
            Event::MappingStart(style, _, _) | Event::SequenceStart(style, _, _) => {
                let is_map = matches!(event, Event::MappingStart(..));
                let path = match stack.last_mut() {
                    None => {
                        root = Some(is_map);
                        braced = style == StructureStyle::Flow;
                        Vec::new()
                    }
                    Some(Frame::Map { key, .. }) => {
                        let Some(index) = key.take() else {
                            // A collection as a key: nothing under it can be found by name.
                            stack.push(Frame::Map {
                                path: vec![Part::Key("?".to_string())],
                                key: None,
                            });
                            continue;
                        };
                        members[index].path.clone()
                    }
                    Some(Frame::List { path, next }) => {
                        let mut item = path.clone();
                        item.push(Part::Index(*next));
                        *next += 1;
                        item
                    }
                };
                stack.push(if is_map {
                    Frame::Map { path, key: None }
                } else {
                    Frame::List { path, next: 0 }
                });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                stack.pop();
                // A collection that was a value was the map's value; its next key comes.
                if let Some(Frame::Map { key, .. }) = stack.last_mut() {
                    *key = None;
                }
            }
            _ => {}
        }
    }
    if root != Some(true) {
        return Err("the top level is not a map".to_string());
    }
    Ok(Scan { members, braced })
}

/// A scalar that was a value has been read: a map is ready for its next key.
fn done(stack: &mut [Frame]) {
    match stack.last_mut() {
        Some(Frame::Map { key, .. }) => *key = None,
        Some(Frame::List { next, .. }) => *next += 1,
        None => {}
    }
}

/// The text of a key written as `written`.
fn key_text(written: &str, style: ScalarStyle) -> String {
    match style {
        ScalarStyle::Plain => written.to_string(),
        ScalarStyle::SingleQuoted => written.trim_matches('\'').replace("''", "'"),
        _ => serde_json::from_str::<String>(written).unwrap_or_else(|_| written.to_string()),
    }
}
