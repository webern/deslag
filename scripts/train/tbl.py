"""Transformation-based learning for tags: the template set, the rules, the corpus a rule is
applied to, and the indexed trainer. Standard library only.

NOTICE: the template list below is adapted from NLTK, <https://github.com/nltk/nltk>, Copyright
(C) 2001-2026 NLTK Project, licensed under the Apache License, Version 2.0
(<http://www.apache.org/licenses/LICENSE-2.0>); a copy as NLTK ships it is
`LICENSES/Apache-2.0-NLTK.txt`. It is `fntbl37()` of `nltk/tag/brill.py` at commit
350c1c70948fda15ba8db1bea973513d33b2c187, rewritten as plain tuples, with the two templates the
list repeats dropped. The rest of this file is not NLTK's code: it follows the design of
`nltk/tag/brill_trainer.py` and `nltk/tbl/` and is written afresh.

A rule is "change tag A to tag B at a token when every condition holds". A condition is a word or
a tag at one position, or at any of a short list of positions, relative to the token. A template
says which conditions a rule may use: it is a list of such features, and a rule is the template
with a value filled in for each feature. The template set is NLTK's `fntbl37` (nltk/tag/brill.py,
Apache-2.0; see ACKNOWLEDGEMENTS.md), which is the template list of the fnTBL distribution minus
the templates that do not condition on the tag at position 0. A feature with several positions
holds when some in-range position has the value. A position outside the sentence never holds.

Rules are applied in order, each one left to right over the whole sentence, so a token sees the
change an earlier token just got from the same rule. `apply` is the one place that does it, for
training and for tagging alike.

The trainer is the indexed one of NLTK's `BrillTaggerTrainer` (nltk/tag/brill_trainer.py, the same
licence) in a counting form, which is how fnTBL does it. Under the tags as they stand, a candidate
rule `A -> B if C` scores `fixed - broken`. `fixed` counts the tokens tagged A, with condition C,
whose right tag is B, and `broken` the tokens tagged A with C whose right tag is A. The first is
kept per (C, A, B) and the second per (C, A), each by counting the conditions every token offers,
so a tag change at one token only recounts the tokens near it, the neighbourhood the templates
reach. NLTK scores all of a rule's matches at once, as if the tags did not change while it is
applied; the rule itself is applied left to right. So a chosen rule is applied for real, its true
gain counted, and a rule whose true gain is under the minimum is put back and skipped.

A token may be fixed, or limited in what it may become: a start tagger that stands behind a word
freezes it, and one that has ruled tags out lists the rest. `allowed` is a bitmask of tags per
token, 0 for a fixed token, and a rule only changes a token to a tag it allows. So a rule breaks
only the right tokens that allow its new tag, and the trainer counts those per (condition, old tag,
new tag). A token with no right tag (`NO_GOLD`) is context only: a rule changes it as it would
at tagging time, but its gain is never counted. The masks never change, so the index stays valid.
"""

from collections import namedtuple

WORD, TAG = 0, 1
NO_GOLD = -1  # the right tag of a token no gold word is aligned to

# NLTK's fntbl37, in its order; a feature is (kind, positions). The list holds two templates twice
# in other words, so `TEMPLATES` drops a template whose features are another's, keeping the first.
_FNTBL37 = (
    ((WORD, (0,)), (WORD, (1,)), (WORD, (2,))),
    ((WORD, (-1,)), (WORD, (0,)), (WORD, (1,))),
    ((WORD, (0,)), (WORD, (-1,))),
    ((WORD, (0,)), (WORD, (1,))),
    ((WORD, (0,)), (WORD, (2,))),
    ((WORD, (0,)), (WORD, (-2,))),
    ((WORD, (1, 2)),),
    ((WORD, (-2, -1)),),
    ((WORD, (1, 2, 3)),),
    ((WORD, (-3, -2, -1)),),
    ((WORD, (0,)), (TAG, (2,))),
    ((WORD, (0,)), (TAG, (-2,))),
    ((WORD, (0,)), (TAG, (1,))),
    ((WORD, (0,)), (TAG, (-1,))),
    ((WORD, (0,)),),
    ((WORD, (-2,)),),
    ((WORD, (2,)),),
    ((WORD, (1,)),),
    ((WORD, (-1,)),),
    ((TAG, (-1,)), (TAG, (1,))),
    ((TAG, (1,)), (TAG, (2,))),
    ((TAG, (-1,)), (TAG, (-2,))),
    ((TAG, (1,)),),
    ((TAG, (-1,)),),
    ((TAG, (-2,)),),
    ((TAG, (2,)),),
    ((TAG, (1, 2, 3)),),
    ((TAG, (1, 2)),),
    ((TAG, (-3, -2, -1)),),
    ((TAG, (-2, -1)),),
    ((TAG, (1,)), (WORD, (0,)), (WORD, (1,))),
    ((TAG, (1,)), (WORD, (0,)), (WORD, (-1,))),
    ((TAG, (-1,)), (WORD, (-1,)), (WORD, (0,))),
    ((TAG, (-1,)), (WORD, (0,)), (WORD, (1,))),
    ((TAG, (-2,)), (TAG, (-1,))),
    ((TAG, (1,)), (TAG, (2,))),
    ((TAG, (1,)), (TAG, (2,)), (WORD, (1,))),
)


