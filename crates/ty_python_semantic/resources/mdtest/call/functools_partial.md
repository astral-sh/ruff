# `functools.partial`

## Basic reduction and invocation

### Basic positional binding

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

### Keyword binding

Keyword-bound parameters are kept with a default, but they become keyword-only in the resulting
callable. `partial` allows overriding them at call time, but only by keyword.

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: partial[(a: int, *, b: str = "hello") -> bool]
```

### Mixed positional and keyword binding

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, 1, c=3.14)
reveal_type(p)  # revealed: partial[(b: str, *, c: int | float = ...) -> bool]
```

### All args bound

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1, "hello")
reveal_type(p)  # revealed: partial[() -> bool]
```

### No args bound

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f)
reveal_type(p)  # revealed: partial[(a: int, b: str) -> bool]
```

### Positional-only params

```py
from functools import partial

def f(a: int, b: str, /) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str, /) -> bool]
```

### Keyword-only params

```py
from functools import partial

def f(a: int, *, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(*, b: str) -> bool]
```

### Keyword-only params bound by keyword

```py
from functools import partial

def f(a: int, *, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: partial[(a: int, *, b: str = "hello") -> bool]
```

### Variadic preserved

```py
from functools import partial

def f(a: int, *args: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(*args: str) -> bool]
```

### Keyword variadic preserved

```py
from functools import partial

def f(a: int, **kwargs: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(**kwargs: str) -> bool]
```

### Defaults preserved

```py
from functools import partial

def f(a: int, b: str = "default") -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str = "default") -> bool]
```

### Lambda

```py
from functools import partial

p = partial(lambda x, y: x + y, 1)
reveal_type(p)  # revealed: partial[(y: Any) -> Unknown]
```

### Bound method

```py
from functools import partial

class Greeter:
    def greet(self, name: str, greeting: str = "Hello") -> str:
        return f"{greeting}, {name}"

g = Greeter()
p = partial(g.greet, "world")
reveal_type(p)  # revealed: partial[(greeting: str = "Hello") -> str]
reveal_type(p())  # revealed: str
```

### Calling the partial result

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, 1)
reveal_type(p("hello", 3.14))  # revealed: bool
reveal_type(p(b="hello", c=3.14))  # revealed: bool
```

## Construction-time diagnostics

### Wrong positional arg type

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, "not_an_int")  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

### Wrong keyword arg type

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b=42)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[(a: int, *, b: str = 42) -> bool]
```

### Unknown keyword argument

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, c=1)  # error: [unknown-argument]
```

### Parameter already assigned

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1, a=2)  # error: [parameter-already-assigned]
```

### Too-many-positional is reported at partial construction

```py
from functools import partial

def f(a: int, b: int) -> int:
    return a + b

p = partial(f, 1, 2, 3)  # error: [too-many-positional-arguments]
reveal_type(p)  # revealed: partial[() -> int]
p()
p(1)  # error: [too-many-positional-arguments]
```

### Non-callable first argument

`partial(42)` is an error caught by the constructor call; we fall back to the default `partial[T]`
type.

```py
from functools import partial

p = partial(42)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[Unknown]
```

### Keyword binding to positional-only param

Positional-only parameters cannot be bound by keyword in `partial()`. The parameter should be
preserved in the resulting callable, while still reporting a construction-time error:

```py
from functools import partial

def f(x: int, /, y: str) -> bool:
    return True

# `x` is positional-only, so `x=1` does not bind it.
p = partial(f, x=1)  # error: [positional-only-parameter-as-kwarg]
reveal_type(p)  # revealed: partial[(x: int, /, y: str) -> bool]
```

## Generics, overloads, and signature inference

### Generic functions

Type variables are inferred from the bound arguments:

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def identity(x: T) -> T:
    return x

p = partial(identity, 1)
reveal_type(p)  # revealed: partial[() -> Literal[1]]
```

### Generic functions with remaining params

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def pair(a: T, b: T) -> tuple[T, T]:
    return (a, b)

p = partial(pair, 1)
reveal_type(p)  # revealed: partial[(b: int) -> tuple[int, int]]
reveal_type(p(2))  # revealed: tuple[int, int]
reveal_type(p(2)[1])  # revealed: int
```

### Generic functions preserve defaults for no-longer-inferable type params

