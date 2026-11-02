# `async` / `await`

## Basic

```py
async def retrieve() -> int:
    return 42

async def main():
    result = await retrieve()

    reveal_type(result)  # revealed: int
```

## Generic `async` functions

```py
from typing import TypeVar

T = TypeVar("T")

async def persist(x: T) -> T:
    return x

async def f(x: int):
    result = await persist(x)

    reveal_type(result)  # revealed: int
```

## Use cases

### `Future`

```py
import asyncio
import concurrent.futures

def blocking_function() -> int:
    return 42

async def main():
    loop = asyncio.get_event_loop()
    with concurrent.futures.ThreadPoolExecutor() as pool:
        result = await loop.run_in_executor(pool, blocking_function)
        reveal_type(result)  # revealed: int
```

### `asyncio.Task`

```py
import asyncio

async def f() -> int:
    return 1

async def main():
    task = asyncio.create_task(f())

    result = await task

    reveal_type(result)  # revealed: int
```

### `asyncio.gather`

```py
import asyncio

async def task(name: str) -> int:
    return len(name)

async def main():
    a, b = await asyncio.gather(
        task("A"),
        task("B"),
    )

    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

## Under the hood

```toml
[environment]
python-version = "3.12"  # Use 3.12 to be able to use PEP 695 generics
```

Let's look at the example from the beginning again:

```py
async def retrieve() -> int:
    return 42
```

When we look at the signature of this function, we see that it actually returns a `CoroutineType`:

```py
reveal_type(retrieve)  # revealed: def retrieve() -> CoroutineType[Any, Any, int]
```

The expression `await retrieve()` desugars into a call to the `__await__` dunder method on the
`CoroutineType` object, followed by a `yield from`. Let's first see the return type of `__await__`:

```py
reveal_type(retrieve().__await__())  # revealed: Generator[Any, None, int]
```

We can see that this returns a `Generator` that yields `Any`, and eventually returns `int`. For the
final type of the `await` expression, we retrieve that third argument of the `Generator` type:

```py
from typing import Generator

def _():
    result = yield from retrieve().__await__()
    reveal_type(result)  # revealed: int
```

## Un-annotated async functions

An `async def` with no annotated return type is still known to return `CoroutineType` of `Unknown`,
not just `Unknown`:

```py
async def f():
    pass

reveal_type(f())  # revealed: CoroutineType[Any, Any, Unknown]
```

## Awaiting intersection types (3.13+)

```toml
[environment]
python-version = "3.13"
```

Intersection types can be awaited when their elements are awaitable. This is important for patterns
like `inspect.isawaitable()` which narrow types to intersections with `Awaitable`.

```py
import inspect
from typing import Any

def get_any() -> Any:
    pass

async def test():
    x = get_any()
    if inspect.isawaitable(x):
        reveal_type(x)  # revealed: Any & Awaitable[Any]
        y = await x
        reveal_type(y)  # revealed: Any
```

The return type of awaiting an intersection is the intersection of the return types of awaiting each
element:

```py
from typing import Coroutine
from ty_extensions import Intersection

class A: ...
class B: ...

async def test(x: Intersection[Coroutine[object, object, A], Coroutine[object, object, B]]):
    y = await x
    reveal_type(y)  # revealed: A & B
```

If some intersection elements are not awaitable, we skip them and use the return types from the
awaitable elements:

```py
from typing import Coroutine
from ty_extensions import Intersection

class NotAwaitable: ...

async def test(x: Intersection[Coroutine[object, object, str], NotAwaitable]):
    y = await x
    reveal_type(y)  # revealed: str
```

When an intersection includes `Any`, awaiting succeeds for both elements. `Any` is awaitable and
returns `Any`:

```py
from typing import Coroutine, Any
from ty_extensions import Intersection

async def test(x: Intersection[Coroutine[object, object, int], Any]):
    y = await x
    reveal_type(y)  # revealed: int & Any
```

When an intersection has three or more elements, some awaitable and some not, the non-awaitable
elements are skipped:

```py
from typing import Coroutine
from ty_extensions import Intersection

class A: ...
class B: ...
class NotAwaitable: ...

async def test(x: Intersection[Coroutine[object, object, A], Coroutine[object, object, B], NotAwaitable]):
    y = await x
    reveal_type(y)  # revealed: A & B
