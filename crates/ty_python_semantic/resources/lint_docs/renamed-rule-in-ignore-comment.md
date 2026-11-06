## What it does

Checks for renamed rules in `ty: ignore` and `type: ignore[ty:...]` comments.

## Why is this bad?

When ty renames a rule, the old name still works in ignore comments. Prefer the current name to keep
ignore comments consistent across your codebase.

## Examples

If ty renames `old-rule-name` to `new-rule-name`, the automatic fix changes
`# ty: ignore[old-rule-name]` to `# ty: ignore[new-rule-name]`.
