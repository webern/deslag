# Creative protocol — novel-idea generation for the hard steps

**Purpose:** a structured way to generate *genuinely novel* candidate strategies
for the hard steps (e.g. the BGS diagonalization, sub-task #22, and any future
proof work) — so we don't settle for the most predictable approach.

**The one governing principle (read this first):**

> **Novelty without verification is crank-adjacent. Novelty + machine-checked
> verification is the entire point of PleaNP.**

This protocol is the **idea engine**. It produces framings and candidate
strategies that *feel* inevitable in retrospect. The **gates** (Lean, the
multi-rendering churn engine, the review loop) are what make an idea *true*
rather than merely *persuasive*. So the protocol feeds the pipeline — it does
not replace it. A novel idea that passes the gates is a contribution; a novel
idea that skips them is a crank claim (see `docs/FAILURE_AUDIT.md`).

---

## When to use this

Use it for **hard, open-ended sub-tasks** — the ones where the "obvious"
approach is known to plateau (the barriers classify the obvious approaches as
dead). Prime targets: BGS diagonalization (#22), any future novel lower-bound
argument, the search-loop design (Rung 6). Do NOT use it for well-specified
mechanical work (#1, #3, #24) — that would be overkill.

---

## The five phases

### Phase 1 — Semantic excavation
Decompose the problem statement. List every word/phrase carrying multiple
meanings. Identify literal vs. figurative registers. Surface **at least 3
hidden assumptions** you're making just by framing it that way. Ask: *what
would the problem look like if worded differently?*

**Output:** a reframed problem statement + the hidden assumptions, written
down.

### Phase 2 — Constraint cartography
List every constraint: physical, economic, temporal, social, regulatory,
psychological. Label each **HARD** (immutable), **SOFT** (conventional), or
**PHANTOM** (assumed but not real). For every SOFT and PHANTOM constraint,
show what happens if you (a) invert it, (b) eliminate it, (c) exaggerate it to
absurdity.

**Output:** the constraint map with the SOFT/PHANTOM experiments.


### Phase 2.5 — Constraint-Net Cartography (the executable-filter step, DEC-023)

**Where this comes from:** the 2026-09-08 Navier-Stokes releases' deepest creativity-strategy lesson(`docs/LEAN_FORMALIZATION_LESSONS_2026-09-10.md` §3/§5): OpenAI's scale jump between their Euler disproof (~100 agents/50h) and the NS proof(~10,000/88h)was NOT "bigger compute" — it was **constraint density**:NS's smooth-forcing + finite-energy + exact-residual-cancellation net is much denser — each constraint is a cheap filter that rejects most candidates — so the parallel search could concentrate on the residual freedom.

The engineering:**compile the independent constraint net into cheap machine-checkable rejectors *before/during* creative search**,so the space of genuinely-hard candidates shrinks to something searchable. A green Lean build is an *late* rejector(checking only the final candidate);the point here is to make the constraints *early* rejectors, each one executable against partial candidates as cheaply as possible.



**Procedure:**for each hard, rung-9/11-shaped creative task:

1. **Enumerate the full independent-constraint net** the construction must satisfy (the analogues of "smooth force","bounded energy","exact residual cancellation","periodic-or-whole-space","computable oracle","set-extensional equality".).
2. **For each constraint, specify an automated rejector** — a cheap, machine-checkable test that runs *before/during* creative search and rejects most candidates(where a constraint can't yet be automated, write it down as a PRIMARY automation target for the rung — the rejector is the deliverable, not a side-effect).
3. **PleaNP-owned proto-rejectors to reuse/stack:**`#barrier_check`(a relativizing proof-is DEAD), the validation suite's must-refute lemmas, the Comparator references(DEC-022),and soon: the load-bearing-choice audit(DEC-023,`docs/VALIDATION_SUITE.md` §"Load-bearing-choice audit")— each is one more cheap filter.

4. **Only the residual freedom goes to creative search**(the agentic/protocol-driven search of Phases 3–5)—with the rejection traces kept as evidence thatthe search was constraint-guided, not free-form(that is what separates "compile-time-constrained search" from "post-hoc-selected construction";see `docs/STATEMENTS/ProofIntuition.template.md` §3–§4).


**Output:**the constraint-net map + one automated rejector spec per constraint + the residual-freedom description(what the creative search may still choose -- and what it may NOT,which is equally load-bearing).

**Anti-pattern (why this phase is NOT optional):** without it,"creative search" silently becomes "generate candidates then pick the one that works" — which is how you get post-hoc-selected constructions(the generic form of the after-the-fact force/oracle/encoding move). The rejectors' existence is what forces the search to find the *mechanism*,not the cherry-pick.


### Phase 3 — Cross-domain transplantation
Identify ≥3 domains with no surface connection (biology, music, geology, game
theory, linguistics, .). For each, find a **deep structural analogy** — not a
superficial metaphor — and extract a **solution mechanism** from that domain.
Which of these "orphan solutions" has never been applied to this problem space
before?

**Output:** ≥3 orphan mechanisms, each with the deep analogy stated.

### Phase 4 — Solution synthesis & persuasion scaffold
Combine the most promising threads from Phases 1–3 into one novel solution.
Then make it persuasive using **all** of:

- **Stepladder:** walk from an uncontroversial premise to the radical
 conclusion in small, individually defensible steps.
- **Anchor:** tie the core insight to a visceral experience the reader already
 understands.
- **Exhaustion:** briefly show why the 2–3 most common existing approaches are
 structurally doomed to plateau.
- **Steelman:** name the single strongest objection a smart skeptic would
 raise, state it *more strongly* than they would, then resolve it.
- **Disclosure:** say explicitly why the idea feels wrong on first hearing and
 why that feeling is a cognitive illusion.

**Output:** the synthesized novel strategy + the persuasion scaffold.

### Phase 5 — Comprehension bridge
Present the same core insight at five levels of resolution:

- **Visceral** — one sentence, child-level analogy
- **Operational** — what to concretely do next week
- **Mechanistic** — the causal chain of why it works
- **Theoretical** — the deeper principle this is an instance of
- **Frontier** — the speculative implication that follows logically but sounds
 alien today

**Output:** the five-level bridge.

---

## How it maps to the existing architecture

| Protocol phase | Feeds | Gate that verifies |
|---|---|---|
| 1–3 (excavation, cartography, transplantation) | candidate framings | (idea generation — no gate yet) |
| 4 (synthesis) | a candidate *strategy/statement* | multi-rendering churn renders it; `dual_render` machine-checks equivalence; `statement_lint` classifies its shape |
| 5 (bridge) | the plain-words framing | review loop (human confirms the meaning, boolean-style) |
| — (the actual proof) | the Lean proof | hygiene/vacuity/axiom gates + `lake build` |

**The rule:** a creative-protocol output becomes *real work* only when it is
turned into a Lean statement/proof and passed through the gates. The protocol
is upstream of the pipeline, never a bypass.

---

## Worked example (the problem: "P vs NP has no obvious next step in Lean")

A full run is in the session record; the distilled synthesis:

**Reframe (Phase 1):** the hidden assumption is that "next step" means "toward
a yes/no answer." The alternative: the next step is *locating the difficulty
precisely*.

**Orphan mechanisms (Phase 3):** evolution's *neutral drift* (search
near-proofs, let them drift); counterpoint's *generative constraints* (the
barriers as grammar, not walls — which is literally `#barrier_check`); geology's
*stratigraphy* (map the hierarchy of weaker statements).

**Synthesis (Phase 4):** *Formalize the provability boundary of the
near-P-vs-NP lattice.* For each weaker statement (P≠NP restricted to one
oracle, to a circuit class, to a construction), prove in Lean whether it
holds. The **boundary — the first unprovable near-statement — is a
well-defined, machine-checkable object.** Everyone tries the top statement;
nobody formalizes the boundary. It is *constructive* progress even though the
top statement is non-constructive (DEC-018).

**Steelman (Phase 4):** *"You're building a museum of near-misses instead of
attempting the climb."* Resolution: the boundary of provability is the *exact
location* of the difficulty — knowing where provability stops shrinks the
search space for the real proof by an exponential factor, and it is
*verifiable* progress.

**Bridge (Phase 5):** *Visceral:* "You can't see the peak in the fog, but you
can map the valley — and the map shows exactly which way is up."
*Operational:* formalize the lattice of near-statements and prove which are
true; the boundary is the deliverable. *Mechanistic:* barriers classify
failures → classified failures define a map → the map's boundary is a formal
object → Lean checks it → the boundary shrinks the search space. *Theoretical:*
the boundary of a search space carries more information than the search
itself. *Frontier:* P vs NP becomes a coordinate in a formalized landscape —
and the boundary may be somewhere nobody looked.

---

## Status

2026-09-06 — documented (DEC-019). The protocol is a *process* doc, not a
tool; agents working hard sub-tasks are expected to run it (or a lighter
version) before settling on an approach, then route the output through the
gates. The worked example's "provability boundary" idea is a candidate for a
future rung — not yet adopted as a task.