---
title: Footnote test cases
noindex: true
draft: true
---

Test cases for native Markdown footnote syntax: `[^label]` for the reference and
`[^label]: text` for the definition.

Footnotes referenced from a table are moved below that table by
`themes/gitlab-docs/src/utils/tables/footnotes.js`. All other footnotes stay in the
page-level block at the bottom of the page.

Footnote numbers are assigned by the Markdown renderer in order of first reference. The
numbers do not depend on the label, or on the order of the definitions.

## Footnote in body prose

This sentence has a footnote.[^prose-one] This sentence has a second one.[^prose-two]

Both definitions stay in the page-level footnotes block at the bottom of the page, because
no table references them.

## Footnotes referenced from several cells of one table

| Setting   | Availability         | Notes                                              |
|-----------|----------------------|----------------------------------------------------|
| Setting A | All tiers            | Requires an instance runner.[^runner]              |
| Setting B | Premium and Ultimate | Not available for personal namespaces.[^namespace] |
| Setting C | Ultimate             | Requires an instance runner.[^runner]              |

`[^runner]` is referenced twice. The renderer emits two superscript references that point to
one definition, and that definition gets two return links. Only one list item should appear
below the table, and both return links should work.

## Second table on the same page

| Feature   | Status               |
|-----------|----------------------|
| Feature X | Beta.[^beta]         |
| Feature Y | Generally available. |

Each table gets its own footnote block. The numbers continue from the previous table. They
do not restart.

## Footnote in a table header cell

| Option   | Default value.[^default] |
|----------|--------------------------|
| Option A | `true`                   |
| Option B | `false`                  |

The sticky header clone must not duplicate the `id` attribute from this cell. In the browser
console, the following should return an empty array:

```javascript
[...document.querySelectorAll('[id]')].map((e) => e.id).filter((v, i, a) => a.indexOf(v) !== i)
```

## Condensed table with a footnote

<!-- The markdownlint version in this repo parses `{.condensed}` as a table row.
     Unrelated to footnotes: any condensed table in `content/` hits this. -->
<!-- markdownlint-disable MD055 MD056 -->

| Key   | Value                |
|-------|----------------------|
| Key A | Value A.[^condensed] |
| Key B | Value B              |
| Key C | Value C              |
{.condensed}

<!-- markdownlint-enable MD055 MD056 -->

## Footnote referenced from both prose and a table

This prose sentence and the following table cell point to the same definition.[^shared]

| Column | Value                  |
|--------|------------------------|
| Row A  | See the note.[^shared] |

The definition moves below the table. The prose reference should still scroll to it.

## Footnote whose text contains a link and a code span

| Component   | Notes                          |
|-------------|--------------------------------|
| Component A | Configuration required.[^rich] |
| Component B | Longer explanation.[^multi]    |

## Numeric label that does not match its rendered number

| Item   | Notes              |
|--------|--------------------|
| Item A | Numeric label.[^1] |

The label `[^1]` is only a name. This footnote renders with the number that its first
reference earns, which on this page is not 1.

## Table inside tabs

Do not use footnotes inside a shortcode. Hugo renders shortcode inner content in a separate
pass, which breaks footnotes in two ways:

- A reference to a definition outside the shortcode does not resolve. It renders as literal
  text, and the definition is dropped from the page.
- A definition inside the shortcode does resolve, but numbering restarts at 1 and a second
  footnotes block is emitted. The page then holds duplicate `fn:1` and `fnref:1` IDs, so the
  return link scrolls to the wrong footnote.

Both behaviors come from Hugo, not from the relocation script. Neither is fixable here.

{{< tabs >}}

{{< tab title="Definition outside the tab" >}}

| Step   | Notes                 |
|--------|-----------------------|
| Step 1 | Run the job.[^tabbed] |

The reference does not resolve. Shortcode inner content is rendered in its own pass, so the
definition at the bottom of the page is not in scope.

{{< /tab >}}

{{< tab title="Definition inside the tab" >}}

Not rendered here, because it would break the rest of this page. Writing this:

```markdown
| Step   | Notes                        |
|--------|------------------------------|
| Step 1 | Run the job.[^tabbed-inside] |

[^tabbed-inside]: Defined in the same shortcode as the reference.
```

resolves the reference, but numbering restarts at 1 and a second footnotes block
is emitted. The page then holds two elements with `id="fn:1"`. Every `#fn:1` link
resolves to whichever comes first in the document, and that one sits inside a
collapsed tab panel. Unrelated footnotes elsewhere on the page stop working.

{{< /tab >}}

{{< /tabs >}}

[^prose-one]: A footnote referenced from body prose only.
[^prose-two]: A second footnote referenced from body prose only.
[^runner]: This setting requires an [instance runner](ci/runners/_index.md).
[^namespace]: Personal namespaces do not support this setting.
[^beta]: For more information, see [beta features](policy/development_stages_support.md).
[^default]: Default values apply to new projects only.
[^condensed]: Condensed tables scroll vertically, so this block renders below the scroll container, not inside it.
[^shared]: Referenced from both prose and a table cell.
[^rich]: Set `ci_default_git_depth` to `20`. For more information, see [pipeline settings](ci/pipelines/settings.md).
[^multi]: First paragraph of a longer footnote.

    Second paragraph of the same footnote, indented by four spaces.

[^1]: A footnote with a numeric label.
[^tabbed]: Referenced from a table inside a tabbed section.
