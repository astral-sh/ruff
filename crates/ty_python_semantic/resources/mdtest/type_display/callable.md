# Display of callable types

## Default values

A concrete function displays its default values. Its abstract callable type records only which
parameters can be omitted, including positional-only and keyword-only parameters.

```py
from ty_extensions._internal import RegularCallableTypeOf

def f(x: int = 1, /, y: str = "value", *, z: bool = True) -> None: ...

reveal_type(f)  # revealed: def f(x: int = 1, /, y: str = "value", *, z: bool = True) -> None

def _(callback: RegularCallableTypeOf[f]):
    reveal_type(callback)  # revealed: (x: int = ..., /, y: str = ..., *, z: bool = ...) -> None
```

## Overloaded defaults

Each overload follows the same distinction between a concrete function and an abstract callable.

```py
from typing import overload
from ty_extensions._internal import RegularCallableTypeOf

@overload
def f(x: int = 1) -> None: ...
@overload
def f(x: str = "value") -> None: ...
def f(x: int | str = 1) -> None: ...

reveal_type(f)  # revealed: Overload[(x: int = 1) -> None, (x: str = "value") -> None]

def _(callback: RegularCallableTypeOf[f]):
    reveal_type(callback)  # revealed: Overload[(x: int = ...) -> None, (x: str = ...) -> None]
```

## Nested defaults

Nested types choose their own display policy. Abstract callback annotations and return types use
ellipsis, while a concrete function represented by `TypeOf` retains its value even inside an
abstract signature.

```py
from ty_extensions._internal import RegularCallableTypeOf, TypeOf

def inner(x: int = 1) -> None: ...
def outer(callback: RegularCallableTypeOf[inner], concrete: TypeOf[inner], flag: bool = True) -> RegularCallableTypeOf[inner]:
    return callback

# revealed: def outer(callback: (x: int = ...) -> None, concrete: def inner(x: int = 1) -> None, flag: bool = True) -> ((x: int = ...) -> None)
reveal_type(outer)

def _(callback: RegularCallableTypeOf[outer]):
    # revealed: (callback: (x: int = ...) -> None, concrete: def inner(x: int = 1) -> None, flag: bool = ...) -> ((x: int = ...) -> None)
    reveal_type(callback)
```

## Defaults in nested collections

Promoting a function through nested collections produces an abstract callable. Diagnostics display
the parameter's optionality without depending on its concrete default.

```py
def f(n=5): ...

# error: [invalid-assignment] "Invalid subscript assignment with key of type `Literal[0]` and value of type `None` on object of type `list[list[(n=...) -> Unknown]]`"
[[f]][0] = None
```

## Parenthesizing callables

### Simple

We parenthesize callable types when they appear inside more complex types, to disambiguate:

```py
from typing import Callable

def f(x: Callable[[], str] | Callable[[int], str]):
    reveal_type(x)  # revealed: (() -> str) | ((int, /) -> str)
```

### Overloaded

We don't parenthesize display of an overloaded callable, since it is already wrapped in
`Overload[...]`:

```py
from typing import Callable, Literal, overload
from ty_extensions._internal import RegularCallableTypeOf

@overload
def f(x: int) -> bool: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str) -> bool | str:
    return bool(x) if isinstance(x, int) else str(x)

def _(x: RegularCallableTypeOf[f] | Literal[True]):
    reveal_type(x)  # revealed: Overload[(x: int) -> bool, (x: str) -> str] | Literal[True]
```

When a union would otherwise display two distinct overloaded callables identically, we include their
names to avoid implying that the union contains duplicate elements:

```py
def f(flag: bool):
    x = str.upper if flag else str.lower
    # revealed: (Overload[def upper(self: LiteralString) -> LiteralString, def upper(self) -> str]) | (Overload[def lower(self: LiteralString) -> LiteralString, def lower(self) -> str])
    reveal_type(x)
```

### Top

And we don't parenthesize the top callable, since it is wrapped in `Top[...]`:

```py
from typing import Callable
from ty_extensions import Top

def f(x: Top[Callable[..., str]] | Callable[[int], int]):
    reveal_type(x)  # revealed: Top[(...) -> str] | ((int, /) -> int)
```

## ParamSpec defaults

```toml
[environment]
python-version = "3.12"
```

A `ParamSpec` inferred from a function retains optional parameters but displays only their default
presence.

```py
from typing import Callable

class C[**P]:
    def __init__(self, callback: Callable[P, None]) -> None: ...

def f(x: int = 1) -> None: ...

reveal_type(C(f))  # revealed: C[(x: int = ...)]
```

## Top ParamSpec

```toml
[environment]
python-version = "3.12"
```

We wrap the signature of a top ParamSpec with `Top[...]`:

```py
from typing import Callable

class C[**P]:
    def __init__(self, f: Callable[P, object]) -> None:
        self.f = f

def _(x: object):
    if callable(x):
        c = C(x)
        reveal_type(c)  # revealed: C[Top[(...)]]
```

## Type aliases are not expanded unless necessary

```toml
[environment]
python-version = "3.12"
```

```py
type Scalar = int | float
type Array1d = list[Scalar] | tuple[Scalar]

def f(x: Scalar | Array1d) -> None:
    pass

reveal_type(f)  # revealed: def f(x: Scalar | Array1d) -> None

class Foo:
    def f(self, x: Scalar | Array1d) -> None:
        pass

reveal_type(Foo().f)  # revealed: bound method Foo.f(x: Scalar | Array1d) -> None

type ArrayNd = Scalar | list[ArrayNd] | tuple[ArrayNd]

def g(x: Scalar | ArrayNd) -> None:
    pass

reveal_type(g)  # revealed: def g(x: Scalar | ArrayNd) -> None

class Bar:
    def g(self, x: Scalar | ArrayNd) -> None:
        pass

reveal_type(Bar().g)  # revealed: bound method Bar.g(x: Scalar | ArrayNd) -> None

type GenericArray1d[T] = list[T] | tuple[T]

def h(x: Scalar | GenericArray1d[Scalar]) -> None:
    pass

reveal_type(h)  # revealed: def h(x: Scalar | GenericArray1d[Scalar]) -> None

class Baz:
    def h(self, x: Scalar | GenericArray1d[Scalar]) -> None:
        pass

reveal_type(Baz().h)  # revealed: bound method Baz.h(x: Scalar | GenericArray1d[Scalar]) -> None
```
