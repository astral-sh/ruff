## What it does

Checks for methods on subclasses that override superclass methods decorated with `@final`.

## Why is this bad?

Decorating a method with `@final` declares to the type checker that it should not be
overridden on any subclass.

## Example

```python
from typing import final


class A:
    @final
    def foo(self): ...


class B(A):
    def foo(self): ...  # error
```
