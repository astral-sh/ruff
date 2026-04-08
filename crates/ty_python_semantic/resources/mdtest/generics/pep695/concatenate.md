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
    reveal_type(func)  # revealed: (int, /, *args: P@foo.args, **kwargs: P@foo.kwargs) -> R@foo
    return func

def f(x: int, y: str) -> bool:
    return True

result = foo(f)
reveal_type(result)  # revealed: (int, /, y: str) -> bool
```

### With ellipsis

```py
from typing import Callable, Concatenate

class Foo[**P]:
    attr: Callable[P, None]

def _(c: Callable[Concatenate[int, str, ...], bool]):
    reveal_type(c)  # revealed: (int, str, /, *args: Any, **kwargs: Any) -> bool

# revealed: (int, str, /, *args: Any, **kwargs: Any) -> None
reveal_type(Foo[Concatenate[int, str, ...]].attr)
```

### Complex types inside `Concatenate`

```py
from typing import Callable, Concatenate

class Foo[**P]:
    attr: Callable[P, None]

def _(c: Callable[Concatenate[int | str, list[int], type[str], ...], None]):
    reveal_type(c)  # revealed: (int | str, list[int], type[str], /, *args: Any, **kwargs: Any) -> None

# revealed: (int | str, list[int], type[str], /, *args: Any, **kwargs: Any) -> None
reveal_type(Foo[Concatenate[int | str, list[int], type[str], ...]].attr)
```

### Nested

```py
from typing import Callable, Concatenate

class Foo[**P]:
    attr: Callable[P, None]

def _(c: Callable[Concatenate[int, Callable[Concatenate[str, ...], None], ...], None]):
    reveal_type(c)  # revealed: (int, (str, /, *args: Any, **kwargs: Any) -> None, /, *args: Any, **kwargs: Any) -> None

# revealed: (int, (str, /, *args: Any, **kwargs: Any) -> None, /, *args: Any, **kwargs: Any) -> None
reveal_type(Foo[Concatenate[int, Callable[Concatenate[str, ...], None], ...]].attr)
```

### Both `*args` and `**kwargs` are required

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(1)  # TODO: error: [missing-argument]
        func(1, *args)  # TODO: error: [missing-argument]
        func(1, **kwargs)  # TODO: error: [missing-argument]
    return wrapper
```

### Imported `ParamSpec` type variable

`module.py`:

```py
from typing import ParamSpec

P = ParamSpec("P")
```

```py
from typing import Concatenate, Callable

import module

def foo(f: Callable[Concatenate[int, module.P], None]):
    reveal_type(f)  # revealed: (int, /, *args: P@foo.args, **kwargs: P@foo.kwargs) -> None
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

reveal_type(f)  # revealed: (int, /, x: str, y: bytes) -> int

reveal_type(f(1, "", b""))  # revealed: int

# error: [missing-argument] "No argument provided for required parameter 1"
# error: [unknown-argument] "Argument `param` does not match any known parameter"
reveal_type(f(param=1, x="", y=b""))  # revealed: int
```

### Removing a parameter

A decorator that removes the first parameter from the callable's signature.

```py
from typing import Callable, Concatenate

def remove_param[**P, R](func: Callable[Concatenate[int, P], R]) -> Callable[P, R]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        return func(0, *args, **kwargs)
    return wrapper

@remove_param
def f(x: int, y: str, z: bytes) -> int:
    return 1

reveal_type(f)  # revealed: (y: str, z: bytes) -> int

reveal_type(f("", b""))  # revealed: int
reveal_type(f(y="", z=b""))  # revealed: int

# error: [unknown-argument] "Argument `x` does not match any known parameter"
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

reveal_type(f)  # revealed: (str, /, y: int) -> int

reveal_type(f("", 1))  # revealed: int
reveal_type(f("", y=1))  # revealed: int

# error: [missing-argument] "No argument provided for required parameter 1"
# error: [unknown-argument] "Argument `param` does not match any known parameter"
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

reveal_type(f)  # revealed: (int, str, /, x: int) -> int

reveal_type(f(1, "", 2))  # revealed: int
reveal_type(f(1, "", x=2))  # revealed: int

# error: [missing-argument] "No arguments provided for required parameters 1, 2"
# error: [unknown-argument] "Argument `a` does not match any known parameter"
# error: [unknown-argument] "Argument `b` does not match any known parameter"
reveal_type(f(a=1, b="", x=2))  # revealed: int
```

## Invalid uses of `Concatenate`

### Standalone annotation (not inside `Callable`)

