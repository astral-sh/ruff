# Invalid assignment diagnostics

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

This file contains various scenarios of `invalid-assignment` (and related) diagnostics where we
(attempt to) do better than just report "type X is not assignable to type Y".

## Basic

Mainly for comparison: this is the most basic kind of `invalid-assignment` diagnostic:

```py
def _(source: str):
    target: bytes = source  # error: [invalid-assignment]
```

## Unions

Assigning a union to a non-union:

```py
def _(source: str | None):
    target: str = source  # error: [invalid-assignment]
```

Assigning a non-union to a union:

```py
def _(source: int):
    target: str | None = source  # error: [invalid-assignment]
```

Assigning a union to a union:

```py
def _(source: str | None):
    target: bytes | None = source  # error: [invalid-assignment]
```

## Intersections

Assigning an intersection to a non-intersection:

```py
from ty_extensions import Intersection

class P: ...
class Q: ...
class R: ...

def _(source: Intersection[P, Q]):
    target: int = source  # error: [invalid-assignment]
```

Assigning a non-intersection to an intersection:

```py
def _(source: P):
    target: Intersection[P, Q] = source  # error: [invalid-assignment]
```

Assigning an intersection to an intersection:

```py
def _(source: Intersection[P, R]):
    target: Intersection[P, Q] = source  # error: [invalid-assignment]
```

## Tuples

Wrong element types:

```py
def _(source: tuple[int, str, bool]):
    target: tuple[int, bytes, bool] = source  # error: [invalid-assignment]
```

Wrong number of elements:

```py
def _(source: tuple[int, str]):
    target: tuple[int, str, bool] = source  # error: [invalid-assignment]
```

## `Callable`

Assigning a function to a `Callable`

```py
from typing import Any, Callable

def source(x: int, y: str) -> None:
    raise NotImplementedError

target: Callable[[int, bytes], bool] = source  # error: [invalid-assignment]
```

Assigning a `Callable` to a `Callable` with wrong parameter type:

```py
def _(source: Callable[[int, str], bool]):
    target: Callable[[int, bytes], bool] = source  # error: [invalid-assignment]
```

Assigning a `Callable` to a `Callable` with wrong return type:

```py
def _(source: Callable[[int, bytes], None]):
    target: Callable[[int, bytes], bool] = source  # error: [invalid-assignment]
```

Assigning a `Callable` to a `Callable` with wrong number of parameters:

```py
def _(source: Callable[[int, str], bool]):
    target: Callable[[int], bool] = source  # error: [invalid-assignment]
```

Assigning a class to a `Callable`

```py
class Number:
    def __init__(self, value: int): ...

target: Callable[[str], Any] = Number  # error: [invalid-assignment]
```

## Function assignability and overrides

Liskov checks use function-to-function assignability.

Wrong parameter type:

```py
class Parent:
    def method(self, x: str) -> bool:
        raise NotImplementedError

class Child1(Parent):
    # error: [invalid-method-override]
    def method(self, x: bytes) -> bool:
        raise NotImplementedError
```

Wrong return type:

```py
class Child2(Parent):
    # error: [invalid-method-override]
    def method(self, x: str) -> None:
        raise NotImplementedError
```

Wrong non-positional-only parameter name:

```py
class Child3(Parent):
    # error: [invalid-method-override]
    def method(self, y: str):
        raise NotImplementedError
```

## `TypedDict`

Incompatible field types:

```py
from typing import Any, TypedDict

class Person(TypedDict):
    name: str

class Other(TypedDict):
    name: bytes

def _(source: Person):
    target: Other = source  # error: [invalid-assignment]
```

Missing required fields:

```py
class PersonWithAge(TypedDict):
    name: str
    age: int

def _(source: Person):
    target: PersonWithAge = source  # error: [invalid-assignment]
```

Assigning a `TypedDict` to a `dict`

```py
class Person(TypedDict):
    name: str

def _(source: Person):
    target: dict[str, Any] = source  # error: [invalid-assignment]
```

## Protocols

Missing protocol members:

```py
from typing import Protocol

class SupportsCheck(Protocol):
    def check(self, x: int, y: str) -> bool: ...

class DoesNotHaveCheck: ...

def _(source: DoesNotHaveCheck):
    target: SupportsCheck = source  # error: [invalid-assignment]
```

Incompatible types for protocol members:

```py
class CheckWithWrongSignature:
    def check(self, x: int, y: bytes) -> bool:
        return False

def _(source: CheckWithWrongSignature):
    target: SupportsCheck = source  # error: [invalid-assignment]
```

Missing protocol properties:

```py
class SupportsName(Protocol):
    @property
    def name(self) -> str: ...

class DoesNotHaveName: ...

def _(source: DoesNotHaveName):
    target: SupportsName = source  # error: [invalid-assignment]
```

## Type aliases

Type aliases should be expanded in diagnostics to understand the underlying incompatibilities:

```py
from typing import Protocol

class SupportsName(Protocol):
    def name(self) -> str: ...

class HasName:
    def name(self) -> bytes:
        return b""

type StringOrName = str | SupportsName

def _(source: HasName):
    target: StringOrName = source  # error: [invalid-assignment]
```

