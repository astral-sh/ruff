# Error context for diagnostics involving assignability checks

```toml
[environment]
python-version = "3.12"
```

A lot of ty's diagnostics are emitted as a direct result of a type-to-type assignability check
(`invalid-assignment`, `invalid-argument-type` or `invalid-method-override`). Types can be complex,
and so we can often help users understand the incompatibility by focusing on the relevant parts of
the two types that are being compared.

## Basic

Mainly for comparison: this is the most basic kind of `invalid-assignment` diagnostic:

```py
def _(source: str):
    target: bytes = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `str` is not assignable to `bytes`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: bytes = source  # snapshot
  |             -----   ^^^^^^ Incompatible value of type `str`
  |             |
  |             Declared type
  |
```

## Unions

Assigning a union to a non-union:

```py
def _(source: str | None):
    target: str = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `str | None` is not assignable to `str`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: str = source  # snapshot
  |             ---   ^^^^^^ Incompatible value of type `str | None`
  |             |
  |             Declared type
  |
info: element `None` of union `str | None` is not assignable to `str`
```

Assigning a non-union to a union:

```py
def _(source: int):
    target: str | None = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `int` is not assignable to `str | None`
 --> src/mdtest_snippet.py:4:13
  |
4 |     target: str | None = source  # snapshot
  |             ----------   ^^^^^^ Incompatible value of type `int`
  |             |
  |             Declared type
  |
```

Assigning a union to a union:

```py
def _(source: str | None):
    target: bytes | None = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `str | None` is not assignable to `bytes | None`
 --> src/mdtest_snippet.py:6:13
  |
6 |     target: bytes | None = source  # snapshot
  |             ------------   ^^^^^^ Incompatible value of type `str | None`
  |             |
  |             Declared type
  |
info: element `str` of union `str | None` is not assignable to `bytes | None`
```

## Intersections

Assigning an intersection to a non-intersection:

```py
from ty_extensions import Intersection

class P: ...
class Q: ...
class R: ...

def _(source: Intersection[P, Q]):
    target: int = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `P & Q` is not assignable to `int`
 --> src/mdtest_snippet.py:8:13
  |
8 |     target: int = source  # snapshot
  |             ---   ^^^^^^ Incompatible value of type `P & Q`
  |             |
  |             Declared type
  |
```

Assigning a non-intersection to an intersection:

```py
def _(source: P):
    target: Intersection[P, Q] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `P` is not assignable to `P & Q`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: Intersection[P, Q] = source  # snapshot
   |             ------------------   ^^^^^^ Incompatible value of type `P`
   |             |
   |             Declared type
   |
```

Assigning an intersection to an intersection:

```py
def _(source: Intersection[P, R]):
    target: Intersection[P, Q] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `P & R` is not assignable to `P & Q`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: Intersection[P, Q] = source  # snapshot
   |             ------------------   ^^^^^^ Incompatible value of type `P & R`
   |             |
   |             Declared type
   |
```

## Tuples

Wrong element types:

```py
def _(source: tuple[int, str, bool]):
    target: tuple[int, bytes, bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `tuple[int, str, bool]` is not assignable to `tuple[int, bytes, bool]`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: tuple[int, bytes, bool] = source  # snapshot
  |             -----------------------   ^^^^^^ Incompatible value of type `tuple[int, str, bool]`
  |             |
  |             Declared type
  |
info: the second tuple element is not compatible: `str` is not assignable to `bytes`
```

Wrong number of elements:

```py
def _(source: tuple[int, str]):
    target: tuple[int, str, bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `tuple[int, str]` is not assignable to `tuple[int, str, bool]`
 --> src/mdtest_snippet.py:4:13
  |
4 |     target: tuple[int, str, bool] = source  # snapshot
  |             ---------------------   ^^^^^^ Incompatible value of type `tuple[int, str]`
  |             |
  |             Declared type
  |
info: a tuple of length 2 is not assignable to a tuple of length 3
```

## `Callable`

