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
at runtime nor, by default, by type checkers. While upcasting to a supertype is always sound, and
casting to a subtype can be sound in some situations if accompanied by careful validation checks,
`cast()` is also deliberately designed to allow unsound narrowing, and most useful applications of
`cast()` in real-world code cannot be fully validated by a type checker.

Nonetheless, even while acknowledging the fact that `cast()` is intentionally designed to allow
unsoundness, casting a value to an entirely *disjoint* type is especially likely to indicate a
mistake in your code. A cast from an `int` to a `str`, for example, likely indicates a bug or
misunderstanding.

This rule therefore provides a means for codebases to partially validate their uses of `cast()`
without banning the API -- or even banning all unsound uses of the API -- entirely.

## Example

```py
from typing import cast


def parse(value: int) -> str:
    return cast(str, value)  # error: [disjoint-cast]
```

Casts between overlapping (non-disjoint) types are allowed:

```py
from collections.abc import Sequence
from typing import cast


def validate(numbers: Sequence[int | None]) -> Sequence[int]:
    if None in numbers:
        raise TypeError("must provide a sequence of numbers!")
    return cast(Sequence[int], numbers)
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

Though if you're able to use covariant types, a type-safe narrowing mechanism that provides runtime
validation, such as using `TypeIs`, is generally preferable to using `cast`:

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

If you're unable to switch to an immutable, covariant generic type, other solutions to this
particular diagnostic might include assigning a new list altogether:

```py
def f(x: list[int]):
    y: list[bool] = []
    for item in x:
        assert isinstance(item, bool)
        y.append(item)
```

Or using a `TypeGuard`. While the "narrowing" below is still unsound, there is at least some runtime
validation of the element types taking place, making it superior to the `cast`:

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
