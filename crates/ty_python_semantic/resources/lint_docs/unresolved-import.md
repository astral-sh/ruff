## What it does

Checks for import statements for which the module cannot be resolved.

## Why is this bad?

Importing a module that cannot be resolved will raise a `ModuleNotFoundError` at runtime.

These diagnostics can often be caused due to ty's Python environment or search paths being
configured incorrectly. See the
[import troubleshooting FAQ](https://docs.astral.sh/ty/reference/typing-faq/#why-cant-ty-resolve-my-imports)
for more details.

## Examples

```python
# ModuleNotFoundError: No module named 'mathh'
import mathh  # error
```
