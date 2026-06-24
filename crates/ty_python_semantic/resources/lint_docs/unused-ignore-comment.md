## What it does

Checks for `ty: ignore` directives that are no longer applicable.

## Why is this bad?

A `ty: ignore` directive that no longer matches any diagnostic violations is likely
included by mistake, and should be removed to avoid confusion.

## Examples

```py
# error
a = 20 / 2  # ty: ignore[division-by-zero]
```

Use instead:

```py
a = 20 / 2
```

## Options

Set [`analysis.respect-type-ignore-comments`](https://docs.astral.sh/ty/reference/configuration/#respect-type-ignore-comments)
to `false` to prevent this rule from reporting unused `type: ignore` comments.
