# Narrowing for `len(..)` checks

When `len(x)` is used in a boolean context, we can narrow the type of `x` based on whether `len(x)`
is truthy (non-zero) or falsy (zero).

We apply `~AlwaysFalsy` narrowing when ANY part of the type is narrowable (string/bytes literals,
`LiteralString`, tuples). This removes types that are always falsy (like `Literal[""]`) while
leaving non-narrowable types (like `str`, `list`) unchanged.

## String literals

The intersection with `~AlwaysFalsy` simplifies to just the non-empty literal.

```py
from typing import Literal

def _(x: Literal["foo", ""]):
    if len(x):
        reveal_type(x)  # revealed: Literal["foo"]
    else:
        reveal_type(x)  # revealed: Literal[""]
```

## Bytes literals

```py
from typing import Literal

def _(x: Literal[b"foo", b""]):
    if len(x):
        reveal_type(x)  # revealed: Literal[b"foo"]
    else:
        reveal_type(x)  # revealed: Literal[b""]
```

## LiteralString

```toml
[environment]
python-version = "3.11"
```

```py
from typing import LiteralString

def _(x: LiteralString):
    if len(x):
        reveal_type(x)  # revealed: LiteralString & ~Literal[""]
    else:
        reveal_type(x)  # revealed: Literal[""]
```

## Tuples

Ideally we'd narrow these types further, e.g. to `tuple[int, ...] & ~tuple[()]` in the positive case
and `tuple[()]` in the negative case (see <https://github.com/astral-sh/ty/issues/560>).

```py
def _(x: tuple[int, ...]):
    if len(x):
        reveal_type(x)  # revealed: tuple[int, ...] & ~AlwaysFalsy
    else:
        reveal_type(x)  # revealed: tuple[int, ...] & ~AlwaysTruthy
```

## Unions of narrowable types

```py
from typing import Literal

def _(x: Literal["foo", ""] | tuple[int, ...]):
    if len(x):
        reveal_type(x)  # revealed: Literal["foo"] | (tuple[int, ...] & ~AlwaysFalsy)
    else:
        reveal_type(x)  # revealed: Literal[""] | (tuple[int, ...] & ~AlwaysTruthy)
```

## Types that are not narrowed

For `str`, `list`, and other types where a subclass could have a `__bool__` that disagrees with
`__len__`, we do not narrow:

```py
def not_narrowed_str(x: str):
    if len(x):
        # No narrowing because `str` could be subclassed with a custom `__bool__`
        reveal_type(x)  # revealed: str

def not_narrowed_list(x: list[int]):
    if len(x):
        # No narrowing because `list` could be subclassed with a custom `__bool__`
        reveal_type(x)  # revealed: list[int]
```

## Mixed unions (narrowable and non-narrowable)

When a union contains both narrowable and non-narrowable types, we narrow the narrowable parts while
leaving the non-narrowable parts unchanged:

```py
from typing import Literal

def _(x: Literal["foo", ""] | list[int]):
    if len(x):
        # `Literal[""]` is removed, `list[int]` is unchanged
        reveal_type(x)  # revealed: Literal["foo"] | list[int]
    else:
        reveal_type(x)  # revealed: Literal[""] | list[int]
```

## Narrowing away empty literals

This pattern is common when a prior truthiness check narrows a type, and then a conditional
expression adds an empty literal back:

```py
def _(lines: list[str]):
    for line in lines:
        if not line:
            continue

        reveal_type(line)  # revealed: str & ~AlwaysFalsy
        value = line if len(line) < 3 else ""
        reveal_type(value)  # revealed: (str & ~AlwaysFalsy) | Literal[""]

        if len(value):
            # `Literal[""]` is removed, `str & ~AlwaysFalsy` is unchanged
            reveal_type(value)  # revealed: str & ~AlwaysFalsy
            # Accessing value[0] is safe here
            _ = value[0]
```