Assigning a function to a `Callable`

```py
from typing import Any, Callable

def source(x: int, y: str) -> None:
    raise NotImplementedError

target: Callable[[int, bytes], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `def source(x: int, y: str) -> None` is not assignable to `(int, bytes, /) -> bool`
 --> src/mdtest_snippet.py:6:9
  |
6 | target: Callable[[int, bytes], bool] = source  # snapshot
  |         ----------------------------   ^^^^^^ Incompatible value of type `def source(x: int, y: str) -> None`
  |         |
  |         Declared type
  |
info: incompatible return types: `None` is not assignable to `bool`
```

Assigning a `Callable` to a `Callable` with wrong parameter type:

```py
def _(source: Callable[[int, str], bool]):
    target: Callable[[int, bytes], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `(int, str, /) -> bool` is not assignable to `(int, bytes, /) -> bool`
 --> src/mdtest_snippet.py:8:13
  |
8 |     target: Callable[[int, bytes], bool] = source  # snapshot
  |             ----------------------------   ^^^^^^ Incompatible value of type `(int, str, /) -> bool`
  |             |
  |             Declared type
  |
info: the second parameter has an incompatible type: `bytes` is not assignable to `str`
```

Assigning a `Callable` to a `Callable` with wrong return type:

```py
def _(source: Callable[[int, bytes], None]):
    target: Callable[[int, bytes], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `(int, bytes, /) -> None` is not assignable to `(int, bytes, /) -> bool`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: Callable[[int, bytes], bool] = source  # snapshot
   |             ----------------------------   ^^^^^^ Incompatible value of type `(int, bytes, /) -> None`
   |             |
   |             Declared type
   |
info: incompatible return types: `None` is not assignable to `bool`
```

Assigning a `Callable` to a `Callable` with wrong number of parameters:

```py
def _(source: Callable[[int, str], bool]):
    target: Callable[[int], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `(int, str, /) -> bool` is not assignable to `(int, /) -> bool`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: Callable[[int], bool] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `(int, str, /) -> bool`
   |             |
   |             Declared type
   |
```

Assigning a class to a `Callable`

```py
class Number:
    def __init__(self, value: int): ...

target: Callable[[str], Any] = Number  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `<class 'Number'>` is not assignable to `(str, /) -> Any`
  --> src/mdtest_snippet.py:16:9
   |
16 | target: Callable[[str], Any] = Number  # snapshot
   |         --------------------   ^^^^^^ Incompatible value of type `<class 'Number'>`
   |         |
   |         Declared type
   |
info: the first parameter has an incompatible type: `str` is not assignable to `int`
```

## Function assignability and overrides

Liskov checks use function-to-function assignability.

Wrong parameter type:

```py
class Parent:
    def method(self, x: str) -> bool:
        raise NotImplementedError

class Child1(Parent):
    # snapshot
    def method(self, x: bytes) -> bool:
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
 --> src/mdtest_snippet.py:7:9
  |
7 |     def method(self, x: bytes) -> bool:
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
  |
 ::: src/mdtest_snippet.py:2:9
  |
2 |     def method(self, x: str) -> bool:
  |         ---------------------------- `Parent.method` defined here
  |
info: parameter `x` has an incompatible type: `str` is not assignable to `bytes`
info: This violates the Liskov Substitution Principle
```

We call out the correct (target) parameter if they are listed in a different order:

```py
class ParentXY:
    def method(self, *, x: str, y: int) -> bool:
        raise NotImplementedError

class ChildYX(ParentXY):
    # snapshot
    def method(self, *, y: int, x: bytes) -> bool:
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.py:15:9
   |
15 |     def method(self, *, y: int, x: bytes) -> bool:
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `ParentXY.method`
   |
  ::: src/mdtest_snippet.py:10:9
   |
10 |     def method(self, *, x: str, y: int) -> bool:
   |         --------------------------------------- `ParentXY.method` defined here
   |
info: parameter `x` has an incompatible type: `str` is not assignable to `bytes`
info: This violates the Liskov Substitution Principle
```