`Concatenate` is only valid as the first argument to `Callable` or in the context of a `ParamSpec`
type argument.

```py
from typing import Callable, Concatenate, ParamSpec

class Foo[T]: ...

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
def invalid0(x: Concatenate): ...

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a parameter annotation"
def invalid1(x: Concatenate[int]): ...

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a parameter annotation"
def invalid2(x: Concatenate[int, ...]) -> None: ...

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a return type annotation"
def invalid3() -> Concatenate[int, ...]: ...

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a return type annotation"
def invalid4() -> Concatenate[()]: ...

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
a: Concatenate

class Foo[**P]:
    # error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
    b: Concatenate[int, P]

# error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
def invalid5[**P](x: Foo[Concatenate[P, ...]]) -> None: ...
```

### Too few arguments

```py
from typing import Callable, Concatenate

class Foo[**P]:
    attr: Callable[P, None]

def _(
    # error: [invalid-type-form] "`typing.Concatenate` requires at least 2 arguments when used in a type expression (got 0)"
    a: Callable[Concatenate[()], int],
    # error: [invalid-type-form] "`typing.Concatenate` requires at least 2 arguments when used in a type expression (got 1)"
    b: Callable[Concatenate[int], int],
    # error: [invalid-type-form] "`typing.Concatenate` requires at least 2 arguments when used in a type expression (got 1)"
    c: Callable[Concatenate[(int,)], int],
    # error: [invalid-type-form] "`typing.Concatenate` requires at least two arguments when used in a type expression"
    d: Callable[Concatenate, int],
):
    reveal_type(a)  # revealed: (...) -> int
    reveal_type(b)  # revealed: (...) -> int
    reveal_type(c)  # revealed: (...) -> int

# error: [invalid-type-form] "`typing.Concatenate` requires at least 2 arguments when used in a type expression (got 0)"
reveal_type(Foo[Concatenate[()]].attr)  # revealed: (...) -> None
# error: [invalid-type-form] "`typing.Concatenate` requires at least 2 arguments when used in a type expression (got 1)"
reveal_type(Foo[Concatenate[int]].attr)  # revealed: (...) -> None
# error: [invalid-type-form] "`typing.Concatenate` requires at least 2 arguments when used in a type expression (got 1)"
reveal_type(Foo[Concatenate[(int,)]].attr)  # revealed: (...) -> None
# error: [invalid-type-form] "`typing.Concatenate` requires at least two arguments when used in a type expression"
reveal_type(Foo[Concatenate].attr)  # revealed: (...) -> None
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[[Concatenate]].attr)  # revealed: (...) -> None
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[[Concatenate, int]].attr)  # revealed: (Unknown, int, /) -> None

# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[[Concatenate[int], str]].attr)  # revealed: (Unknown, str, /) -> None
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[[Concatenate[int, str], str]].attr)  # revealed: (Unknown, str, /) -> None
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[[Concatenate[()], str]].attr)  # revealed: (Unknown, str, /) -> None
```

### Last argument must be `ParamSpec` or `...`

The final argument to `Concatenate` must be a `ParamSpec` or `...`.

```py
from typing import Callable, Concatenate

class Foo[**P]:
    attr: Callable[P, None]

# error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `str`"
def _(c: Callable[Concatenate[int, str], bool]): ...

# error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `str`"
reveal_type(Foo[Concatenate[int, str]].attr)  # revealed: (...) -> None

# error: [invalid-type-form] "`typing.Concatenate` requires at least two arguments when used in a type expression"
reveal_type(Foo[Concatenate[int, Concatenate]].attr)  # revealed: (...) -> None

# error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `Unknown`"
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[Concatenate[int, Concatenate[()]]].attr)  # revealed: (...) -> None

# error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `Unknown`"
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[Concatenate[int, Concatenate[int]]].attr)  # revealed: (...) -> None

# error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `Unknown`"
# error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context in a type expression"
reveal_type(Foo[Concatenate[int, Concatenate[int, str]]].attr)  # revealed: (...) -> None
```

### `ParamSpec` must be last

If a `ParamSpec` appears in `Concatenate`, it must be the last element.

