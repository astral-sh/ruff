# Function return type

When a function's return type is annotated, all return statements are checked to ensure that the
type of the returned value is assignable to the annotated return type.

## Basic examples

A return value assignable to the annotated return type is valid.

```py
def f() -> int:
    return 1
```

The type of the value obtained by calling a function is the annotated return type, not the inferred
return type.

```py
reveal_type(f())  # revealed: int
```

A `raise` is equivalent to a return of `Never`, which is assignable to any annotated return type.

```py
def f() -> str:
    raise ValueError()

reveal_type(f())  # revealed: str
```

## Stub functions

"Stub" function definitions (that is, function definitions with an empty body) are permissible in
stub files, or in a few other locations: Protocol method definitions, abstract methods, and
overloads. In this case the function body is considered to be omitted (thus no return type checking
is performed on it), not assumed to implicitly return `None`.

A stub function's "empty" body may contain only an optional docstring, followed (optionally) by an
ellipsis (`...`) or `pass`.

### In stub file

```pyi
def f() -> int: ...

def f() -> int:
    pass

def f() -> int:
    """Some docstring"""

def f() -> int:
    """Some docstring"""
    ...
```

### In Protocol

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, TypeVar

class Bar(Protocol):
    def f(self) -> int: ...

class Baz(Bar):
    # error: [invalid-return-type]
    def f(self) -> int: ...

T = TypeVar("T")

class Qux(Protocol[T]):
    def f(self) -> int: ...

class Foo(Protocol):
    def f[T](self, v: T) -> T: ...

t = (Protocol, int)
reveal_type(t[0])  # revealed: typing.Protocol

class Lorem(t[0]):
    def f(self) -> int: ...
```

### In abstract method

```toml
[environment]
python-version = "3.12"
```

```py
from abc import ABC, abstractmethod

class Foo(ABC):
    @abstractmethod
    def f(self) -> int: ...
    @abstractmethod
    def g[T](self, x: T) -> T: ...

class Bar[T](ABC):
    @abstractmethod
    def f(self) -> int: ...
    @abstractmethod
    def g[T](self, x: T) -> T: ...

# error: [invalid-return-type]
def f() -> int: ...
@abstractmethod  # Semantically meaningless, accepted nevertheless
def g() -> int: ...
```

### In overload

```py
from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str):
    return x
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

# error: [invalid-return-type]
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

## NotImplemented

### Default Python version

`NotImplemented` is a special symbol in Python. It is commonly used to control the fallback behavior
of special dunder methods. You can find more details in the
[documentation](https://docs.python.org/3/library/numbers.html#implementing-the-arithmetic-operations).

```py
from __future__ import annotations

class A:
    def __add__(self, o: A) -> A:
        return NotImplemented
```

However, as shown below, `NotImplemented` should not cause issues with the declared return type.

```py
def f() -> int:
    return NotImplemented

def f(cond: bool) -> int:
    if cond:
        return 1
    else:
        return NotImplemented

def f(x: int) -> int | str:
    if x < 0:
        return -1
    elif x == 0:
        return NotImplemented
    else:
        return "test"

def f(cond: bool) -> str:
    return "hello" if cond else NotImplemented

def f(cond: bool) -> int:
    # error: [invalid-return-type] "Return type does not match returned value: expected `int`, found `Literal["hello"]`"
    return "hello" if cond else NotImplemented
```

### Python 3.10+

Unlike Ellipsis, `_NotImplementedType` remains in `builtins.pyi` regardless of the Python version.
Even if `builtins._NotImplementedType` is fully replaced by `types.NotImplementedType` in the
future, it should still work as expected.

```toml
[environment]
python-version = "3.10"
```

```py
def f() -> int:
    return NotImplemented

def f(cond: bool) -> str:
    return "hello" if cond else NotImplemented
```

## Generator functions

<!-- snapshot-diagnostics -->

A function with a `yield` or `yield from` expression anywhere in its body is a
[generator function](https://docs.python.org/3/glossary.html#term-generator). A generator function
implicitly returns an instance of `types.GeneratorType` even if it does not contain any `return`
statements.

```py
import types
import typing

def f() -> types.GeneratorType:
    yield 42

def g() -> typing.Generator:
    yield 42

def h() -> typing.Iterator:
    yield 42

def i() -> typing.Iterable:
    yield 42

def i2() -> typing.Generator:
    yield from i()

def j() -> str:  # error: [invalid-return-type]
    yield 42
```

If it is an `async` function with a `yield` statement in its body, it is an
[asynchronous generator function](https://docs.python.org/3/glossary.html#term-asynchronous-generator).
An asynchronous generator function implicitly returns an instance of `types.AsyncGeneratorType` even
if it does not contain any `return` statements.

```py
import types
import typing

async def f() -> types.AsyncGeneratorType:
    yield 42

async def g() -> typing.AsyncGenerator:
    yield 42

async def h() -> typing.AsyncIterator:
    yield 42

async def i() -> typing.AsyncIterable:
    yield 42

async def j() -> str:  # error: [invalid-return-type]
    yield 42
```

## Diagnostics for `invalid-return-type` on non-protocol subclasses of protocol classes

<!-- snapshot-diagnostics -->

We emit a nice subdiagnostic in this situation explaining the probable error here:

```py
from typing_extensions import Protocol

class Abstract(Protocol):
    def method(self) -> str: ...

class Concrete(Abstract):
    def method(self) -> str: ...  # error: [invalid-return-type]
```
