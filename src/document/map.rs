//! Where a stretch of text is in the text it was made from.
//!
//! A [`SourceMap`] tiles a text, the inner text, with [`Segment`]s, each of which says where in an
//! outer text, usually the file, its bytes are, and how: held as written ([`SegmentKind::Verbatim`]),
//! written some other way, such as an entity ([`SegmentKind::Escaped`]), or not written at all
//! ([`SegmentKind::Synthetic`]). [`SourceMap::to_file`] turns a range of the inner text into a range
//! of the outer text and says whether an edit to it would be an edit to the file.
//!
//! [`Gathered`] builds a map as it gathers text from pieces of a source.

use std::ops::Range;

/// How the outer text holds the bytes of a [`Segment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentKind {
    /// The outer text holds the same bytes. A range inside it maps exactly.
    Verbatim,
    /// The outer text holds other bytes that read as these, such as the entity `&amp;` for `&`.
    Escaped,
    /// The outer text holds nothing for it, as for a line break read as a space. Its outer range is
    /// the empty range at the point it stands at.
    Synthetic,
}

/// A stretch of the inner text and where the outer text holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    /// Its bytes in the inner text.
    pub inner: Range<usize>,
    /// Its bytes in the outer text.
    pub outer: Range<usize>,
    /// How the outer text holds it.
    pub kind: SegmentKind,
}

/// Where a range of the inner text is in the outer text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mapped {
    /// The range of the outer text.
    pub range: Range<usize>,
    /// Whether the range holds exactly the bytes of the inner range, so that replacing it replaces
    /// them.
    pub editable: bool,
}

/// The segments that tile an inner text, and so say where each byte of it is in an outer text.
///
/// The segments are sorted, contiguous and do not overlap on the inner side. On the outer side
/// they are sorted and disjoint, except that the points of synthetic segments may tie with their
/// neighbours. [`SourceMap::push`] is the only way to add one, and asserts all of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceMap {
    segments: Vec<Segment>,
}

impl SourceMap {
    /// Adds a segment of `kind` and `len` bytes at the end of the inner text, held by `outer`.
    ///
    /// A verbatim segment that starts where a verbatim one ended joins it, so that two pieces of
    /// one stretch of the outer text are one segment and a range across them is editable.
    ///
    /// # Panics
    ///
    /// If `len` is zero, if `outer` does not suit `kind` (a verbatim segment holds `len` bytes, an
    /// escaped one some, a synthetic one none), or if `outer` starts before the last segment's
    /// ends. A wrong map would answer wrongly without a sound.
    pub fn push(&mut self, kind: SegmentKind, len: usize, outer: Range<usize>) {
        assert!(len > 0, "a segment of no bytes: outer {outer:?}");
        let fits = match kind {
            SegmentKind::Verbatim => outer.len() == len,
            SegmentKind::Escaped => !outer.is_empty(),
            SegmentKind::Synthetic => outer.is_empty(),
        };
        assert!(
            fits && outer.start <= outer.end,
            "{kind:?} segment of {len} bytes cannot be held by {outer:?}"
        );
        let start = self.len();
        let Some(last) = self.segments.last_mut() else {
            self.segments.push(Segment {
                inner: 0..len,
                outer,
                kind,
            });
            return;
        };
        assert!(
            last.outer.end <= outer.start,
            "segment at {outer:?} starts before the one before it ends, at {:?}",
            last.outer
        );
        if kind == SegmentKind::Verbatim
            && last.kind == SegmentKind::Verbatim
            && last.outer.end == outer.start
        {
            last.inner.end += len;
            last.outer.end = outer.end;
            return;
        }
        self.segments.push(Segment {
            inner: start..start + len,
            outer,
            kind,
        });
    }

    /// Adds `text`, which is what the bytes of `source` at `outer` read as, as the segment they
    /// make: verbatim if they are `text`, synthetic if `outer` is empty, escaped otherwise. Empty
    /// `text` adds nothing.
    ///
    /// # Panics
    ///
    /// If `outer` is not on characters of `source`, and as [`SourceMap::push`] does.
    pub fn push_text(&mut self, source: &str, outer: Range<usize>, text: &str) {
        if text.is_empty() {
            return;
        }
        let kind = if source[outer.clone()] == *text {
            SegmentKind::Verbatim
        } else if outer.is_empty() {
            SegmentKind::Synthetic
        } else {
            SegmentKind::Escaped
        };
        self.push(kind, text.len(), outer);
    }

