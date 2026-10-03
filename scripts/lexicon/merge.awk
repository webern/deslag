# Merges the four normalised source files that generate.sh writes into the open-class lexicon: one
# line per word, "word<TAB>readings", sorted bytewise. POSIX awk, so it runs on macOS and Linux.
#
#   awk -v size=60 -f merge.awk scowl.tsv wordnet.tsv moby.tsv agid.tsv
#
# The inputs, one record per line, tab separated:
#   scowl.tsv    form, size level, kind (w: a word, u: a name or other capitalised word)
#   wordnet.tsv  lemma, pos (n v a r, or P for an instance name), SemCor count, sense count
#   moby.tsv     word, Moby's tag letters in priority order, 1 if the entry is capitalised
#   agid.tsv     form, lemma, pos (N V A), slot (pl past pp pastpp ing s cmp sup)
#
# A form is in the lexicon when SCOWL lists it at `size` or below, and it has at least one reading
# from the others. A form gets a reading for each of these:
#   - itself, as a lemma WordNet or Moby lists;
#   - an inflection AGID gives of a lemma that WordNet or Moby confirms with that part of speech;
#   - a name, from a capitalised Moby entry, a WordNet instance, or SCOWL's list of capitalised words.
#
# Readings are ranked per tag, then the tags among themselves, by this key, most important first:
#   1. the SemCor count of the lemma for that part of speech, spread over the forms the part of
#      speech has: how often its senses were met in WordNet's sense-tagged corpus, the only real
#      frequency any source gives. The count is the lemma's, summed over its senses and its forms
#      (`use`, `uses`, `used`, `using` all add to the verb `use`), so it says how often the lemma is
#      that part of speech, not how often one form is. A form can only be one of a part of speech's
#      forms, so the count is divided by how many it has, as if the lemma met them equally often:
#      2 for a noun (singular, plural), 4 for a verb (base, -s, past, -ing), 1 for the rest. Else a
#      verb would outrank a noun for the count of its extra forms alone. The divisors come from
#      the grammar, not from a corpus;
#   2. where Moby puts that part of speech among the lemma's: Moby lists them in priority order,
#      the principal usage first, and a part of speech it does not list comes last;
#   3. the WordNet sense count;
#   4. a fixed order of tags: noun, verb, adjective, adverb, then the rest, names last.
# A form inflected from a lemma takes the lemma's counts and place, so `used` ranks as a verb
# by how often `use` is one. A name has no evidence of its own, so it ranks last unless it is the
# only reading.
#
# The features of a tag come from one reading of it, not from the key: an inflection before the
# form listed as a lemma, because Moby and WordNet list `using` and `better` as lemmas but
# AGID says which form of `use` and `good` they are.
#
# The line is the tags in rank order as lowercase letters, and after the first an uppercase letter
# for its features, taken from the best reading of that tag:
#   tags     n noun, p proper noun, v verb, a adjective, r adverb, q pronoun, d determiner,
#            i adposition, c conjunction, j interjection
#   features S singular, P plural, I infinitive, D finite past, E past participle, G present
#            participle, Z third person singular present, O positive, C comparative, T superlative
# So `runs<TAB>vZn` is a verb, finite present third person singular, that may also be a noun.