```py
from typing import Callable, Concatenate

class Foo[**P1]:
    attr: Callable[P1, None]

# error: [invalid-type-form] "Bare ParamSpec `P2` is not valid in this context"
# error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `int`"
def invalid1[**P2](c: Callable[Concatenate[P2, int], bool]):
    reveal_type(c)  # revealed: (...) -> bool
    # error: [invalid-type-form] "Bare ParamSpec `P2` is not valid in this context"
    # error: [invalid-type-arguments] "The last argument to `typing.Concatenate` must be either `...` or a `ParamSpec` type variable: Got `int`"
    reveal_type(Foo[Concatenate[P2, int]].attr)  # revealed: (...) -> None

# error: [invalid-type-form] "Bare ParamSpec `P2` is not valid in this context"
def invalid2[**P2](c: Callable[Concatenate[P2, ...], bool]):
    # The bare `P2` falls back to `Unknown` as a prefix parameter, while `...` is a valid
    # gradual tail, resulting in `(Unknown, /, *args: Any, **kwargs: Any) -> bool`.
    reveal_type(c)  # revealed: (Unknown, /, *args: Any, **kwargs: Any) -> bool

    # error: [invalid-type-form] "Bare ParamSpec `P2` is not valid in this context"
    # revealed: (Unknown, /, *args: Any, **kwargs: Any) -> None
    reveal_type(Foo[Concatenate[P2, ...]].attr)

def valid[**P2](c: Callable[Concatenate[int, P2], bool]):
    reveal_type(c)  # revealed: (int, /, *args: P2@valid.args, **kwargs: P2@valid.kwargs) -> bool

    # revealed: (int, /, *args: P2@valid.args, **kwargs: P2@valid.kwargs) -> None
    reveal_type(Foo[Concatenate[int, P2]].attr)

type Alias[**P1] = int

def invalid3[**P2, **P3](
    # error: [invalid-type-form] "Bare ParamSpec `P2` is not valid in this context"
    x: Foo[Concatenate[P2, P3]],
    # error: [invalid-type-form] "Bare ParamSpec `P2` is not valid in this context"
    y: Alias[Concatenate[P2, P3]],
):
    pass
```

### Nested `Concatenate`

```py
from typing import Callable, Concatenate

def invalid[**P](
    # error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context"
    c: Callable[Concatenate[Concatenate[int, ...], P], None],
    # error: [invalid-type-form] "`typing.Concatenate` is not allowed in this context"
    d: Callable[Concatenate[Concatenate, P], int],
):
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
    return wrapper

# error: [invalid-argument-type] "Argument to function `decorator` is incorrect: Expected `(int, /, *args: Unknown, **kwargs: Unknown) -> bool`, found `def f0() -> bool`"
@decorator
def f0() -> bool:
    return True

@decorator
def f1(a: int) -> bool:
    return True

@decorator
def f2(a: int, b: str) -> bool:
    return True

# error: [invalid-argument-type] "Argument to function `decorator` is incorrect: Expected `(int, /, *args: Unknown, **kwargs: Unknown) -> bool`, found `def f3(a: str, b: int) -> bool`"
@decorator
def f3(a: str, b: int) -> bool:
    return True

reveal_type(f1)  # revealed: () -> bool
reveal_type(f2)  # revealed: (b: str) -> bool
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
reveal_type(m)  # revealed: Middleware[(x: int, y: int | float), bool]
```

### Specializing `ParamSpec` with `Concatenate`

When explicitly specializing a generic class that takes a `ParamSpec`, a `Concatenate` form can be
provided as a type argument.

```py
from typing import Callable, Concatenate

class Foo[**P1]:
    attr: Callable[P1, None]

def with_paramspec[**P2](f: Foo[Concatenate[int, P2]]) -> None:
    reveal_type(f.attr)  # revealed: (int, /, *args: P2@with_paramspec.args, **kwargs: P2@with_paramspec.kwargs) -> None
```

## `Concatenate` in type aliases

### Using `type` statement (PEP 695)

```py
from typing import Callable, Concatenate

type Foo[**P, R] = Callable[Concatenate[int, P], R]

def _(f: Foo[[str], bool]) -> None:
    reveal_type(f)  # revealed: (int, str, /) -> bool
```

### Using `TypeAlias`

```py
from typing import Callable, Concatenate, ParamSpec, TypeVar
from typing import TypeAlias

P = ParamSpec("P")
R = TypeVar("R")

Foo: TypeAlias = Callable[Concatenate[int, P], R]

def _(f: Foo[[str], bool]) -> None:
    reveal_type(f)  # revealed: (int, str, /) -> bool
```

## `Concatenate` with different parameter kinds

### Function with keyword-only parameters after `Concatenate` prefix

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(0, *args, **kwargs)
    return wrapper

@decorator
def kwonly(x: int, *, key: str) -> None: ...