Wrong return type:

```py
class Child2(Parent):
    # snapshot
    def method(self, x: str) -> None:
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.py:19:9
   |
19 |     def method(self, x: str) -> None:
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
   |
  ::: src/mdtest_snippet.py:2:9
   |
 2 |     def method(self, x: str) -> bool:
   |         ---------------------------- `Parent.method` defined here
   |
info: incompatible return types: `None` is not assignable to `bool`
info: This violates the Liskov Substitution Principle
```

Wrong non-positional-only parameter name:

```py
class Child3(Parent):
    # snapshot
    def method(self, y: str):
        raise NotImplementedError
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.py:23:9
   |
23 |     def method(self, y: str):
   |         ^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
   |
  ::: src/mdtest_snippet.py:2:9
   |
 2 |     def method(self, x: str) -> bool:
   |         ---------------------------- `Parent.method` defined here
   |
info: the parameter named `y` does not match `x` (and can be used as a keyword parameter)
info: This violates the Liskov Substitution Principle
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
    target: Other = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Person` is not assignable to `Other`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: Other = source  # snapshot
   |             -----   ^^^^^^ Incompatible value of type `Person`
   |             |
   |             Declared type
   |
```

Missing required fields:

```py
class PersonWithAge(TypedDict):
    name: str
    age: int

def _(source: Person):
    target: PersonWithAge = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Person` is not assignable to `PersonWithAge`
  --> src/mdtest_snippet.py:16:13
   |
16 |     target: PersonWithAge = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `Person`
   |             |
   |             Declared type
   |
```

Assigning a `TypedDict` to a `dict`

```py
class Person(TypedDict):
    name: str

def _(source: Person):
    target: dict[str, Any] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Person` is not assignable to `dict[str, Any]`
  --> src/mdtest_snippet.py:21:13
   |
21 |     target: dict[str, Any] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `Person`
   |             |
   |             Declared type
   |
info: TypedDict `Person` is not assignable to `dict`
help: A TypedDict is not usually assignable to any `dict[..]` type; `dict` types allow destructive operations like `clear()`.
help: Consider using `Mapping[..]` instead of `dict[..]`.
```

## Protocols

Missing protocol members:

```py
from typing import Protocol

class SupportsCheck(Protocol):
    def check(self, x: int, y: str) -> bool: ...

class DoesNotHaveCheck: ...

def _(source: DoesNotHaveCheck):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `DoesNotHaveCheck` is not assignable to `SupportsCheck`
 --> src/mdtest_snippet.py:9:13
  |
9 |     target: SupportsCheck = source  # snapshot
  |             -------------   ^^^^^^ Incompatible value of type `DoesNotHaveCheck`
  |             |
  |             Declared type
  |
info: type `DoesNotHaveCheck` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check` is not defined on type `DoesNotHaveCheck`
```

Incompatible types for protocol members:

```py
class CheckWithWrongSignature:
    def check(self, x: int, y: bytes) -> bool:
        return False

def _(source: CheckWithWrongSignature):
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `CheckWithWrongSignature` is not assignable to `SupportsCheck`
  --> src/mdtest_snippet.py:15:13
   |
15 |     target: SupportsCheck = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `CheckWithWrongSignature`
   |             |
   |             Declared type
   |
info: type `CheckWithWrongSignature` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check` is incompatible
info:     └── parameter `y` has an incompatible type: `str` is not assignable to `bytes`
```

Missing protocol properties:

```py
class SupportsName(Protocol):
    @property
    def name(self) -> str: ...

class DoesNotHaveName: ...

def _(source: DoesNotHaveName):
    target: SupportsName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `DoesNotHaveName` is not assignable to `SupportsName`
  --> src/mdtest_snippet.py:23:13
   |
23 |     target: SupportsName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `DoesNotHaveName`
   |             |
   |             Declared type
   |