BEGIN {
    FS = "\t"
    OFS = "\t"
    if (size == "") {
        print "merge.awk: pass -v size=N" > "/dev/stderr"
        exit 2
    }
    # The tag each letter of the output stands for, and its place in the fixed order of tags.
    split("n v a r q d i c j p", order, " ")
    for (i = 1; i <= 10; i++) rank[order[i]] = i
    # Moby's tag letters: the letter of the tag they give, and the features of the lemma.
    mtag["N"] = "n"; mfeat["N"] = "S"
    mtag["h"] = "n"; mfeat["h"] = "S"
    mtag["p"] = "n"; mfeat["p"] = "P"
    mtag["V"] = "v"; mfeat["V"] = "I"
    mtag["t"] = "v"; mfeat["t"] = "I"
    mtag["i"] = "v"; mfeat["i"] = "I"
    mtag["A"] = "a"; mfeat["A"] = "O"
    mtag["v"] = "r"; mfeat["v"] = "O"
    mtag["C"] = "c"; mfeat["C"] = ""
    mtag["P"] = "i"; mfeat["P"] = ""
    mtag["!"] = "j"; mfeat["!"] = ""
    mtag["r"] = "q"; mfeat["r"] = ""
    mtag["D"] = "d"; mfeat["D"] = ""
    mtag["I"] = "d"; mfeat["I"] = ""
    # How a slot of AGID is read: the tag letter's features, and the slot's place among a form's
    # readings of one tag when the keys tie: the lemma first.
    sfeat["pl"] = "P"; sprio["pl"] = 1
    sfeat["past"] = "D"; sprio["past"] = 2
    sfeat["pp"] = "E"; sprio["pp"] = 3
    sfeat["pastpp"] = "D"; sprio["pastpp"] = 2
    sfeat["ing"] = "G"; sprio["ing"] = 4
    sfeat["s"] = "Z"; sprio["s"] = 5
    sfeat["cmp"] = "C"; sprio["cmp"] = 6
    sfeat["sup"] = "T"; sprio["sup"] = 7
    nslots["n"] = 2; nslots["v"] = 4; nslots["a"] = 1; nslots["r"] = 1
    nslots["q"] = 1; nslots["d"] = 1; nslots["i"] = 1; nslots["c"] = 1; nslots["j"] = 1; nslots["p"] = 1
    wn2tag["n"] = "n"; wn2tag["v"] = "v"; wn2tag["a"] = "a"; wn2tag["r"] = "r"
    wn2feat["n"] = "S"; wn2feat["v"] = "I"; wn2feat["a"] = "O"; wn2feat["r"] = "O"
    nforms = 0
}

# A reading of `form` as tag `t` with features `f`, from a lemma with these WordNet counts, and
# Moby's place for the tag, and the priority of the slot it comes from: 1 to 7 for an inflection,
# 8 for the form listed as a lemma. Each tag of a form keeps the best key of its readings, which
# ranks it, and the features of the reading with the best priority, an inflection before a lemma
# entry: Moby lists `using` as a verb, but it is the -ing form of `use` that says which.
function add(form, t, f, count, senses, mrank, prio,    k, key) {
    if (!(form in gate)) return
    count = count * (12 / nslots[t])
    key = (count > 999999 ? 999999 : count) * 100000000 + (9 - mrank) * 1000000 + \
        (senses > 999 ? 999 : senses) * 1000 + (99 - rank[t])
    if (t == "p") key = 0
    k = form SUBSEP t
    if (!(k in best)) {
        if (!(form in seen)) {
            seen[form] = 1
            forms[++nforms] = form
        }
        tagsof[form] = tagsof[form] t
        best[k] = f
        bestprio[k] = prio
        bestkey[k] = key
        featkey[k] = key
        return
    }
    if (prio < bestprio[k] || (prio == bestprio[k] && key > featkey[k])) {
        best[k] = f
        bestprio[k] = prio
        featkey[k] = key
    }
    if (key > bestkey[k]) bestkey[k] = key
}

# The place Moby gives tag `t` among the lemma's tags, 0 first, 9 when it does not give it.
function moby_rank(lemma, t,    s, i, n, c, seen_t, place) {
    s = mtags[lemma]
    place = 0
    seen_t = ""
    n = length(s)
    for (i = 1; i <= n; i++) {
        c = substr(s, i, 1)
        if (!(c in mtag)) continue
        if (index(seen_t, mtag[c]) > 0) continue
        if (mtag[c] == t) return place
        seen_t = seen_t mtag[c]
        place++
    }
    return 9
}

FILENAME ~ /scowl\.tsv$/ {
    if ($2 + 0 > size + 0) next
    if (!($1 in gate)) gate[$1] = $2
    if ($3 == "u") upper[$1] = 1
    next
}

FILENAME ~ /wordnet\.tsv$/ {
    if ($2 == "P") {
        wnproper[$1] = 1
    } else {
        wntagged[$1 SUBSEP $2] = $3
        wnsenses[$1 SUBSEP $2] = $4
        wnlemma[$1] = 1
    }
    next
}

FILENAME ~ /moby\.tsv$/ {
    if ($3 == "1") {
        capital[$1] = capital[$1] $2
    } else {
        mtags[$1] = mtags[$1] $2
        mlemma[$1] = 1
    }
    next
}

