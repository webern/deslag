# `aivm-2404-jon` — where the friction was

Slice: 114 human prompts across 10 sessions, 8 active days, 2026-07-27 to 2026-08-10,
127,089 human-authored characters, 6,831 tool calls. Machine `aivm-2404`, account `agent`,
checkout `/home/joncrall/code/aiq-dkps-formalization`, entrypoint `cli` on 114/114.

Distinct from `aivm-2404-edward` despite the shared hostname: different account
(`/home/agent` vs `/home/local/KHQ/edward.wang`), different checkout, no session overlap.

## Validation, and one failure mode that is not in the prompt

**Resume duplication: none, and that is real.** 11,257 raw in-scope user records, 11,257
unique uuids, and **zero uuids appear in more than one file**. `AGENT-PROMPT.md` says to
expect raw ≫ deduped (toothbrush-jon: 1280 → 389) and to suspect the dedup if they are equal.
Here they are equal because nothing was resumed or forked into a new file — dedup runs and
finds nothing. Reported rather than smoothed over.

**Sidechain leakage: none.** 0 records in `prompts.jsonl` carry `isSidechain`. But the
structural fact underneath is worth more than the check: **67 transcript files are in scope
and only 11 contain a single non-sidechain user record. The other 56 are subagent
transcripts.** This slice is a coordinator/subagent workflow, so any file-count-based measure
of "sessions" overstates human sessions by about 6×.

**Synthetic turns: none, after a fix.** My first pass at this check was itself wrong — I
grepped a field named `text` that does not exist in `prompts.jsonl` (the field is `prompt`),
so the check passed against an empty string and told me nothing. Re-run against the real
field: clean.

### A fourth failure mode: skill bodies counted as human prompts

Section 4 of `AGENT-PROMPT.md` lists three ways the extraction silently corrupts the numbers.
There is a fourth.

The harness injects a **bundled skill's markdown body** as a `user` record prefixed with its
unpack path. One such record on this machine — the `design-sync` skill, **28,011 characters** —
was counted as a human prompt. It defeats every check in section 4: it is not `isSidechain`,
it starts with none of the known synthetic prefixes, and the prompt's own warning about
register is no help because its register is documentation, not instruction.

It carried **18% of this slice's entire human character count.** Any claim stated in
characters — which is the unit section 4 tells you to prefer — was wrong by that much.

Two smaller variants of the same leak: four bare `/compact` turns (the tool routes
`<command-name>`-wrapped slash commands to events but not unwrapped ones), and one
harness-generated `Continue from where you left off.`

**Tool change**, recorded here and in `manifest.json` so the numbers stay comparable: three
routing rules added to `classify()` in `tools/extract_prompts.py`, all in the spirit of the
existing `slash_body` / `slash_command` routes rather than new policy. Net effect on this
slice: **120 prompts / 155,165 chars → 114 / 127,089** — a 5% drop in count and an 18% drop in
characters. Other slices will move slightly if re-run; the direction is always down, and
concentrated in characters rather than counts.

### The cross-check that looked like data loss and was not

`~/.claude/history.jsonl` exists here (unlike on `aivm-2404-edward`) and logs **188** typed
prompts for this project against 114 delivered. Same date range on both ends, so nothing was
pruned.

The gap is `queued_prompt_dropped`: **66 prompts the human typed while the agent was running,
which were flushed from the queue and never became a user record.** The extractor already
distinguishes these from prompts that were re-typed and delivered. 114 + 66 = 180, and the
remaining ~8 are bare slash commands.

**So roughly 37% of everything the human typed never reached the agent.** They were not
noise. A sample:

> `Get to a pause point, I want to restart the VM with more CPUs and RAM.`

> `And what are the other open lanes, because we need to distinguish: still needs a proof vs tauceti polish.`

That is a steering channel with a one-in-three drop rate, and it is invisible to anyone
reading transcripts alone.

## What the steering was actually for

The regex taxonomy needs a hand audit before any share is quotable, exactly as section 4
warns. Six prompts exceed 2,000 characters; **four of them are binned `correction_redirect`
and are not corrections.** They are relayed GPT output whose opening clause happens to carry a
correction token ("Note, I updated…", "Here is what GPT says…"). First rule wins, and a
20,000-character brief matches everything.

| category | n (raw) | n (audited) | char % raw | char % audited |
|---|---|---|---|---|
| `ferry_relay` | 22 | **26** | 17.8% | **92.0%** |
| `other` | 24 | 24 | 3.1% | 3.1% |
| `coordination` | 16 | 16 | 1.7% | 1.7% |
| `status_query` | 20 | 20 | 1.2% | 1.2% |
| `correction_redirect` | 14 | **10** | 74.9% | **0.7%** |
| `decision_answer` | 5 | 5 | 0.5% | 0.5% |
| everything else | 13 | 13 | 0.8% | 0.8% |

Reclassifying four prompts moves the headline from *"three quarters of human input was
correction"* to *"corrections are 0.7% of it."* Quote the audited column.

**The dominant activity was ferrying another model's output.** 92% of all human-authored
characters on this machine are GPT-5 responses pasted in by the human, and they are
concentrated in a single five-hour window on 2026-08-08/09:

> `I have a GPT response: The M5 distinction is real, and my earlier comment conflated two different converses…` (28,032 chars)

> `here is gpts results: Worked for 32m 46s…` (20,022 chars)

