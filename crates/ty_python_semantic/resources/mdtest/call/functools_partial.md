# `functools.partial`

## Basic positional binding

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: (b: str) -> bool
```

## Keyword binding

Keyword-bound parameters are kept with a default, since `partial` allows overriding keyword
arguments at call time.

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: (a: int, b: str = "hello") -> bool
```

## Mixed positional and keyword binding

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, 1, c=3.14)
reveal_type(p)  # revealed: (b: str, c: int | float = ...) -> bool
```

## All args bound

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, 1, "hello")
reveal_type(p)  # revealed: () -> bool
```

## No args bound

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f)
reveal_type(p)  # revealed: (a: int, b: str) -> bool
```

## Positional-only params

```py
from functools import partial

def f(a: int, b: str, /) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: (b: str, /) -> bool
```

## Keyword-only params

```py
from functools import partial

def f(a: int, *, b: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: (*, b: str) -> bool
```

## Keyword-only params bound by keyword

```py
from functools import partial

def f(a: int, *, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: (a: int, *, b: str = "hello") -> bool
```

## Variadic preserved

```py
from functools import partial

def f(a: int, *args: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: (*args: str) -> bool
```

## Keyword variadic preserved

```py
from functools import partial

def f(a: int, **kwargs: str) -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: (**kwargs: str) -> bool
```

## Defaults preserved

```py
from functools import partial

def f(a: int, b: str = "default") -> bool:
    return True

p = partial(f, 1)
reveal_type(p)  # revealed: (b: str = "default") -> bool
```

## Lambda

```py
from functools import partial

p = partial(lambda x, y: x + y, 1)
reveal_type(p)  # revealed: (y) -> Unknown
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
reveal_type(p)  # revealed: (b: str) -> bool
```

## Wrong keyword arg type

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b=42)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: (a: int, b: str = 42) -> bool
```

## Bound method

```py
from functools import partial

class Greeter:
    def greet(self, name: str, greeting: str = "Hello") -> str:
        return f"{greeting}, {name}"

g = Greeter()
p = partial(g.greet, "world")
reveal_type(p)  # revealed: (greeting: str = "Hello") -> str
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
reveal_type(p)  # revealed: () -> int
```

## Generic functions with remaining params

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def pair(a: T, b: T) -> tuple[T, T]:
    return (a, b)

p = partial(pair, 1)
reveal_type(p)  # revealed: (b: int) -> tuple[int, int]
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
reveal_type(p)  # revealed: Overload[() -> int, () -> str]
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
reveal_type(p)  # revealed: Overload[(b: str) -> int, (b: str) -> str]
```

## Starred args with fixed-length tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args: tuple[int] = (1,)
p = partial(f, *args)
reveal_type(p)  # revealed: (b: str) -> bool
```

## Starred args with multiple elements

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[int, str] = (1, "hello")
p = partial(f, *args)
reveal_type(p)  # revealed: (c: int | float) -> bool
```

## Mixed positional and starred args

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[str] = ("hello",)
p = partial(f, 1, *args)
reveal_type(p)  # revealed: (c: int | float) -> bool
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
reveal_type(p)  # revealed: (a: int, b: str = ...) -> bool
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
reveal_type(p)  # revealed: (a: int, b: str = "hello", c: int | float = ...) -> bool
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
reveal_type(p)  # revealed: () -> bool
```

## Nested partial

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p1 = partial(f, 1)
reveal_type(p1)  # revealed: (b: str, c: int | float) -> bool

p2 = partial(p1, "hello")
reveal_type(p2)  # revealed: (c: int | float) -> bool
```

## Class constructor

```py
from functools import partial

class MyClass:
    def __init__(self, x: int, y: str) -> None:
        pass

p = partial(MyClass, 1)
reveal_type(p)  # revealed: (y: str) -> MyClass
```

## Binding a default parameter

Binding a parameter that has a default value removes it from the signature.

```py
from functools import partial

def f(a: int, b: str = "default", c: float = 0.0) -> bool:
    return True

p = partial(f, 1, "hello")
reveal_type(p)  # revealed: (c: int | float = ...) -> bool
```

## Multiple keyword bindings

```py
from functools import partial

def f(a: int, b: str, c: float, d: bool) -> int:
    return 0

p = partial(f, b="hello", d=True)
reveal_type(p)  # revealed: (a: int, b: str = "hello", c: int | float, d: bool = True) -> int
```

## Mixed positional-only, regular, and keyword-only

```py
from functools import partial

def f(a: int, /, b: str, *, c: float) -> bool:
    return True

# Bind the positional-only param
p1 = partial(f, 1)
reveal_type(p1)  # revealed: (b: str, *, c: int | float) -> bool

# Bind a keyword-only param by keyword
p2 = partial(f, c=3.14)
reveal_type(p2)  # revealed: (a: int, /, b: str, *, c: int | float = ...) -> bool

# Bind both positional-only and keyword-only
p3 = partial(f, 1, c=3.14)
reveal_type(p3)  # revealed: (b: str, *, c: int | float = ...) -> bool
```

## Starred args combined with keyword args

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

args: tuple[int] = (1,)
p = partial(f, *args, c=3.14)
reveal_type(p)  # revealed: (b: str, c: int | float = ...) -> bool
```

## Starred args with empty tuple

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args: tuple[()] = ()
p = partial(f, *args)
reveal_type(p)  # revealed: (a: int, b: str) -> bool
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
reveal_type(p)  # revealed: (b: Unknown) -> tuple[int, Unknown]
```

## Callable object (class with `__call__`)

```py
from functools import partial

class Adder:
    def __call__(self, a: int, b: int) -> int:
        return a + b

adder = Adder()
p = partial(adder, 1)
reveal_type(p)  # revealed: (b: int) -> int
```

## Staticmethod

```py
from functools import partial

class MyClass:
    @staticmethod
    def f(a: int, b: str) -> bool:
        return True

p = partial(MyClass.f, 1)
reveal_type(p)  # revealed: (b: str) -> bool
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
reveal_type(p)  # revealed: Overload[() -> int, () -> str]
```

## Overriding keyword-bound args at call time

`partial` allows keyword arguments to be overridden when calling the result:

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: (a: int, b: str = "hello", c: int | float) -> bool

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
reveal_type(p)  # revealed: (x: int, /, y: str) -> bool
```
