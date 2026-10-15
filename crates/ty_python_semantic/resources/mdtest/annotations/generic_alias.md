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

## Generic aliases are not instances of `type`

A generic alias is not assignable to `type`, even though it can construct instances of its origin.
This distinction also applies to type variables bound to `type` and to specialized `type[...]`
annotations.

```py
from types import GenericAlias
from typing import Callable
from typing_extensions import TypeForm
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_assignable_to, is_subtype_of, is_disjoint_from

static_assert(not is_subtype_of(TypeOf[list[int]], type))
static_assert(not is_assignable_to(TypeOf[list[int]], type))
static_assert(not is_assignable_to(TypeOf[list[int]], type[list[int]]))
static_assert(is_disjoint_from(TypeOf[list[int]], type))
static_assert(is_subtype_of(TypeOf[list[int]], GenericAlias))
static_assert(is_assignable_to(TypeOf[list[int]], TypeForm[list[int]]))
static_assert(not is_assignable_to(TypeOf[type[list[int]]], type))
static_assert(is_assignable_to(TypeOf[type[list[int]]], TypeForm[type[list[int]]]))

constructor: Callable[[], list[int]] = list[int]
reveal_type(constructor())  # revealed: list[int]
```

Functions such as `singledispatch.register` require an actual class for dispatch registration.

```py
from functools import singledispatch

@singledispatch
def render(value: object) -> str:
    return str(value)

@render.register(list)  # no diagnostic
def render_list(value: list[int]) -> str:
    return str(value)

render.register(list[int])  # error: [no-matching-overload]
```

The overload for bare decorator usage also accepts callables, but a generic alias cannot serve as an
implementation with an inferred dispatch type. This remains an error when the return type is
unknown.

```py
from functools import singledispatchmethod

@singledispatch
def unannotated(value): ...

unannotated.register(list[int])  # error: [invalid-argument-type]
register = unannotated.register
register(list[int])  # error: [invalid-argument-type]

@unannotated.register  # no diagnostic
def handle_int(value: int): ...

class Handler:
    @singledispatchmethod
    def render(self, value): ...

    render.register(list[int])  # error: [invalid-argument-type]
```

## Subscripts returning class objects

A custom subscript can return an actual class object, including a specialized final class.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import final

@final
class Box[T]: ...

class Classes:
    def __getitem__(self, index: int) -> type[Box[int]]:
        return Box

def requires_type(cls: type) -> None: ...
def from_class[T: type](cls: T) -> T:
    return cls

requires_type(Classes()[0])  # no diagnostic
super(Classes()[0], Box[int]())  # no diagnostic
super(Box, Classes()[0])  # no diagnostic
from_class(Classes()[0])  # no diagnostic
from_class(Box[int])  # error: [invalid-argument-type]
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

## Attributes of specialized generic aliases

Aliases retain their own runtime attributes and forward other attributes to their origin.

```py
reveal_type(list[int].__class__)  # revealed: <class 'GenericAlias'>
reveal_type(list[int].__args__)  # revealed: tuple[Any, ...]
list[int].__origin__  # no diagnostic
list[int].__mro_entries__((object,))  # no diagnostic
list[int].missing  # error: [unresolved-attribute]
```
