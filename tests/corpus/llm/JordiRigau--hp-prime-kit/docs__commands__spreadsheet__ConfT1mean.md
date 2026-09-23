# ConfT1mean

A t confidence interval for one mean, refused from Home.

| | |
|---|---|
| Syntax | `ConfT1mean(input_list, ["configuration"])` |
| Group | spreadsheet |
| Runs on the PC | no |

## Examples

| Call | Result | Known from |
|---|---|---|
| `EXPR("ConfT1mean({1,2,3,4,5})")` | *error* | [emulator](../results.tsv) |

## Behaviour

**The call is refused** (emulator), given an ordinary list, which is the
shape HP's own syntax names.

**Eighteen of the group's twenty-two names are refused even with the Spreadsheet app active** (emulator),
and [SUM](SUM.md) carries the account and the two probes that would settle
it.

**This name and eleven like it sit in two menus at once** (HP help): HP files
them under `Toolbox App Spreadsheet` and under `Toolbox App Inference`, while
the inventory gives them one home, `spreadsheet`. That is the inventory's
arrangement rather than a statement about where they work, and the
`inference` group has nine names of its own that this batch did not reach.

The interpreter does not implement it, so `hpprime run` cannot check a program
that uses it (unverified).

## Related

[ConfT2mean](ConfT2mean.md) · [HypT1mean](HypT1mean.md) · [SUM](SUM.md)