```

If all intersection elements fail to be awaitable, the await is invalid:

```py
from ty_extensions import Intersection

class NotAwaitable1: ...
class NotAwaitable2: ...

async def test(x: Intersection[NotAwaitable1, NotAwaitable2]):
    # error: [invalid-await]
    await x
```

When a callable is narrowed with `TypeIs[Top[Callable[..., Awaitable[...]]]]`, the narrowed
intersection should contribute the top-callable return type to the call result, even though the
top-callable itself cannot be safely called.

```py
from typing import Awaitable, Callable
from typing_extensions import TypeIs
from ty_extensions import Top

def is_async_callable(x: object) -> TypeIs[Top[Callable[..., Awaitable[object]]]]:
    return True

async def f(fn: Callable[[int], int | Awaitable[int]]) -> None:
    if is_async_callable(fn):
        reveal_type(fn)  # revealed: ((int, /) -> int | Awaitable[int]) & Top[(...) -> Top[Awaitable[object]]]
        result = fn(1)
        # This includes `int & Top[Awaitable[object]]`: an `int` subtype could define `__await__`.
        reveal_type(result)  # revealed: (int & Top[Awaitable[object]]) | Awaitable[int]
        reveal_type(await result)  # revealed: object
```

## Awaiting intersection types (Python 3.12 or lower)

```toml
[environment]
python-version = "3.12"
```

The return type of awaiting an intersection is the intersection of the return types of awaiting each
element:

```py
from typing import Coroutine
from ty_extensions import Intersection

class A: ...
class B: ...

async def test(x: Intersection[Coroutine[object, object, A], Coroutine[object, object, B]]):
    y = await x
    reveal_type(y)  # revealed: A & B
```

## Async generator stubs

An `async def` function without a `yield` expression returns a coroutine, even if its return
annotation is `AsyncIterator` or `AsyncGenerator`. To describe an async generator in a stub, use
`def` with the async iterator return annotation, or include a `yield` expression in the body.

### Iterating over a stubbed async generator

When a stub omits `yield`, iterating over the resulting coroutine reports an error that points to
the declaration and explains how to declare an async generator. This also applies to callable
objects and async comprehensions.

`stubs.pyi`:

```pyi
from collections.abc import AsyncGenerator, AsyncIterator

async def values() -> AsyncIterator[int]: ...

class Values:
    async def __call__(self) -> AsyncGenerator[int, None]: ...
```

`main.py`:

```py
from stubs import values

async def consume() -> None:
    # snapshot
    async for value in values():
        pass
```

```snapshot
error[not-iterable]: Object of type `CoroutineType[Any, Any, AsyncIterator[int]]` is not async-iterable
 --> src/main.py:5:24
  |
5 |     async for value in values():
  |                        ^^^^^^^^
  |
 ::: src/stubs.pyi:3:1
  |
3 | async def values() -> AsyncIterator[int]: ...
  | ---------------------------------------- Without `yield` in the function body this function returns a coroutine
info: It has no `__aiter__` method
help: `await` the coroutine before iterating over its result
help: To declare `values` as an async generator, use `def` rather than `async def` or add `yield` to the body
```

`callable.py`:

```py
from stubs import Values

async def consume() -> None:
    # snapshot
    [value async for value in Values()()]
```

```snapshot
error[not-iterable]: Object of type `CoroutineType[Any, Any, AsyncGenerator[int, None]]` is not async-iterable
 --> src/callable.py:5:31
  |
5 |     [value async for value in Values()()]
  |                               ^^^^^^^^^^
  |
 ::: src/stubs.pyi:6:5
  |
6 |     async def __call__(self) -> AsyncGenerator[int, None]: ...
  |     ----------------------------------------------------- Without `yield` in the function body this function returns a coroutine
info: It has no `__aiter__` method
help: `await` the coroutine before iterating over its result
help: To declare `__call__` as an async generator, use `def` rather than `async def` or add `yield` to the body
```

### Overriding a stubbed async generator

An abstract method with an empty `async def` body describes a coroutine function. An async generator
cannot override it, and the diagnostic explains how to correct the abstract declaration.

```py
from abc import ABC, abstractmethod
from collections.abc import AsyncIterator

class Base(ABC):
    @abstractmethod
    async def values(self) -> AsyncIterator[int]: ...

