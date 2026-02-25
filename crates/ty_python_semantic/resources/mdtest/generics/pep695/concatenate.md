# `typing.Concatenate`

```toml
[environment]
python-version = "3.12"
```

## Basic usage in `Callable`

`Concatenate` is valid as the first argument to `Callable`, with a `ParamSpec` or `...` as its final
element.

### With `ParamSpec`

```py
from typing import Callable, Concatenate

def foo[**P, R](func: Callable[Concatenate[int, P], R]) -> Callable[Concatenate[int, P], R]:
    # TODO: Should reveal `(int, /, *args: P@foo.args, **kwargs: P@foo.kwargs) -> R@foo`
    reveal_type(func)  # revealed: (...) -> R@foo
    return func

def f(x: int, y: str) -> bool:
    return True

result = foo(f)
# TODO: Should reveal `(int, /, y: str) -> bool`
reveal_type(result)  # revealed: (...) -> bool
```

### With ellipsis

```py
from typing import Callable, Concatenate

def _(c: Callable[Concatenate[int, str, ...], bool]):
    # TODO: Should reveal `(int, str, /, ...) -> bool`
    reveal_type(c)  # revealed: (...) -> bool
```

### Complex types inside `Concatenate`

```py
from typing import Callable, Concatenate

def _(c: Callable[Concatenate[int | str, list[int], type[str], ...], None]):
    # TODO: Should reveal `(int | str, list[int], type[str], ...) -> None`
    reveal_type(c)  # revealed: (...) -> None
```

### Nested

```py
from typing import Callable, Concatenate

def _(c: Callable[Concatenate[int, Callable[Concatenate[str, ...], None], ...], None]):
    # TODO: Should reveal `(int, (str, ...) -> None, /, ...) -> None`
    reveal_type(c)  # revealed: (...) -> None
```

## Decorator patterns

### Adding a parameter

A decorator that adds a parameter to the beginning of the callable's signature.

```py
from typing import Callable, Concatenate

def add_param[**P, R](func: Callable[P, R]) -> Callable[Concatenate[int, P], R]:
    def wrapper(param: int, *args: P.args, **kwargs: P.kwargs) -> R:
        return func(*args, **kwargs)
    return wrapper

@add_param
def f(x: str, y: bytes) -> int:
    return 1

# TODO: Should reveal `(int, /, x: str, y: bytes) -> int`
reveal_type(f)  # revealed: (...) -> int

reveal_type(f(1, "", b""))  # revealed: int

# TODO: This should be an error since `param` is a positional-only parameter
reveal_type(f(param=1, x="", y=b""))  # revealed: int
```

### Removing a parameter

A decorator that removes the first parameter from the callable's signature.

```py
from typing import Callable, Concatenate

def remove_param[**P, R](func: Callable[Concatenate[int, P], R]) -> Callable[P, R]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        return func(0, *args, **kwargs)
    # TODO: no error expected here
    return wrapper  # error: [invalid-return-type]

@remove_param
def f(x: int, y: str, z: bytes) -> int:
    return 1

# TODO: Should reveal `(y: str, z: bytes) -> int`
reveal_type(f)  # revealed: [**P'return](**P'return) -> int

# TODO: Shouldn't be an error
# error: [missing-argument]
reveal_type(f("", b""))  # revealed: int
# TODO: Shouldn't be an error
# error: [missing-argument]
reveal_type(f(y="", z=b""))  # revealed: int

# TODO: missing-argument is an incorrect error, it should be [unknown-argument] since `x` is removed
# error: [missing-argument] "No argument provided for required parameter `*args`"
reveal_type(f(x=1, y="", z=b""))  # revealed: int
```

### Transforming a parameter

A decorator that transforms the first parameter type.