info: type `DoesNotHaveName` is not assignable to protocol `SupportsName`
info: └── protocol member `name` is not defined on type `DoesNotHaveName`
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
    target: StringOrName = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasName` is not assignable to `StringOrName`
  --> src/mdtest_snippet.py:13:13
   |
13 |     target: StringOrName = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `HasName`
   |             |
   |             Declared type
   |
info: type `HasName` is not assignable to any element of the union `str | SupportsName`
info: ├── type `HasName` is not assignable to protocol `SupportsName`
info: │   └── protocol member `name` is incompatible
info: │       └── incompatible return types: `bytes` is not assignable to `str`
info: └── ... omitted 1 union element without additional context
```

## Deeply nested incompatibilities

```py
from typing import Callable

def source(x: tuple[int, str]) -> bool:
    return False

target: Callable[[tuple[int, bytes]], bool] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `def source(x: tuple[int, str]) -> bool` is not assignable to `(tuple[int, bytes], /) -> bool`
 --> src/mdtest_snippet.py:6:9
  |
6 | target: Callable[[tuple[int, bytes]], bool] = source  # snapshot
  |         -----------------------------------   ^^^^^^ Incompatible value of type `def source(x: tuple[int, str]) -> bool`
  |         |
  |         Declared type
  |
info: the first parameter has an incompatible type: `tuple[int, bytes]` is not assignable to `tuple[int, str]`
info: └── the second tuple element is not compatible: `bytes` is not assignable to `str`
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
    target: SupportsCheck = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `Incompatible` is not assignable to `SupportsCheck`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: SupportsCheck = source  # snapshot
   |             -------------   ^^^^^^ Incompatible value of type `Incompatible`
   |             |
   |             Declared type
   |
info: type `Incompatible` is not assignable to protocol `SupportsCheck`
info: └── protocol member `check1` is incompatible
info:     └── parameter `x` has an incompatible type: `str` is not assignable to `bytes`
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
    target: SupportsFoo | SupportsBar = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `HasNeither` is not assignable to `SupportsFoo | SupportsBar`
  --> src/mdtest_snippet.py:12:13
   |
12 |     target: SupportsFoo | SupportsBar = source  # snapshot
   |             -------------------------   ^^^^^^ Incompatible value of type `HasNeither`
   |             |
   |             Declared type
   |
info: type `HasNeither` is not assignable to any element of the union `SupportsFoo | SupportsBar`
info: ├── type `HasNeither` is not assignable to protocol `SupportsFoo`
info: │   └── protocol member `foo` is not defined on type `HasNeither`
info: └── type `HasNeither` is not assignable to protocol `SupportsBar`
info:     └── protocol member `bar` is not defined on type `HasNeither`
```

## Failures for many union elements

```py
def _(source: int):
    target: str | bytes | bool | None = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `int` is not assignable to `str | bytes | bool | None`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: str | bytes | bool | None = source  # snapshot
  |             -------------------------   ^^^^^^ Incompatible value of type `int`
  |             |
  |             Declared type
  |
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
    target: SupportsFoo = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `DoesNotSupportFoo1 & DoesNotSupportFoo2` is not assignable to `SupportsFoo`
  --> src/mdtest_snippet.py:11:13
   |
11 |     target: SupportsFoo = source  # snapshot
   |             -----------   ^^^^^^ Incompatible value of type `DoesNotSupportFoo1 & DoesNotSupportFoo2`
   |             |
   |             Declared type
   |
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
    target: SupportsFooAndBar = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `IncompatibleFoo` is not assignable to `SupportsFooAndBar`
  --> src/mdtest_snippet.py:16:13
   |
16 |     target: SupportsFooAndBar = source  # snapshot
   |             -----------------   ^^^^^^ Incompatible value of type `IncompatibleFoo`
   |             |
   |             Declared type
   |
info: type `IncompatibleFoo` is not assignable to protocol `SupportsFooAndBar`
info: └── protocol member `foo` is incompatible
info:     └── the parameter named `name_` does not match `name` (and can be used as a keyword parameter)
```

