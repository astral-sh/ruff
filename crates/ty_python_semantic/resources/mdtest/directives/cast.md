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
    # error: [redundant-cast] "Cast to `dict[str, Any]` can be replaced with a type annotation"
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

## Redundant casts in annotated assignments

```py
from typing import Any, Sequence, cast
```

When a `cast` call appears on the right-hand side of an annotated assignment, we check if the `cast`
is really necessary. In some situations, the type annotation already serves the same purpose.

For example, the following cast is redundant because `a_no_cast` has an inferred type of `int`
regardless:

```py
def _(x_any: Any, x_int: int, x_bool: bool, x_sequence_any: Sequence[Any]):
    # error: [redundant-cast] "Unnecessary cast to type `int` in annotated assignment"
    a_cast: int = cast(int, x_any)
    a_no_cast: int = x_any

    reveal_type(a_no_cast)  # revealed: int
```

The same is true for all of these:

```py
    b_cast: Any = cast(Any, x_int)  # error: [redundant-cast]
    b_no_cast: Any = x_int
    reveal_type(b_no_cast)  # revealed: Any

    c_cast: Sequence[Any] = cast(Sequence[Any], x_sequence_any)  # error: [redundant-cast]
    c_no_cast: Sequence[Any] = x_sequence_any
    reveal_type(c_no_cast)  # revealed: Sequence[Any]
```

On the other hand, this cast is *not* redundant, because it has observable effects on the inferred
type:

```py
    a_cast: int = cast(int, x_bool)  # not redundant!
    a_no_cast: int = x_bool

    reveal_type(a_cast)  # revealed: int
    reveal_type(a_no_cast)  # revealed: bool
```

Removing this downcast would lead to a type error, so it is certainly not redundant:

```py
    b_cast: bool = cast(bool, x_int)  # not redundant!
```

In the following situation, the cast is not redundant because it has observable effects on the
inferred type (`Any | str` and `int` are not mutually assignable, so we prefer the inferred type for
`c_no_cast` over the annotated type):

```py
    c_cast: Any | str = cast(Any | str, x_int)  # not redundant!
    c_no_cast: Any | str = x_int
    reveal_type(c_cast)  # revealed: Any | str
    reveal_type(c_no_cast)  # revealed: int
```

The same is true in the following example:

```py
    d_cast: int | None = cast(int | None, x_int)  # not redundant!
    d_no_cast: int | None = x_int
    reveal_type(d_cast)  # revealed: int | None
    reveal_type(d_no_cast)  # revealed: int
```

We only report these types of `redundant-cast` diagnostics for annotated assignments. The following
`cast` is technically also redundant, but there might be a good reason to keep it (e.g. to be
"notified" once the return type of that function changes):

```py
def returns_int(x_any: Any) -> int:
    return cast(int, x_any)  # technically redundant, but no diagnostic
```

## Redundant casts in plain assignments

A cast like `x = cast(int, expr)` where `expr: Any` can be written more idiomatically as
`x: int = expr`. We report a `[redundant-cast]` diagnostic for this case too:

```py
from typing import Any, Sequence, cast

def _(x_any: Any, x_int: int, x_sequence_any: Sequence[Any]):
    # error: [redundant-cast] "Cast to `int` can be replaced with a type annotation"
    a = cast(int, x_any)

    # error: [redundant-cast] "Cast to `Any` can be replaced with a type annotation"
    b = cast(Any, x_int)

    # error: [redundant-cast]
    c = cast(Sequence[Any], x_sequence_any)
```

As with annotated assignments, the diagnostic is not emitted when the cast has observable effects on
the inferred type. The cast below converts `bool` to `int`, which is a narrowing that cannot be
expressed just by annotating the variable as `int`:

```py
from typing import cast

def _(x_bool: bool):
    a = cast(int, x_bool)  # not redundant: annotating `a: int = x_bool` would infer `bool`
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
```