    /// Where the outer text holds `inner`, a range of the inner text that starts and ends on
    /// characters.
    ///
    /// Inside one verbatim segment the answer is exact and editable. A range that starts or ends in
    /// an escaped or synthetic segment takes all of that segment, and one across several segments
    /// takes everything from the first byte of the first to the last byte of the last, gaps
    /// included; neither is editable. A start on the boundary of two segments belongs to the later
    /// one and an end to the earlier one. An empty range is the point its start maps to, or at the
    /// end of the text the end of the last segment.
    ///
    /// # Panics
    ///
    /// If `inner` is reversed or ends past the inner text, or the map is empty.
    pub fn to_file(&self, inner: Range<usize>) -> Mapped {
        let len = self.len();
        assert!(
            inner.start <= inner.end && inner.end <= len && len > 0,
            "range {inner:?} is not in a text of {len} bytes"
        );
        let at = |from: usize| {
            &self.segments[self.segments.partition_point(|s| s.inner.start <= from) - 1]
        };
        let first = at(inner.start);
        let start = match first.kind {
            SegmentKind::Verbatim => first.outer.start + (inner.start - first.inner.start),
            _ if inner.start == len => first.outer.end,
            _ => first.outer.start,
        };
        if inner.is_empty() {
            return Mapped {
                range: start..start,
                editable: first.kind == SegmentKind::Verbatim,
            };
        }
        let last = at(inner.end - 1);
        let end = match last.kind {
            SegmentKind::Verbatim => last.outer.start + (inner.end - last.inner.start),
            _ => last.outer.end,
        };
        Mapped {
            range: start..end,
            editable: first.kind == SegmentKind::Verbatim && std::ptr::eq(first, last),
        }
    }

    /// Where the inner text holds `outer`, a range of the outer text that starts and ends on
    /// characters: the inverse of [`SourceMap::to_file`].
    ///
    /// The inner range starts at the first byte of the inner text that lies at or after the start of
    /// `outer`, and ends after the last byte that lies at or before its end. An escaped segment is
    /// in the range only when all of its outer bytes are. A synthetic segment, which stands at a
    /// point, is in the range when the point is its start or inside it, but not its end. So the text
    /// between the end of a token and the start of the next, on two lines, is the line break
    /// between them. A range that lies wholly in a gap is empty.
    ///
    /// For an inner range that does not end in a synthetic segment, `to_inner` of what `to_file`
    /// says holds the inner range, and is equal to it when `to_file` says it is editable, unless a
    /// synthetic segment stands at its start.
    pub fn to_inner(&self, outer: Range<usize>) -> Range<usize> {
        let first = self.segments.partition_point(|s| match s.kind {
            SegmentKind::Verbatim => s.outer.end <= outer.start,
            _ => s.outer.start < outer.start,
        });
        let last = self.segments.partition_point(|s| match s.kind {
            SegmentKind::Escaped => s.outer.end <= outer.end,
            _ => s.outer.start < outer.end,
        });
        let start = self
            .segments
            .get(first)
            .map_or(self.len(), |s| match s.kind {
                SegmentKind::Verbatim => s.inner.start + outer.start.saturating_sub(s.outer.start),
                _ => s.inner.start,
            });
        let end = last.checked_sub(1).map_or(0, |last| {
            let s = &self.segments[last];
            match s.kind {
                SegmentKind::Verbatim => {
                    s.inner.start + (outer.end - s.outer.start).min(s.inner.len())
                }
                _ => s.inner.end,
            }
        });
        start..end.max(start)
    }

