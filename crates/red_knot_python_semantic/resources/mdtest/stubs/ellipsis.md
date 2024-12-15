# Ellipsis

## Function and Methods

For default values the ellipsis literal `...` can be used.

```py
from typing import Dict

def f(x: int = ...) -> None: ...
def f2(x: Dict = ...) -> None: ...

# error: [invalid-assignment] "Object of type `EllipsisType | ellipsis` is not assignable to `int`"
y: int = ...
```

## Class and Module Level Attributes

Using ellipsis literal for classes and module level attributes is unnecessary and results in an
error.

- <https://typing.readthedocs.io/en/latest/guides/writing_stubs.html#module-level-attributes>

```py
# error: [invalid-assignment] "Object of type `EllipsisType | ellipsis` is not assignable to `float`"
y: float = ...

class Foo:
    # error: [invalid-assignment] "Object of type `EllipsisType | ellipsis` is not assignable to `int`"
    y: int = ...
```
