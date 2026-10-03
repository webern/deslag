# Part-of-speech annotation guide

How to tag the deslag gold set. It condenses the Universal Dependencies (UD) English guidelines
and adds rules for technical Markdown. Where this guide and your intuition differ, follow the
guide. Where the guide is silent, follow UD English as the English Web Treebank (EWT) applies it.

Tag each word for its use in this sentence, not its usual use. Capital letters never decide a tag.

## 1. Input and output

Each sentence comes as an id and deslag's tokens in order. Every token that is not a word (its
kind is shown: punctuation, symbol, number, code span, URL, HTML, image, footnote) gets `_`. You
tag only word tokens. Write one line per sentence: the id, a colon, then one code per token,
separated by single spaces. The number of codes must equal the number of tokens.

| Code | Tag | Code | Tag |
|---|---|---|---|
| `N` | noun | `D` | determiner |
| `PN` | proper noun | `P` | adposition (preposition) |
| `V` | verb | `C` | conjunction, coordinating or subordinating |
| `AX` | auxiliary | `T` | particle |
| `J` | adjective | `NM` | numeral |
| `R` | adverb | `I` | interjection |
| `PR` | pronoun | `X` | other, only the cases in section 5 |
| | | `_` | any token that is not a word |

Features follow the code after a dot, and only these:

- `N`, `PN`, `PR`: number, `.s` singular or `.p` plural. Always mark it, except on words with no
  number: *you, your, yours, who, whom, whose, what, which*, relative *that*, expletive *there*.
  Mass nouns are `.s` (*software*, *code*). *data* and *criteria* are `.p` unless a singular verb
  agrees. A name is `.s` unless it is plural (*Windows* `.s`, *GitHub Actions* `PN.s PN.p`).
- `V`, `AX`: verb form, always one of: `.pr` finite present (*runs*, *are*, *do*), `.pa` finite
  past (*ran*, *was*), `.fi` finite with no tense (imperatives, modals *can, could, will, would,
  should, must, may, might, shall*), `.in` infinitive (after *to*, a modal, *do*, *let*, *make*,
  *help*, causative *have*, *go*, *come*: *have the bot merge it*, *go fetch it*), `.ing` any
  *-ing* verb, `.pp` past participle (after *have* or *be*, or alone as a modifier).

A script turns codes into CoNLL-U: `C` becomes `CCONJ` for *and, or, but, nor, yet, plus* and
paired *both, either, neither*, else `SCONJ`; `.s`/`.p` become `Number=Sing|Plur`; `.pr`/`.pa`
`VerbForm=Fin|Tense=Pres|Past`; `.fi` `VerbForm=Fin`; `.in` `VerbForm=Inf`; `.ing` `VerbForm=Ger`;
`.pp` `VerbForm=Part|Tense=Past`. A `_` becomes `PUNCT`, `SYM` or `NUM` by token kind, and `X`
for code, URLs, HTML, images and footnotes. None of those non-word tokens is scored.
Adjudication uses the same codes and rules.

## 2. The tags

- **N** a common noun: a thing, person, idea, or action named as a thing. Test: takes *the* and
  a plural. A noun modifying a noun stays `N` (*error codes*, *user input*, *default value*), and
  so does a bare verb (*a skip marker* `D N.s N.s`). *today, tomorrow, yesterday, tonight* are
  always `N.s`. *thanks*, *thx* are `N.p`.
- **PN** a name of a specific person, company, product, project, tool, language, format,
  protocol, place or file: *Rust, Docker, GitHub, Python, JSON, HTTP, Markdown, npm, cargo,
  README.md*. Every noun and adjective inside a multiword name is `PN` (*Visual Studio Code*,
  *Star Trek*); function words inside it keep their tags (*Bank of America* `PN.s P PN.s`). Lower
  case does not stop a name (*npm*). Title Case does not make one (*Data Structures* `N.s N.p`).
  Adjectives from names are `J` (*European*, *English words*).
