## What it does

Checks for `ty: ignore` comments that don't specify which rules to ignore.

When `unused-ignore-comment` is enabled, unused blanket comments are reported
by that rule instead.

## Why is this bad?

A blanket `ty: ignore` comment suppresses every type-checking diagnostic on the
applicable line or file. Specifying rule codes documents which diagnostics are
expected and prevents the comment from hiding unrelated errors.

## Examples

```py
# error
value = unknown  # ty: ignore
```

Use instead:

```py
value = unknown  # ty: ignore[unresolved-reference]
```
