# `cast`

## Behavior

```toml
[environment]
python-version = "3.12"

[rules]
# Disabled by default in production, but enabled by default in mdtests.
# Tests for this rule are lower down in the file; for this section, we disable the rule.
disjoint-cast = "ignore"
```

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
from ty_extensions._internal import Unknown

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

Recursive aliases that fall back to `Divergent` should not trigger `redundant-cast`.

```py
from typing import cast

RecursiveAlias = list["RecursiveAlias | None"]

def f(x: RecursiveAlias):
    cast(RecursiveAlias, x)
```

## Redundant casts of tuple classes with unknown elements

A tuple class with an `Unknown` element is not fully static, even when its other element is `object`
and their union simplifies to `object`. A cast involving that tuple class must not be reported as
redundant.

```py
from typing import cast
from ty_extensions._internal import Unknown

def cast_gradual_tuple_class(value: type[tuple[object, Unknown]]) -> None:
    cast(type[tuple[object, Unknown]], value)
```

## Disjoint casts

### Basics

Casting between disjoint types often indicates a mistake in the user's code. When enabled,
`disjoint-cast` reports casts whose source and destination types have no overlap.

```py
from typing import cast
from typing_extensions import cast as extension_cast

def incompatible_casts(integer: int, string: str) -> None:
    # error: [disjoint-cast] "Cannot cast from `int` to `str`: the types are disjoint"
    cast(str, integer)

    # error: [disjoint-cast] "Cannot cast from `str` to `int`: the types are disjoint"
    cast(int, string)

    # error: [disjoint-cast] "Cannot cast from `int` to `str`: the types are disjoint"
    cast(val=integer, typ=str)

    # error: [disjoint-cast] "Cannot cast from `int` to `str`: the types are disjoint"
    extension_cast(str, integer)
```

### Disjoint casts involving literals and unions

Literal types and unions are rejected only when none of their possible values overlaps with the
destination type.

```py
from typing import Literal, cast

# error: [disjoint-cast] "Cannot cast from `Literal[1]` to `str`: the types are disjoint"
cast(str, 1)

# error: [disjoint-cast] "Cannot cast from `Literal["left"]` to `Literal["right"]`: the types are disjoint"
cast(Literal["right"], "left")

def cast_union(value: int | str) -> None:
    # error: [disjoint-cast] "Cannot cast from `int | str` to `bytes`: the types are disjoint"
    cast(bytes, value)

    cast(str, value)
    cast(int | bytes, value)
```

### Disjoint casts involving generic types

Incompatible generic specializations are rejected, while gradual types remain valid because they may
overlap with the destination type.

```py
from typing import Any, cast

def cast_generic(integers: list[int], dynamic_values: list[Any], dynamic: Any) -> None:
    # error: [disjoint-cast] "Cannot cast from `list[int]` to `list[str]`: the types are disjoint"
    cast(list[str], integers)

    cast(list[str], dynamic_values)
    cast(str, dynamic)
    cast(Any, integers)
```

### Disjoint casts between identically named types

Disjoint types with the same display name are qualified so the diagnostic identifies which type
comes from each module.

```py
from typing import cast

import first
import second

def cast_identically_named(value: first.Value) -> None:
    # error: [disjoint-cast] "Cannot cast from `first.Value` to `second.Value`: the types are disjoint"
    cast(second.Value, value)
```

`first.py`:

```py
from typing import final

@final
class Value:
    pass
```

`second.py`:

```py
from typing import final

@final
class Value:
    pass
```

### Casts to `Never`

`Never` is disjoint from every type, but excluded from `disjoint-cast`. It is assumed that the user
knows what they're doing if they cast to `Never` explicitly:

```py
from typing_extensions import Never, cast

x = cast(Never, 0)  # no diagnostic
```

Upcasts from a `Never`-inferred type to a supertype are also permitted without the rule being
triggered:

```py
from typing_extensions import Never, cast

def test(x: Never):
    y = cast(str, x)  # no diagnostic
```

### Casts in stub files

`disjoint-cast` is not applied to stub files:

```pyi
from typing import cast

x = cast(int, ...)  # no diagnostic
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
help: Remove the redundant `cast`
   |
15 |     # snapshot: redundant-cast
   -     print(cast(int, x + y))
16 +     print(x + y)
   |
```

## Fixes for multiline conditional expressions

Removing a redundant cast preserves the parentheses that allow its argument to span multiple lines.

```py
from typing import cast

# fmt: off
def choose(x: int, y: int, flag: bool) -> int:
    # snapshot: redundant-cast
    return cast(int, (x if flag
                     else y))
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:6:12
  |
6 |       return cast(int, (x if flag
  |  ____________^
7 | |                      else y))
  | |_____________________________^
help: Remove the redundant `cast`
  |
5 |     # snapshot: redundant-cast
  -     return cast(int, (x if flag
  -                      else y))
6 +     return (x if flag
7 +                      else y)
  |
```

## Fixes for multiline arithmetic expressions

An argument can rely on the call's parentheses for line continuation without having parentheses of
its own. Removing the call adds parentheses to keep the arithmetic expression on one logical line.

```py
from typing import cast

# fmt: off
def add(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return cast(int, x +
                    y)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
 --> src/mdtest_snippet.py:6:12
  |
6 |       return cast(int, x +
  |  ____________^
7 | |                     y)
  | |______________________^
help: Remove the redundant `cast`
  |
5 |     # snapshot: redundant-cast
  -     return cast(int, x +
6 +     return (x +
7 |                     y)
  |
```

A line break before an operator also needs parentheses. Without them, the following fix would
produce valid syntax but return only `x`, leaving `+ y` as an unreachable statement.

```py
# fmt: off
def add_with_leading_operator(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return cast(int, x
    + y)
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:11:12
   |
11 |       return cast(int, x
   |  ____________^
12 | |     + y)
   | |________^
help: Remove the redundant `cast`
   |
10 |     # snapshot: redundant-cast
   -     return cast(int, x
11 +     return (x
12 |     + y)
   |
```

## Fixes preserve comments in parenthesized arguments

The fix retains comments inside an argument's parentheses, including when the value is passed by
keyword before the type argument.

```py
from typing import cast

def add(x: int, y: int) -> int:
    # snapshot: redundant-cast
    return cast(
        val=(
            # Leading comment.
            x + y  # Trailing comment.
        ),
        typ=int,
    )
```

```snapshot
warning[redundant-cast]: Value is already of type `int`
  --> src/mdtest_snippet.py:5:12
   |
 5 |       return cast(
   |  ____________^
 6 | |         val=(
 7 | |             # Leading comment.
 8 | |             x + y  # Trailing comment.
 9 | |         ),
10 | |         typ=int,
11 | |     )
   | |_____^
help: Remove the redundant `cast`
  |
4 |     # snapshot: redundant-cast
  -     return cast(
  -         val=(
5 +     return (
6 |             # Leading comment.
7 |             x + y  # Trailing comment.
  -         ),
  -         typ=int,
  -     )
8 +         )
  |
```