def _unique(templates):
    seen, out = set(), []
    for features in templates:
        canonical = tuple(sorted(features))
        if canonical not in seen:
            seen.add(canonical)
            out.append(features)
    return tuple(out)


TEMPLATES = _unique(_FNTBL37)
SLOTS = 3  # the most features of any template

# Rule(template, values, orig, repl): `values` has one value per feature of the template, a word
# id or a tag index; the ids are the vocabulary's of whoever made the rule.
Rule = namedtuple("Rule", "template values orig repl")

# What the trainer logs for each rule it keeps.
Step = namedtuple("Step", "rule estimate gain fired fixed broken errors")


def matcher(rule, template_set=TEMPLATES):
    """The test `fn(words, tags, g, lo, hi)` of whether the rule's conditions hold at index g of
    the flat arrays, for a sentence in [lo, hi). The tag at g itself is not tested."""
    conditions = [
        (words_or_tags, positions, value)
        for (words_or_tags, positions), value in zip(template_set[rule.template], rule.values)
    ]
    single = all(len(positions) == 1 for _, positions, _ in conditions)
    if single:
        flat = tuple((k == WORD, positions[0], value) for k, positions, value in conditions)

        def fn(words, tags, g, lo, hi):
            for is_word, offset, value in flat:
                j = g + offset
                if j < lo or j >= hi or (words if is_word else tags)[j] != value:
                    return False
            return True

        return fn

    def general(words, tags, g, lo, hi):
        for kind, positions, value in conditions:
            array = words if kind == WORD else tags
            for offset in positions:
                j = g + offset
                if lo <= j < hi and array[j] == value:
                    break
            else:
                return False
        return True

    return general


class Corpus:
    """Sentences as flat arrays: `words` (ids), `tags` (indices), `allowed` (a mask per index), and
    for each index its sentence's [lo, hi). `by_tag[t]` holds the indices tagged t that a rule may
    change; a token whose mask is 0 is in none."""

    def __init__(self, sentences, ntags):
        """`sentences` is a list of (word ids, tag indices) or (word ids, tag indices, masks); with
        no masks every token may become any tag."""
        self.full = (1 << ntags) - 1
        self.words, self.tags, self.allowed, self.lo, self.hi = [], [], [], [], []
        self.starts = []
        for sentence in sentences:
            words, tags = sentence[0], sentence[1]
            masks = sentence[2] if len(sentence) > 2 and sentence[2] is not None else None
            start = len(self.words)
            self.starts.append(start)
            self.words.extend(words)
            self.tags.extend(tags)
            self.allowed.extend([self.full] * len(words) if masks is None else masks)
            self.lo.extend([start] * len(words))
            self.hi.extend([start + len(words)] * len(words))
        self.by_tag = [set() for _ in range(ntags)]
        for g, tag in enumerate(self.tags):
            if self.allowed[g]:
                self.by_tag[tag].add(g)

    def apply(self, rule, test=None):
        """Applies `rule` left to right everywhere; returns the indices it changed, in order."""
        test = test or matcher(rule)
        tags, words, lo, hi, allowed = self.tags, self.words, self.lo, self.hi, self.allowed
        fired = []
        for g in sorted(self.by_tag[rule.orig]):
            if allowed[g] >> rule.repl & 1 and test(words, tags, g, lo[g], hi[g]):
                tags[g] = rule.repl
                fired.append(g)
        self.move(fired, rule.orig, rule.repl)
        return fired

    def move(self, fired, old, new):
        self.by_tag[old].difference_update(fired)
        self.by_tag[new].update(fired)

    def undo(self, fired, rule):
        for g in fired:
            self.tags[g] = rule.orig
        self.move(fired, rule.repl, rule.orig)