FILENAME ~ /agid\.tsv$/ {
    agid[++nagid] = $0
    next
}

END {
    # The lemmas themselves.
    for (w in wnlemma) {
        for (p in wn2tag) {
            if (((w SUBSEP p) in wntagged) && (w in gate)) {
                t = wn2tag[p]
                add(w, t, wn2feat[p], tagged(w, p), senses(w, p), moby_rank(w, t), 8)
            }
        }
    }
    for (w in mlemma) {
        if (!(w in gate)) continue
        s = mtags[w]
        for (i = 1; i <= length(s); i++) {
            c = substr(s, i, 1)
            if (!(c in mtag)) continue
            t = mtag[c]
            add(w, t, mfeat[c], tagged(w, revwn(t)), senses(w, revwn(t)), moby_rank(w, t), 8)
        }
    }
    # Names.
    for (w in capital) if (w in gate) {
        s = capital[w]
        if ((index(s, "N") > 0 || index(s, "h") > 0) && !(w in mlemma) && !(w in wnlemma)) add(w, "p", "S", 0, 0, 9, 8)
    }
    for (w in wnproper) add(w, "p", "S", 0, 0, 9, 8)
    for (w in upper) add(w, "p", "S", 0, 0, 9, 8)

    # The inflections AGID gives of the lemmas the others confirm.
    for (n = 1; n <= nagid; n++) {
        split(agid[n], f, "\t")
        form = f[1]; lemma = f[2]; pos = f[3]; slot = f[4]
        if (!(form in gate)) continue
        if (pos == "N") {
            if (confirmed(lemma, "n")) inflect(form, lemma, "n", "n", slot)
        } else if (pos == "V") {
            if (confirmed(lemma, "v")) inflect(form, lemma, "v", "v", slot)
        } else {
            if (confirmed(lemma, "a")) inflect(form, lemma, "a", "a", slot)
            if (confirmed(lemma, "r")) inflect(form, lemma, "r", "r", slot)
        }
    }

    # Write each form with its tags in rank order.
    for (i = 1; i <= nforms; i++) {
        form = forms[i]
        s = tagsof[form]
        m = length(s)
        # Selection sort by key, descending; the tags of one form are few.
        out = ""
        for (j = 1; j <= m; j++) {
            pick = ""
            for (x = 1; x <= m; x++) {
                t = substr(s, x, 1)
                if (index(out, t) > 0) continue
                if (pick == "" || bestkey[form SUBSEP t] > bestkey[form SUBSEP pick] ||
                    (bestkey[form SUBSEP t] == bestkey[form SUBSEP pick] && rank[t] < rank[pick]))
                    pick = t
            }
            out = out pick
        }
        line = ""
        for (j = 1; j <= m; j++) {
            t = substr(out, j, 1)
            line = line t
            if (j == 1) line = line best[form SUBSEP t]
        }
        print form, line
    }
}

# The WordNet part of speech a tag letter stands for, or "" when WordNet has none for it.
function revwn(t) {
    if (t == "n") return "n"
    if (t == "v") return "v"
    if (t == "a") return "a"
    if (t == "r") return "r"
    return "-"
}

# Whether WordNet or Moby has `lemma` as part of speech `p` (n v a r).
function confirmed(lemma, p,    t) {
    if ((lemma SUBSEP p) in wntagged) return 1
    t = wn2tag[p]
    return (lemma in mlemma) && moby_rank(lemma, t) < 9
}

# Adds the reading of `form` as slot `slot` of `lemma`, of tag `t`.
function inflect(form, lemma, p, t, slot) {
    add(form, t, sfeat[slot], tagged(lemma, p), senses(lemma, p), moby_rank(lemma, t), sprio[slot])
}

# WordNet's SemCor count and sense count of `lemma` as part of speech `p`, 0 when it has none.
function tagged(lemma, p) {
    return ((lemma SUBSEP p) in wntagged) ? wntagged[lemma SUBSEP p] + 0 : 0
}

function senses(lemma, p) {
    return ((lemma SUBSEP p) in wnsenses) ? wnsenses[lemma SUBSEP p] + 0 : 0
}
