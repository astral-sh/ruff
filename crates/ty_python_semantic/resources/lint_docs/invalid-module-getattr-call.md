## What it does

Checks for imports that fail when calling a module-level `__getattr__` function.

## Why is this bad?

If a module defines `__getattr__`, Python calls it when a `from` import requests a name that is not
otherwise defined. The import raises an exception if `__getattr__` cannot accept the requested name.

## Examples

`module.py`:

```python
def __getattr__() -> str:
    return "fallback"
```

`main.py`:

```python
# TypeError: __getattr__() takes 0 positional arguments but 1 was given
from module import missing  # error
```
