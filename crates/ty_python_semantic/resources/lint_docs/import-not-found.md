## What it does

Checks for imports where ty cannot resolve the module.

## Why is this bad?

Importing a missing module raises `ModuleNotFoundError` at runtime.

These diagnostics can often be caused due to ty's Python environment or search paths being
configured incorrectly. See the
[import troubleshooting FAQ](https://docs.astral.sh/ty/reference/typing-faq/#why-cant-ty-resolve-my-imports)
for more details.

## Examples

```python
# ModuleNotFoundError: No module named 'mathh'
import mathh  # error
```

## Rule history

- v0.0.97: Renamed from `unresolved-import`.

    `unresolved-import` remains as a compatibility alias for this rule.

- v0.0.102: Split out `module-attribute-not-found` for missing names in resolved modules.

    To retain a custom severity set for `import-not-found` or `unresolved-import` for both kinds of
    errors, set the same severity for `module-attribute-not-found`. In ignore comments for missing
    names in resolved modules, replace `import-not-found` or `unresolved-import` with
    `module-attribute-not-found`.
