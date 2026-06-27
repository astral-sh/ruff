## What it does

Checks for `ty: ignore` comments that don't specify which rules to ignore.

Unused blanket comments aren't reported by this rule because they don't suppress
any diagnostics. Enable `unused-ignore-comment` to report them separately.

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
