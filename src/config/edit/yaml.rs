//! The keys of a YAML config and their places, read from the parser's events.
//!
//! The parser is `granit-parser`, which `serde-saphyr` re-exports: it gives a span for every scalar,
//! keys included, which `serde-saphyr` itself does not. A key reached through an alias or a `<<`
//! merge does not have a place of its own and is not listed, and a file that has either is
//! flagged ([`Scan::aliased`]), so that nothing edits it.

use serde_saphyr::granit_parser::{Event, Parser, ScalarStyle, StructureStyle};

use super::{Member, Part, Scan};

/// Where a collection is, and what the next scalar or collection in it is.
enum Frame {
    /// A map, and the member whose value comes next, if a key was just read.
    Map {
        path: Vec<Part>,
        key: Option<usize>,
        /// Whether it is written in braces.
        flow: bool,
        /// The member whose value this map is.
        owner: Option<usize>,
    },
    /// A list, and the index of its next item.
    List {
        path: Vec<Part>,
        next: usize,
        /// Whether it is written in brackets.
        flow: bool,
        owner: Option<usize>,
    },
}

impl Frame {
    fn owner(&self) -> Option<usize> {
        match self {
            Frame::Map { owner, .. } | Frame::List { owner, .. } => *owner,
        }
    }

    fn flow(&self) -> bool {
        match self {
            Frame::Map { flow, .. } | Frame::List { flow, .. } => *flow,
        }
    }
}

/// Reads the keys of `text`.
pub(super) fn scan(text: &str) -> Result<Scan, String> {
    let mut members: Vec<Member> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut braced = false;
    let mut aliased = false;
    // Where the last scalar, alias or closing brace or bracket that was read ends.
    let mut last_end = 0;
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
                let block = matches!(style, ScalarStyle::Literal | ScalarStyle::Folded);
                if block {
                    // The span of a block scalar does not say where its lines end, so a key that
                    // holds one is not edited by span.
                    for owner in stack.iter().filter_map(Frame::owner) {
                        members[owner].raw = true;
                    }
                }
                match stack.last_mut() {
                    Some(Frame::Map {
                        path, key, flow, ..
                    }) if key.is_none() => {
                        // A key. Its text is read back from the file, so an escape in it is read.
                        let name = key_text(&text[range.clone()], style);
                        // `<<` merges another map in, whatever the file does with it.
                        aliased |= name == "<<";
                        last_end = range.end;
                        let mut member_path = path.clone();
                        member_path.push(Part::Key(name));
                        members.push(Member {
                            path: member_path,
                            end: range.end,
                            key: range,
                            line: span.start.line(),
                            value: None,
                            empty: false,
                            flow: *flow,
                            decorated: anchor != 0 || tag.is_some(),
                            raw: false,
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
                        if let Some(index) = key.take() {
                            let member = &mut members[index];
                            member.raw |= block;
                            if range.is_empty() {
                                // No value was written: the parser puts an empty one at the key.
                                member.empty = true;
                            } else {
                                member.end = range.end;
                                last_end = range.end;
                            }
                            if replaceable {
                                member.value = Some(range);
                            }
                        }
                    }
                    Some(Frame::List { next, .. }) => {
                        *next += 1;
                        last_end = last_end.max(range.end);
                    }
                    None => return Err("the top level is a scalar".to_string()),
                }
            }
            Event::Alias(_) => {
                aliased = true;
                let end = bytes(&span.end)?;
                last_end = end;
                if let Some(Frame::Map {
                    key: Some(index), ..
                }) = stack.last()
                {
                    members[*index].end = end;
                }
                done(&mut stack);
            }
            Event::MappingStart(style, _, _) | Event::SequenceStart(style, _, _) => {
                let is_map = matches!(event, Event::MappingStart(..));
                let flow = style == StructureStyle::Flow;
                let (path, owner) = match stack.last_mut() {
                    None => {
                        root = Some(is_map);
                        braced = flow;
                        (Vec::new(), None)
                    }
                    Some(Frame::Map { key, .. }) => {
                        let Some(index) = key.take() else {
                            // A collection as a key: nothing under it can be found by name.
                            stack.push(Frame::Map {
                                path: vec![Part::Key("?".to_string())],
                                key: None,
                                flow,
                                owner: None,
                            });
                            continue;
                        };
                        (members[index].path.clone(), Some(index))
                    }
                    Some(Frame::List { path, next, .. }) => {
                        let mut item = path.clone();
                        item.push(Part::Index(*next));
                        *next += 1;
                        (item, None)
                    }
                };
                stack.push(if is_map {
                    Frame::Map {
                        path,
                        key: None,
                        flow,
                        owner,
                    }
                } else {
                    Frame::List {
                        path,
                        next: 0,
                        flow,
                        owner,
                    }
                });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let popped = stack.pop();
                // The closing brace or bracket of a flow collection is where its value ends. A
                // block collection ends where its last scalar does.
                if popped.as_ref().is_some_and(Frame::flow) {
                    last_end = bytes(&span.end)?;
                }
                if let Some(owner) = popped.and_then(|frame| frame.owner()) {
                    members[owner].end = last_end;
                }
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
    Ok(Scan {
        members,
        braced,
        aliased,
    })
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
