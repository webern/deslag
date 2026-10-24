---
llm_edit: false
---
# Deslag

Deslag should be a best-in-class linter for LLM written files.

## Performance Characteristics

In release mode:
- Linting a small file should take less than 1 second on normal hardware (i.e. a cold load of any
  onboard models should take less than 1 second).
- Linting should process at least 5 MB of prose per second on normal hardware, in one thread. Code
  files contribute only their comments, so our benchmark is: 10 MB of Markdown, a large
  documentation set, should take about 2 seconds.
- The binary should be smaller than 200MB