reveal_type(kwonly)  # revealed: (*, key: str) -> None
```

### Function with default values

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(0, *args, **kwargs)
    return wrapper

@decorator
def defaults(x: int, y: str = "default", z: int = 0) -> None: ...

reveal_type(defaults)  # revealed: (y: str = "default", z: int = 0) -> None
```

### Function with `*args` and `**kwargs`

```py
from typing import Callable, Concatenate

def decorator[**P](func: Callable[Concatenate[int, P], None]) -> Callable[P, None]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> None:
        func(0, *args, **kwargs)
    return wrapper

@decorator
def variadic(x: int, *args: str, **kwargs: int) -> None: ...

reveal_type(variadic)  # revealed: (*args: str, **kwargs: int) -> None

# error: [invalid-argument-type] "Argument to function `decorator` is incorrect: Expected `(int, /, *args: Unknown, **kwargs: Unknown) -> None`, found `def only_variadic(*args: str, **kwargs: int) -> None`"
@decorator
def only_variadic(*args: str, **kwargs: int) -> None: ...

reveal_type(only_variadic)  # revealed: (...) -> None

# TODO: This should accept the callable and reveal `(*args: str, **kwargs: int) -> None`.
# error: [invalid-argument-type]
@decorator
def unpack_variadic(*args: *tuple[int, *tuple[str, ...]], **kwargs: int) -> None: ...

reveal_type(unpack_variadic)  # revealed: (...) -> None
```

## `Concatenate` with `ParamSpec` in generic function calls

### Basic call with inferred `ParamSpec`

```py
from typing import Callable, Concatenate

def foo[**P, R](func: Callable[Concatenate[int, P], R], *args: P.args, **kwargs: P.kwargs) -> R:
    return func(0, *args, **kwargs)

def valid(x: int, y: str) -> bool:
    return True

def invalid(x: str, y: str) -> bool:
    return True

reveal_type(foo(valid, ""))  # revealed: bool
reveal_type(foo(valid, y=""))  # revealed: bool

# error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `str`, found `Literal[1]`"
# error: [too-many-positional-arguments] "Too many positional arguments to function `foo`: expected 1, got 2"
reveal_type(foo(valid, 1, ""))  # revealed: bool

# TODO: These should reveal `bool`
# error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `(int, /, *args: Unknown, **kwargs: Unknown) -> Unknown`, found `def invalid(x: str, y: str) -> bool`"
reveal_type(foo(invalid, ""))  # revealed: Unknown
# error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `(int, /, *args: Unknown, **kwargs: Unknown) -> Unknown`, found `def invalid(x: str, y: str) -> bool`"
reveal_type(foo(invalid, 1, ""))  # revealed: Unknown
```

### Prepended type variable

```py
from typing import Callable, Concatenate

def decorator[T, R, **P](func: Callable[Concatenate[T, P], R], *args: P.args, **kwargs: P.kwargs) -> Callable[[T], R]:
    def wrapper(arg: T, /) -> R:
        return func(arg, *args, **kwargs)
    return wrapper

def test1(x: int, y: str) -> bool:
    return True

# error: [missing-argument] "No argument provided for required parameter `y` of function `decorator`"
reveal_type(decorator(test1))  # revealed: (int, /) -> bool
reveal_type(decorator(test1, ""))  # revealed: (int, /) -> bool

decorated_test1 = decorator(test1, y="")

reveal_type(decorated_test1(1))  # revealed: bool
# error: [too-many-positional-arguments] "Too many positional arguments: expected 1, got 2"
reveal_type(decorated_test1(1, ""))  # revealed: bool

# error: [invalid-argument-type] "Argument to function `decorator` is incorrect: Expected `(Unknown, /, *args: Unknown, **kwargs: Unknown) -> Unknown`, found `def test2(*, x: int) -> bool`"
@decorator
def test2(*, x: int) -> bool:
    return True

# TODO: This could reveal `(T, /, x: int) -> bool` using partial specialization
reveal_type(test2)  # revealed: (Unknown, /) -> Unknown
```

## `Concatenate` with overloaded functions

A function that accepts an overloaded callable via `Callable[Concatenate[int, P], R]` should be able
to strip the first parameter and infer `P` from the remaining overload signatures.