```py
from functools import partial
from typing import cast
from typing_extensions import TypeVar

T = TypeVar("T")
U = TypeVar("U", default=T)

def with_default(x: T) -> tuple[T, U]:
    return (x, cast(U, x))

reveal_type(with_default(1))  # revealed: tuple[Literal[1], Literal[1]]

p = partial(with_default, 1)
reveal_type(p)  # revealed: partial[() -> tuple[Literal[1], Literal[1]]]
reveal_type(p())  # revealed: tuple[Literal[1], Literal[1]]
```

### Generic constructors

```py
from functools import partial
from typing import Generic, TypeVar

T = TypeVar("T")

class Box(Generic[T]):
    def __init__(self, value: T) -> None:
        self.value = value

list_factory = partial(list, [1])
# TODO: should reveal `partial[() -> list[int]]` once constructor partials are modeled.
reveal_type(list_factory)  # revealed: partial[Unknown]
# TODO: should reveal `list[int]` once constructor partials are modeled.
reveal_type(list_factory())  # revealed: Unknown

box_factory = partial(Box, "hi")
# TODO: should reveal `partial[() -> Box[str]]` once constructor partials are modeled.
reveal_type(box_factory)  # revealed: partial[Unknown]
# TODO: should reveal `Box[str]` once constructor partials are modeled.
reveal_type(box_factory())  # revealed: Unknown
```

### Stored partials specialize through generic instances

```py
from functools import partial
from typing import Callable, Generic, TypeVar

T = TypeVar("T")

def identity(x: T) -> T:
    return x

class Box(Generic[T]):
    def __init__(self, value: T) -> None:
        self.callback = partial(identity, value)

box = Box[int](1)
reveal_type(box.callback)  # revealed: partial[() -> int]

target: Callable[[], int] = box.callback
```

### Overloaded functions

```py
from functools import partial
from typing import overload

@overload
def f(a: int) -> int: ...
@overload
def f(a: str) -> str: ...
def f(a: int | str) -> int | str:
    return a

p = partial(f, 1)
reveal_type(p)  # revealed: partial[() -> int]
```

### Union of callables preserves union of partials

```py
from functools import partial
from typing import Callable

def zero_arg(x: int) -> int:
    return x

def one_arg(x: int, y: str) -> int:
    return x + len(y)

def test_union_partial(
    f: Callable[[int], int] | Callable[[int, str], int],
) -> None:
    p = partial(f, 1)
    reveal_type(p)  # revealed: partial[() -> int] | partial[(str, /) -> int]

    bad: Callable[[bytes, bytes], int] = p  # error: [invalid-assignment]
```

### Keyword-bound overload filtering

```py
from functools import partial
from typing import overload

@overload
def g(path: bytes, start: bytes | None = b".") -> bytes: ...
@overload
def g(path: str, start: str | None = ".") -> str: ...
def g(path: bytes | str, start: bytes | str | None = None) -> bytes | str:
    return path

p = partial(g, start=".")
paths: list[str] = ["x"]
reveal_type(p)  # revealed: partial[(path: str, *, start: str | None = ".") -> str]
reveal_type(list(map(p, paths)))  # revealed: list[str]
```

### ParamSpec callable bound with `partial`

```py
from functools import partial
from typing import Any, Callable, TypeVar
from typing_extensions import ParamSpec

P = ParamSpec("P")
R = TypeVar("R")

def invoke(func: Callable[P, R], *args: P.args, **kwargs: P.kwargs) -> R:
    return func(*args, **kwargs)

def pre(cfg: Any) -> Any:
    return cfg

bound = partial(invoke, pre)
reveal_type(bound)  # revealed: partial[(cfg: Any) -> Any]
reveal_type(bound({}))  # revealed: Any
```

### ParamSpec callable with keyword-bound wrapper parameters

```py
from functools import partial
from typing import Callable, TypeVar
from typing_extensions import ParamSpec

P = ParamSpec("P")
R = TypeVar("R")

def invoke(flag: int, func: Callable[P, R], *args: P.args, **kwargs: P.kwargs) -> R:
    return func(*args, **kwargs)

def pre(*, cfg: str) -> int:
    return 1

bound = partial(invoke, flag=1, func=pre)
reveal_type(bound(cfg="x"))  # revealed: int
```

