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
reveal_type(t[0])  # revealed: <special-form 'typing.Protocol'>

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

### In `if TYPE_CHECKING` block

Inside an `if TYPE_CHECKING` block, we allow "stub" style function definitions with empty bodies,
since these functions will never actually be called.

`compat/__init__.py`:

```py
```

`compat/sub/__init__.py`:

```py
```

`compat/sub/sub.py`:

```py
from typing import TYPE_CHECKING
```

`main.py`:

```py
from typing import TYPE_CHECKING
import typing
import typing as t
import compat.sub.sub

if TYPE_CHECKING:
    def f() -> int: ...

else:
    def f() -> str:
        return "hello"

reveal_type(f)  # revealed: def f() -> int

if not TYPE_CHECKING:
    ...
elif True:
    def g() -> str: ...

else:
    def h() -> str: ...

if not TYPE_CHECKING:
    def i() -> int:
        return 1

else:
    def i() -> str: ...

reveal_type(i)  # revealed: def i() -> str

if False:
    ...
elif TYPE_CHECKING:
    def j() -> str: ...

else:
    def j_() -> str: ...  # error: [invalid-return-type]

if False:
    ...
elif not TYPE_CHECKING:
    def k_() -> str: ...  # error: [invalid-return-type]

else:
    def k() -> str: ...

class Foo:
    if TYPE_CHECKING:
        def f(self) -> int: ...

if TYPE_CHECKING:
    class Bar:
        def f(self) -> int: ...

def get_bool() -> bool:
    return True

if TYPE_CHECKING:
    if get_bool():
        def l() -> str: ...

if get_bool():
    if TYPE_CHECKING:
        def m() -> str: ...

if TYPE_CHECKING:
    if not TYPE_CHECKING:
        def n() -> str: ...

if typing.TYPE_CHECKING:
    def o() -> str: ...

if not typing.TYPE_CHECKING:
    def p() -> str: ...  # error: [invalid-return-type]

if compat.sub.sub.TYPE_CHECKING:
    def q() -> str: ...

if not compat.sub.sub.TYPE_CHECKING:
    def r() -> str: ...  # error: [invalid-return-type]

if t.TYPE_CHECKING:
    def s() -> str: ...

if not t.TYPE_CHECKING:
    def t() -> str: ...  # error: [invalid-return-type]
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

```toml
[environment]
python-version = "3.12"
```

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

class A[T]: ...

def f() -> A[int]:
    class A[T]: ...
    return A[int]()  # error: [invalid-return-type]

class B: ...

def g() -> B:
    class B: ...
    return B()  # error: [invalid-return-type]
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

## Invalid implicit return type always None

<!-- snapshot-diagnostics -->

If the function has no `return` statement or if it has only bare `return` statement (no variable in
the return statement), then we show a diagnostic hint that the return annotation should be `-> None`
or a `return` statement should be added.

```py
# error: [invalid-return-type]
def f() -> int:
    print("hello")
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

`NotImplemented` is only special-cased for return types (mirroring the way the interpreter applies
special casing for the symbol at runtime). It is not generally considered assignable to every other
type:

```py
# Other type checkers do not emit an error here,
# but this is likely not a deliberate feature they've implemented;
# it's probably because `NotImplementedType` inherits from `Any`
# according to typeshed. We override typeshed's incorrect MRO
# for more precise type inference.
x: int = NotImplemented  # error: [invalid-assignment]
```

### Python 3.10+

We correctly understand the semantics of `NotImplemented` on all Python versions, even though the
class `types.NotImplementedType` is only exposed in the `types` module on Python 3.10+.

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

### Synchronous

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

### Asynchronous

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

## Diagnostics for `invalid-return-type` on dynamic type

```toml
environment.python-version = "3.12"
```

```py
from typing import Never, Any

def f(func: Any) -> Never:  # error: [invalid-return-type]
    func()
```