- **V** a main verb, including gerunds and participles that act as verbs, and verb forms used
  like prepositions (*including*, *following* meaning "after", *regarding, given, based on*).
- **AX** *be* in every use (copula *is green*, passive *was deleted*, progressive *is running*)
  except existential *there is/are* (`V`). *have* only in the perfect (*has run*); *have* a thing
  and *have to* are `V`. *do* only in questions, negation and emphasis (*does not*, *Do you*);
  *do the work* is `V`. *get* only in the passive (*got deleted*). The modals listed in section
  1, and *ought*; *need*, *dare* only before a bare verb (*need not*).
- **J** describes a noun or is predicated of it (*a fast build*, *it is fast*); ordinals (*first,
  3rd*, *last*) with a noun or alone (*the second fails*); quantity words *many, few, fewer,
  several, much, more, most, less, least, enough, other, same, own, such*, before a noun or
  standing alone (*want more*, *more of it*). *more, most, less, least* modifying an adjective or
  verb are `R` (*least likely*); so is an ordinal modifying a verb or clause (*First, clone it*;
  *ask first*).
- **R** modifies a verb, adjective, adverb or clause: *also, only, just, very, yet (not yet),
  then, here, there* (place), *e.g., i.e.*, *once, twice*. *how, why, when, whenever, where,
  wherever* are `R` in every use, even introducing a clause (*When it starts*), as in EWT.
  *back, away, forward, together, apart, ahead* after a verb are `R`.
- **PR** *I, me, my, mine, myself* and the rest of the personal set, possessives included (*my,
  its, their* are `PR`, not `D`); *who, whom, whose*; *something, anyone, nothing, none*;
  expletive *there*; *this/that/these/those, which, what* when they stand alone; relative *that*.
- **D** *a, an, the, every, no*; *this, that, these, those, which, what, whatever* before their
  noun; *all, both, each, some, any, either, neither, another, half*, before a noun or alone
  (*some of them* `D`).
- **P** links a noun phrase: *in, of, to* (before a noun), *for, via, per, vs, than* (before a
  noun phrase), *like* (*tools like X*). Phrasal-verb particles are `P`: *set up, log in, turn
  off, find out*.
- **C** coordinators *and, or, but, nor, yet, plus*; subordinators *that* (content clause), *if,
  whether, because, although, though, while, unless, until, since, as, once, so that*; *for*
  opening a clause with its own subject and *to* (*for it to work*). *so*
  joining two clauses mid-sentence is `C`; *So,* opening a sentence is `R`.
- **T** only: *not*, the infinitive *to*, and a possessive *'s* or *s* that is its own token.
- **NM** cardinal numbers as words (*one, two, hundred*), Roman numerals, version labels written
  as one word (*v1.2*, *v13*).
- **I** *yes, no* (as answers), *OK* (as an answer or discourse marker), *please, hello, oh*.

## 3. Hard calls

- **that**: before its noun `D` (*that file*); relative, where *which* fits, `PR` (*the file
  that changed*); introducing a clause, where it can be dropped, `C` (*says that it works*);
  alone `PR` (*that works*).
- **before, after, since, until, as, than**: `P` before a noun phrase, `C` before a clause,
  including an *-ing* clause (*after running it*); *by, for, of, in, without* before an *-ing*
  clause stay `P`. *as fast as*: first *as* `R`, second `P` (`C` before a clause).
- **to**: before a verb `T`; else `P`. *in order to*: `P N.s T`.
- **one**: `NM` by default (*one file*, *one of*); `PR.s` for a generic person (*one can*); `N`
  when it could be plural (*the old one*, *another one*).
