## What it does

Checks for `ty: ignore` comments that don't specify which rules to ignore.

Blanket comments are reported whether or not they suppress a diagnostic. If
`unused-ignore-comment` is also enabled, unused blanket comments emit both diagnostics.

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
