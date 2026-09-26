---
llm_instructions: >
  When we specifically look at a codebase for inspiration, record it here. Do not get overly
  technical about this; no need to link to issues, lines of code, etc. Just give a general idea of
  how we used it.
---
# Acknowledgements

<owner>

This project is LLM-written, and proper acknowledgements would include the entire corpus of human
history. But at times I am specifically asking the LLM to "look at such and such" repo and take
ideas from it. So this file is for attribution of those specifically perused in this way. If code
was ported directly, it will say so here.

</owner>

## Vale

- URL: https://github.com/vale-cli/vale
- Licence: MIT
- Copyright: "Copyright (c) 2016 Joseph Kato"
- Studied at: v3.23.0, commit
  [2753160f](https://github.com/vale-cli/vale/tree/2753160f8e5835976340183ddc4a91ed10b7d3ed)

Code was not ported, but we like the idea of a different exit code for internal errors, the `Block`,
`Scope` and `Parent` linkage in the tree, the command which shows a file's resolved settings, the
testing strategy, and some of the lints and corpus analyses.