### Partial assignability with a keyword-bound middle parameter

```py
from functools import partial
from typing import Any, Protocol

class Conv(Protocol):
    def __call__(self, __x: Any, *, _target_: str = "set", CBuildsFn: type[Any]) -> Any: ...

class ConfigFromTuple:
    def __init__(self, _args_: tuple[Any, ...], _target_: str, CBuildsFn: type[Any]) -> None: ...

p = partial(ConfigFromTuple, _target_="set")
# TODO: should preserve the keyword-bound middle parameter in the reduced signature.
reveal_type(p)  # revealed: partial[Unknown]

conversion: dict[type, Conv] = {}
conversion[set] = p
```

### Overloaded stdlib callable narrowed by bound args

`partial(reduce, operator.mul)` should keep the narrowed return type from the bound reducer:

```py
from functools import partial, reduce
import operator

prod = partial(reduce, operator.mul)
shape: list[int] = [1, 2, 3]

reveal_type(prod(shape))  # revealed: Any
```

### Overloaded stdlib callable with keyword-only binding

`partial(zip, strict=True)` should accept the keyword-only argument and preserve the element types
of the resulting iterator:

```toml
[environment]
python-version = "3.12"
```

```py
from functools import partial
import builtins

zips = partial(builtins.zip, strict=True)

xs = [1]
ys = ["a"]
pairs = list(zips(xs, ys))

# TODO: should reveal `list[tuple[int, str]]` once keyword-only constructor bindings are preserved.
reveal_type(pairs)  # revealed: list[Unknown]
```

### Keyword argument with literal sequence annotation

`partial(...)` should accept keyword arguments whose literal container types are inferred without
context at the call site:

```py
from functools import partial
from typing import Literal, Sequence

Distribution = Literal["sdist", "wheel", "editable"]

def build(distributions: Sequence[Distribution]) -> None:
    pass

p = partial(build, distributions=["wheel"])  # error: [invalid-argument-type]
# TODO: should accept this keyword literal without a construction-time error.
reveal_type(p)  # revealed: partial[(*, distributions: Sequence[Literal["sdist", "wheel", "editable"]] = ...) -> None]
reveal_type(p())  # revealed: None
```

### Keyword argument with empty literal sequence annotation

`partial(...)` should still re-run argument refinement even when the initial constructor binding
already succeeds, so empty literals keep the parameter's contextual element type:

```py
from functools import partial
from typing import Literal, Sequence

Distribution = Literal["sdist", "wheel", "editable"]

def build(distributions: Sequence[Distribution]) -> None:
    pass

p = partial(build, distributions=[])
reveal_type(p)  # revealed: partial[(*, distributions: Sequence[Literal["sdist", "wheel", "editable"]] = ...) -> None]
reveal_type(p())  # revealed: None
```

### Overloaded functions with remaining params

```py
from functools import partial
from typing import overload

@overload
def g(a: int, b: str) -> int: ...
@overload
def g(a: str, b: str) -> str: ...
def g(a: int | str, b: str) -> int | str:
    return a

p = partial(g, 1)
reveal_type(p)  # revealed: partial[(b: str) -> int]
```

## Argument unpacking and nested partials

### Starred args with fixed-length tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args: tuple[int] = (1,)
p = partial(f, *args)
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

### Starred args with multiple elements

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[int, str] = (1, "hello")
p = partial(f, *args)
reveal_type(p)  # revealed: partial[(c: int | float) -> bool]
```

### Mixed positional and starred args

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[str] = ("hello",)
p = partial(f, 1, *args)
reveal_type(p)  # revealed: partial[(c: int | float) -> bool]
```

### Fallback for starred args with variable-length tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

def get_args() -> tuple[int, ...]:
    return (1,)

p = partial(f, *get_args())
reveal_type(p)  # revealed: partial[bool]
```

### Kwargs splat with TypedDict

```py
from functools import partial
from typing import TypedDict

class MyKwargs(TypedDict):
    b: str

def f(a: int, b: str) -> bool:
    return True

kwargs: MyKwargs = {"b": "hello"}
p = partial(f, **kwargs)
reveal_type(p)  # revealed: partial[(a: int, *, b: str = ...) -> bool]
```

### Mixed keywords and kwargs splat

```py
from functools import partial
from typing import TypedDict

