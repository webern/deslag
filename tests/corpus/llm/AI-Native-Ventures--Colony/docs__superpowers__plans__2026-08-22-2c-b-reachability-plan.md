# 2c-B: make the People and Roles screen reachable

Scope: gap 3 from the phase 2b review. One task, done properly.

**Your files:** navigation, sidebar, and the routing seam in `AgentsView.tsx`.
Do NOT touch `PeopleSection`'s internals, `EmployeeRoleDialog`, `orgMembers.ts`,
the profile panel, or the asks feature. Other agents are inside those right now.

## The gap

`PeopleSection` renders at the bottom of the Agents page, below Agent teams. There
is no sidebar entry, no route, and no deep link. The owner's reaction to the same
class of gap before this work started was "where do I actually set this up?", and
today the honest answer is "scroll to the bottom of Agents".

## Deliver

- A route for the People and Roles view.
- A sidebar entry that reaches it, matching how the existing entries (Agents,
  Spend, Discovery) are declared and styled. Follow the established pattern rather
  than inventing one.
- Deep-linkable, so a message or an agent can point at it, and so the owner can
  bookmark it.
- Decide and justify: does People and Roles become its own top-level view, or an
  anchored section within Agents that the route scrolls to? Either is defensible.
  Pick one, say why in the commit message, and make it consistent.

Whatever you choose, arriving via the route must land with the section actually in
view. A route that dumps the user at the top of a long page has not fixed this.

## Tests

- The route renders the section.
- The sidebar entry navigates there.
- Arriving by deep link puts the section in view.

## Gates

```
. ./bin/activate-hermit
cd desktop && node --import ./test-loader.mjs --experimental-strip-types \
  --test src/<your test files>
just desktop-check
```

**Never `just ci`**, and **never `just desktop-lint`** (no such recipe).

If you shift line numbers in any file `native-inventory.json` references, run
`pnpm generate:native-inventory` and commit the result.

## Done means

- Route, sidebar entry and deep link all work, with tests proven failing first.
- `desktop-check` green.
- A screenshot arriving via the deep link, with the section in view.
- `git commit -s`. No PR, no merge.