class Derived(Base):
    # snapshot
    async def values(self) -> AsyncIterator[int]:
        yield 1
```

```snapshot
error[invalid-method-override]: Invalid override of method `values`
  --> src/mdtest_snippet.py:10:15
   |
 6 |     async def values(self) -> AsyncIterator[int]: ...
   |     --------------------------------------------
   |     |         |
   |     |         `Base.values` defined here
   |     Without `yield` in the function body this function returns a coroutine
 7 |
 8 | class Derived(Base):
 9 |     # snapshot
10 |     async def values(self) -> AsyncIterator[int]:
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Base.values`
info: incompatible return types: `AsyncIterator[int]` is not assignable to `CoroutineType[Any, Any, AsyncIterator[int]]`
help: To declare `values` as an async generator, use `def` rather than `async def` or add `yield` to the body
info: This violates the Liskov Substitution Principle
```

### Assigning a stubbed async generator

The same guidance applies when assigning a stubbed function to an async-generator callable type.

`declarations-script.pyi`:

```pyi
from collections.abc import AsyncIterator, Callable

async def values() -> AsyncIterator[int]: ...

# snapshot
factory: Callable[[], AsyncIterator[int]] = values
```

```snapshot
error[invalid-assignment]: Object of type `def values() -> CoroutineType[Any, Any, AsyncIterator[int]]` is not assignable to `() -> AsyncIterator[int]`
 --> src/declarations-script.pyi:6:45
  |
3 | async def values() -> AsyncIterator[int]: ...
  | ---------------------------------------- Without `yield` in the function body this function returns a coroutine
4 |
5 | # snapshot
6 | factory: Callable[[], AsyncIterator[int]] = values
  |          --------------------------------   ^^^^^^ Incompatible value of type `def values() -> CoroutineType[Any, Any, AsyncIterator[int]]`
  |          |
  |          Declared type
info: incompatible return types: `CoroutineType[Any, Any, AsyncIterator[int]]` is not assignable to `AsyncIterator[int]`
info: └── type `CoroutineType[Any, Any, AsyncIterator[int]]` is not assignable to protocol `AsyncIterator[int]`
info:     └── protocol member `__aiter__` is not defined on type `CoroutineType[Any, Any, AsyncIterator[int]]`
help: To declare `values` as an async generator, use `def` rather than `async def` or add `yield` to the body
```

### Correct async generator declarations

Both forms of declaration can be overridden by an async generator and called without `await`.

```py
from abc import ABC, abstractmethod
from collections.abc import AsyncIterator

class Base(ABC):
    @abstractmethod
    def values(self) -> AsyncIterator[int]: ...
    @abstractmethod
    async def more_values(self) -> AsyncIterator[int]:
        yield 1

class Derived(Base):
    async def values(self) -> AsyncIterator[int]:
        yield 1

    async def more_values(self) -> AsyncIterator[int]:
        yield 2

async def consume(base: Base) -> None:
    async for value in base.values():
        reveal_type(value)  # revealed: int
    async for value in base.more_values():
        reveal_type(value)  # revealed: int
```

### Coroutines returning async iterators

A coroutine can intentionally return an async iterator. Its caller must await it before iterating
over its result; an async iterator return annotation does not make the function an async generator.

```py
from collections.abc import AsyncIterator

async def values() -> AsyncIterator[int]:
    yield 1

async def factory() -> AsyncIterator[int]:
    return values()

async def consume() -> None:
    # snapshot
    async for value in factory():
        pass

    async for value in await factory():
        reveal_type(value)  # revealed: int
```

```snapshot
error[not-iterable]: Object of type `CoroutineType[Any, Any, AsyncIterator[int]]` is not async-iterable
  --> src/mdtest_snippet.py:11:24
   |
11 |     async for value in factory():
   |                        ^^^^^^^^^
info: It has no `__aiter__` method
help: `await` the coroutine before iterating over its result
```

### Unrelated coroutine errors

Coroutines returning other types do not receive advice about async generator stubs.

```py
async def value() -> int:
    return 1

async def consume() -> None:
    # snapshot
    async for item in value():
        pass
```

```snapshot
error[not-iterable]: Object of type `CoroutineType[Any, Any, int]` is not async-iterable
 --> src/mdtest_snippet.py:6:23
  |
6 |     async for item in value():
  |                       ^^^^^^^
info: It has no `__aiter__` method
```