```py
from typing import Callable, Concatenate

def transform[**P, R](func: Callable[Concatenate[int, P], R]) -> Callable[Concatenate[str, P], R]:
    def wrapper(param: str, *args: P.args, **kwargs: P.kwargs) -> R:
        return func(int(param), *args, **kwargs)
    return wrapper

@transform
def f(x: int, y: int) -> int:
    return 1

# TODO: Should reveal `(str, /, y: int) -> int`
reveal_type(f)  # revealed: (...) -> int

reveal_type(f("", 1))  # revealed: int
reveal_type(f("", y=1))  # revealed: int

# TODO: This should be an error since `param` is a positional-only parameter
reveal_type(f(param="", y=1))  # revealed: int
```

### Prepending multiple parameters

```py
from typing import Callable, Concatenate

def multi[**P, R](func: Callable[P, R]) -> Callable[Concatenate[int, str, P], R]:
    def wrapper(a: int, b: str, *args: P.args, **kwargs: P.kwargs) -> R:
        return func(*args, **kwargs)
    return wrapper

@multi
def f(x: int) -> int:
    return 1

# TODO: Should reveal `(int, str, /, x: int) -> int`
reveal_type(f)  # revealed: (...) -> int

reveal_type(f(1, "", 2))  # revealed: int
reveal_type(f(1, "", x=2))  # revealed: int

# TODO: This should be an error since `a` and `b` are positional-only parameters
reveal_type(f(a=1, b="", x=2))  # revealed: int
```

## Invalid uses of `Concatenate`

### Standalone annotation (not inside `Callable`)

`Concatenate` is only valid as the first argument to `Callable` or in the context of a `ParamSpec`
type argument.

```py
from typing import Concatenate

# error: [invalid-type-form] "`typing.Concatenate` requires at least two arguments when used in a type expression"
def _(x: Concatenate): ...

# TODO: Should be an error - Concatenate is not a valid standalone type
def invalid1(x: Concatenate[int, ...]) -> None: ...

# TODO: Should be an error - Concatenate is not a valid standalone type
def invalid2() -> Concatenate[int, ...]: ...
```

### Too few arguments

```py
from typing import Callable, Concatenate

def _(
    # error: [invalid-type-form] "Special form `typing.Concatenate` expected at least 2 parameters but got 0"
    a: Callable[Concatenate[()], int],
    # error: [invalid-type-form] "Special form `typing.Concatenate` expected at least 2 parameters but got 1"
    b: Callable[Concatenate[int], int],
    # error: [invalid-type-form] "Special form `typing.Concatenate` expected at least 2 parameters but got 1"
    c: Callable[Concatenate[(int,)], int],
):
    reveal_type(a)  # revealed: (...) -> int
    reveal_type(b)  # revealed: (...) -> int
    reveal_type(c)  # revealed: (...) -> int
```

### Last argument must be `ParamSpec` or `...`

The final argument to `Concatenate` must be a `ParamSpec` or `...`.

```py
from typing import Callable, Concatenate

# TODO: Should be an error - last arg is not ParamSpec or `...`
def _(c: Callable[Concatenate[int, str], bool]): ...
```

### `ParamSpec` must be last

If a `ParamSpec` appears in `Concatenate`, it must be the last element.

```py
from typing import Callable, Concatenate

# TODO: Should be an error - ParamSpec not in last position
def invalid1[**P](c: Callable[Concatenate[P, int], bool]):
    reveal_type(c)  # revealed: (...) -> bool

# TODO: Should be an error - ParamSpec not in last position
def invalid2[**P](c: Callable[Concatenate[P, ...], bool]):
    reveal_type(c)  # revealed: (...) -> bool

def valid[**P](c: Callable[Concatenate[int, P], bool]):
    # TODO: Should reveal `(int, /, **P@valid) -> bool`
    reveal_type(c)  # revealed: (...) -> bool
```

### Nested `Concatenate`

```py
from typing import Callable, Concatenate

# TODO: This should be an error
def invalid[**P](c: Callable[Concatenate[Concatenate[int, ...], P], None]):
    pass
```

## Specialization with concrete types

When a `Callable[Concatenate[X, P], R]` is specialized with concrete arguments, `P` should be
inferred from the remaining parameters.

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], bool]) -> Callable[P, bool]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> bool:
        return func(0, *args, **kwargs)
    # TODO: no error expected here
    return wrapper  # error: [invalid-return-type]

