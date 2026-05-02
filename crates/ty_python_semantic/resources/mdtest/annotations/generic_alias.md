# GenericAlias in type expressions

We recognize if a `types.GenericAlias` instance is created by specializing a generic class. We don't
explicitly mention it in our type display, but `list[int]` in the example below is a `GenericAlias`
instance at runtime:

```py
Numbers = list[int]

# At runtime, `Numbers` is an instance of `types.GenericAlias`. Showing
# this as `list[int]` is more helpful, though:
reveal_type(Numbers)  # revealed: <class 'list[int]'>

def _(numbers: Numbers) -> None:
    reveal_type(numbers)  # revealed: list[int]
```

It is also valid to create `GenericAlias` instances manually:

```py
from types import GenericAlias

Strings = GenericAlias(list, (str,))

reveal_type(Strings)  # revealed: GenericAlias
```

However, using such a `GenericAlias` instance in a type expression is currently not supported:

```py
# error: [invalid-type-form] "Variable of type `GenericAlias` is not allowed in a parameter annotation"
def _(strings: Strings) -> None:
    reveal_type(strings)  # revealed: Unknown
```

It is valid to use a known class-object value as the argument to `typing.Type` at runtime:

```py
from typing import Any, Type
import typing_extensions as tp

def type_hint_from_value(value: Any) -> Any:
    if isinstance(value, type):
        return Type[value]
    return value

def typing_extensions_type_hint_from_value(value: Any) -> Any:
    if isinstance(value, type):
        return tp.Type[value]
    return value
```
