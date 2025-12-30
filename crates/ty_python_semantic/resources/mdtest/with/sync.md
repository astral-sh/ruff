# With statements

## Basic `with` statement

The type of the target variable in a `with` statement is the return type from the context manager's
`__enter__` method.

```py
class Target: ...

class Manager:
    def __enter__(self) -> Target:
        return Target()

    def __exit__(self, exc_type, exc_value, traceback): ...

with Manager() as f:
    reveal_type(f)  # revealed: Target
```

## Union context manager

```py
def _(flag: bool):
    class Manager1:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class Manager2:
        def __enter__(self) -> int:
            return 42

        def __exit__(self, exc_type, exc_value, traceback): ...

    context_expr = Manager1() if flag else Manager2()

    with context_expr as f:
        reveal_type(f)  # revealed: str | int
```

## Context manager without an `__enter__` or `__exit__` method

```py
class Manager: ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Context manager without an `__enter__` method

```py
class Manager:
    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__`"
with Manager():
    ...
```

## Context manager without an `__exit__` method

```py
class Manager:
    def __enter__(self): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__exit__`"
with Manager():
    ...
```

## Context manager with non-callable `__enter__` attribute

```py
class Manager:
    __enter__: int = 42

    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__enter__`"
with Manager():
    ...
```

## Context manager with non-callable `__exit__` attribute

```py
from typing_extensions import Self

class Manager:
    def __enter__(self) -> Self:
        return self
    __exit__: int = 32

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__exit__`"
with Manager():
    ...
```

## Context expression with possibly-unbound union variants

```py
def _(flag: bool):
    class Manager1:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class NotAContextManager: ...
    context_expr = Manager1() if flag else NotAContextManager()

    # error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `with` because the methods `__enter__` and `__exit__` are possibly missing"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Context expression with "sometimes" callable `__enter__` method

```py
def _(flag: bool):
    class Manager:
        if flag:
            def __enter__(self) -> str:
                return "abcd"

        def __exit__(self, *args): ...

    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because the method `__enter__` may be missing"
    with Manager() as f:
        reveal_type(f)  # revealed: str
```

## Invalid `__enter__` signature

```py
class Manager:
    def __enter__() -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

context_expr = Manager()

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__enter__`"
with context_expr as f:
    reveal_type(f)  # revealed: str
```

## Accidental use of non-async `with`

<!-- snapshot-diagnostics -->

If a synchronous `with` statement is used on a type with `__aenter__` and `__aexit__`, we show a
diagnostic hint that the user might have intended to use `async with` instead.

```py
class Manager:
    async def __aenter__(self): ...
    async def __aexit__(self, *args): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Incorrect signatures

The sub-diagnostic is also provided if the signatures of `__aenter__` and `__aexit__` do not match
the expected signatures for a context manager:

```py
class Manager:
    async def __aenter__(self): ...
    async def __aexit__(self, typ: str, exc, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Incorrect number of arguments

Similarly, we also show the hint if the functions have the wrong number of arguments:

```py
class Manager:
    async def __aenter__(self, wrong_extra_arg): ...
    async def __aexit__(self, typ, exc, traceback, wrong_extra_arg): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## `@contextmanager`

```py
from contextlib import contextmanager
from typing import Generator

class Session: ...

@contextmanager
def connect() -> Generator[Session, None, None]:
    yield Session()

# revealed: () -> _GeneratorContextManager[Session, None, None]
reveal_type(connect)

def main():
    with connect() as session:
        reveal_type(session)  # revealed: Session
```

This also works with `Iterator` return types:

```py
from typing import Iterator

@contextmanager
def connect_iterator() -> Iterator[Session]:
    yield Session()

# revealed: () -> _GeneratorContextManager[Session, None, None]
reveal_type(connect_iterator)

def main_iterator():
    with connect_iterator() as session:
        reveal_type(session)  # revealed: Session
```

And with `GeneratorType` return types:

```py
from types import GeneratorType

@contextmanager
def connect_generator() -> GeneratorType[Session, None, None]:
    yield Session()

# revealed: () -> _GeneratorContextManager[Session, None, None]
reveal_type(connect_generator)

def main_generator():
    with connect_generator() as session:
        reveal_type(session)  # revealed: Session
```

## Generic classmethod with `@contextmanager` and TypeVar

Generic classmethods with `@contextmanager` should correctly infer the type parameter when called on
subclasses:

```py
from contextlib import contextmanager
from typing import Iterator, TypeVar

T = TypeVar("T", bound="Base")

class Base:
    @classmethod
    def create(cls: type[T]) -> T:
        return cls()

    @classmethod
    @contextmanager
    def yielder(cls: type[T]) -> Iterator[T]:
        yield cls.create()

class Child(Base): ...

def main():
    with Child.yielder() as child:
        reveal_type(child)  # revealed: Child
```

## Generic classmethod with `@contextmanager` and Self

```py
from contextlib import contextmanager
from typing import Iterator
from typing_extensions import Self

class Base:
    @classmethod
    def create(cls) -> Self:
        return cls()

    @classmethod
    @contextmanager
    def yielder(cls) -> Iterator[Self]:
        yield cls.create()

class Child(Base): ...

def main():
    with Child.yielder() as child:
        reveal_type(child)  # revealed: Child
```