> `Here is what GPT says, I hope this helps. Give me more messages I can send it so it can do some parallel work…` (22,745 chars)

The human is acting as a message bus between two model instances that cannot talk to each
other. That last quote is the clearest statement of it: the human is asking for material to
send *back*, so the round trip is a manual loop the human is executing by hand.

**The human's own voice is short.** Median prompt 79.5 characters. 87 of 114 prompts (76%) are
under 200 characters and together carry **4.8%** of the characters. The genuine corrections —
all 10 of them — total **835 characters**, median 78:

> `Stop checking` (13)

> `Dont start new work` (19)

> `No you should write them.` (25)

> `Oh, fix the bad tauceti pointer` (31)

> `Don't bother with the measurement to confirm speedup. That's not your job.` (74)

> `Don't spend your time optimizing. Pull in the new optimization from GPT though` (78)

> `Don't do too much work in parallel. The point of the agent is to reduce context, this is still mostly serial work.` (114)

Prompt-count and character-count tell opposite stories here, and neither alone is honest. By
count the human was steering constantly (105 steering bursts). By characters the human barely
spoke — the channel was saturated with another model's prose.

## How the mix moved

Two distinct phases, with no activity between 2026-07-31 and 2026-08-08:

| phase | dates | prompts | character mass | what it was |
|---|---|---|---|---|
| 1 | 07-27 → 07-31 | 65 | ~3% | Tau Ceti API polish, multi-agent lane board on `dev/LANES.md` |
| 2 | 08-08 → 08-10 | 49 | ~97% | Davis--Kahan completion campaign, coordinator/subagent |

Phase 1 friction is coordination: claiming lanes, merging other agents' branches, avoiding
collisions. Phase 2 friction is relay. The shift is not a change in how much the human
steered — it is a change in *what the human was for*. In phase 1 they were a scheduler; in
phase 2 they were a transport layer.

## What real autonomy looked like

Median unattended run **23.6 minutes**; p25 4.6, p75 57.1, longest **241.9 minutes** (4 hours).
24 of 90 measurable gaps are under 5 minutes.

That bimodality is the real story. The long tail is genuine autonomy — a 4-hour unattended run
covering a full subagent mission plus verification and integration. The sub-5-minute cluster is
not: it is the human sitting at the terminal during a relay exchange, pasting one GPT response,
waiting, pasting the next.

**Which prompts bought the most work.** Tool calls per prompt: median 14, mean 59.9 — a 4×
gap, so a minority of prompts do nearly all the work. The largest single purchases were short:

> `status update?` → a full verification sweep

> `No you should write them.` (25 chars) → decided a standing policy question about who wires
> external contributions into `All.lean`, which then governed every subsequent integration

The cheapest characters in the corpus bought the most durable outcomes. The most expensive
(28k-character relays) mostly re-stated mathematics that then had to be independently checked
anyway.

## Where the tight loops were, and what caused them

**105 Stop-hook replays** against 114 human prompts — very nearly one machine-generated "the
goal is not met, continue" turn per human turn. These are not friction from the human; they are
an automated goal-condition harness firing. In the final session it fires while the human has
explicitly asked the agent to *pause and talk*, which puts the agent in a genuine conflict
between an automated condition and a human instruction. The agent held the pause. Worth
recording as a design observation: a stop-hook has no way to know it is overriding a live human
request.

**32 interrupts** and **66 dropped queued prompts** cluster in the same relay window. The
pattern is the human typing a follow-up while a long tool call is in flight, then interrupting
because the follow-up was more urgent than what was running.

**7 compactions** in 10 sessions. The coordinator design — one subagent at a time, coordinator
does not read proof terms — exists specifically to control this, and phase 2 still compacted
every ~1.4 sessions.

## What should not have been human at all

1. **The GPT relay.** 92% of human-authored characters. Nothing about it required a human:
   it is copy, paste, wait, copy, paste. It also introduces a failure the transcripts show
   directly — the human relaying a brief they had not read, then having to correct the agent's
   reading of it. Two model instances with a shared branch would remove the entire category.
2. **Status queries.** 20 prompts asking what state things were in — `status update?`,
   `Is an agent still running?`, `status on m14?`. Every one is answerable from the repository.
   That is a dashboard, not a conversation.
3. **Merge-and-push shepherding.** Repeated instructions to pull, merge, push, update the lane
   board. The `lane-work-loop` memory exists precisely because this was said often enough to be
   worth persisting; it is a workflow, and workflows belong in a script or a hook.
4. **Telling the agent to check its own exit codes.** See `MISTAKES.md`, class C — this had to
   be said, was persisted to memory, and the failure still recurred in a variant form on
   2026-08-09.

## Caveats

- The `mistakes.py` "correction" heuristic over-fires. Windows [3] and [4] in
  `local/mistake-evidence.md` are classified as corrections and are plain task assignments
  (`claim the §9 SelfAdjointGapInverse lane`). Treat the 74 windows as candidates, not
  instances; `MISTAKES.md` cites only hand-verified ones.
- `active_days` is 8, but two of those days are a single relay session. Day counts overstate
  engagement breadth.
- **This analysis session is itself in the corpus** — the last 4 prompts on 2026-08-10,
  including the one that commissioned it. The session that measures the corpus is inside it.
- I fixed a tool, which `AGENT-PROMPT.md` sanctions and requires me to declare. The
  pre-fix numbers are preserved above and in `manifest.json` so any comparison against another
  agent's slice can be made on either basis.
