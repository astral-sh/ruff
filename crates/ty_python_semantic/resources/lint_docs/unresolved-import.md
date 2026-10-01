## What it does

Checks for import statements for which the module cannot be resolved.

## Why is this bad?

Importing a module that cannot be resolved will raise a `ModuleNotFoundError` at runtime.

ty may also be unable to resolve an import that works at runtime if its Python environment or search
paths are configured incorrectly. See the
[import troubleshooting FAQ](https://docs.astral.sh/ty/reference/typing-faq/#why-cant-ty-resolve-my-imports)
for more details.

## Examples

```python
# ModuleNotFoundError: No module named 'mathh'
import mathh  # error
```
