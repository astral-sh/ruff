## What it does

Checks for `ty: ignore[code]` or `type: ignore[ty:code]` comments where `code` isn't a known lint rule.

## Why is this bad?

A `ty: ignore[code]` or a `type: ignore[ty:code]` directive with a `code` that doesn't match
any known rule will not suppress any type errors, and is probably a mistake.

## Examples

```py
# error
a = 20 / 1  # ty: ignore[division-by-zer]
```

Use instead:

```py
a = 20 / 0  # ty: ignore[division-by-zero]
```
