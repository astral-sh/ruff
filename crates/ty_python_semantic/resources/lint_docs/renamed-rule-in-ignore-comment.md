## What it does

Checks for renamed rules in `ty: ignore` and `type: ignore[ty:...]` comments.

## Why is this bad?

When ty renames a rule, the old name still works in ignore comments. Prefer the current name to keep
ignore comments consistent across your codebase.

## Examples

```py
# error
import missing_module  # ty: ignore[unresolved-import]
```

The automatic fix changes only the rule name.

```py
import missing_module  # ty: ignore[import-not-found]
```

If the comment already lists both names, remove the old name manually.
