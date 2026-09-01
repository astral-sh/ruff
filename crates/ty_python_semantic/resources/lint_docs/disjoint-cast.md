## What it does

Detects `cast` calls where the inferred type of the value is disjoint from the destination type.

Two types are disjoint if they are entirely non-overlapping. For example, `str` and `int` are
disjoint types because it is impossible to create a Python object that is both a `str` and an `int`
at the same time: Python forbids multiple inheritance between these two classes:

```pycon
>>> class StrAndInt(int, str): ...
Traceback (most recent call last):
  File "<python-input-0>", line 1, in <module>
    class StrAndInt(int, str): ...
TypeError: multiple bases have instance lay-out conflict
```

This means that any object of type `int` can never also be of type `str`, and any object of type
`str` can never also inhabit the type `int`. The only common subtype of these two types is
[`Never`][never], the uninhabited type, which has no members.

## Why is this bad?

`cast()` is deliberately designed as an "escape hatch" in the type system that is neither validated
at runtime nor, by default, by type checkers. While safe uses of `cast()` are possible -- for
example, upcasting from a subtype to a supertype can be sound -- `cast()` is deliberately designed
to allow unsound narrowing, and most useful applications of `cast()` in real-world code are unsound.

Nonetheless, while allowing for the fact that `cast()` is intentionally designed to allow
unsoundness, casting a value to an entirely *disjoint* type is especially likely to indicate a
mistake in your code. Casting from an `int` to a `str`, for example, is extremely suspicious. ty
provides no mechanism by which you could ever narrow a type from `int` to `str` without using escape
hatches such as `cast` or `TypeGuard` that allow for unsound uses.

This rule therefore provides a means for codebases to partially validate their uses of `cast()`
without banning the API entirely.

## Example

```py
from typing import cast


def parse(value: int) -> str:
    return cast(str, value)  # error: [disjoint-cast]
```

Casts between overlapping (non-disjoint) types are allowed:

```py
from typing import cast


def parse(value: int | str) -> str:
    return cast(str, value)
```

Note that disjointness between types can sometimes be surprising. For example, `list[int]` is
disjoint from `list[bool]` even though `bool` is a subtype of `int`. Due to the fact that `list` is
[mutable and invariant], it would be deeply unsound for ty to ever narrow an object of type
`list[int]` to the type `list[bool]`. As such, ty will complain about a cast from `list[int]` to
`list[bool]` when this rule is enabled.

Similarly, two `NewType`s can be disjoint even when they share the same underlying nominal base
type, unless one `NewType` is explicitly declared as a sub-newtype of the other.

```py
from typing import NewType, cast


UserId = NewType("UserId", int)
ProUserId = NewType("ProUserId", int)


def f(x: list[int], user_id: UserId):
    y = cast(list[bool], x)  # error: [disjoint-cast]
    pro_user_id = cast(ProUserId, user_id)  # error: [disjoint-cast]
```

## Alternatives

In many cases, the diagnostic can be avoided by switching to use covariant generic types rather than
invariant ones:

```py
# `Sequence`, unlike `list`, is immutable and covariant
from collections.abc import Sequence
from typing import cast


def f(x: Sequence[int]):
    y = cast(Sequence[bool], x)  # no diagnostic
```

Although a pattern that provides more soundness and validation is always to avoid `cast()`
altogether, and instead write a custom `TypeIs` function for this:

```py
# `Sequence`, unlike `list`, is immutable and covariant
from collections.abc import Sequence
from typing_extensions import TypeIs, reveal_type


def is_sequence_of_bools(x: Sequence[int]) -> TypeIs[Sequence[bool]]:
    return all(isinstance(item, bool) for item in x)


def f(x: Sequence[int]):
    assert is_sequence_of_bools(x)
    reveal_type(x)  # revealed: Sequence[bool]
```

If you're unable to switch to immutable, covariant generic types, other solutions to this particular
diagnostic might include assigning a new list altogether:

```py
def f(x: list[int]):
    y: list[bool] = []
    for item in x:
        assert isinstance(item, bool)
        y.append(item)
```

Or using a `TypeGuard` -- which is still unsound, but, unlike a `cast()`, at least provides the
opportunity for some runtime validation:

```py
from typing_extensions import TypeGuard, reveal_type


def is_list_of_bools(x: list[int]) -> TypeGuard[list[bool]]:
    return all(isinstance(item, bool) for item in x)


def f(x: list[int]):
    assert is_list_of_bools(x)
    reveal_type(x)  # revealed: list[bool]
```

## Default level

This rule is disabled by default. It is designed as a strict rule for users who want additional
soundness checks from their type checker, and it may have false positives in some situations.

## See also

- The Ruff rule [`banned-api`][banned-api] can be used to ban the use of `cast()` entirely in your
    codebase.
- `redundant-cast` detects casts where the value already has the destination type.

[banned-api]: https://docs.astral.sh/ruff/rules/banned-api/
[mutable and invariant]: https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
[never]: https://docs.python.org/3/library/typing.html#typing.Never