## Deeply nested incompatibilities

```py
from typing import Callable

def source(x: tuple[int, str]) -> bool:
    return False

target: Callable[[tuple[int, bytes]], bool] = source  # error: [invalid-assignment]
```

## Multiple nested incompatibilities

```py
from typing import Protocol

class SupportsCheck(Protocol):
    def check1(self, x: str): ...
    def check2(self, x: int) -> bool: ...

class Incompatible:
    def check1(self, x: bytes): ...
    def check2(self, x: int) -> None: ...

def _(source: Incompatible):
    target: SupportsCheck = source  # error: [invalid-assignment]
```

## Failures for multiple union elements

```py
from typing import Protocol

class SupportsFoo(Protocol):
    def foo(self, x: int) -> bool: ...

class SupportsBar(Protocol):
    def bar(self, x: str) -> bool: ...

class HasNeither: ...

def _(source: HasNeither):
    target: SupportsFoo | SupportsBar = source  # error: [invalid-assignment]
```

## Failures for many union elements

```py
def _(source: int):
    target: str | bytes | bool | None = source  # error: [invalid-assignment]
```

## Failures for multiple intersection elements

```py
from ty_extensions import Intersection
from typing import Protocol

class SupportsFoo(Protocol):
    def foo(self, x: int) -> bool: ...

class DoesNotSupportFoo1: ...
class DoesNotSupportFoo2: ...

def _(source: Intersection[DoesNotSupportFoo1, DoesNotSupportFoo2]):
    target: SupportsFoo = source  # error: [invalid-assignment]
```

## Assigning an overload set

This test makes sure that error context from failed overload candidates does not leak
(`IncompatibleFoo.bar` is assignable to `SupportsFooAndBar.bar`):

```py
from typing import Protocol, overload, SupportsIndex

class SupportsFooAndBar(Protocol):
    def foo(self, name: str): ...
    def bar(self, x: bytes): ...

class IncompatibleFoo:
    def foo(self, name_: str): ...
    @overload
    def bar(self, x: SupportsIndex): ...
    @overload
    def bar(self, x: bytes): ...
    def bar(self, x: SupportsIndex | bytes): ...

def _(source: IncompatibleFoo):
    target: SupportsFooAndBar = source  # error: [invalid-assignment]
```

## Assigning to `Iterable`

```py
from collections.abc import Iterable

def _(source: list[str]):
    target: Iterable[bytes] = source  # error: [invalid-assignment]
```

## Deleting a read-only property

```py
class C:
    @property
    def attr(self) -> int:
        return 1

c = C()
del c.attr  # error: [invalid-assignment]
```

## Invariant generic classes

We show a special diagnostic hint for invariant generic classes. For example, if you try to assign a
`list[bool]` to a `list[int]`:

```py
def _(source: list[bool]):
    target: list[int] = source  # error: [invalid-assignment]
```

We do the same for other invariant generic classes:

```py
from collections import ChainMap, Counter, OrderedDict, defaultdict, deque
from collections.abc import MutableSequence, MutableMapping, MutableSet

def _(source: set[bool]):
    target: set[int] = source  # error: [invalid-assignment]

def _(source: dict[str, bool]):
    target: dict[str, int] = source  # error: [invalid-assignment]

def _(source: dict[bool, str]):
    target: dict[int, str] = source  # error: [invalid-assignment]

def _(source: dict[bool, bool]):
    target: dict[int, int] = source  # error: [invalid-assignment]

def _(source: defaultdict[str, bool]):
    target: defaultdict[str, int] = source  # error: [invalid-assignment]

def _(source: defaultdict[bool, str]):
    target: defaultdict[int, str] = source  # error: [invalid-assignment]

def _(source: OrderedDict[str, bool]):
    target: OrderedDict[str, int] = source  # error: [invalid-assignment]

def _(source: OrderedDict[bool, str]):
    target: OrderedDict[int, str] = source  # error: [invalid-assignment]

def _(source: ChainMap[str, bool]):
    target: ChainMap[str, int] = source  # error: [invalid-assignment]

def _(source: ChainMap[bool, str]):
    target: ChainMap[int, str] = source  # error: [invalid-assignment]

def _(source: deque[bool]):
    target: deque[int] = source  # error: [invalid-assignment]

def _(source: Counter[bool]):
    target: Counter[int] = source  # error: [invalid-assignment]

def _(source: MutableSequence[bool]):
    target: MutableSequence[int] = source  # error: [invalid-assignment]
```

We also show this hint for custom invariant generic classes:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class MyContainer(Generic[T]):
    value: T

def _(source: MyContainer[bool]):
    target: MyContainer[int] = source  # error: [invalid-assignment]
```

We do *not* show this hint if the element types themselves wouldn't be assignable:

```py
def _(source: list[int]):
    target: list[str] = source  # error: [invalid-assignment]
```

We do not emit any error if the collection types are covariant:

```py
from collections.abc import Sequence

def _(source: list[bool]):
    target: Sequence[int] = source

def _(source: frozenset[bool]):
    target: frozenset[int] = source

def _(source: tuple[bool, bool]):
    target: tuple[int, int] = source
```
