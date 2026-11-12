## What it does

Checks for expressions that are used as [type expressions] but cannot validly be interpreted as
such.

## Why is this bad?

Such expressions cannot be understood by ty. In some cases, they might raise errors at runtime.

## Examples

```python
from typing import Annotated

# Int literals are not allowed in this context in type expressions
a: list[1]  # error
# `Annotated` requires at least one metadata argument after the type
b: Annotated[int]  # error
```

## Rule history

- v0.0.102: Wrong counts of explicit type arguments for forms such as `Callable` now use
    `invalid-type-argument-count`.

[type expressions]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
