## What it does

Checks for renamed rules in `ty: ignore` and `type: ignore[ty:...]` comments.

## Why is this bad?

The old name still suppresses the rule. Using the current name makes it easier to find the rule in
diagnostics and documentation. The automatic fix replaces just the rule name. If the comment already
includes the current name, remove the old name from the list.

## Examples

```py
# error
import missing_module  # ty: ignore[unresolved-import]
```

Use instead:

```py
import missing_module  # ty: ignore[import-not-found]
```