@decorator
def f1(a: int) -> bool:
    return True

@decorator
def f2(a: int, b: str) -> bool:
    return True

# TODO: This call should be an error because the `str` is not assignable to `int`
@decorator
def f3(a: str, b: int) -> bool:
    return True

# TODO: Should reveal `() -> bool`
reveal_type(f1)  # revealed: [**P'return](**P'return) -> bool
# TODO: Should reveal `(b: str) -> bool`
reveal_type(f2)  # revealed: [**P'return](**P'return) -> bool
```

## Generic classes

### In class attributes

```py
from typing import Callable, Concatenate

class Middleware[**P, R]:
    handler: Callable[Concatenate[str, P], R]

    def __init__(self, handler: Callable[Concatenate[str, P], R]) -> None:
        self.handler = handler

def my_handler(env: str, x: int, y: float) -> bool:
    return True

m = Middleware(my_handler)
# TODO: Should reveal `Middleware[((x: int, y: float)), bool]` or similar
reveal_type(m)  # revealed: Middleware[(...), bool]
```

### Specializing `ParamSpec` with `Concatenate`

When explicitly specializing a generic class that takes a `ParamSpec`, a `Concatenate` form can be
provided as a type argument.

```py
from typing import Callable, Concatenate

class Foo[**P1]:
    attr: Callable[P1, None]

def with_paramspec[**P2](f: Foo[Concatenate[int, P2]]) -> None:
    # TODO: Should reveal `Callable[Concatenate[int, P2], None]`
    reveal_type(f.attr)  # revealed: (...) -> None
```

## `Concatenate` in type aliases

### Using `type` statement (PEP 695)

```py
from typing import Callable, Concatenate

type Foo[**P, R] = Callable[Concatenate[int, P], R]

def _(f: Foo[[str], bool]) -> None:
    # TODO: Should reveal `(int, str, /) -> bool`
    reveal_type(f)  # revealed: (...) -> bool
```

### Using `TypeAlias`

```py
from typing import Callable, Concatenate, ParamSpec, TypeVar
from typing import TypeAlias

P = ParamSpec("P")
R = TypeVar("R")

Foo: TypeAlias = Callable[Concatenate[int, P], R]

def _(f: Foo[[str], bool]) -> None:
    # TODO: Should reveal `(int, str, /) -> bool`
    reveal_type(f)  # revealed: Unknown
```

## `Concatenate` with different parameter kinds

### Function with keyword-only parameters after `Concatenate` prefix

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(0, *args, **kwargs)
    # TODO: no error expected here
    return wrapper  # error: [invalid-return-type]

@decorator
def kwonly(x: int, *, key: str) -> None: ...

# TODO: Should reveal `(*, key: str) -> None`
reveal_type(kwonly)  # revealed: [**P'return](**P'return) -> None
```

### Function with default values

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(0, *args, **kwargs)
    # TODO: no error expected here
    return wrapper  # error: [invalid-return-type]

@decorator
def defaults(x: int, y: str = "default", z: int = 0) -> None: ...

# TODO: Should reveal `(y: str = "default", z: int = 0) -> None`
reveal_type(defaults)  # revealed: [**P'return](**P'return) -> None
```

### Function with `*args` and `**kwargs`

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(0, *args, **kwargs)
    # TODO: no error expected here
    return wrapper  # error: [invalid-return-type]

@decorator
def variadic(x: int, *args: str, **kwargs: int) -> None: ...

# TODO: Should reveal `(*args: str, **kwargs: int) -> None`
reveal_type(variadic)  # revealed: [**P'return](**P'return) -> None
```

## `Concatenate` with `ParamSpec` in generic function calls

### Basic call with inferred `ParamSpec`

