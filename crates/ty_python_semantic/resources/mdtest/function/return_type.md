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

### In `if TYPE_CHECKING` block

Inside an `if TYPE_CHECKING` block, we allow "stub" style function definitions with empty bodies,
since these functions will never actually be called.

```py
from typing import TYPE_CHECKING

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

## Inferred return type

### Free function

If a function's return type is not annotated, it is inferred. The inferred type is the union of all
possible return types.

```py
def f():
    return 1

reveal_type(f())  # revealed: Literal[1]

def g(cond: bool):
    if cond:
        return 1
    else:
        return "a"

reveal_type(g(True))  # revealed: Literal[1, "a"]

# This function implicitly returns `None`.
def h(x: int, y: str):
    if x > 10:
        return x
    elif x > 5:
        return y

reveal_type(h(1, "a"))  # revealed: int | str | None

def generator():
    yield 1
    yield 2
    return None

# TODO: Should be `Generator[Literal[1, 2], Any, None]`
reveal_type(generator())  # revealed: None
```

The return type of a recursive function is also inferred. When the return type inference would
diverge, it is truncated and replaced with the type `Unknown`.

```py
def fibonacci(n: int):
    if n == 0:
        return 0
    elif n == 1:
        return 1
    else:
        return fibonacci(n - 1) + fibonacci(n - 2)

reveal_type(fibonacci(5))  # revealed: int

def even(n: int):
    if n == 0:
        return True
    else:
        return odd(n - 1)

def odd(n: int):
    if n == 0:
        return False
    else:
        return even(n - 1)

reveal_type(even(1))  # revealed: bool
reveal_type(odd(1))  # revealed: bool

def repeat_a(n: int):
    if n <= 0:
        return ""
    else:
        return repeat_a(n - 1) + "a"

reveal_type(repeat_a(3))  # revealed: str

def divergent(value):
    if type(value) is tuple:
        return (divergent(value[0]),)
    else:
        return None

# tuple[tuple[tuple[...] | None] | None] | None => tuple[Unknown] | None
reveal_type(divergent((1,)))  # revealed: Divergent | None

def call_divergent(x: int):
    return (divergent((1, 2, 3)), x)

# TODO: it would be better to reveal `tuple[Divergent | None, int]`
reveal_type(call_divergent(1))  # revealed: Divergent

def nested_scope():
    def inner():
        return nested_scope()
    return inner()

reveal_type(nested_scope())  # revealed: Never

def eager_nested_scope():
    class A:
        x = eager_nested_scope()

    return A.x

reveal_type(eager_nested_scope())  # revealed: Unknown
```

### Class method

If a method's return type is not annotated, it is also inferred, but the inferred type is a union of
all possible return types and `Unknown`. This is because a method of a class may be overridden by
its subtypes. For example, if the return type of a method is inferred to be `int`, the type the
coder really intended might be `int | None`, in which case it would be impossible for the overridden
method to return `None`.

```py
class C:
    def f(self):
        return 1

class D(C):
    def f(self):
        return None

reveal_type(C().f())  # revealed: Literal[1] | Unknown
reveal_type(D().f())  # revealed: None | Literal[1] | Unknown
```

However, in the following cases, `Unknown` is not included in the inferred return type because there
is no ambiguity in the subclass.

- The class or the method is marked as `final`.

```py
from typing import final

@final
class C:
    def f(self):
        return 1

class D:
    @final
    def f(self):
        return "a"

reveal_type(C().f())  # revealed: Literal[1]
reveal_type(D().f())  # revealed: Literal["a"]
```

- The method overrides the methods of the base classes, and the return types of the base class
    methods are known (In this case, the return type of the method is the intersection of the return
    types of the methods in the base classes).

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

class C:
    def f(self) -> int:
        return 1

    def g[T](self, x: T) -> T:
        return x

    def h[T: int](self, x: T) -> T:
        return x

    def i[T: int](self, x: T) -> list[T]:
        return [x]

class D(C):
    def f(self):
        return 2
    # TODO: This should be an invalid-override error.
    # If the override is invalid, the type of the method should be that of the base class method.
    def g(self, x: int):
        return 2
    # A strict application of the Liskov Substitution Principle would consider
    # this an invalid override because it violates the guarantee that the method returns
    # the same type as its input type (any type smaller than int),
    # but neither mypy nor pyright will throw an error for this.
    def h(self, x: int):
        return 2

    def i(self, x: int):
        return [2]

class E(D):
    def f(self):
        return 3

reveal_type(C().f())  # revealed: int
reveal_type(D().f())  # revealed: int
reveal_type(E().f())  # revealed: int
reveal_type(C().g(1))  # revealed: Literal[1]
reveal_type(D().g(1))  # revealed: Literal[2] | Unknown
reveal_type(C().h(1))  # revealed: Literal[1]
reveal_type(D().h(1))  # revealed: Literal[2] | Unknown
reveal_type(C().h(True))  # revealed: Literal[True]
reveal_type(D().h(True))  # revealed: Literal[2] | Unknown
reveal_type(C().i(1))  # revealed: list[Literal[1]]
reveal_type(D().i(1))  # revealed: list[Unknown]

class F:
    def f(self) -> Literal[1, 2]:
        return 2

class G:
    def f(self) -> Literal[2, 3]:
        return 2

class H(F, G):
    # TODO: should be an invalid-override error
    def f(self):
        raise NotImplementedError

class I(F, G):
    # TODO: should be an invalid-override error
    @final
    def f(self):
        raise NotImplementedError

# We use a return type of `F.f` according to the MRO.
reveal_type(H().f())  # revealed: Literal[1, 2]
reveal_type(I().f())  # revealed: Never

class C2[T]:
    def f(self, x: T) -> T:
        return x

class D2(C2[int]):
    def f(self, x: int):
        return x

reveal_type(D2().f(1))  # revealed: int
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
