# `cast`

## Behavior

`cast()` takes two arguments, one type and one value, and returns a value of the given type.

The (inferred) type of the value and the given type do not need to have any correlation.

```py
from typing import Literal, cast, Any

reveal_type(True)  # revealed: Literal[True]
reveal_type(cast(str, True))  # revealed: str
reveal_type(cast("str", True))  # revealed: str

reveal_type(cast(int | str, 1))  # revealed: int | str

reveal_type(cast(val="foo", typ=int))  # revealed: int

# error: [invalid-type-form]
reveal_type(cast(Literal, True))  # revealed: Unknown

# error: [invalid-type-form]
reveal_type(cast(1, True))  # revealed: Unknown

# error: [missing-argument] "No argument provided for required parameter `val` of function `cast`"
cast(str)
# error: [too-many-positional-arguments] "Too many positional arguments to function `cast`: expected 2, got 3"
cast(str, b"ar", "foo")

def function_returning_int() -> int:
    return 10

# error: [redundant-cast] "Value is already of type `int`"
cast(int, function_returning_int())

def function_returning_any() -> Any:
    return "blah"

# error: [redundant-cast] "Value is already of type `Any`"
cast(Any, function_returning_any())
```

Complex type expressions (which may be unsupported) do not lead to spurious `[redundant-cast]`
diagnostics.

```py
from typing import Callable

def f(x: Callable[[dict[str, int]], None], y: tuple[dict[str, int]]):
    a = cast(Callable[[list[bytes]], None], x)
    b = cast(tuple[list[bytes]], y)
```

A cast from `Todo` or `Unknown` to `Any` is not considered a "redundant cast": even if these are
understood as gradually equivalent types by ty, they are understood as different types by human
readers of ty's output. For `Unknown` in particular, we may consider it differently in the context
of some opt-in diagnostics, as it indicates that the gradual type has come about due to an invalid
annotation, missing annotation or missing type argument somewhere.

A cast from `Unknown` to `Todo` or `Any` is also not considered a "redundant cast", as this breaks
the gradual guarantee and leads to cascading errors when an object is inferred as having type
`Unknown` due to a missing import or similar.

```py
from ty_extensions import Unknown

def f(x: Any, y: Unknown, z: Any | str | int):
    a = cast(dict[str, Any], x)
    reveal_type(a)  # revealed: dict[str, Any]

    b = cast(Any, y)
    reveal_type(b)  # revealed: Any

    c = cast(Unknown, y)
    reveal_type(c)  # revealed: Unknown

    d = cast(Unknown, x)
    reveal_type(d)  # revealed: Unknown

    e = cast(str | int | Any, z)  # error: [redundant-cast]
```

The unknown check follows lazy wrappers such as type aliases and protocol interfaces. If a recursive
protocol is encountered under a different specialization, the check is indeterminate and
conservatively suppresses the diagnostic.

```py
from typing import Any, Protocol, TypeVar, cast

from ty_extensions import Unknown

type Alias = Unknown

def alias_unknown(value: Alias) -> None:
    cast(Alias, value)

class HasUnknown(Protocol):
    value: Unknown

def protocol_member_unknown(value: HasUnknown) -> None:
    cast(HasUnknown, value)

class HasInt(Protocol):
    value: int

def protocol_member_known(value: HasInt) -> None:
    cast(HasInt, value)  # error: [redundant-cast]

class HasAny(Protocol):
    value: Any

def distinct_protocols(value: HasAny) -> None:
    reveal_type(value.value)  # revealed: Any
    reveal_type(cast(HasUnknown, value).value)  # revealed: Unknown

class GenericProtocol[T](Protocol):
    value: T

def protocol_specialization_unknown(value: GenericProtocol[Unknown]) -> None:
    cast(GenericProtocol[Unknown], value)

DynamicBoundT = TypeVar("DynamicBoundT", bound=Unknown)

class DynamicBoundProtocol(Protocol[DynamicBoundT]):
    value: DynamicBoundT

def protocol_specialization_dynamic_bound(value: DynamicBoundProtocol[int]) -> None:
    cast(DynamicBoundProtocol[int], value)  # error: [redundant-cast]

class RecursiveProtocol(Protocol):
    next: "RecursiveProtocol"

def exactly_recursive_protocol(value: RecursiveProtocol) -> None:
    cast(RecursiveProtocol, value)  # error: [redundant-cast]

class ExactGenericProtocol[T](Protocol):
    next: "ExactGenericProtocol[T]"

def exactly_recursive_generic_protocol(value: ExactGenericProtocol[int]) -> None:
    cast(ExactGenericProtocol[int], value)  # error: [redundant-cast]

class GrowingProtocol[T](Protocol):
    value: T
    next: "GrowingProtocol[list[T]]"

def recursively_specialized_protocol(value: GrowingProtocol[int]) -> None:
    cast(GrowingProtocol[int], value)
```

