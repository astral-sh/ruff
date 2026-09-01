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
