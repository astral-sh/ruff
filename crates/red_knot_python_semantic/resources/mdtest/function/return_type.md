# Function return type

When a function's return type is specified, all return statements are checked to ensure that the
type of the returned value is assignable to the specified type. A raise statement is interpreted as
returning a value of type `Never`. The type of the value obtained by calling a function is the one
specified as the return type, not the inferred return type.

## Basic example

```py
from typing import Any

def f() -> int:
    return 1

reveal_type(f())  # revealed: int

def f() -> str:
    raise ValueError()

reveal_type(f())  # revealed: str

def f(x: Any) -> Any:
    reveal_type(x)  # revealed: Any
```

## Empty function with return type

```py
from typing import overload, Protocol
from abc import ABC, abstractmethod

class Foo(ABC):
    @abstractmethod
    # TODO: no error
    # error: [invalid-return-type]
    def f(self) -> int: ...
    @abstractmethod
    # error: [invalid-return-type]
    def g[T](self, x: T) -> T: ...

class Bar(Protocol):
    # TODO: no error
    # error: [invalid-return-type]
    def f(self) -> int: ...

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str):
    return x
```

## Stub file

If you specify a return type for a function in a stub file, its body must be empty, i.e. it must
consist only of ellipsis (`...`) or docstrings.

```pyi
def f() -> int: ...

def f() -> int:
    ...
    ...

def f() -> int:
    """Some docstring"""
    ...
```

## Conditional return type

```py
def f(cond: bool) -> int:
    if cond:
        return 1
    else:
        return 2

def f(cond: bool) -> int | None:
    if cond:
        return 1
    else:
        return

def f(cond: bool) -> int:
    if cond:
        return 1
    else:
        raise ValueError()

def f(cond: bool) -> str | int:
    if cond:
        return "a"
    else:
        return 1
```

## Implicit return type

```py
def f(cond: bool) -> int | None:
    if cond:
        return 1

# no implicit return
def f() -> int:
    if True:
        return 1

# no implicit return
def f(cond: bool) -> int:
    cond = True
    if cond:
        return 1

def f(cond: bool) -> int:
    if cond:
        cond = True
    else:
        return 1
    if cond:
        return 2
```

## Invalid return type

<!-- snapshot-diagnostics -->

```py
# error: [invalid-return-type]
def f() -> int:
    1

def f() -> str:
    # error: [invalid-return-type]
    return 1

def f() -> int:
    # error: [invalid-return-type]
    return

from typing import TypeVar

T = TypeVar("T")

# TODO: `invalid-return-type` error should be emitted
def m(x: T) -> T: ...
```

## Invalid return type in stub file

<!-- snapshot-diagnostics -->

```pyi
def f() -> int:
    # error: [invalid-return-type]
    return ...

# error: [invalid-return-type]
def foo() -> int:
    print("...")
    ...

# error: [invalid-return-type]
def foo() -> int:
    f"""{foo} is a function that ..."""
    ...
```

## Invalid conditional return type

<!-- snapshot-diagnostics -->

```py
def f(cond: bool) -> str:
    if cond:
        return "a"
    else:
        # error: [invalid-return-type]
        return 1

def f(cond: bool) -> str:
    if cond:
        # error: [invalid-return-type]
        return 1
    else:
        # error: [invalid-return-type]
        return 2
```

## Invalid implicit return type

<!-- snapshot-diagnostics -->

```py
def f() -> None:
    if False:
        # error: [invalid-return-type]
        return 1

# error: [invalid-return-type]
def f(cond: bool) -> int:
    if cond:
        return 1

# error: [invalid-return-type]
def f(cond: bool) -> int:
    if cond:
        raise ValueError()

# error: [invalid-return-type]
def f(cond: bool) -> int:
    if cond:
        cond = False
    else:
        return 1
    if cond:
        return 2
```
