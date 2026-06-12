## What it does

Checks for expressions that are used as [type expressions]
but cannot validly be interpreted as such.

## Why is this bad?

Such expressions cannot be understood by ty.
In some cases, they might raise errors at runtime.

## Examples

```python
from typing import Annotated

a: type[1]  # `1` is not a type
b: Annotated[int]  # `Annotated` expects at least two arguments
```

[type expressions]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