## Assigning to `Iterable`

```py
from collections.abc import Iterable

def _(source: list[str]):
    target: Iterable[bytes] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `list[str]` is not assignable to `Iterable[bytes]`
 --> src/mdtest_snippet.py:4:13
  |
4 |     target: Iterable[bytes] = source  # snapshot
  |             ---------------   ^^^^^^ Incompatible value of type `list[str]`
  |             |
  |             Declared type
  |
info: type `list[str]` is not assignable to protocol `Iterable[bytes]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `Iterator[str]` is not assignable to `Iterator[bytes]`
info:         └── protocol `Iterator[str]` is not assignable to protocol `Iterator[bytes]`
info:             └── incompatible return types: `str` is not assignable to `bytes`
```

## Invariant generic classes

We show a special diagnostic hint for invariant generic classes. For example, if you try to assign a
`list[bool]` to a `list[int]`:

```py
def _(source: list[bool]):
    target: list[int] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `list[bool]` is not assignable to `list[int]`
 --> src/mdtest_snippet.py:2:13
  |
2 |     target: list[int] = source  # snapshot
  |             ---------   ^^^^^^ Incompatible value of type `list[bool]`
  |             |
  |             Declared type
  |
info: `list` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Sequence`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```

We do the same for other invariant generic classes:

```py
from collections import ChainMap, Counter, OrderedDict, defaultdict, deque
from collections.abc import MutableSequence, MutableMapping, MutableSet

def _(source: set[bool]):
    target: set[int] = source  # snapshot

def _(source: dict[str, bool]):
    target: dict[str, int] = source  # snapshot

def _(source: dict[bool, str]):
    target: dict[int, str] = source  # snapshot

def _(source: dict[bool, bool]):
    target: dict[int, int] = source  # snapshot

def _(source: defaultdict[str, bool]):
    target: defaultdict[str, int] = source  # snapshot

def _(source: defaultdict[bool, str]):
    target: defaultdict[int, str] = source  # snapshot

def _(source: OrderedDict[str, bool]):
    target: OrderedDict[str, int] = source  # snapshot

def _(source: OrderedDict[bool, str]):
    target: OrderedDict[int, str] = source  # snapshot

def _(source: ChainMap[str, bool]):
    target: ChainMap[str, int] = source  # snapshot

def _(source: ChainMap[bool, str]):
    target: ChainMap[int, str] = source  # snapshot

def _(source: deque[bool]):
    target: deque[int] = source  # snapshot

def _(source: Counter[bool]):
    target: Counter[int] = source  # snapshot

def _(source: MutableSequence[bool]):
    target: MutableSequence[int] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `set[bool]` is not assignable to `set[int]`
 --> src/mdtest_snippet.py:7:13
  |
7 |     target: set[int] = source  # snapshot
  |             --------   ^^^^^^ Incompatible value of type `set[bool]`
  |             |
  |             Declared type
  |
info: `set` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Set`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `dict[str, bool]` is not assignable to `dict[str, int]`
  --> src/mdtest_snippet.py:10:13
   |
10 |     target: dict[str, int] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `dict[str, bool]`
   |             |
   |             Declared type
   |
info: `dict` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `dict[bool, str]` is not assignable to `dict[int, str]`
  --> src/mdtest_snippet.py:13:13
   |
13 |     target: dict[int, str] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `dict[bool, str]`
   |             |
   |             Declared type
   |
info: `dict` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `dict[bool, bool]` is not assignable to `dict[int, int]`
  --> src/mdtest_snippet.py:16:13
   |
16 |     target: dict[int, int] = source  # snapshot
   |             --------------   ^^^^^^ Incompatible value of type `dict[bool, bool]`
   |             |
   |             Declared type
   |
info: `dict` is invariant in its first and second type parameters
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `defaultdict[str, bool]` is not assignable to `defaultdict[str, int]`
  --> src/mdtest_snippet.py:19:13
   |