The interface of a specialized protocol is not necessarily a simple substitution of its type
arguments. Descriptor overload resolution can expose `Unknown` only after recursive specialization,
so an indeterminate result must suppress the diagnostic:

```py
from typing import Callable, Protocol, cast, overload

from ty_extensions import Unknown

class Descriptor:
    @overload
    def __get__(
        self,
        instance: "DescriptorProtocol[list[int]]",
        owner: type["DescriptorProtocol[list[int]]"],
    ) -> Unknown: ...
    @overload
    def __get__(self, instance: object, owner: type[object]) -> int: ...
    def __get__(self, instance: object, owner: type[object]) -> object:
        return object()

def descriptor(function: Callable[..., object]) -> Descriptor:
    return Descriptor()

class DescriptorProtocol[T](Protocol):
    marker: T
    next: "DescriptorProtocol[list[T]]"

    @descriptor
    def value(self) -> object: ...

def descriptor_specialization(value: DescriptorProtocol[int]) -> None:
    reveal_type(value.value)  # revealed: int
    reveal_type(value.next.value)  # revealed: Unknown
    cast(DescriptorProtocol[int], value)
```

Recursive aliases that fall back to `Divergent` should not trigger `redundant-cast`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import cast

RecursiveAlias = list["RecursiveAlias | None"]

def f(x: RecursiveAlias):
    cast(RecursiveAlias, x)
```

## Diagnostic snapshots

```py
import secrets
from typing import cast

# snapshot: redundant-cast
cast(int, secrets.randbelow(10))
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:5:1
  |
5 | cast(int, secrets.randbelow(10))
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the redundant `cast`
  |
4 | # snapshot: redundant-cast
  - cast(int, secrets.randbelow(10))
5 + secrets.randbelow(10)
6 | # snapshot: redundant-cast
  |
```

```py
# snapshot: redundant-cast
cast(val=secrets.randbelow(10), typ=int)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:7:1
  |
7 | cast(val=secrets.randbelow(10), typ=int)
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the redundant `cast`
  |
6 | # snapshot: redundant-cast
  - cast(val=secrets.randbelow(10), typ=int)
7 + secrets.randbelow(10)
8 | def f(x: int, y: int, z: int) -> int:
  |
```

```py
def f(x: int, y: int, z: int) -> int:
    # snapshot: redundant-cast
    return cast(int, x + y) * z
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:10:12
   |
10 |     return cast(int, x + y) * z
   |            ^^^^^^^^^^^^^^^^
   |
help: Remove the redundant `cast`
   |
9  |     # snapshot: redundant-cast
   -     return cast(int, x + y) * z
10 +     return (x + y) * z
11 | def g(x: int, y: int) -> int:
   |
```

```py
def g(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return -cast(int, x + y)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:13:13
   |
13 |     return -cast(int, x + y)
   |             ^^^^^^^^^^^^^^^^
   |
help: Remove the redundant `cast`
   |
12 |     # snapshot: redundant-cast
   -     return -cast(int, x + y)
13 +     return -(x + y)
14 | def h(x: int, y: int) -> None:
   |
```

```py
def h(x: int, y: int) -> None:
    # snapshot: redundant-cast
    print(cast(int, x + y))
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:16:11
   |
16 |     print(cast(int, x + y))
   |           ^^^^^^^^^^^^^^^^
   |
help: Remove the redundant `cast`
   |
15 |     # snapshot: redundant-cast
   -     print(cast(int, x + y))
16 +     print(x + y)
   |
```
