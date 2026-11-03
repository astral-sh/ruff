## What it does

Checks that the name passed to the functional form of `TypedDict` matches the variable it is
assigned to. This rule also warns if ty cannot determine the value of a string name argument.

## Why is this bad?

The name argument sets the `TypedDict` class's `__name__`. A mismatch can make it harder to
recognize the type in diagnostics.

## Examples

```python
from typing import TypedDict

Movie = TypedDict("Film", {"title": str})  # error: [mismatched-typeddict-name]
```