def apply_sentence(rules, words, tags, notify=None, allowed=None):
    """Applies compiled `rules`, [(orig, repl, fn)] in order, to one sentence's word ids and tag
    indices, in place. `allowed`, if given, is a mask per token of the tags it may become. `notify(
    rule number, i, orig, repl)` is called on each change."""
    n = len(words)
    counts = {}
    for tag in tags:
        counts[tag] = counts.get(tag, 0) + 1
    for number, (orig, repl, fn) in enumerate(rules):
        if not counts.get(orig):
            continue
        for i in range(n):
            if tags[i] == orig and (allowed is None or allowed[i] >> repl & 1) and \
                    fn(words, tags, i, 0, n):
                tags[i] = repl
                counts[orig] -= 1
                counts[repl] = counts.get(repl, 0) + 1
                if notify is not None:
                    notify(number, i, orig, repl)
                if not counts[orig]:
                    break


class Trainer:
    """The indexed trainer. `corpus` holds the tags as they are, which the trainer changes as it
    keeps rules; `gold` is the right tag of each index, or NO_GOLD; `vocab` is the number of word
    ids. Only a token with a gold tag and a rule that may change it is counted."""

    def __init__(self, corpus, gold, vocab, ntags):
        self.corpus = corpus
        self.gold = gold
        self.ntags = ntags
        self.base = max(vocab, ntags) + 1
        self.cube = self.base**SLOTS
        self.fixed = {}  # (condition, orig, repl) -> count
        # What a rule would break, per (condition, orig): the counted tokens that are right already
        # and may become any tag, and per (condition, orig, repl): those that may become repl.
        self.right_all = {}
        self.right = {}
        self.members = {}  # mask -> the tags it holds
        self.tag_templates = [
            n for n, features in enumerate(TEMPLATES) if any(k == TAG for k, _ in features)
        ]
        self.all_templates = list(range(len(TEMPLATES)))
        self.index_all(range(len(corpus.tags)), +1, self.all_templates)

    # The conditions every token offers.

    def conditions(self, template, g):
        """The packed conditions of `template` at index g: one number per way the template's
        features can be filled in there; none if a feature has no position in the sentence."""
        corpus = self.corpus
        lo, hi = corpus.lo[g], corpus.hi[g]
        base = self.base
        packed = [template]
        for kind, positions in TEMPLATES[template]:
            array = corpus.words if kind == WORD else corpus.tags
            if len(positions) == 1:
                j = g + positions[0]
                if j < lo or j >= hi:
                    return ()
                packed = [p * base + array[j] for p in packed]
            else:
                values = {array[g + o] for o in positions if lo <= g + o < hi}
                if not values:
                    return ()
                packed = [p * base + v for p in packed for v in values]
        for _ in range(SLOTS - len(TEMPLATES[template])):
            packed = [p * base for p in packed]
        return packed

    def index_all(self, indices, sign, templates):
        corpus = self.corpus
        tags, gold, allowed, full = corpus.tags, self.gold, corpus.allowed, corpus.full
        fixed, right_all, right, ntags = self.fixed, self.right_all, self.right, self.ntags
        for g in indices:
            cur, truth, mask = tags[g], gold[g], allowed[g]
            if truth == NO_GOLD or not mask:
                continue
            if cur != truth:
                if not mask >> truth & 1:
                    continue  # no rule may give this token its right tag
            elif mask != full:
                members = self.members.get(mask)
                if members is None:
                    members = self.members[mask] = [t for t in range(ntags) if mask >> t & 1]
            for template in templates:
                for condition in self.conditions(template, g):
                    base = condition * ntags + cur
                    if cur != truth:
                        key, table = base * ntags + truth, fixed
                        self._add(table, key, sign)
                    elif mask == full:
                        self._add(right_all, base, sign)
                    else:
                        for repl in members:
                            if repl != cur:
                                self._add(right, base * ntags + repl, sign)

    @staticmethod
    def _add(table, key, sign):
        count = table.get(key, 0) + sign
        if count:
            table[key] = count
        else:
            table.pop(key, None)

    def reindex(self, changed, change):
        """Recounts what the tokens near `changed` offer, around `change()`, which changes tags."""
        near = set()
        corpus = self.corpus
        for g in changed:
            for j in range(max(corpus.lo[g], g - 3), min(corpus.hi[g], g + 4)):
                near.add(j)
        own = set(changed)
        around = near - own
        self.index_all(sorted(own), -1, self.all_templates)
        self.index_all(sorted(around), -1, self.tag_templates)
        change()
        self.index_all(sorted(own), +1, self.all_templates)
        self.index_all(sorted(around), +1, self.tag_templates)

    # Choosing a rule.

    def decode(self, key):
        """(template, values, orig, repl) of a packed `fixed` key."""
        ntags = self.ntags
        key, repl = divmod(key, ntags)
        condition, orig = divmod(key, ntags)
        values = []
        rest = condition
        for _ in range(SLOTS):
            rest, value = divmod(rest, self.base)
            values.append(value)
        template = rest
        values.reverse()
        return Rule(template, tuple(values[: len(TEMPLATES[template])]), orig, repl)

    def order(self, scored):
        """The fixed order of equal scores: lowest template number, then the original tag, the new
        tag, and the values (a word's id is its place in the sorted vocabulary)."""
        score, key = scored
        rule = self.decode(key)
        return (-score, rule.template, rule.orig, rule.repl, rule.values)

    def best(self, threshold, skip):
        """The candidates scoring at least `threshold` and not in `skip`, best first, as
        (score, key); the ones of equal score are in the fixed order."""
        fixed, right_all, right, ntags = self.fixed, self.right_all, self.right, self.ntags
        found = []
        for key, count in [item for item in fixed.items() if item[1] >= threshold]:
            score = count - right_all.get(key // ntags, 0) - right.get(key, 0)
            if score >= threshold and key not in skip:
                found.append((score, key))
        found.sort(key=lambda item: -item[0])
        ordered, start = [], 0
        while start < len(found):
            end = start
            while end < len(found) and found[end][0] == found[start][0]:
                end += 1
            group = found[start:end]
            if len(group) > 1:
                group.sort(key=self.order)
            ordered.extend(group)
            start = end
        return ordered

    def errors(self):
        tags, gold = self.corpus.tags, self.gold
        return sum(1 for a, b in zip(tags, gold) if b != NO_GOLD and a != b)

    def learn(self, cap, min_gain, on_rule=None):
        """Keeps up to `cap` rules of true gain at least `min_gain`; returns the Steps."""
        steps = []
        corpus, gold = self.corpus, self.gold
        errors = self.errors()
        threshold = max(self.fixed.values(), default=0)
        while len(steps) < cap:
            skip = set()
            chosen = None
            threshold = max(threshold, min_gain)
            while chosen is None:
                found = self.best(threshold, skip)
                for score, key in found:
                    rule = self.decode(key)
                    test = matcher(rule)
                    fired = corpus.apply(rule, test)
                    fixed = sum(1 for g in fired if gold[g] == rule.repl)
                    broken = sum(1 for g in fired if gold[g] == rule.orig)
                    if fixed - broken >= min_gain:
                        chosen = (score, key, rule, fired, fixed, broken)
                        break
                    corpus.undo(fired, rule)
                    skip.add(key)
                if chosen is None:
                    if threshold <= min_gain:
                        return steps
                    threshold = max(min_gain, threshold // 2)
            score, key, rule, fired, fixed, broken = chosen
            threshold = score
            # The rule is applied already; count the index around its changes as they were.
            corpus.undo(fired, rule)
            self.reindex(fired, lambda: corpus.apply(rule))
            errors -= fixed - broken
            step = Step(rule, score, fixed - broken, len(fired), fixed, broken, errors)
            steps.append(step)
            if on_rule is not None:
                on_rule(len(steps), step)
        return steps
