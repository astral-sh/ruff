# Generic aliases

## Type expressions

We recognize if a `types.GenericAlias` instance is created by specializing a generic class. We don't
explicitly mention it in our type display, but `list[int]` in the example below is a `GenericAlias`
instance at runtime:

```py
Numbers = list[int]

# At runtime, `Numbers` is an instance of `types.GenericAlias`. Showing
# this as `list[int]` is more helpful, though:
reveal_type(Numbers)  # revealed: <class 'list[int]'>

import types
from typing_extensions import TypeForm

generic_alias: types.GenericAlias = list[int]
generic_alias_typeform: TypeForm = list[int]

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

## Attributes of `type` aliases

The alias objects `type[Any]` and `typing.Type[Any]` delegate attribute access to their origin,
`type`. They do not have arbitrary attributes, even though their type argument is dynamic.

```py
from typing import Any, Type, TypeAlias

Modern: TypeAlias = type[Any]
Legacy: TypeAlias = Type[Any]

Modern.missing  # error: [unresolved-attribute]
Legacy.missing  # error: [unresolved-attribute]

reveal_type(Modern.__name__)  # revealed: str
reveal_type(Legacy.__name__)  # revealed: str
```

Attributes belonging to the alias itself remain accessible.

```py
reveal_type(Modern.__args__)  # revealed: tuple[Any, ...]
reveal_type(Legacy.__args__)  # revealed: tuple[Any, ...]
Modern.__origin__
Legacy.__origin__
Modern.__mro_entries__((object,))
```

The origin is `type` regardless of the type argument. Attributes of `int` are not available on the
alias object `type[int]`.

```py
Integers = type[int]
LegacyIntegers = Type[int]

Integers.bit_length  # error: [unresolved-attribute]
LegacyIntegers.bit_length  # error: [unresolved-attribute]
```

When these aliases are used as annotations, their inhabitants can be arbitrary class objects.
Accessing an unknown attribute through such a parameter remains valid.

```py
def dynamic_class(modern: Modern, legacy: Legacy) -> None:
    reveal_type(modern.missing)  # revealed: Any
    reveal_type(legacy.missing)  # revealed: Any
```

## Attributes of arbitrary `GenericAlias` instances

When the origin is unknown, we allow arbitrary attribute access through a `GenericAlias` instance.

```py
from types import GenericAlias

def unknown_origin(alias: GenericAlias) -> None:
    reveal_type(alias.missing)  # revealed: Any
```
