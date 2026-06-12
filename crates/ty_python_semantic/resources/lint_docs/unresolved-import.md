## What it does

Checks for import statements for which the module cannot be resolved.

## Why is this bad?

Importing a module that cannot be resolved will raise a `ModuleNotFoundError`
at runtime.

## Examples

```python
# ModuleNotFoundError: No module named 'foo'
import foo  # error
```