class MyKwargs(TypedDict):
    c: float

def f(a: int, b: str, c: float) -> bool:
    return True

kwargs: MyKwargs = {"c": 3.14}
p = partial(f, b="hello", **kwargs)
reveal_type(p)  # revealed: partial[(a: int, *, b: str = "hello", c: int | float = ...) -> bool]
```

### Fallback for kwargs splat with dict

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

kwargs = {"a": 1}
p = partial(f, **kwargs)
reveal_type(p)  # revealed: partial[bool]
```

### Fallback for kwargs splat with optional TypedDict keys

```py
from functools import partial
from typing import TypedDict

class MaybeKwargs(TypedDict, total=False):
    b: str

def f(a: int, *, b: str) -> None:
    pass

def make(kwargs: MaybeKwargs) -> None:
    p = partial(f, **kwargs)
    reveal_type(p)  # revealed: partial[None]
```

### Nested partial

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p1 = partial(f, 1)
reveal_type(p1)  # revealed: partial[(b: str, c: int | float) -> bool]

p2 = partial(p1, "hello")
reveal_type(p2)  # revealed: partial[(c: int | float) -> bool]
```

## Constructors and advanced signatures

### Class constructor

```py
from functools import partial

class MyClass:
    def __init__(self, x: int, y: str) -> None:
        pass

p = partial(MyClass, 1)
# TODO: should reveal `partial[(y: str) -> MyClass]` once constructor partials are modeled.
reveal_type(p)  # revealed: partial[Unknown]
```

### Class constructor with both `__new__` and `__init__`

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, x: int) -> None: ...

p = partial(MyClass, 1)
reveal_type(p)  # revealed: partial[Unknown]
p()
# TODO: should reveal `partial[() -> MyClass]` once constructor signatures are merged.
# TODO: should error: [too-many-positional-arguments] once constructor signatures are merged.
p("extra")
```

### Class constructor partial preserves one-sided bound `__new__` positional params

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int) -> "MyClass":
        return super().__new__(cls)
    def __init__(self) -> None: ...