19 |     target: defaultdict[str, int] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `defaultdict[str, bool]`
   |             |
   |             Declared type
   |
info: `defaultdict` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `defaultdict[bool, str]` is not assignable to `defaultdict[int, str]`
  --> src/mdtest_snippet.py:22:13
   |
22 |     target: defaultdict[int, str] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `defaultdict[bool, str]`
   |             |
   |             Declared type
   |
info: `defaultdict` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `OrderedDict[str, bool]` is not assignable to `OrderedDict[str, int]`
  --> src/mdtest_snippet.py:25:13
   |
25 |     target: OrderedDict[str, int] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `OrderedDict[str, bool]`
   |             |
   |             Declared type
   |
info: `OrderedDict` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `OrderedDict[bool, str]` is not assignable to `OrderedDict[int, str]`
  --> src/mdtest_snippet.py:28:13
   |
28 |     target: OrderedDict[int, str] = source  # snapshot
   |             ---------------------   ^^^^^^ Incompatible value of type `OrderedDict[bool, str]`
   |             |
   |             Declared type
   |
info: `OrderedDict` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `ChainMap[str, bool]` is not assignable to `ChainMap[str, int]`
  --> src/mdtest_snippet.py:31:13
   |
31 |     target: ChainMap[str, int] = source  # snapshot
   |             ------------------   ^^^^^^ Incompatible value of type `ChainMap[str, bool]`
   |             |
   |             Declared type
   |
info: `ChainMap` is invariant in its second type parameter
info: Consider using the supertype `collections.abc.Mapping`, which is covariant in its value type
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `ChainMap[bool, str]` is not assignable to `ChainMap[int, str]`
  --> src/mdtest_snippet.py:34:13
   |
34 |     target: ChainMap[int, str] = source  # snapshot
   |             ------------------   ^^^^^^ Incompatible value of type `ChainMap[bool, str]`
   |             |
   |             Declared type
   |
info: `ChainMap` is invariant in its first type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `deque[bool]` is not assignable to `deque[int]`
  --> src/mdtest_snippet.py:37:13
   |
37 |     target: deque[int] = source  # snapshot
   |             ----------   ^^^^^^ Incompatible value of type `deque[bool]`
   |             |
   |             Declared type
   |
info: `deque` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Sequence`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `Counter[bool]` is not assignable to `Counter[int]`
  --> src/mdtest_snippet.py:40:13
   |
40 |     target: Counter[int] = source  # snapshot
   |             ------------   ^^^^^^ Incompatible value of type `Counter[bool]`
   |             |
   |             Declared type
   |
info: `Counter` is invariant in its type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics


error[invalid-assignment]: Object of type `MutableSequence[bool]` is not assignable to `MutableSequence[int]`
  --> src/mdtest_snippet.py:43:13
   |
43 |     target: MutableSequence[int] = source  # snapshot
   |             --------------------   ^^^^^^ Incompatible value of type `MutableSequence[bool]`
   |             |
   |             Declared type
   |
info: `MutableSequence` is invariant in its type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```

We also show this hint for custom invariant generic classes:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class MyContainer(Generic[T]):
    value: T

def _(source: MyContainer[bool]):
    target: MyContainer[int] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `MyContainer[bool]` is not assignable to `MyContainer[int]`
  --> src/mdtest_snippet.py:52:13
   |
52 |     target: MyContainer[int] = source  # snapshot
   |             ----------------   ^^^^^^ Incompatible value of type `MyContainer[bool]`
   |             |
   |             Declared type
   |
info: `MyContainer` is invariant in its type parameter
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```

We do *not* show this hint if the element types themselves wouldn't be assignable:

```py
def _(source: list[int]):
    target: list[str] = source  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `list[int]` is not assignable to `list[str]`
  --> src/mdtest_snippet.py:54:13
   |
54 |     target: list[str] = source  # snapshot
   |             ---------   ^^^^^^ Incompatible value of type `list[int]`
   |             |
   |             Declared type
   |
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
