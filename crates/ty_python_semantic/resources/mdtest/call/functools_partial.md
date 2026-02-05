# `functools.partial`

## Basic positional binding

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

## Keyword binding

Keyword-bound parameters are kept with a default, since `partial` allows overriding keyword
arguments at call time.

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: partial[(a: int, b: str = "hello") -> bool]
```

## Mixed positional and keyword binding

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, 1, c=3.14)
reveal_type(p)  # revealed: partial[(b: str, c: int | float = ...) -> bool]
```

## All args bound

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1, "hello")
reveal_type(p)  # revealed: partial[() -> bool]
```

## No args bound

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f)
reveal_type(p)  # revealed: partial[(a: int, b: str) -> bool]
```

## Positional-only params

```py
from functools import partial

def f(a: int, b: str, /) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str, /) -> bool]
```

## Keyword-only params

```py
from functools import partial

def f(a: int, *, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(*, b: str) -> bool]
```

## Keyword-only params bound by keyword

```py
from functools import partial

def f(a: int, *, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: partial[(a: int, *, b: str = "hello") -> bool]
```

## Variadic preserved

```py
from functools import partial

def f(a: int, *args: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(*args: str) -> bool]
```

## Keyword variadic preserved

```py
from functools import partial

def f(a: int, **kwargs: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(**kwargs: str) -> bool]
```

## Defaults preserved

```py
from functools import partial

def f(a: int, b: str = "default") -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: partial[(b: str = "default") -> bool]
```

## Lambda

```py
from functools import partial

p = partial(lambda x, y: x + y, 1)
reveal_type(p)  # revealed: partial[(y) -> Unknown]
```

## Calling the partial result

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, 1)
reveal_type(p("hello", 3.14))  # revealed: bool
reveal_type(p(b="hello", c=3.14))  # revealed: bool
```

## Wrong positional arg type

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, "not_an_int")  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

## Wrong keyword arg type

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b=42)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[(a: int, b: str = 42) -> bool]
```

## Bound method

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

## Non-callable first argument

`partial(42)` is an error caught by the constructor call; we fall back to the default `partial[T]`
type.

```py
from functools import partial

p = partial(42)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[Unknown]
```

## Generic functions

Type variables are inferred from the bound arguments:

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def identity(x: T) -> T:
    return x

p = partial(identity, 1)
reveal_type(p)  # revealed: partial[() -> int]
```

## Generic functions with remaining params

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def pair(a: T, b: T) -> tuple[T, T]:
    return (a, b)

p = partial(pair, 1)
reveal_type(p)  # revealed: partial[(b: int) -> tuple[int, int]]
reveal_type(p(2))  # revealed: tuple[int, int]
```

## Overloaded functions

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

## Overloaded stdlib callable narrowed by bound args

`partial(reduce, operator.mul)` should keep the narrowed return type from the bound reducer:

```py
from functools import partial, reduce
import operator

prod = partial(reduce, operator.mul)
shape: list[int] = [1, 2, 3]

reveal_type(prod(shape))  # revealed: int
```

## Keyword argument with literal sequence annotation

`partial(...)` should accept keyword arguments whose literal container types are inferred without
context at the call site:

```py
from functools import partial
from typing import Literal, Sequence

Distribution = Literal["sdist", "wheel", "editable"]

def build(distributions: Sequence[Distribution]) -> None:
    pass

p = partial(build, distributions=["wheel"])
reveal_type(p)  # revealed: partial[(distributions: Sequence[Literal["sdist", "wheel", "editable"]] = ...) -> None]
reveal_type(p())  # revealed: None
```

## Overloaded functions with remaining params

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
reveal_type(p)  # revealed: partial[Overload[(b: str) -> int, (b: str) -> str]]
```

## Starred args with fixed-length tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args: tuple[int] = (1,)
p = partial(f, *args)
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

## Starred args with multiple elements

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[int, str] = (1, "hello")
p = partial(f, *args)
reveal_type(p)  # revealed: partial[(c: int | float) -> bool]
```

## Mixed positional and starred args

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[str] = ("hello",)
p = partial(f, 1, *args)
reveal_type(p)  # revealed: partial[(c: int | float) -> bool]
```

## Fallback for starred args with variable-length tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

def get_args() -> tuple[int, ...]:
    return (1,)

p = partial(f, *get_args())
reveal_type(p)  # revealed: partial[bool]
```

## Kwargs splat with TypedDict

```py
from functools import partial
from typing import TypedDict

class MyKwargs(TypedDict):
    b: str

def f(a: int, b: str) -> bool:
    return True

kwargs: MyKwargs = {"b": "hello"}
p = partial(f, **kwargs)
reveal_type(p)  # revealed: partial[(a: int, b: str = ...) -> bool]
```

## Mixed keywords and kwargs splat

```py
from functools import partial
from typing import TypedDict

class MyKwargs(TypedDict):
    c: float

def f(a: int, b: str, c: float) -> bool:
    return True

kwargs: MyKwargs = {"c": 3.14}
p = partial(f, b="hello", **kwargs)
reveal_type(p)  # revealed: partial[(a: int, b: str = "hello", c: int | float = ...) -> bool]
```

