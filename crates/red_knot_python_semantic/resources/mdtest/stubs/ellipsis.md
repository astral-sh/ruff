# Ellipsis

## Function and Methods

For default values the ellipsis literal `...` can be used, in a stub file only.

```py path=test.pyi
def f(x: int = ...) -> None: ...
def f2(x: dict = ...) -> None: ...

# error: [invalid-assignment] "Object of type `EllipsisType | ellipsis` is not assignable to `int`"
y: int = ...
```

## Use of Ellipsis Symbol

When the ellipsis symbol is used as default value the assignment is checked.

```py
from typing import Dict

# error: [invalid-parameter-default] "Default value of type `EllipsisType | ellipsis` is not assignable to annotated parameter type `int`"
def f(x: int = Ellipsis) -> None: ...
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
