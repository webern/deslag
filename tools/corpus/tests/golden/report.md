# Corpus report

Measured on `ghcr.io/example/blobs:test@sha256:0000`; human is one register: Markdown kept in repositories before 2022-01-01; llm and mixed files are newer; filters: none; lints at `config.toml`.

## Summary

**labels (counts, unweighted)**

| label | files | repos | en files | en repos | MB | prose tokens | words | sentences | tokens/file q1 med q3 | twins |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| human | 46 | 41 | 45 | 40 | 0.0 | 1514 | 1251 | 185 | 34 34 34 | 0 |
| llm | 81 | 81 | 80 | 80 | 0.0 | 4053 | 3262 | 405 | 39 48 58 | 0 |
| mixed | 6 | 6 | 6 | 6 | 0.0 | 126 | 102 | 18 | 21 21 21 | 0 |

**tools: files (repos); a tool is compared when it alone marked llm files in 25 repositories**

| tool | llm alone | llm any | mixed any | compared |
|---|--:|--:|--:|--:|
| claude-code | 27 (27) | 27 (27) | 2 (2) | yes |
| codex | 0 (0) | 1 (1) | 0 (0) | no |
| copilot | 26 (26) | 26 (26) | 2 (2) | yes |
| cursor | 27 (27) | 28 (28) | 2 (2) | yes |

## Characters

the English llm files, against the English human files; rates per million prose tokens, each repository weighing once.

**banned_chars groups**

| group | focus files (repos) | focus rate | reference files (repos) | reference rate | ratio | 95% low | 95% high |
|---|--:|--:|--:|--:|--:|--:|--:|
| dashes | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 |
| arrows | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 |
| ellipsis | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| bullets | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| math | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| checks | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| section | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| box_drawing | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| spaces | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| invisible | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| quotes | 0 (0) | 0.0 | 40 (40) | 54394.1 | 0.0 | 0.0 | 0.0 |
| emoji | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |
| none | 0 (0) | 0.0 | 0 (0) | 0.0 | 1.0 | 1.0 | 1.0 |

**characters, most frequent in the focus side first**

| char | code | group | focus files (repos) | focus rate | reference files (repos) | reference rate | ratio | 95% low | 95% high |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| — | U+2014 | dashes | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 |
| → | U+2192 | arrows | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 |
| “ | U+201C | quotes | 0 (0) | 0.0 | 40 (40) | 27197.0 | 0.0 | 0.0 | 0.0 |
| ” | U+201D | quotes | 0 (0) | 0.0 | 40 (40) | 27197.0 | 0.0 | 0.0 | 0.0 |

## Candidates

the English llm files, against the English human files; rates per million prose tokens, each repository weighing once. Ranked by the lower bound of a 95% interval from resampling repositories. Compared tools: claude-code, copilot, cursor.

**funnel**

| step | left |
|---|--:|
| n-grams of 1 to 4 tokens in at least 10 focus repositories | 217 |
| no one repository holds more than 25% of its focus files | 217 |
| no human file in the tree holds it | 175 |
| the interval's lower bound is at least 4 | 135 |
| focus files of at least 3 compared tools hold it | 96 |
| neither in the catalogue nor refused for it | 93 |
| after merging each n-gram into a shorter one it holds | 23 |

The catalog gate, n-grams through the sieve that no reference file holds, in any language, and at least 40 focus repositories do, merged among themselves: 12, of which 1 hold no rare word.

`no`, `the default`, `and`, `is stale`, `load`, `bearing`, `keep`, `stays`, `byte`, `identical`, `across`, `runs`

**candidates**

| phrase | focus files (repos) | focus rate | reference files (repos) | reference rate | ratio | 95% low | 95% high | top repo | tools | rare words |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| no | 80 (80) | 23953.4 | 0 (0) | 0.0 | 73.0 | 68.8 | 77.3 | 1% | 3 | no |
| the default | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 | 1% | 3 | - |
| and | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 | 1% | 3 | and |
| load | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 | 1% | 3 | load |
| bearing | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 | 1% | 3 | bearing |
| keep | 80 (80) | 20440.9 | 0 (0) | 0.0 | 62.4 | 60.0 | 65.0 | 1% | 3 | keep |

## Lints

config: config.toml; the files its globs select, at their path in their repository; left out: repo_layout, since a corpus file has no repository around it

**failing files by label: failing/checked, file share (repository-weighted share)**

| lint | human | llm | mixed |
|---|--:|--:|--:|
| max_size_bytes | 0/46 0.0% (0.0% of repos) | 16/81 19.8% (19.8% of repos) | 0/6 0.0% (0.0% of repos) |
| banned_phrases | 0/46 0.0% (0.0% of repos) | 80/81 98.8% (98.8% of repos) | 6/6 100.0% (100.0% of repos) |
| any | 0/46 0.0% (0.0% of repos) | 80/81 98.8% (98.8% of repos) | 6/6 100.0% (100.0% of repos) |

**failing llm files by the one tool that marked them: failing/checked, file share (repository-weighted share)**

| lint | claude-code | copilot | cursor |
|---|--:|--:|--:|
| max_size_bytes | 0/27 0.0% (0.0% of repos) | 0/26 0.0% (0.0% of repos) | 16/27 59.3% (59.3% of repos) |
| banned_phrases | 27/27 100.0% (100.0% of repos) | 26/26 100.0% (100.0% of repos) | 26/27 96.3% (96.3% of repos) |
| any | 27/27 100.0% (100.0% of repos) | 26/26 100.0% (100.0% of repos) | 26/27 96.3% (96.3% of repos) |
