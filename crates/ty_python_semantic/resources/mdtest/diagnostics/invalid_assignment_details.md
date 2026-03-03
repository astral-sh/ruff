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
    target: SupportsName = source  # error: [invalid-assignment]
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
