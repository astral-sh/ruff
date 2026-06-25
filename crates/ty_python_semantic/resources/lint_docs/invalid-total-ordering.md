## What it does

Checks for classes decorated with `@functools.total_ordering` that don't
define any ordering method (`__lt__`, `__le__`, `__gt__`, or `__ge__`).

## Why is this bad?

The `@total_ordering` decorator requires the class to define at least one
ordering method. If none is defined, Python raises a `ValueError` at runtime.

## Example

```python
from functools import total_ordering


# no ordering method defined
@total_ordering  # error
class MyClass:
    def __eq__(self, other: object) -> bool:
        return True
```

Use instead:

```python
from functools import total_ordering


@total_ordering
class MyClass:
    def __eq__(self, other: object) -> bool:
        return True

    def __lt__(self, other: "MyClass") -> bool:
        return True
```
