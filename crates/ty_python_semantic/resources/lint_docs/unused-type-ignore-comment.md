## What it does

Checks for `type: ignore` directives that are no longer applicable.

## Why is this bad?

A `type: ignore` directive that no longer matches any diagnostic violations is likely
included by mistake, and should be removed to avoid confusion.

## Examples

```py
# error
a = 20 / 2  # type: ignore
```

Use instead:

```py
a = 20 / 2
```

## Options

This rule is skipped if [`analysis.respect-type-ignore-comments`](https://docs.astral.sh/ty/reference/configuration/#respect-type-ignore-comments)
to `false`.
