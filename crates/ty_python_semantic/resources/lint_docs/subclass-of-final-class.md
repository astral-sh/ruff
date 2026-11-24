## What it does

Checks for classes that subclass final classes.

## Why is this bad?

Decorating a class with `@final` declares to the type checker that it should not be subclassed.

## Example

```python
from typing import final


@final
class A: ...


class B(A): ...  # error
```
