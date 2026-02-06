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

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

p = partial(f, b="hello")
reveal_type(p)  # revealed: (a: int) -> bool
```

## Mixed positional and keyword binding

```py
from functools import partial

def f(a: int, b: str, c: float) -> bool:
    return True

p = partial(f, 1, c=3.14)
reveal_type(p)  # revealed: (b: str) -> bool
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
reveal_type(p)  # revealed: (a: int) -> bool
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
reveal_type(p)  # revealed: (a: int) -> bool
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
reveal_type(p)  # revealed: (a: int) -> bool
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
reveal_type(p)  # revealed: (a: int) -> bool
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