## Fallback for kwargs splat with dict

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

kwargs = {"a": 1}
p = partial(f, **kwargs)
reveal_type(p)  # revealed: partial[bool]
```

## Too many positional args

Extra positional arguments beyond the wrapped function's positional parameters are flagged.

```py
from functools import partial

def f(a: int) -> bool:
    return True

p = partial(f, 1, 2, 3)  # error: [too-many-positional-arguments]
reveal_type(p)  # revealed: partial[() -> bool]
```

## Nested partial

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p1 = partial(f, 1)
reveal_type(p1)  # revealed: partial[(b: str, c: int | float) -> bool]

p2 = partial(p1, "hello")
reveal_type(p2)  # revealed: partial[(c: int | float) -> bool]
```

## Class constructor

```py
from functools import partial

class MyClass:
    def __init__(self, x: int, y: str) -> None:
        pass

p = partial(MyClass, 1)
reveal_type(p)  # revealed: partial[(y: str) -> MyClass]
```

## Binding a default parameter

Binding a parameter that has a default value removes it from the signature.

```py
from functools import partial

def f(a: int, b: str = "default", c: float = 0.0) -> bool:
    return True

p = partial(f, 1, "hello")
reveal_type(p)  # revealed: partial[(c: int | float = ...) -> bool]
```

## Multiple keyword bindings

```py
from functools import partial

def f(a: int, b: str, c: float, d: bool) -> int:
    return 0

p = partial(f, b="hello", d=True)
reveal_type(p)  # revealed: partial[(a: int, b: str = "hello", c: int | float, d: bool = True) -> int]
```

## Mixed positional-only, regular, and keyword-only

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

## Starred args combined with keyword args

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[int] = (1,)
p = partial(f, *args, c=3.14)
reveal_type(p)  # revealed: partial[(b: str, c: int | float = ...) -> bool]
```

## Starred args with empty tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args: tuple[()] = ()
p = partial(f, *args)
reveal_type(p)  # revealed: partial[(a: int, b: str) -> bool]
```

## Generic function with multiple type variables

Unresolved type variables are replaced with `Unknown` since the signature is fully specialized.

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")
U = TypeVar("U")

def combine(a: T, b: U) -> tuple[T, U]:
    return (a, b)

p = partial(combine, 1)
reveal_type(p)  # revealed: partial[(b: Unknown) -> tuple[int, Unknown]]
```

## Callable object (class with `__call__`)

```py
from functools import partial

class Adder:
    def __call__(self, a: int, b: int) -> int:
        return a + b

adder = Adder()
p = partial(adder, 1)
reveal_type(p)  # revealed: partial[(b: int) -> int]
```

## Staticmethod

```py
from functools import partial

class MyClass:
    @staticmethod
    def f(a: int, b: str) -> bool:
        return True

p = partial(MyClass.f, 1)
reveal_type(p)  # revealed: partial[(b: str) -> bool]
```

## Overloaded function with later matching overload

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

## Overriding keyword-bound args at call time

`partial` allows keyword arguments to be overridden when calling the result:

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: partial[(a: int, b: str = "hello", c: int | float) -> bool]

# Override b at call time
reveal_type(p(1, b="world", c=3.14))  # revealed: bool
```

## Keyword binding to positional-only param

Positional-only parameters cannot be bound by keyword in `partial()`. The parameter should be
preserved in the resulting callable:

```py
from functools import partial

def f(x: int, /, y: str) -> bool:
    return True

# `x` is positional-only, so `x=1` does not bind it.
p = partial(f, x=1)
reveal_type(p)  # revealed: partial[(x: int, /, y: str) -> bool]
```

## Assignability to callable

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

## Ambiguous binding preserves overload

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

## Bound classmethod callback with weakref

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
        p = partial(self.callback, ref(self))
        reveal_type(p)  # revealed: partial[(x: int) -> None]
```

## Assignability to protocol

A `partial` result is assignable to a `Protocol` with a matching `__call__` signature. Extra
keyword-only parameters with defaults in the `partial` are allowed, since they don't need to be
provided by the caller:

```py
from functools import partial
from typing import Protocol

class Callback(Protocol):
    def __call__(self, *, x: int) -> None: ...

def f(*, x: int, y: str) -> None: ...

p = partial(f, y="hello")
reveal_type(p)  # revealed: partial[(*, x: int, y: str = "hello") -> None]

def takes_callback(cb: Callback) -> None: ...

takes_callback(p)  # OK — extra `y` with default is fine
```

## Accessing `__call__` directly

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

## Attribute access on partial results

Standard `partial` attributes like `.func`, `.args`, and `.keywords` should be accessible:

```py
from functools import partial
from typing import Callable

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p.func)  # revealed: (...) -> bool
reveal_type(p.args)  # revealed: tuple[Any, ...]
reveal_type(p.keywords)  # revealed: dict[str, Any]
```

## Attribute assignment on partial results

Attribute assignment should go through the standard nominal instance path:

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
p.func = f  # error: [invalid-assignment]
```
