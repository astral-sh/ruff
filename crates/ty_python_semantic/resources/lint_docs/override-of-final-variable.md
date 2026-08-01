## What it does

Checks for class variables on subclasses that override a superclass variable
that has been declared as `Final`.

## Why is this bad?

Declaring a variable as `Final` indicates to the type checker that it should not be
overridden on any subclass.

## Example

```python
from typing import Final


class A:
    X: Final[int] = 1


class B(A):
    X = 2  # error
```
