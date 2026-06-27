# Blanket `ty: ignore` comments

The optional `blanket-ignore-comment` rule requires `ty: ignore` comments to include specific rule
codes. Unused blanket comments aren't reported because they don't suppress any diagnostics; use
`unused-ignore-comment` to report them separately.

```toml
[rules]
blanket-ignore-comment = "error"
```

## Line-level ignores

```py
# error: [blanket-ignore-comment]
a = unresolved  # ty: ignore

b = unresolved  # ty: ignore[unresolved-reference]

# Blanket `type: ignore` comments are covered by Ruff's PGH003 rule.
c = unresolved  # type: ignore

# error: [unused-ignore-comment] "Unused `ty: ignore` without a code"
d = 1  # ty: ignore[]

# error: [unused-ignore-comment] "Unused blanket `ty: ignore` directive"
e = 1  # ty: ignore
```

## Unused blanket ignores

Unused blanket comments aren't reported by this rule, even when `unused-ignore-comment` is disabled:

```toml
[rules]
blanket-ignore-comment = "error"
unused-ignore-comment = "ignore"
```

```py
a = 1  # ty: ignore
```

## Suppression diagnostics

Suppression-related diagnostics also count as using a blanket ignore.

```py
# error: [blanket-ignore-comment]
a = 1  # ty: ignore # ty: ignore[not-a-rule]

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