- **-ing words**: `V.ing` after *be*, heading a clause (with an object, adverb or subject), after
  a preposition, after a noun it describes (*a job running in CI*), in headings (*Installing
  Rust*). `N` with a determiner, adjective or plural (*the setting*, *settings*), alone without
  object (*Logging is enabled*), after a noun that is its object (*Key Signing*, *error
  handling*), or before a noun when it means "for/of X-ing" (*parsing errors, loading spinner*).
  Before a noun when the noun does it now, `V.ing` (*running containers, failing tests*). `J`
  only when *very* or *more* fits (*interesting, confusing, misleading*), or one of *following,
  existing, missing, remaining, pending, upcoming, ongoing, underlying, corresponding, leading,
  trailing* before a noun or after a determiner with none (*The remaining fail*).
- **-ed/-en words** before a noun or after *be/seem/become*: `J` only when *very* or *more* fits
  (*detailed, advanced, limited, complicated, outdated, interested, related*); else `V.pp`
  (*generated files, is deprecated, is required, are supported*).
- **Past or participle**: *it deleted* `V.pa`; *was/has deleted*, *the deleted file*, *files
  deleted by X* `V.pp`. An *-ed* word opening a fragment (changelog, list item, cell) is `V.pa`
  with an object (*Renamed the flag*, *Dropped a check*), else `V.pp` (*Saved to disk*).
  **Present or infinitive**: *they run* `V.pr`; *to run, can run, does not run, let it run*
  `V.in`; *Run the tests* `V.fi`; *Don't run it*: *Don't* `AX.fi`.
- **Adjective or noun modifier**: `J` if it compares or takes *very* or is normally an adjective
  (*main, custom, local, public, real*); else `N` (*default, test, source*).
- **Adverb without -ly**: modifying a verb, `R` (*runs fast*); modifying a noun, `J`.
- **Multiword**: *such as* `J P`; *due to* `J P`; *because of* `C P`; *instead of, rather than*
  `R P`; *as well as* `R R P`; *each other* `D J`; *no one* `D PR.s`; *not only* `T R`; *of
  course* `P N.s`; *at least* `P J`; *more/less/fewer than* before a number `J P`; *up to* before
  a quantity `R P`; *up to date* `R P N.s`.

## 4. Technical text and Markdown

- **Formatting** (emphasis, links, headings, list items, table cells, quotes) never changes a
  tag; tag the words inside as usual. List markers and table pipes are not tokens.
- **Fragments** (headings, cells, labels): tag as the phrase they shorten. Noun-phrase headings
  get noun-phrase tags; *Install the CLI* `V.fi D N.s`; *Waits for web* `V.pr P N.s`; *Getting
  Started* `V.ing V.pp`. Labels: *Why:* `R`, *Note:* `N.s`, *Default* `N.s`; *Yes/No* in a cell
  `I`. A verb-phrase label is a command (*Clear history* `V.fi N.s`).
  Letter-plus-digit labels (*Q3*) and capitalised section labels (*DRAFT/FINAL*) are `N.s`.
- **Sentence cuts**: a sentence may start or stop at a wrong place (after *e.g.*). Tag what is
  there.
- **Identifiers outside code spans** (commands, file names, paths, flags, variables, packages):
  `PN.s` for each word token (*foo_bar, userId, riscv64, main.rs, docs:setup*; *run cargo build*
  `V.fi PN.s PN.s`; *src/main.rs* `PN.s _ PN.s`). An ordinary word in its ordinary sense stays
  normal (*the build failed*). Inside a code span the whole span is one `_` token. In a line
  of source code outside a code span, and for code keywords in prose (*SWITCH/CASE*), every word
  token is `PN.s` (*export default App*).
- **Slugs**: hyphen-joined words naming a file, skill, package, repository or section, where
  English would write spaces, are identifiers: every word piece `PN.s` (*the
  quick-start-checklist skill*, *07-auth-token-refresh*, *acme-as-service*).
- **Acronyms**: the tag of what they stand for. Names `PN` (*AWS, JSON, HTTP, SQL, HTML*);
  common things `N` (*API, CLI, SDK, URL, PR, CI, LLM, UI, ID, OS*); plural *APIs* `N.p`.
  Abbreviations of one word take that word's tag (*config, repo, info* `N`; *approx* `R`). An
  abbreviation split into tokens tags each piece as its word (*T/F* "true/false" `J _ J`).
