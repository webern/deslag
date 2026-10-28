# The words of deslag instructions update

`{from}` is the release the config was last updated by and `{to}` the one running. A heading names
its piece and is not printed.

## Heading

What is new in deslag {to}, since {from}

## Closing

Nothing `check` finds depends on `deslag_version`.

Offer each new lint to the person, with what it fails, and add the table of each they choose.
Offer each new phrase too, and add its `ban` line to each table they choose. A new setting keeps
its default until the config sets it; name it to the person, and set it only if they ask.

Once they have chosen, run `deslag update --to {to}`, adding your `--config-path` if any. It sets
`deslag_version` to "{to}", ending this list, and edits renamed or removed settings. If the person
cannot be asked now, change nothing, not even `deslag_version`, and tell them what is new.

## Current

Nothing is new in deslag {to} since {from}: the config is current.

## Nothing new

Nothing is new in deslag {to} since {from}.

## No config

No deslag config was found here, so this shows what is new since {from}, the release a config with
no `deslag_version` is taken to be from. Run it at the root of the repository to read its config.