    /// The map from this map's inner text to `outer`'s outer text, where `self` maps its inner text
    /// to `outer`'s inner text.
    ///
    /// A synthetic segment of `self` that stands inside an escaped segment of `outer` is escaped
    /// there. Where several segments of `self` end up in one escaped or synthetic segment of
    /// `outer`, their outer ranges would overlap, so they become one escaped segment. A range of
    /// the result can therefore take more than mapping twice does: it always holds what mapping
    /// twice gives, and is equal to it when editable.
    ///
    /// # Panics
    ///
    /// If `outer` is empty and `self` is not.
    // TODO: remove the dead_code guard when the reader of fenced code uses it.
    #[allow(dead_code)]
    pub fn compose(&self, outer: &SourceMap) -> SourceMap {
        let mut parts: Vec<Part> = Vec::new();
        for segment in &self.segments {
            let len = segment.inner.len();
            match segment.kind {
                SegmentKind::Synthetic => {
                    let point = segment.outer.start;
                    let at = outer.to_file(point..point).range.start;
                    let held = &outer.segments
                        [outer.segments.partition_point(|s| s.inner.start <= point) - 1];
                    if held.kind == SegmentKind::Escaped
                        && held.inner.start < point
                        && point < held.inner.end
                    {
                        join(&mut parts, SegmentKind::Escaped, len, held.outer.clone());
                    } else {
                        join(&mut parts, SegmentKind::Synthetic, len, at..at);
                    }
                }
                SegmentKind::Escaped => {
                    let range = outer.to_file(segment.outer.clone()).range;
                    let kind = match range.is_empty() {
                        true => SegmentKind::Synthetic,
                        false => SegmentKind::Escaped,
                    };
                    join(&mut parts, kind, len, range);
                }
                SegmentKind::Verbatim => {
                    let first = outer
                        .segments
                        .partition_point(|s| s.inner.end <= segment.outer.start);
                    let across = outer.segments[first..]
                        .iter()
                        .take_while(|s| s.inner.start < segment.outer.end);
                    for part in across {
                        let from = segment.outer.start.max(part.inner.start);
                        let to = segment.outer.end.min(part.inner.end);
                        let held = match part.kind {
                            SegmentKind::Verbatim => {
                                let at = part.outer.start + (from - part.inner.start);
                                at..at + (to - from)
                            }
                            _ => part.outer.clone(),
                        };
                        join(&mut parts, part.kind, to - from, held);
                    }
                }
            }
        }
        let mut composed = SourceMap::default();
        for Part { kind, len, outer } in parts {
            composed.push(kind, len, outer);
        }
        composed
    }

    /// The segments, in order.
    // TODO: remove the dead_code guard when the reader of fenced code uses it.
    #[allow(dead_code)]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// How many bytes of inner text the segments tile.
    pub fn len(&self) -> usize {
        self.segments.last().map_or(0, |last| last.inner.end)
    }

    /// Whether the map does not tile any text.
    // TODO: remove the dead_code guard when the reader of fenced code uses it.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Forgets the segments, keeping the room for more.
    pub fn clear(&mut self) {
        self.segments.clear();
    }
}

/// A segment being composed: the kind, the length of inner text and the outer range it will be
/// pushed with.
// TODO: remove the dead_code guard when the reader of fenced code uses it.
#[allow(dead_code)]
struct Part {
    kind: SegmentKind,
    len: usize,
    outer: Range<usize>,
}

/// Adds a part to `parts`. A part whose outer range starts before the last one's ends becomes one
/// escaped part with it, and so on back while the parts overlap.
// TODO: remove the dead_code guard when the reader of fenced code uses it.
#[allow(dead_code)]
fn join(parts: &mut Vec<Part>, kind: SegmentKind, len: usize, outer: Range<usize>) {
    parts.push(Part { kind, len, outer });
    while let Some(after) = parts.pop() {
        match parts.last_mut() {
            Some(before) if after.outer.start < before.outer.end => {
                before.kind = SegmentKind::Escaped;
                before.len += after.len;
                before.outer = before.outer.start.min(after.outer.start)
                    ..before.outer.end.max(after.outer.end);
            }
            _ => {
                parts.push(after);
                return;
            }
        }
    }
}

/// Text gathered from pieces of a source, such as a block's pieces, that can say where a stretch of
/// itself is in the source.
pub(crate) struct Gathered<'a> {
    pub(crate) source: &'a str,
    /// The text gathered so far.
    pub(crate) text: String,
    /// Where each stretch of `text` is in `source`.
    map: SourceMap,
}

