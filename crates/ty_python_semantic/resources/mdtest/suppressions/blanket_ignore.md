# Blanket `ty: ignore` comments

The optional `blanket-ignore-comment` rule requires `ty: ignore` comments to include specific rule
codes.

```toml
[rules]
blanket-ignore-comment = "error"
```

## Line-level ignores

```py
# error: [blanket-ignore-comment]
a = unresolved  # ty: ignore

b = unresolved  # ty: ignore[unresolved-reference]

# `blanket-ignore-comment` covers only `ty: ignore`; we can also add `blanket-type-ignore-comment`,
# parallel to `unused-ignore-comment` and `unused-type-ignore-comment`. In the meantime, blanket
# `type: ignore` can also be caught by Ruff's PGH003 rule.
c = unresolved  # type: ignore
```

## Unused ignore comments

When `unused-ignore-comment` is enabled, an unused `ty: ignore` comment without rule codes triggers
both rules.

```py
# error: [unused-ignore-comment] "Unused `ty: ignore` without a code"
d = 1  # ty: ignore[]

# error: [blanket-ignore-comment]
# error: [unused-ignore-comment] "Unused blanket `ty: ignore` directive"
e = 1  # ty: ignore
```

## Suppression diagnostics

Suppression-related diagnostics are checked before `blanket-ignore-comment`. A blanket ignore that
suppresses an `ignore-comment-unknown-rule` or `invalid-ignore-comment` diagnostic therefore counts
as used:

```py
# The nested ignore contains an unknown rule.
# error: [blanket-ignore-comment]
a = 1  # ty: ignore # ty: ignore[not-a-rule]

# The nested ignore is invalid.
# error: [blanket-ignore-comment]
b = 1  # ty: ignore # ty: ignore[*]
```

## File-level ignores

The rule also detects file-level blanket ignores:

```py
# error: [blanket-ignore-comment]
# ty: ignore

a = unresolved
```

## Suppressing the rule

A blanket ignore can be suppressed by a code-specific ignore:

```py
a = unresolved  # ty: ignore # ty: ignore[blanket-ignore-comment]
```

An own-line `blanket-ignore-comment` suppression also covers a blanket ignore on the following
logical line.

```py
def f():
    # ty: ignore[blanket-ignore-comment]
    return missing  # ty: ignore
```
