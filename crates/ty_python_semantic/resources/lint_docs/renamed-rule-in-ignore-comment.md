## What it does

Checks for renamed rules in `ty: ignore` and `type: ignore[ty:...]` comments.

## Why is this bad?

When ty renames a rule, the old name still works in ignore comments. Prefer the current name to keep
ignore comments consistent across your codebase.

## Examples

<div hidden>

```toml
[rules]
unused-ignore-comment = "ignore"
```

</div>

Suppose ty renames `old-rule-name` to `new-rule-name`.

```py
# error
x = 1  # ty: ignore[old-rule-name]
```

The automatic fix changes only the rule name.

```py
x = 1  # ty: ignore[new-rule-name]
```

If the comment already lists both names, remove the old name manually.
