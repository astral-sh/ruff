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

`partial(42)` is an error caught by the constructor call; we fall back
to the default `partial[T]` type.

```py
from functools import partial

p = partial(42)  # error: [invalid-argument-type]
reveal_type(p)  # revealed: partial[Unknown]
```

## Fallback for generic functions

```py
from functools import partial
from typing import TypeVar

T = TypeVar("T")

def identity(x: T) -> T:
    return x

# TODO: support generic functions in partial
p = partial(identity, 1)
reveal_type(p)  # revealed: partial[Unknown]
```

## Fallback for overloaded functions

```py
from functools import partial
from typing import overload

@overload
def f(a: int) -> int: ...
@overload
def f(a: str) -> str: ...
def f(a: int | str) -> int | str:
    return a

# TODO: support overloaded functions in partial
p = partial(f, 1)
reveal_type(p)  # revealed: partial[int | str]
```

## Fallback for starred args

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

args = (1,)
# TODO: support starred args in partial call
p = partial(f, *args)
reveal_type(p)  # revealed: partial[bool]
```

## Fallback for kwargs splat

```py
from functools import partial

def f(a: int, b: str) -> bool:
    return True

kwargs = {"a": 1}
# TODO: support **kwargs in partial call
p = partial(f, **kwargs)
reveal_type(p)  # revealed: partial[bool]
```
