# Widget

A repo's own config, not the shared one, wins when both are present. The tool reads a file. It
splits the text into sentences. It checks each sentence against the rules. It prints a report. It
exits with a code. The report names the file. It lists each place. A person reads the report. The
person fixes the text. The tool runs again. The second run is clean. The build waits for the tool.
The tool needs no network. It reads the config from the repo.

Each worker runs its own process rather than a thread, so a crash stays small. The config lists the
globs. Each glob selects some files. The files are plain text. Some are Markdown. Some are source
code. The tool reads their comments. A comment is prose too. The tool skips the code. It never edits
a file. It only reads. The report is for an agent. The agent reads it on stderr. The agent knows
what to do next. A clean run prints nothing. A failing run prints one report per file.

Each report has a heading. The heading names the check. The body says what is wrong. The advice says
how to fix it. The places show where. Each place has a line. The line counts from one. The text may
be long. The text may be short. A short text is not read for this check. A long text is read in
full. The rules do not change between runs. The same text gives the same report. No setting is read
from the environment. The config is one file.
