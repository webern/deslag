`repo_commands` fails a file that tells an agent to run a `make` target that no Makefile defines,
in a shell block or a code span that is one call, such as `make ci` or `make ci-fast check-release`.
The fix is whichever side lies, the doc or the Makefile. It reads Makefiles and never runs make, and
passes what it cannot tell: no Makefile, a `cd` it cannot follow, a target that is a variable.

```toml
[md.lints.repo_commands]
message = "..."  # optional; replaces the advice
```