- **Contractions** are one token. Tag the first part, with its features: *don't, doesn't, isn't*
  `AX.pr` (`AX.fi` in a command: *Don't run it*); *didn't* `AX.pa`; *can't, won't, cannot*
  `AX.fi`; *it's, I'm, that's* `PR.s`; *we'll, they're* `PR.p`; *you're, there's* `PR`; *let's*
  `V.fi`; *user's* `N.s`; *Python's* `PN.s`.
- **Hyphenated words** are split into pieces with `-` between. Tag each piece by its role inside
  the compound: *well-known* `R _ V.pp`; *state-of-the-art* `N.s _ P _ D _ N.s`; *full-text* `J _
  N.s`; *self-hosted* `N.s _ V.pp`; *LLM-written* `N.s _ V.pp`; *3rd-party* `J _ N.s`; an adverb
  piece is `R` (*down-sync* `R _ N.s`). A verb piece takes the feature of its form: base `.in`,
  *-ed* `.pp`, *-ing* `.ing` (*fail-safe* `V.in _ J`; *drop-in* `V.in _ P`). A bound prefix
  piece is `X` (*re, non, pre, co, multi, anti, sub, semi, super, ultra, hyper, inter, mid, post,
  un, mis, e*): *to re-run* `T X _ V.in`.
- **Numbers**: digit tokens are `_`. Word tokens with digits: ordinals `J` (*1st*); decades `N.p`
  (*1990s*); number plus unit `N`, plural unless the number is 1 (*100ms* `N.p`); a unit as its
  own token likewise (*512 GB* `_ N.p`, *1 TB* `_ N.s`, bare *GB* `N.p`);
  version labels `NM` (*v1.2*), and *x* as a version wildcard (*4.x* `_ _ NM`); pre-release
  words `N.s` (*beta*, *rc1*); product names `PN` (*GPT-4o* `PN.s _ PN.s`, *C++* `PN.s _ _`).
- **Emoji** are symbol tokens, `_`. A shortcode word (*rocket* in `:rocket:`) is `X`.
- After a code span: *s* as a plural ending (`` `Vec`s ``) is `X`; *s* after an apostrophe
  (`` `foo`'s ``) is `T`.
- **Mentioned words** (*avoid "very"*) and quoted values written as ordinary words keep their
  own tag, with the features of the form written (*status "passes"*: `V.pr`; bare verb `.in`).

## 5. X, and when unsure

`X` is only for: *etc*; a bound prefix split off by a hyphen; a plural *s* after a code span;
letter labels in prose (*(a)*); emoji shortcode words; words of a stretch that is not English,
including English pieces of a non-English compound (*Kunden-Support* `X _ X`), though names in it
stay `PN` (*Colegio Arbolito*; a loanword in an English sentence gets its normal tag);
gibberish. Never use `X` because a word is hard.

A typo is tagged as the word intended, features included (*This files is*: *files* `N.s`).

When unsure, every word token still gets a real tag: apply the tests above, then choose the tag
the word most often has in technical English in that position. Between `N` and `PN` choose `N`;
between `J` and a participle choose `V`; between `P` and `C` choose `P` unless a clause follows;
for a single unknown word, choose `N.s`.

## 6. Worked examples

Bracketed tokens are not words.

1. `s1`: Run [code: cargo build] to compile [,] then don't forget that it's slow [.]

   `s1: V.fi _ T V.in _ R AX.fi V.in C PR.s J _`

2. `s2` (list item): Why [:] The user's files aren't re [-] run by the existing well [-] known CLI
   tools [.]

   `s2: R _ D N.s N.p AX.pr X _ V.pp P D J R _ V.pp N.s N.p _`

3. `s3` (heading): Set up GitHub Actions so that each PR that changes [code: src/] is checked
   against Python [3.12]

   `s3: V.fi P PN.s PN.p C C D N.s PR V.pr _ AX.pr V.pp P PN.s _`