```py
from typing import Callable, Concatenate, overload

def remove_param[**P, R](func: Callable[Concatenate[int, P], R]) -> Callable[P, R]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        return func(0, *args, **kwargs)
    return wrapper

@overload
def f1(x: int, y: str) -> str: ...
@overload
def f1(x: int, y: int) -> int: ...
@remove_param
def f1(x: int, y: str | int) -> str | int:
    return y

# TODO: Should reveal `Overloaded[(y: str) -> str, (y: int) -> int]`
reveal_type(f1)  # revealed: (y: str) -> str | int
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
reveal_type(f2)  # revealed: Overload[(int, /, y: str) -> str | int, (int, /, y: int) -> str | int]
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
reveal_type(f3)  # revealed: Overload[(int, x: int, /, y: str) -> str | int, (int, x: int, /, y: int) -> str | int]
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

reveal_type(process(MyHandler(), "hello"))  # revealed: bool
reveal_type(process(MyHandler(), name="hello"))  # revealed: bool

def use_callable[**P, R](func: Callable[Concatenate[int, P], R], handler: Handler[P, R]) -> None: ...
```

## Importing from `typing_extensions`

`Concatenate` should work the same whether imported from `typing` or `typing_extensions`.

```py
from typing_extensions import Callable, Concatenate

def _(c: Callable[Concatenate[int, str, ...], bool]):
    reveal_type(c)  # revealed: (int, str, /, *args: Any, **kwargs: Any) -> bool
```

## Assignability

### Implicit concatenate to non-concatenated callable

As per the [spec](https://typing.python.org/en/latest/spec/generics.html#id5):

> A function declared as `def inner(a: A, b: B, *args: P.args, **kwargs: P.kwargs) -> R` has type
> `Callable[Concatenate[A, B, P], R]`.

```py
from typing import Callable, Concatenate

def decorator[**P1](func: Callable[P1, None]) -> Callable[P1, None]:
    def wrapper(*args: P1.args, **kwargs: P1.kwargs) -> None:
        func(*args, **kwargs)

    return wrapper

@decorator
def f1[**P2](fn: Callable[P2, None], x: int, *args: P2.args, **kwargs: P2.kwargs) -> None:
    pass

reveal_type(f1)  # revealed: [**P2](fn: (**P2) -> None, x: int, *args: P2.args, **kwargs: P2.kwargs) -> None

def test(a: str) -> None: ...

reveal_type(f1(test, 1, ""))  # revealed: None

# error: [missing-argument] "No argument provided for required parameter `x`"
# error: [missing-argument] "No argument provided for required parameter `a`"
reveal_type(f1(test))  # revealed: None

# TODO: Currently, this is allowed but should probably raise a diagnostic given that
# `x` is now a positional-only parameter because of the Concatenate form but it might
# be too strict.
reveal_type(f1(fn=test, x=1, a=""))  # revealed: None
```

### Non-concatenated to concatenated callable

```py
from typing import Callable, Concatenate

def decorator[**P1](func: Callable[Concatenate[int, P1], None]) -> Callable[P1, None]:
    def wrapper(*args: P1.args, **kwargs: P1.kwargs) -> None:
        pass
    return wrapper

def foo[**P2](f: Callable[P2, None]) -> None:
    reveal_type(f)  # revealed: (**P2@foo) -> None
    # TODO: This should raise an invalid-argument-type error
    reveal_type(decorator(f))  # revealed: (...) -> None
```

### Concatenate `ParamSpec` to concatenate `...`

```py
from typing import Callable, Concatenate

def gradual_generic[T](func: Callable[..., T]) -> T:
    return func()

def concat_paramspec[**P, T](fn: Callable[Concatenate[int, P], T]):
    reveal_type(gradual_generic(fn))  # revealed: T@concat_paramspec
```

### Concatenate `...` to concatenate `ParamSpec`

```py
from typing import Callable, Concatenate

def concat_paramspec[**P, T](fn: Callable[Concatenate[int, P], T]) -> Callable[Concatenate[int, P], T]:
    return fn

def gradual_generic[T](func: Callable[..., T]):
    # revealed: (int, /, *args: Any, **kwargs: Any) -> T@gradual_generic
    reveal_type(concat_paramspec(func))
```

### Type alias callable

```py
from collections.abc import Callable
from typing import Concatenate

type ConsumerType[**P1] = Callable[Concatenate[Callable[P1, None], P1], None]

def consumer[**P2](x: Callable[P2, None], /, *args: P2.args, **kwargs: P2.kwargs) -> None: ...
def assign[**P3](x: Callable[P3, None], /, *args: P3.args, **kwargs: P3.kwargs) -> None:
    # TODO: This shouldn't be an error
    # error: [invalid-assignment] "Object of type `def consumer[**P2](x: (**P2) -> None, /, *args: P2.args, **kwargs: P2.kwargs) -> None` is not assignable to `ConsumerType[P3@assign]`"
    wrapped: ConsumerType[P3] = consumer
```
