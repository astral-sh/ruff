## What it does
Checks for uses of deprecated items

## Why is this bad?
Deprecated items should no longer be used.

## Examples
```python
@warnings.deprecated("use new_func instead")
def old_func(): ...

old_func()  # emits [deprecated] diagnostic
```
