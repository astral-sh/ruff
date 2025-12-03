# `Annotated`

`Annotated` attaches arbitrary metadata to a given type.

## Usages

`Annotated[T, ...]` is equivalent to `T`: All metadata arguments are simply ignored.

```py
from typing_extensions import Annotated

def _(x: Annotated[int, "foo"]):
    reveal_type(x)  # revealed: int

def _(x: Annotated[int, lambda: 0 + 1 * 2 // 3, _(4)]):
    reveal_type(x)  # revealed: int

def _(x: Annotated[int, "arbitrary", "metadata", "elements", "are", "fine"]):
    reveal_type(x)  # revealed: int

def _(x: Annotated[tuple[str, int], bytes]):
    reveal_type(x)  # revealed: tuple[str, int]
```

## Parameterization

It is invalid to parameterize `Annotated` with less than two arguments.

```py
from typing_extensions import Annotated

# error: [invalid-type-form] "`typing.Annotated` requires at least two arguments when used in a type expression"
def _(x: Annotated):
    reveal_type(x)  # revealed: Unknown

def _(flag: bool):
    if flag:
        X = Annotated
    else:
        X = bool

    # error: [invalid-type-form] "`typing.Annotated` requires at least two arguments when used in a type expression"
    def f(y: X):
        reveal_type(y)  # revealed: Unknown | bool

# error: [invalid-type-form] "`typing.Annotated` requires at least two arguments when used in a type expression"
def _(x: Annotated | bool):
    reveal_type(x)  # revealed: Unknown | bool

# error: [invalid-type-form] "Special form `typing.Annotated` expected at least 2 arguments (one type and at least one metadata element)"
def _(x: Annotated[()]):
    reveal_type(x)  # revealed: Unknown

# error: [invalid-type-form]
def _(x: Annotated[int]):
    # `Annotated[T]` is invalid and will raise an error at runtime,
    # but we treat it the same as `T` to provide better diagnostics later on.
    # The subscription itself is still reported, regardless.
    # Same for the `(int,)` form below.
    reveal_type(x)  # revealed: int

# error: [invalid-type-form]
def _(x: Annotated[(int,)]):
    reveal_type(x)  # revealed: int
```

## Inheritance

### Correctly parameterized

Inheriting from `Annotated[T, ...]` is equivalent to inheriting from `T` itself.

```py
from typing_extensions import Annotated
from ty_extensions import reveal_mro

class C(Annotated[int, "foo"]): ...

# revealed: (<class 'C'>, <class 'int'>, <class 'object'>)
reveal_mro(C)

class D(Annotated[list[str], "foo"]): ...

# revealed: (<class 'D'>, <class 'list[str]'>, <class 'MutableSequence[str]'>, <class 'Sequence[str]'>, <class 'Reversible[str]'>, <class 'Collection[str]'>, <class 'Iterable[str]'>, <class 'Container[str]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(D)

class E(Annotated[list["E"], "metadata"]): ...

# error: [revealed-type] "Revealed MRO: (<class 'E'>, <class 'list[E]'>, <class 'MutableSequence[E]'>, <class 'Sequence[E]'>, <class 'Reversible[E]'>, <class 'Collection[E]'>, <class 'Iterable[E]'>, <class 'Container[E]'>, typing.Protocol, typing.Generic, <class 'object'>)"
reveal_mro(E)
```

### Not parameterized

```py
from typing_extensions import Annotated
from ty_extensions import reveal_mro

# At runtime, this is an error.
# error: [invalid-base]
class C(Annotated): ...

reveal_mro(C)  # revealed: (<class 'C'>, Unknown, <class 'object'>)
```