impl<'a> Gathered<'a> {
    /// No text yet, from pieces of `source`.
    pub(crate) fn new(source: &'a str) -> Gathered<'a> {
        Gathered {
            source,
            text: String::new(),
            map: SourceMap::default(),
        }
    }

    /// Adds `text`, which is what the source's bytes at `range` read as.
    pub(crate) fn push(&mut self, text: &str, range: Range<usize>) {
        self.map.push_text(self.source, range, text);
        self.text.push_str(text);
    }

    /// Where the source holds `range`, a range of bytes of the text that starts and ends on
    /// characters, as [`SourceMap::to_file`] says.
    pub(crate) fn source_range(&self, range: Range<usize>) -> Range<usize> {
        self.map.to_file(range).range
    }

    /// Forgets the text gathered so far.
    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::SegmentKind::{Escaped, Synthetic, Verbatim};
    use super::*;

    /// A map of `segments`, each a kind, a length of inner text and an outer range.
    fn map_of(segments: &[(SegmentKind, usize, Range<usize>)]) -> SourceMap {
        let mut map = SourceMap::default();
        for (kind, len, outer) in segments {
            map.push(*kind, *len, outer.clone());
        }
        map
    }

    /// What `to_file` says of `inner`, as the range and whether it is editable.
    fn mapped(map: &SourceMap, inner: Range<usize>) -> (Range<usize>, bool) {
        let mapped = map.to_file(inner);
        (mapped.range, mapped.editable)
    }

    #[test]
    #[should_panic(expected = "before the one before it ends")]
    fn unsorted_segments_panic() {
        map_of(&[(Verbatim, 2, 5..7), (Verbatim, 1, 3..4)]);
    }

    #[test]
    #[should_panic(expected = "before the one before it ends")]
    fn overlapping_segments_panic() {
        map_of(&[(Escaped, 1, 0..4), (Escaped, 1, 2..6)]);
    }

    #[test]
    #[should_panic(expected = "cannot be held by")]
    fn a_verbatim_segment_of_another_length_panics() {
        map_of(&[(Verbatim, 3, 0..2)]);
    }

    #[test]
    #[should_panic(expected = "cannot be held by")]
    fn an_escaped_segment_of_no_bytes_panics() {
        map_of(&[(Escaped, 1, 2..2)]);
    }

    #[test]
    #[should_panic(expected = "cannot be held by")]
    fn a_synthetic_segment_with_bytes_panics() {
        map_of(&[(Synthetic, 1, 2..3)]);
    }

    #[test]
    #[should_panic(expected = "a segment of no bytes")]
    fn a_segment_with_no_inner_text_panics() {
        map_of(&[(Verbatim, 0, 0..0)]);
    }

    #[test]
    #[should_panic(expected = "is not in a text of 2 bytes")]
    fn a_range_past_the_text_panics() {
        map_of(&[(Verbatim, 2, 0..2)]).to_file(1..3);
    }

    #[test]
    fn synthetic_points_may_tie() {
        let map = map_of(&[
            (Synthetic, 1, 2..2),
            (Synthetic, 1, 2..2),
            (Verbatim, 1, 2..3),
        ]);

        assert_eq!(map.len(), 3);
    }

    #[test]
    fn editable_only_inside_one_verbatim_segment() {
        let map = map_of(&[
            (Verbatim, 3, 0..3),
            (Escaped, 1, 3..8),
            (Verbatim, 2, 8..10),
        ]);

        assert_eq!(mapped(&map, 0..2), (0..2, true));
        assert_eq!(mapped(&map, 0..3), (0..3, true));
        assert_eq!(mapped(&map, 4..6), (8..10, true));
        assert_eq!(mapped(&map, 1..4), (1..8, false));
    }

    #[test]
    fn escaped_snaps_outward() {
        let map = map_of(&[
            (Verbatim, 3, 0..3),
            (Escaped, 1, 3..8),
            (Verbatim, 2, 8..10),
        ]);

        assert_eq!(mapped(&map, 3..4), (3..8, false));
        assert_eq!(mapped(&map, 2..4), (2..8, false));
        assert_eq!(mapped(&map, 3..5), (3..9, false));
        assert_eq!(mapped(&map, 0..6), (0..10, false));
    }

    #[test]
    fn synthetic_gives_its_point() {
        let map = map_of(&[
            (Verbatim, 2, 0..2),
            (Synthetic, 1, 2..2),
            (Verbatim, 2, 2..4),
        ]);

        assert_eq!(mapped(&map, 2..3), (2..2, false));
        assert_eq!(mapped(&map, 1..4), (1..3, false));
    }

    #[test]
    fn boundary_start_right_end_left_empty_follows_start() {
        let map = map_of(&[(Verbatim, 2, 0..2), (Escaped, 1, 2..5), (Verbatim, 2, 5..7)]);

        assert_eq!(mapped(&map, 2..3), (2..5, false));
        assert_eq!(mapped(&map, 1..2), (1..2, true));
        assert_eq!(mapped(&map, 3..5), (5..7, true));
        assert_eq!(mapped(&map, 2..2), (2..2, false));
        assert_eq!(mapped(&map, 1..1), (1..1, true));
        assert_eq!(mapped(&map, 3..3), (5..5, true));
    }

    #[test]
    fn an_empty_range_at_the_end_is_the_end_of_the_last_segment() {
        let verbatim = map_of(&[(Escaped, 1, 0..4), (Verbatim, 2, 6..8)]);
        let escaped = map_of(&[(Verbatim, 2, 0..2), (Escaped, 1, 2..5)]);

        assert_eq!(mapped(&verbatim, 3..3), (8..8, true));
        assert_eq!(mapped(&escaped, 3..3), (5..5, false));
    }

    #[test]
    fn across_two_verbatim_segments_covers_the_gap_and_is_not_editable() {
        let map = map_of(&[(Verbatim, 2, 0..2), (Verbatim, 2, 5..7)]);

        assert_eq!(map.segments().len(), 2);
        assert_eq!(mapped(&map, 1..3), (1..6, false));
    }

    #[test]
    fn abutting_verbatim_segments_are_one() {
        let map = map_of(&[(Verbatim, 2, 0..2), (Verbatim, 3, 2..5)]);

        assert_eq!(
            map.segments(),
            [Segment {
                inner: 0..5,
                outer: 0..5,
                kind: Verbatim
            }]
        );
        assert_eq!(mapped(&map, 1..4), (1..4, true));
    }

    #[test]
    fn verbatim_segments_with_something_between_stay_apart() {
        let by_gap = map_of(&[(Verbatim, 2, 0..2), (Verbatim, 2, 3..5)]);
        let by_point = map_of(&[
            (Verbatim, 2, 0..2),
            (Synthetic, 1, 2..2),
            (Verbatim, 2, 2..4),
        ]);

        assert_eq!(by_gap.segments().len(), 2);
        assert_eq!(by_point.segments().len(), 3);
    }

    #[test]
    fn push_text_classifies_what_it_is_given() {
        let source = "a&amp;b";
        let mut map = SourceMap::default();
        map.push_text(source, 0..1, "a");
        map.push_text(source, 1..6, "&");
        map.push_text(source, 6..7, "b");
        map.push_text(source, 7..7, " ");
        map.push_text(source, 7..7, "");

        let kinds: Vec<SegmentKind> = map.segments().iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Verbatim, Escaped, Verbatim, Synthetic]);
        assert_eq!(map.len(), 4);
    }

    #[test]
    fn clear_forgets_the_segments() {
        let mut map = map_of(&[(Verbatim, 2, 0..2)]);
        map.clear();

        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn compose_follows_both_maps() {
        // The file is `x&amp;y`, which reads as `x&y`, and a second reading turns `&` into `and`.
        let inner = map_of(&[(Verbatim, 1, 0..1), (Escaped, 1, 1..6), (Verbatim, 1, 6..7)]);
        let outer = map_of(&[(Verbatim, 1, 0..1), (Escaped, 3, 1..2), (Verbatim, 1, 2..3)]);

        let composed = outer.compose(&inner);

        assert_eq!(
            composed.segments(),
            [
                Segment {
                    inner: 0..1,
                    outer: 0..1,
                    kind: Verbatim
                },
                Segment {
                    inner: 1..4,
                    outer: 1..6,
                    kind: Escaped
                },
                Segment {
                    inner: 4..5,
                    outer: 6..7,
                    kind: Verbatim
                },
            ]
        );
    }

    #[test]
    fn compose_merges_what_lands_in_one_escaped_segment() {
        // Two stretches of the second text both fall inside the one entity of the file.
        let inner = map_of(&[(Verbatim, 1, 0..1), (Escaped, 2, 1..8), (Verbatim, 1, 8..9)]);
        let outer = map_of(&[
            (Verbatim, 2, 0..2),
            (Synthetic, 1, 2..2),
            (Verbatim, 2, 2..4),
        ]);

        let composed = outer.compose(&inner);

        assert_eq!(
            composed.segments(),
            [
                Segment {
                    inner: 0..1,
                    outer: 0..1,
                    kind: Verbatim
                },
                Segment {
                    inner: 1..4,
                    outer: 1..8,
                    kind: Escaped
                },
                Segment {
                    inner: 4..5,
                    outer: 8..9,
                    kind: Verbatim
                },
            ]
        );
    }

    #[test]
    fn compose_makes_a_point_inside_an_entity_the_whole_entity() {
        // The second reading has a point between the two bytes that the file's one entity reads
        // as. A range that ends at the point ends after the entity, and one that starts at it
        // starts before, so only the entity as a whole holds both.
        let inner = map_of(&[(Verbatim, 1, 0..1), (Escaped, 2, 1..8), (Verbatim, 1, 8..9)]);
        let outer = map_of(&[
            (Verbatim, 1, 0..1),
            (Synthetic, 1, 2..2),
            (Verbatim, 1, 3..4),
        ]);

        let composed = outer.compose(&inner);

        assert_eq!(
            composed.segments(),
            [
                Segment {
                    inner: 0..1,
                    outer: 0..1,
                    kind: Verbatim
                },
                Segment {
                    inner: 1..2,
                    outer: 1..8,
                    kind: Escaped
                },
                Segment {
                    inner: 2..3,
                    outer: 8..9,
                    kind: Verbatim
                },
            ]
        );
        assert_eq!(mapped(&composed, 0..2), (0..8, false));
        assert_eq!(mapped(&composed, 1..2), (1..8, false));
    }

    /// A small, fast pseudo-random generator, xorshift64*, so every seed draws the same case.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Rng {
            Rng(seed.max(1))
        }

        fn draw(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// A number below `bound`, which must not be zero.
        fn below(&mut self, bound: usize) -> usize {
            (self.draw() % bound as u64) as usize
        }

        /// One to `most` characters of `alphabet`.
        fn word(&mut self, alphabet: &[char], most: usize) -> String {
            let count = 1 + self.below(most);
            self.text(alphabet, count)
        }

        /// `count` characters of `alphabet`.
        fn text(&mut self, alphabet: &[char], count: usize) -> String {
            (0..count)
                .map(|_| alphabet[self.below(alphabet.len())])
                .collect()
        }
    }

    /// The characters of an outer text. `&` and `;` are there so that stretches look like entities.
    const OUTER: [char; 6] = ['a', ' ', 'é', '—', '&', ';'];

    /// The characters of an inner text, which has one that no outer text has.
    const INNER: [char; 5] = ['a', ' ', 'é', '—', 'Z'];

    /// An outer text, the pieces gathered from it and the map they make.
    struct Case {
        source: String,
        /// Each piece's text, which may be empty, and where it is in `source`.
        pieces: Vec<(String, Range<usize>)>,
        /// The pieces' texts joined.
        text: String,
        map: SourceMap,
    }

    impl Case {
        /// A case over `source`: up to twelve pieces after gaps of up to three characters, each
        /// verbatim, escaped, synthetic or empty.
        fn over(rng: &mut Rng, source: String) -> Case {
            let bounds: Vec<usize> = source
                .char_indices()
                .map(|(at, _)| at)
                .chain([source.len()])
                .collect();
            let last = bounds.len() - 1;
            let mut at = 0;
            let mut pieces: Vec<(String, Range<usize>)> = Vec::new();
            for _ in 0..rng.below(13) {
                at = (at + rng.below(4)).min(last);
                if rng.below(8) == 0 {
                    let end = (at + rng.below(3)).min(last);
                    pieces.push((String::new(), bounds[at]..bounds[end]));
                }
                let taken = (at + 1 + rng.below(6)).min(last);
                let outer = bounds[at]..bounds[taken];
                match rng.below(8) {
                    0..=3 => {
                        let end = (at + 1 + rng.below(4)).min(taken);
                        let outer = bounds[at]..bounds[end];
                        if !outer.is_empty() {
                            pieces.push((source[outer.clone()].to_string(), outer));
                        }
                        at = end;
                    }
                    4 | 5 => {
                        if !outer.is_empty() {
                            pieces.push((rng.word(&INNER, 2), outer));
                        }
                        at = taken;
                    }
                    _ => {
                        pieces.push((rng.word(&INNER, 2), bounds[at]..bounds[at]));
                    }
                }
            }
            let mut text = String::new();
            let mut map = SourceMap::default();
            for (piece, outer) in &pieces {
                map.push_text(&source, outer.clone(), piece);
                text.push_str(piece);
            }
            Case {
                source,
                pieces,
                text,
                map,
            }
        }

        /// A case over a random outer text of up to forty characters.
        fn new(rng: &mut Rng) -> Case {
            let count = rng.below(41);
            let source = rng.text(&OUTER, count);
            Case::over(rng, source)
        }

        /// The offsets of the characters of the text, and its end.
        fn bounds(&self) -> Vec<usize> {
            self.text
                .char_indices()
                .map(|(at, _)| at)
                .chain([self.text.len()])
                .collect()
        }
    }

    /// The search over a list of pieces that mapped a range of the gathered text to a range of the
    /// source before `SourceMap` existed (the algorithm `Run::source_of` used), over the pieces as
    /// it kept them: empty ones too, and verbatim ones apart.
    fn old_source_of(case: &Case, range: Range<usize>) -> Range<usize> {
        let mut pieces: Vec<(usize, Range<usize>, bool)> = Vec::new();
        let mut at = 0;
        for (text, outer) in &case.pieces {
            pieces.push((at, outer.clone(), case.source[outer.clone()] == *text));
            at += text.len();
        }
        let first = &pieces[pieces.partition_point(|piece| piece.0 <= range.start) - 1];
        let last = &pieces[pieces.partition_point(|piece| piece.0 < range.end) - 1];
        let start = match first.2 {
            true => first.1.start + (range.start - first.0),
            false => first.1.start,
        };
        let end = match last.2 {
            true => last.1.start + (range.end - last.0),
            false => last.1.end,
        };
        start..end
    }

    /// Whether the segments of `map` tile an inner text and hold an outer one in order.
    fn assert_tiles(map: &SourceMap, context: &str) {
        let mut inner_end = 0;
        let mut outer_end = 0;
        let mut before: Option<&Segment> = None;
        for segment in map.segments() {
            assert_eq!(segment.inner.start, inner_end, "{context}: inner gap");
            assert!(!segment.inner.is_empty(), "{context}: empty inner");
            assert!(segment.outer.start >= outer_end, "{context}: outer order");
            let fits = match segment.kind {
                Verbatim => segment.outer.len() == segment.inner.len(),
                Escaped => !segment.outer.is_empty(),
                Synthetic => segment.outer.is_empty(),
            };
            assert!(fits, "{context}: {segment:?}");
            if let Some(before) = before {
                let joined = before.kind == Verbatim
                    && segment.kind == Verbatim
                    && before.outer.end == segment.outer.start;
                assert!(
                    !joined,
                    "{context}: abutting verbatim {before:?} {segment:?}"
                );
            }
            inner_end = segment.inner.end;
            outer_end = segment.outer.end;
            before = Some(segment);
        }
        assert_eq!(inner_end, map.len(), "{context}: len");
    }

    /// Whether `at` is in, or on the edge of, a segment's outer range.
    fn on_a_segment(map: &SourceMap, at: usize) -> bool {
        map.segments()
            .iter()
            .any(|segment| segment.outer.start <= at && at <= segment.outer.end)
    }

    #[test]
    fn the_map_tiles_for_any_pieces() {
        for seed in 1..=2000 {
            let case = Case::new(&mut Rng::new(seed));
            assert_tiles(&case.map, &format!("seed {seed}"));
            assert_eq!(case.map.len(), case.text.len(), "seed {seed}");
        }
    }

    #[test]
    fn to_file_agrees_with_the_search_it_replaced() {
        for seed in 1..=2000 {
            let case = Case::new(&mut Rng::new(seed));
            let bounds = case.bounds();
            for (index, &start) in bounds.iter().enumerate() {
                for &end in &bounds[index + 1..] {
                    assert_eq!(
                        case.map.to_file(start..end).range,
                        old_source_of(&case, start..end),
                        "seed {seed}, range {start}..{end}, pieces {:?}",
                        case.pieces
                    );
                }
            }
        }
    }

    #[test]
    fn editable_ranges_hold_the_bytes_asked_for() {
        for seed in 1..=2000 {
            let case = Case::new(&mut Rng::new(seed));
            if case.text.is_empty() {
                continue;
            }
            let bounds = case.bounds();
            for (index, &start) in bounds.iter().enumerate() {
                for &end in &bounds[index..] {
                    let mapped = case.map.to_file(start..end);
                    let segments = case.map.segments();
                    let inside_one = match start == end {
                        true => segments
                            .iter()
                            .rev()
                            .find(|segment| segment.inner.start <= start)
                            .is_some_and(|segment| segment.kind == Verbatim),
                        false => segments.iter().any(|segment| {
                            segment.kind == Verbatim
                                && segment.inner.start <= start
                                && end <= segment.inner.end
                        }),
                    };
                    let context = format!("seed {seed}, range {start}..{end}");
                    assert_eq!(mapped.editable, inside_one, "{context}");
                    if mapped.editable {
                        assert_eq!(
                            &case.source[mapped.range],
                            &case.text[start..end],
                            "{context}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn ends_lie_on_segments_and_keep_their_order() {
        for seed in 1..=2000 {
            let case = Case::new(&mut Rng::new(seed));
            if case.text.is_empty() {
                continue;
            }
            let bounds = case.bounds();
            let mut before = 0;
            for (index, &start) in bounds.iter().enumerate() {
                let point = case.map.to_file(start..start).range.start;
                let context = format!("seed {seed}, start {start}");
                assert!(on_a_segment(&case.map, point), "{context}");
                assert!(point >= before, "{context}: out of order");
                before = point;
                let mut ended = point;
                for &end in &bounds[index + 1..] {
                    let range = case.map.to_file(start..end).range;
                    let context = format!("seed {seed}, range {start}..{end}");
                    assert_eq!(range.start, point, "{context}: start rule");
                    assert!(on_a_segment(&case.map, range.end), "{context}");
                    assert!(range.end >= ended, "{context}: out of order");
                    ended = range.end;
                }
            }
        }
    }

    #[test]
    fn a_range_that_touches_an_escaped_segment_covers_it() {
        for seed in 1..=2000 {
            let case = Case::new(&mut Rng::new(seed));
            let bounds = case.bounds();
            for (index, &start) in bounds.iter().enumerate() {
                for &end in &bounds[index + 1..] {
                    let range = case.map.to_file(start..end).range;
                    let touched = case.map.segments().iter().filter(|segment| {
                        segment.kind == Escaped
                            && segment.inner.start < end
                            && start < segment.inner.end
                    });
                    for segment in touched {
                        assert!(
                            range.start <= segment.outer.start && segment.outer.end <= range.end,
                            "seed {seed}, range {start}..{end} maps to {range:?}, not over {segment:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn to_inner_reads_the_text_between_two_stretches_as_the_break_that_parts_them() {
        // `ab\ncd`, as two lines `// ab` and `// cd` hold it.
        let map = map_of(&[
            (Verbatim, 2, 3..5),
            (Synthetic, 1, 5..5),
            (Verbatim, 2, 9..11),
        ]);

        assert_eq!(map.to_inner(3..5), 0..2);
        assert_eq!(map.to_inner(4..5), 1..2);
        assert_eq!(map.to_inner(5..9), 2..3);
        assert_eq!(map.to_inner(4..10), 1..4);
        assert_eq!(map.to_inner(3..11), 0..5);
        // A range in the gap, or at a point, does not hold text.
        assert_eq!(map.to_inner(6..8), 3..3);
        assert_eq!(map.to_inner(6..6), 3..3);
        assert_eq!(map.to_inner(0..3), 0..0);
        assert_eq!(map.to_inner(11..14), 5..5);
    }

    #[test]
    fn to_inner_takes_an_escape_whole_or_not_at_all() {
        let map = map_of(&[(Verbatim, 1, 0..1), (Escaped, 1, 1..6), (Verbatim, 1, 6..7)]);

        assert_eq!(map.to_inner(0..7), 0..3);
        assert_eq!(map.to_inner(1..6), 1..2);
        assert_eq!(map.to_inner(0..5), 0..1);
        assert_eq!(map.to_inner(2..7), 2..3);
    }

    #[test]
    fn to_inner_holds_the_range_it_came_from() {
        for seed in 1..=2000 {
            let case = Case::new(&mut Rng::new(seed));
            if case.text.is_empty() {
                continue;
            }
            let bounds = case.bounds();
            for (index, &start) in bounds.iter().enumerate() {
                for &end in &bounds[index + 1..] {
                    let segments = case.map.segments();
                    let in_synthetic = |segment: &Segment| segment.kind == Synthetic;
                    if (segments.iter())
                        .any(|s| in_synthetic(s) && s.inner.start < end && end <= s.inner.end)
                    {
                        continue;
                    }
                    let mapped = case.map.to_file(start..end);
                    let back = case.map.to_inner(mapped.range.clone());
                    let context = format!(
                        "seed {seed}, range {start}..{end} maps to {:?}",
                        mapped.range
                    );
                    assert!(
                        back.start <= start && end <= back.end,
                        "{context}: {back:?}"
                    );
                    let point_at_start = segments.iter().any(|s| {
                        in_synthetic(s)
                            && s.outer.start == mapped.range.start
                            && s.inner.end <= start
                    });
                    if mapped.editable && !point_at_start {
                        assert_eq!(back, start..end, "{context}");
                    }
                }
            }
        }
    }

    #[test]
    fn compose_holds_what_mapping_twice_gives() {
        for seed in 1..=500 {
            let mut rng = Rng::new(seed);
            let inner = Case::new(&mut rng);
            if inner.text.is_empty() {
                continue;
            }
            let outer = Case::over(&mut rng, inner.text.clone());
            let composed = outer.map.compose(&inner.map);
            let context = format!("seed {seed}");
            assert_tiles(&composed, &context);
            assert_eq!(composed.len(), outer.map.len(), "{context}");
            let bounds = outer.bounds();
            for (index, &start) in bounds.iter().enumerate() {
                for &end in &bounds[index + 1..] {
                    let first = outer.map.to_file(start..end);
                    let second = inner.map.to_file(first.range.clone());
                    let twice = second.range.clone();
                    let once = composed.to_file(start..end);
                    let context = format!("seed {seed}, range {start}..{end}");
                    assert!(
                        once.range.start <= twice.start && twice.end <= once.range.end,
                        "{context}: {:?} does not hold {twice:?}",
                        once.range
                    );
                    if first.editable && second.editable {
                        assert!(once.editable, "{context}: both steps are editable");
                    }
                    if once.editable {
                        assert_eq!(once.range, twice, "{context}");
                        assert_eq!(
                            &inner.source[once.range],
                            &outer.text[start..end],
                            "{context}"
                        );
                    }
                }
            }
        }
    }
}
