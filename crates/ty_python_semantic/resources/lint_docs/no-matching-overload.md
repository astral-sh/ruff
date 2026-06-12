## What it does

Checks for calls to an overloaded function that do not match any of the overloads.

## Why is this bad?

Failing to provide the correct arguments to one of the overloads will raise a `TypeError`
at runtime.

## Examples

```python
from typing import overload


@overload
def func(x: int): ...
@overload
def func(x: bool): ...
def func(x: int | bool): ...


func("string")  # error: [no-matching-overload]
```
