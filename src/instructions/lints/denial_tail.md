`denial_tail` fails a sentence that says what something is and then ends in a comma or dash, `not`
or `never`, and a short denial: `The contract is defined on the body, not on the transport coding.`

If at all possible, just delete the trailing negation entirely. If it truly damages the meaning or
leaves something out, reword the whole sentence to say it without the negative trailer. Do not
mangle the sentence. It should be easier to read and understand after your edit than it was before.

It passes a one-word tail, and idioms such as `not only`, `not even` and `not yet`. Ask the human
before turning it off or scoping it.

```toml
[md.lints.denial_tail]
message = "..."  # optional; replaces the advice
```