p = partial(MyClass, 1)
# TODO: should reveal `partial[(x: Never) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should error: [missing-argument] once constructor signatures are merged.
p()
# TODO: should error: [invalid-argument-type] once constructor signatures are merged.
p(1)
```

### Class constructor partial preserves one-sided bound `__new__` keyword params

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int) -> "MyClass":
        return super().__new__(cls)
    def __init__(self) -> None: ...

p = partial(MyClass, x=1)
# TODO: should reveal `partial[(*, x: Never) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should error: [missing-argument] once constructor signatures are merged.
p()
# TODO: should error: [invalid-argument-type] once constructor signatures are merged.
p(x=1)
```

### Class constructor preserves downstream params after partial binding

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, x: int, y: str) -> None: ...

p = partial(MyClass, 1)
# TODO: should reveal `partial[(y: Never) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should error: [missing-argument] once constructor signatures are merged.
p()
# TODO: should error: [invalid-argument-type] once constructor signatures are merged.
p("extra")
```

### Class constructor partial preserves one-sided `__init__` params

```py
from functools import partial

class MyClass:
    def __new__(cls) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, x: int) -> None: ...

p = partial(MyClass)
# TODO: should reveal `partial[(x: Never) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should error: [missing-argument] once constructor signatures are merged.
p()
# TODO: should error: [invalid-argument-type] once constructor signatures are merged.
p(1)
```

### Class constructor partial preserves downstream keyword-only params

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, *, y: str) -> None: ...

p = partial(MyClass, 1)
# TODO: should reveal `partial[(x: Never, *, y: Never) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should error: [missing-argument] once constructor signatures are merged.
p()
# TODO: should error: [missing-argument] and [invalid-argument-type] once constructor signatures are merged.
p(y="extra")
```

### Class constructor partial keeps the narrower subtype-compatible signature

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, x: object) -> None: ...

p = partial(MyClass)
# TODO: should reveal `partial[(x: int) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should reveal `MyClass` once constructor signatures are merged.
reveal_type(p(1))  # revealed: Unknown
# TODO: should error: [invalid-argument-type] once constructor signatures are merged.
p("s")
```

### Class constructor partial matches reordered params by name

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int, *, y: str) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, *, y: str, x: int) -> None: ...

p = partial(MyClass)
# TODO: should reveal `partial[(*, x: int, y: str) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should reveal `MyClass` once constructor signatures are merged.
reveal_type(p(x=1, y="s"))  # revealed: Unknown
# TODO: should error: [missing-argument] once constructor signatures are merged.
p(y="s")
```

### Class constructor partial keeps reordered positional params keyword-only

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int, y: str) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, y: str, x: int) -> None: ...

p = partial(MyClass)
# TODO: should reveal `partial[(*, x: int, y: str) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should reveal `MyClass` once constructor signatures are merged.
reveal_type(p(x=1, y="s"))  # revealed: Unknown
# TODO: should error: [missing-argument] and [too-many-positional-arguments] once constructor signatures are merged.
p(1, "s")
```

### Class constructor partial preserves both `__new__` and `__init__`

```py
from functools import partial

class MyClass:
    def __new__(cls, x: int | str) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, x: int) -> None: ...

p = partial(MyClass)
# TODO: should reveal `partial[(x: int) -> MyClass]` once constructor signatures are merged.
reveal_type(p)  # revealed: partial[Unknown]
p(1)
# TODO: should error: [invalid-argument-type] once constructor signatures are merged.
p("s")
```

### Class constructor partial preserves per-overload correlations

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

class MyClass:
    def __new__(cls, x: T, y: T) -> "MyClass":
        return super().__new__(cls)
    def __init__(self, x: int, y: str) -> None: ...

p = partial(MyClass)
# TODO: should error twice with [invalid-argument-type] once constructor signatures are merged.
p(1, "s")
```

### Class constructor partial keeps non-instance `__new__` overloads

```py
from __future__ import annotations

from functools import partial
from typing import overload

class MyClass:
    @overload
    def __new__(cls, x: int) -> "MyClass": ...
    @overload
    def __new__(cls, x: str) -> str: ...
    def __new__(cls, x: int | str) -> "MyClass" | str:
        if isinstance(x, str):
            return x
        return super().__new__(cls)

    def __init__(self, x: int) -> None: ...

p = partial(MyClass)
# TODO: should preserve the non-instance `__new__` overload in the reduced partial signature.
reveal_type(p)  # revealed: partial[Unknown]
# TODO: should reveal `MyClass` once constructor signatures are merged.
reveal_type(p(1))  # revealed: Unknown
# TODO: should reveal `str` once constructor signatures are merged.
reveal_type(p("s"))  # revealed: Unknown
```

## Additional signature-shaping cases

### Binding a default parameter

Binding a parameter that has a default value removes it from the signature.

```py
from functools import partial

def f(a: int, b: str = "default", c: float = 0.0) -> bool:
    return True

p = partial(f, 1, "hello")
reveal_type(p)  # revealed: partial[(c: int | float = ...) -> bool]
```

### Multiple keyword bindings

```py
from functools import partial

def f(a: int, b: str, c: float, d: bool) -> int:
    return 0

p = partial(f, b="hello", d=True)
reveal_type(p)  # revealed: partial[(a: int, *, b: str = "hello", c: int | float, d: bool = True) -> int]
```

### Mixed positional-only, regular, and keyword-only

```py
from functools import partial

def f(a: int, /, b: str, *, c: float) -> bool:
    return True

# Bind the positional-only param
p1 = partial(f, 1)
reveal_type(p1)  # revealed: partial[(b: str, *, c: int | float) -> bool]

# Bind a keyword-only param by keyword
p2 = partial(f, c=3.14)
reveal_type(p2)  # revealed: partial[(a: int, /, b: str, *, c: int | float = ...) -> bool]

# Bind both positional-only and keyword-only
p3 = partial(f, 1, c=3.14)
reveal_type(p3)  # revealed: partial[(b: str, *, c: int | float = ...) -> bool]
```

### Starred args combined with keyword args

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[int] = (1,)
p = partial(f, *args, c=3.14)
reveal_type(p)  # revealed: partial[(b: str, *, c: int | float = ...) -> bool]
```

### Starred args with empty tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args: tuple[()] = ()
p = partial(f, *args)
reveal_type(p)  # revealed: partial[(a: int, b: str) -> bool]
```

### Generic function with multiple type variables

TODO: preserve uninferred type variables in the resulting partial signature.

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")
U = TypeVar("U")

def combine(a: T, b: U) -> tuple[T, U]:
    return (a, b)

p = partial(combine, 1)
reveal_type(p)  # revealed: partial[(b: Unknown) -> tuple[Literal[1], Unknown]]
```

### Callable object (class with `__call__`)

```py
from functools import partial

class Adder:
    def __call__(self, a: int, b: int) -> int:
        return a + b

adder = Adder()
p = partial(adder, 1)
reveal_type(p)  # revealed: partial[(b: int) -> int]
```

### Staticmethod

```py
from functools import partial

class MyClass:
    @staticmethod
    def f(a: int, b: str) -> bool:
        return True

p = partial(MyClass.f, 1)
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

### Overloaded function with later matching overload

When the bound argument matches a later overload but not the first, no error should be emitted:

```py
from functools import partial
from typing import overload

@overload
def f(a: int) -> int: ...
@overload
def f(a: str) -> str: ...
def f(a: int | str) -> int | str:
    return a

# "hello" matches the second overload (str -> str), so no error.
p = partial(f, "hello")
reveal_type(p)  # revealed: partial[() -> str]
```

### Overriding keyword-bound args at call time

`partial` allows keyword arguments to be overridden when calling the result:

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: partial[(a: int, *, b: str = "hello", c: int | float) -> bool]

# Override b at call time
reveal_type(p(1, b="world", c=3.14))  # revealed: bool
```

### Overriding keyword-bound generic args at call time

TODO: preserve the override branch when a keyword-bound generic is rebound at call time.

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def pair(a: T, b: T) -> tuple[T, T]:
    return (a, b)

p = partial(pair, b=1)
reveal_type(p)  # revealed: partial[(a: int, *, b: int = 1) -> tuple[int, int]]
p("x")  # error: [invalid-argument-type]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
p("x", b="y")
```

## Assignability and partial object behavior

### Assignability to callable

A `partial` result is assignable to a `Callable` with the matching signature:

```py
from functools import partial
from typing import Callable

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str) -> bool]

def takes_callable(fn: Callable[[str], bool]) -> None:
    pass

takes_callable(p)  # OK -- partial[(b: str) -> bool] is callable with (str) -> bool

def takes_wrong_callable(fn: Callable[[int], bool]) -> None:
    pass

takes_wrong_callable(p)  # error: [invalid-argument-type]

def returns_partial() -> partial[bool]:
    return p  # OK -- partial[(b: str) -> bool] is assignable to partial[bool]
```

### Assignability to a stub-style function alias

```py
from functools import partial
from typing import TYPE_CHECKING

def f(a: int, b: str | None, c: dict[str, int]) -> dict[str, int]:
    return c

if TYPE_CHECKING:
    def g(a: int, b: str | None) -> dict[str, int]: ...

g = partial(f, c={})
```

### Ambiguous binding preserves overload

```py
from functools import partial
from typing import overload

@overload
def f(a: int) -> int: ...
@overload
def f(a: str) -> str: ...
def f(a: int | str) -> int | str:
    return a

def make_partial(x):
    p = partial(f, x)
    reveal_type(p)  # revealed: partial[Overload[() -> int, () -> str]]
    return p

p = make_partial(1)
```

### Invalid overloaded binding falls back to default partial type

```py
from functools import partial
from typing import overload

@overload
def f(a: int) -> int: ...
@overload
def f(a: str) -> str: ...
def f(a):
    return a

p = partial(f, 1.0)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[Unknown]
```

### Partial of bound classmethod is assignable to zero-arg callable

```py
from functools import partial
from typing import Callable
from typing_extensions import Self

class C:
    @classmethod
    def make(cls, *, x: int = 0) -> Self:
        raise RuntimeError

factory: Callable[[], C] = partial(C.make, x=0)
```

### Partials of nested local functions with same signature

Two `partial(...)` values from distinct nested local functions should still be assignable when their
remaining callable signatures match:

```py
from functools import partial

def outer(x, y):
    def left(a, b):
        return a, x

    def right(a, b):
        return a, y

    branches = [partial(left, 1)]
    branches.append(partial(right, 2))
```

### Keyword-bound predicate remains unary for filter

Binding a predicate parameter via keyword should still produce a unary predicate acceptable to
`filter(...)`:

```py
from functools import partial

def has_same_ip_version(addr_or_net: str, is_ipv6: bool) -> bool:
    return is_ipv6

values = ["127.0.0.1", "::1"]
predicate = partial(has_same_ip_version, is_ipv6=False)
reveal_type(predicate)  # revealed: partial[(addr_or_net: str, *, is_ipv6: bool = False) -> bool]
reveal_type(list(filter(predicate, values)))  # revealed: list[str]
```

### Overloaded partial reports type mismatch, not unknown keyword

```py
from functools import partial
from typing import Callable, Literal, Optional, cast, overload

@overload
def task(__fn: Callable[[], int]) -> int: ...
@overload
def task(
    __fn: Literal[None] = None,
    *,
    retries: int = 0,
) -> Callable[[Callable[[], int]], int]: ...
@overload
def task(
    *,
    retries: int = 0,
) -> Callable[[Callable[[], int]], int]: ...
def task(
    __fn: Optional[Callable[[], int]] = None,
    *,
    retries: Optional[int] = None,
):
    if __fn:
        return 1
    return cast(
        Callable[[Callable[[], int]], int],
        partial(task, retries=retries),  # error: [invalid-argument-type]
    )
```

### Bound classmethod callback with weakref

Binding the first explicit parameter of a bound classmethod callback should preserve assignability
for `ReferenceType[Self]` arguments:

```py
from functools import partial
from typing import Any, Generic, TypeVar
from weakref import ReferenceType, ref

T = TypeVar("T")

class CallbackHost(Generic[T]):
    @classmethod
    def callback(cls, wself: ReferenceType["CallbackHost[Any]"], x: int) -> None: ...
    def __init__(self) -> None:
        p = partial(self.callback, ref(self))  # error: [invalid-argument-type]
        # TODO: should accept `ReferenceType[Self]` here and preserve the reduced signature.
        reveal_type(p)  # revealed: partial[(x: int) -> None]
```

### Assignability to protocol

A `partial` result is assignable to a `Protocol` with a matching `__call__` signature. Required
keyword-only parameters can be bound away, and extra keyword-only parameters with defaults in the
resulting `partial` are allowed, since they don't need to be provided by the caller:

```py
from functools import partial
from typing import Protocol

class Request: ...
class Response: ...
class Context: ...

class Handler(Protocol):
    def __call__(
        self,
        request: Request,
        *,
        header: str | None = None,
    ) -> Response: ...

def handle(
    request: Request,
    *,
    header: str | None = None,
    verbose: bool = False,
    context: Context,
) -> Response:
    return Response()

handler: Handler = partial(handle, context=Context())
```

### Accessing `__call__` directly

`__call__` on a `partial` result should reflect the refined callable signature, not the broad
`(*args: Any, **kwargs: Any) -> T` from the `partial` class stub.

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p.__call__)  # revealed: (b: str) -> bool

reveal_type(p.__call__("hello"))  # revealed: bool
```

### Attribute access on partial results

Standard `partial` attributes like `.func`, `.args`, and `.keywords` should be accessible:

```py
from functools import partial
from typing import Callable

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p.func)  # revealed: def f(a: int, b: str) -> bool
reveal_type(p.func(2, "hello"))  # revealed: bool
reveal_type(p.args)  # revealed: tuple[Any, ...]
reveal_type(p.keywords)  # revealed: dict[str, Any]
```

### `partial.func` keeps the original callable type

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")
U = TypeVar("U")

def combine(a: T, b: U) -> tuple[T, U]:
    return (a, b)

p = partial(combine, 1)
reveal_type(p.func(2, "x"))  # revealed: tuple[Literal[2], Literal["x"]]
```

### Attribute assignment on partial results

Attribute assignment should go through the standard nominal instance path:

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
p.func = f  # error: [invalid-assignment]
```

### Unknown attribute assignment on partial results

We intentionally reject ad-hoc attributes on `functools.partial` results. This matches `pyright` and
`mypy`, even though these assignments work at runtime.

```py
from functools import partial

def f() -> None:
    pass

p = partial(f)
# error: [unresolved-attribute]
p.__name__ = "renamed"
```