### Third-party async generator stubs

When the declaration comes from a dependency, the diagnostic shows its location and suggests
reporting an incorrect stub to the library maintainers. This applies to iteration, callable
assignments, and method overrides.

```toml
[environment]
python = "/.venv"
```

`/.venv/<path-to-site-packages>/dependency.pyi`:

```pyi
from collections.abc import AsyncIterator

async def values() -> AsyncIterator[int]: ...

class Base:
    async def values(self) -> AsyncIterator[int]: ...
```

`main.py`:

```py
from dependency import values

async def consume() -> None:
    # snapshot
    async for value in values():
        pass
```

```snapshot
error[not-iterable]: Object of type `CoroutineType[Any, Any, AsyncIterator[int]]` is not async-iterable
 --> src/main.py:5:24
  |
5 |     async for value in values():
  |                        ^^^^^^^^
  |
 ::: .venv/<path-to-site-packages>/dependency.pyi:3:1
  |
3 | async def values() -> AsyncIterator[int]: ...
  | ---------------------------------------- Without `yield` in the function body this function returns a coroutine
info: It has no `__aiter__` method
help: `await` the coroutine before iterating over its result
help: If an async generator was intended, report this stub to the library maintainers
```

`assignment.py`:

```py
from collections.abc import AsyncIterator, Callable
from dependency import values

# snapshot
factory: Callable[[], AsyncIterator[int]] = values
```

```snapshot
error[invalid-assignment]: Object of type `def values() -> CoroutineType[Any, Any, AsyncIterator[int]]` is not assignable to `() -> AsyncIterator[int]`
 --> src/assignment.py:5:45
  |
5 | factory: Callable[[], AsyncIterator[int]] = values
  |          --------------------------------   ^^^^^^ Incompatible value of type `def values() -> CoroutineType[Any, Any, AsyncIterator[int]]`
  |          |
  |          Declared type
  |
 ::: .venv/<path-to-site-packages>/dependency.pyi:3:1
  |
3 | async def values() -> AsyncIterator[int]: ...
  | ---------------------------------------- Without `yield` in the function body this function returns a coroutine
info: incompatible return types: `CoroutineType[Any, Any, AsyncIterator[int]]` is not assignable to `AsyncIterator[int]`
info: └── type `CoroutineType[Any, Any, AsyncIterator[int]]` is not assignable to protocol `AsyncIterator[int]`
info:     └── protocol member `__aiter__` is not defined on type `CoroutineType[Any, Any, AsyncIterator[int]]`
help: If an async generator was intended, report this stub to the library maintainers
```

`override.py`:

```py
from collections.abc import AsyncIterator
from dependency import Base

class Derived(Base):
    # snapshot
    async def values(self) -> AsyncIterator[int]:
        yield 1
```

```snapshot
error[invalid-method-override]: Invalid override of method `values`
 --> src/override.py:6:15
  |
6 |     async def values(self) -> AsyncIterator[int]:
  |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Base.values`
  |
 ::: .venv/<path-to-site-packages>/dependency.pyi:6:5
  |
6 |     async def values(self) -> AsyncIterator[int]: ...
  |     --------------------------------------------
  |     |         |
  |     |         `Base.values` defined here
  |     Without `yield` in the function body this function returns a coroutine
info: incompatible return types: `AsyncIterator[int]` is not assignable to `CoroutineType[Any, Any, AsyncIterator[int]]`
help: If an async generator was intended, report this stub to the library maintainers
info: This violates the Liskov Substitution Principle
```

### Coroutines without a known declaration

When a coroutine that returns an async iterator is stored in a variable, the diagnostic suggests
awaiting it. It does not suggest editing a stub without knowing which declaration to point to.

```py
from collections.abc import AsyncIterator

async def values() -> AsyncIterator[int]:
    yield 1

async def factory() -> AsyncIterator[int]:
    return values()

async def consume() -> None:
    coroutine = factory()
    # snapshot
    async for value in coroutine:
        pass
```

```snapshot
error[not-iterable]: Object of type `CoroutineType[Any, Any, AsyncIterator[int]]` is not async-iterable
  --> src/mdtest_snippet.py:12:24
   |
12 |     async for value in coroutine:
   |                        ^^^^^^^^^
info: It has no `__aiter__` method
help: `await` the coroutine before iterating over its result
```
