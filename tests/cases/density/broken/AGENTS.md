# Widget

Widget checks files against a set of rules.

Widget reads every file under the input directory, parses each one into a tree, checks the tree against the rules in the config, and writes one report per file to the output directory, which it creates if it does not exist, and then it prints a summary of how many files passed and how many failed, and it exits with a nonzero code when any file failed so that a CI job can gate on it, and it caches the parsed trees between runs in a hidden directory so that a second run over the same files is faster, though the cache can be cleared with the clean command, which also removes the reports. It also watches the input directory when it is given the watch flag, and reruns the checks on each file that changes, printing only the reports that changed.

## A heading is never a block, however long it grows, since it is not a paragraph of text

| Step | What happens, which a table may say at whatever length it needs to, since no row is prose |
|---|---|
| read | the files |

```
A code block is never a block either, whatever it holds.
```

- Short items pass.
- The config: where it lives, which of its keys are required, what each of the optional keys does, how an override for one directory is merged with the settings above it, and which errors a broken config produces when the run starts, before any file is read. It also covers the environment variables that override a key.

> Widget reads every file under the input directory, parses each one into a tree, checks the tree against the rules in the config, and writes one report per file to the output directory, which it creates if it does not exist, and then it prints a summary of how many files passed and how many failed, and it exits with a nonzero code when any file failed so that a CI job can gate on it, and it caches the parsed trees between runs in a hidden directory so that a second run over the same files is faster, though the cache can be cleared with the clean command, which also removes the reports. It also watches the input directory when it is given the watch flag, and reruns the checks on each file that changes, printing only the reports that changed.
