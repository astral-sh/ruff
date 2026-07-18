# Unsatisfiable generic constraints

These are regressions for cases where constraint-set inference proved that a generic argument was
not assignable, but the hybrid solver discarded that failure and continued with the legacy
specialization.

```toml
[environment]
python-version = "3.13"
```

## Protocol inference rejects a non-matching overload

Regression test for [#4022](https://github.com/astral-sh/ty/issues/4022).

```py
from collections.abc import Iterable
from typing import TypeVar, assert_type, overload

T = TypeVar("T", bound=str)

@overload
def pick(x: Iterable[T]) -> T: ...
@overload
def pick(x: Iterable[int]) -> bool: ...
def pick(x: object) -> str | bool:
    raise NotImplementedError

assert_type(pick([1]), bool)
```

## Callable inference rejects a non-matching overload

Regression test for [#4024](https://github.com/astral-sh/ty/issues/4024).

```py
from collections.abc import Awaitable, Callable
from typing import Any, assert_type, overload

@overload
def make_awaitable[**P, T: Awaitable[Any]](f: Callable[P, T]) -> Callable[P, T]: ...
@overload
def make_awaitable[**P, T](f: Callable[P, T]) -> Callable[P, Awaitable[T]]: ...
def make_awaitable(f: Callable[..., Any]) -> Callable[..., Awaitable[Any]]:
    raise NotImplementedError

@make_awaitable
def f() -> str:
    return ""

async def check() -> None:
    assert_type(f(), Awaitable[str])
    await f()
```

## Comparison protocols reject incompatible union elements

Regression test for [#3857](https://github.com/astral-sh/ty/issues/3857).

```py
min([None, 2])  # error: [invalid-argument-type]
max([2, None])  # error: [invalid-argument-type]
sorted([None, 2])  # error: [invalid-argument-type]
```

## Inferred path-bound conflicts are not declared-bound failures

An inferred lower bound can satisfy the type variable's declared bound while conflicting with an
upper bound inferred from a contravariant occurrence.

```py
from collections.abc import Callable

class A: ...
class B: ...

def call[T: A](callback: Callable[[T], T], fallback: T) -> None:
    raise NotImplementedError

def callback(value: B) -> A:
    raise NotImplementedError

# error: [invalid-argument-type] "Argument to function `call` is incorrect: Expected `(A, /) -> A`, found `def callback(value: B) -> A`"
call(callback, A())
```

## Precise constraint errors suppress fallback mismatches

Once constraint inference reports the violated bound, the later assignability check should not
repeat the same incompatibility against its fallback specialization.

```py
from typing import Protocol

class A: ...
class B(A): ...
class C: ...

class P[T, U](Protocol):
    def put(self, value: T) -> None: ...
    def get(self) -> U: ...

class Bad:
    def put(self, value: B) -> None: ...
    def get(self) -> C:
        return C()

def f[T: A](x: P[T, T], fallback: T) -> None:
    raise NotImplementedError

# error: [invalid-argument-type] "Argument to function `f` is incorrect: Argument type `C` does not satisfy upper bound `A` of type variable `T`"
f(Bad(), B())
```