```py
from typing import Callable, Concatenate

def foo[**P, R](func: Callable[Concatenate[int, P], R], *args: P.args, **kwargs: P.kwargs) -> R:
    return func(0, *args, **kwargs)

def test(x: str, y: str) -> bool:
    return True

reveal_type(foo(test, "", ""))  # revealed: bool
reveal_type(foo(test, y="", x=""))  # revealed: bool

# TODO: These calls should raise an error
reveal_type(foo(test, 1, ""))  # revealed: bool
reveal_type(foo(test, ""))  # revealed: bool
```

## `Concatenate` with overloaded functions

A function that accepts an overloaded callable via `Callable[Concatenate[int, P], R]` should be able
to strip the first parameter and infer `P` from the remaining overload signatures.

```py
from typing import Callable, Concatenate, overload

def remove_param[**P, R](func: Callable[Concatenate[int, P], R]) -> Callable[P, R]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        return func(0, *args, **kwargs)
    # TODO: no error expected here
    return wrapper  # error: [invalid-return-type]

@overload
def f1(x: int, y: str) -> str: ...
@overload
def f1(x: int, y: int) -> int: ...
@remove_param
def f1(x: int, y: str | int) -> str | int:
    return y

# TODO: Should reveal `Overloaded[(y: str) -> str, (y: int) -> int]`
reveal_type(f1)  # revealed: [**P'return](**P'return) -> str | int
```

But, it's not possible to _add_ a parameter to an overloaded function using `Concatenate` because
the overload signatures don't have the extra parameter.

```py
def add_param[**P, R](func: Callable[P, R]) -> Callable[Concatenate[int, P], R]:
    def wrapper(param: int, *args: P.args, **kwargs: P.kwargs) -> R:
        return func(*args, **kwargs)
    return wrapper

# TODO: Raise a diagnostic stating that the signature of the implementation doesn't match the
# overloads because the overloads don't have the extra `int` parameter.
@overload
def f2(y: str) -> str: ...
@overload
def f2(y: int) -> int: ...
@add_param
def f2(y: str | int) -> str | int:
    return y

# TODO: Should this reveal `Overloaded[(int, /, y: str) -> str, (int, /, y: int) -> int]` ?
reveal_type(f2)  # revealed: (...) -> str | int
```

But, it's possible to add the additional parameter just to the overload signatures and not the
implementation:

```py
@overload
def f3(x: int, /, y: str) -> str: ...
@overload
def f3(x: int, /, y: int) -> int: ...
@add_param
def f3(y: str | int) -> str | int:
    return y

# TODO: Should reveal `Overloaded[(int, /, y: str) -> str, (int, /, y: int) -> int]`
reveal_type(f3)  # revealed: (...) -> str | int
```

## `Concatenate` with protocol classes

A protocol with `ParamSpec` in its `__call__` can be used where `Callable[Concatenate[...], ...]` is
expected.

```py
from typing import Protocol, Concatenate, Callable

class Handler[**P, R](Protocol):
    def __call__(self, value: int, *args: P.args, **kwargs: P.kwargs) -> R: ...

def process[**P, R](handler: Handler[P, R], *args: P.args, **kwargs: P.kwargs) -> R:
    return handler(0, *args, **kwargs)

class MyHandler:
    def __call__(self, value: int, name: str) -> bool:
        return True

# TODO: P should be inferred as [name: str], R as bool from MyHandler.__call__
# TODO: These should not be errors
# TODO: Should reveal `bool`
# error: [invalid-argument-type]
reveal_type(process(MyHandler(), "hello"))  # revealed: Unknown
# error: [invalid-argument-type]
reveal_type(process(MyHandler(), name="hello"))  # revealed: Unknown

def use_callable[**P, R](func: Callable[Concatenate[int, P], R], handler: Handler[P, R]) -> None: ...
```

## Importing from `typing_extensions`

`Concatenate` should work the same whether imported from `typing` or `typing_extensions`.

```py
from typing_extensions import Callable, Concatenate

def _(c: Callable[Concatenate[int, str, ...], bool]):
    # TODO: Should reveal `(int, str, ...) -> bool`
    reveal_type(c)  # revealed: (...) -> bool
```
