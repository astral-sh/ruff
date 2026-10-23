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

## String annotations

Metadata in a string annotation can include calls with unpacked dictionaries. The metadata does not
affect the annotated type, regardless of where the annotation appears.

```py
from typing_extensions import Annotated

value: "Annotated[int, dict(**{})]"

def convert(value: "Annotated[str, dict(**{'name': 'value'})]") -> "Annotated[int, dict(**{})]":
    reveal_type(value)  # revealed: str
    return 1
```

Conditional expressions are also valid metadata and do not affect the annotated type.

```py
def flag() -> bool:
    return True

conditional_value: "Annotated[int, 1 if flag() else 2]" = 1
```

## Inside `type[...]`

`Annotated` can wrap a class or specialized generic class inside `type[...]` without changing the
resulting class object type.

```py
from typing_extensions import Annotated

def _(
    simple: type[Annotated[int, "metadata"]],
    generic: type[Annotated[list[str], "metadata"]],
):
    reveal_type(simple)  # revealed: type[int]
    reveal_type(generic)  # revealed: type[list[str]]
```

This also works for unions of classes and nested `Annotated` forms.

```py
def _(
    union: type[Annotated[int | str, "metadata"]],
    nested: type[Annotated[Annotated[int, "inner"], "outer"]],
):
    reveal_type(union)  # revealed: type[int | str]
    reveal_type(nested)  # revealed: type[int]
```

Wrapping a non-class type in `Annotated` does not make it a valid argument to `type[...]`.

```py
from typing import Callable

def _(
    # error: [invalid-type-form] "The argument to `type[]` must be a class object type"
    invalid: type[Annotated[Callable[[], int], "metadata"]],
):
    reveal_type(invalid)  # revealed: type[Unknown]
```

## Parameterization

It is invalid to parameterize `Annotated` with less than two arguments.

```py
from typing_extensions import Annotated

# error: [invalid-type-form] "`typing.Annotated` requires at least two arguments when used in a parameter annotation"
def _(x: Annotated):
    reveal_type(x)  # revealed: Unknown

def _(flag: bool):
    if flag:
        X = Annotated
    else:
        X = bool

    # error: [invalid-type-form] "`typing.Annotated` requires at least two arguments when used in a parameter annotation"
    def f(y: X):
        reveal_type(y)  # revealed: Unknown | bool

# error: [invalid-type-form] "`typing.Annotated` requires at least two arguments when used in a parameter annotation"
def _(x: Annotated | bool):
    reveal_type(x)  # revealed: Unknown | bool

# error: [invalid-type-form] "Special form `typing.Annotated` expected at least 2 arguments (one type and at least one metadata element)"
# error: [invalid-type-form] "Special form `typing.Annotated` expected at least 2 arguments (one type and at least one metadata element)"
def _(x: Annotated[()], y: list[Annotated[()]]):
    reveal_type(x)  # revealed: Unknown
    reveal_type(y)  # revealed: list[Unknown]

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
from typing_extensions import Annotated, Any
from ty_extensions._internal import reveal_mro

class C(Annotated[int, "foo"]): ...

# revealed: (<class 'C'>, <class 'int'>, <class 'object'>)
reveal_mro(C)

class D(Annotated[list[str], "foo"]): ...

# revealed: (<class 'D'>, <class 'list[str]'>, <class 'MutableSequence[str]'>, <class 'Sequence[str]'>, <class 'Reversible[str]'>, <class 'Collection[str]'>, <class 'Iterable[str]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(D)

class E(Annotated[list["E"], "metadata"]): ...

# error: [revealed-type] "Revealed MRO: (<class 'E'>, <class 'list[E]'>, <class 'MutableSequence[E]'>, <class 'Sequence[E]'>, <class 'Reversible[E]'>, <class 'Collection[E]'>, <class 'Iterable[E]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)"
reveal_mro(E)

class F(Annotated[Any, "metadata"]): ...

# revealed: (<class 'F'>, Any, <class 'object'>)
reveal_mro(F)
```

### Not parameterized

```py
from typing_extensions import Annotated
from ty_extensions._internal import reveal_mro

# At runtime, this is an error.
# error: [invalid-base]
class C(Annotated): ...

reveal_mro(C)  # revealed: (<class 'C'>, Unknown, <class 'object'>)
```

## Recursive runtime values

`TypeOf` can make an `Annotated` value refer to the type of another runtime `Annotated` object.
Inference preserves the outer object and collapses the recursive wrapped type. The resulting
approximation can overlap with a concrete `Annotated` object, so identity comparisons remain
ambiguous.

```py
from typing import Annotated
from ty_extensions._internal import TypeOf

value = int
while True:
    value = Annotated[TypeOf[value], "metadata"]
    reveal_type(value)  # revealed: <special-form 'typing.Annotated[Divergent, <metadata>]'>
    reveal_type(value is Annotated[TypeOf[int], "metadata"])  # revealed: bool
    reveal_type(Annotated[TypeOf[int], "metadata"] is value)  # revealed: bool
    reveal_type(value is not Annotated[TypeOf[int], "metadata"])  # revealed: bool
```

## Recursive inferred attributes

An inferred attribute can store successive runtime `Annotated` objects. The recursive approximation
accepts the next assignment, both with and without a separate initializer.

```py
from typing import Annotated
from ty_extensions._internal import TypeOf

class Initialized:
    def __init__(self):
        self.value = int

    def update(self):
        self.value = Annotated[TypeOf[self.value], "metadata"]

class Uninitialized:
    def update(self):
        self.value = Annotated[TypeOf[self.value], "metadata"]
```

## Distinct runtime values

An `Annotated` object wrapping `int` is distinct from one wrapping `object`, even though `int` is a
subtype of `object`.

```py
from typing import Annotated
from ty_extensions._internal import TypeOf

integer = Annotated[int, "metadata"]
object_ = Annotated[object, "metadata"]

reveal_type(integer is object_)  # revealed: Literal[False]
reveal_type(integer is not object_)  # revealed: Literal[True]

def accepts_integer(value: TypeOf[integer]): ...
def accepts_object(value: TypeOf[object_]): ...

accepts_integer(integer)
accepts_object(object_)
accepts_integer(object_)  # error: [invalid-argument-type]
accepts_object(integer)  # error: [invalid-argument-type]
```
