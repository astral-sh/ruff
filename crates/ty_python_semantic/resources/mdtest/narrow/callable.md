# Narrowing for `callable()`

## Basic narrowing

### Non-strict mode

```toml
[analysis]
strict-generic-narrowing = false
```

Narrowing with `callable(x)` or `isinstance(x, Callable)` narrow a union to just the callable types,
while preserving their signatures:

```py
from typing import Any, Callable

def _(f: Callable[[int, str], bytes] | None):
    if callable(f):
        reveal_type(f)  # revealed: (int, str, /) -> bytes
    else:
        reveal_type(f)  # revealed: None

    if isinstance(f, Callable):
        reveal_type(f)  # revealed: (int, str, /) -> bytes
    else:
        reveal_type(f)  # revealed: None
```

When narrowing from `object`, the result is a gradual callable that can be called with any
arguments.

```py
def f(f: object):
    if callable(f):
        # Note: typeshed annotates `callable` with a return type of `TypeIs[Callable[..., object]]`, which
        # is a hybrid between the fully gradual callable `Callable[..., Unknown]` and the top materialization
        # `Top[Callable[..., Unknown]]` which returns `object` and cannot be called. For consistency with
        # `isinstance` narrowing below, `(...) -> Unknown` would be better here, but we currently follow
        # the typeshed annotation.
        reveal_type(f)  # revealed: (...) -> object
        f(1, keyword="value")
    else:
        reveal_type(f)  # revealed: ~Top[(...) -> object]

    if isinstance(f, Callable):
        reveal_type(f)  # revealed: (...) -> Unknown
        f(1, keyword="value")
    else:
        reveal_type(f)  # revealed: ~Top[(...) -> object]
```

### Strict mode

```toml
[analysis]
strict-generic-narrowing = true
```

Narrowing with `callable(x)` or `isinstance(x, Callable)` narrow a union to just the callable types,
while preserving their signatures. Exactly the same as in non-strict mode.

```py
from typing import Any, Callable

def _(f: Callable[[int, str], bytes] | None):
    if callable(f):
        reveal_type(f)  # revealed: (int, str, /) -> bytes
    else:
        reveal_type(f)  # revealed: None

    if isinstance(f, Callable):
        reveal_type(f)  # revealed: (int, str, /) -> bytes
    else:
        reveal_type(f)  # revealed: None
```

However, when narrowing from `object`, the result is the top-materialized callable type
`Top[(...) -> object]`. This type represents the set of all possible callable types (including,
e.g., functions that take no arguments and functions that require arguments). While such objects
*are* callable (they pass `callable()`), no specific set of arguments can be guaranteed to be valid.

```py
def f(f: object):
    if callable(f):
        reveal_type(f)  # revealed: Top[(...) -> object]
        f(1, keyword="value")  # error: [call-top-callable]
    else:
        reveal_type(f)  # revealed: ~Top[(...) -> object]

    if isinstance(f, Callable):
        reveal_type(f)  # revealed: Top[(...) -> object]
        f(1, keyword="value")  # error: [call-top-callable]
    else:
        reveal_type(f)  # revealed: ~Top[(...) -> object]
```

## Narrowing from gradual callable types

```py
from typing import Any, Callable

def h(x: Callable[..., int] | None):
    if callable(x):
        reveal_type(x)  # revealed: (...) -> int
    else:
        reveal_type(x)  # revealed: None
```

## Intersections with the top-callable

```toml
[analysis]
strict-generic-narrowing = true
```

If a top-callable is part of an intersection, it should still contribute its return type even when
the other intersection elements are not callable:

```py
from typing import Callable

def resolve(value: str):
    if callable(value):
        reveal_type(value)  # revealed: str & Top[(...) -> object]
        # error: [call-top-callable]
        reveal_type(value())  # revealed: object
```

## Narrowing with named expressions (walrus operator)

When `callable()` is used with a named expression, the target of the named expression should be
narrowed.

```py
from typing import Any

class Foo:
    func: Any | None

def f(foo: Foo):
    first = getattr(foo, "func", None)
    if callable(first):
        reveal_type(first)  # revealed: Any & ((...) -> object)
    else:
        reveal_type(first)  # revealed: (Any & ~Top[(...) -> object]) | None

    if callable(second := getattr(foo, "func", None)):
        reveal_type(second)  # revealed: Any & ((...) -> object)
    else:
        reveal_type(second)  # revealed: (Any & ~Top[(...) -> object]) | None
```

## Assignability of narrowed callables

```toml
[analysis]
strict-generic-narrowing = true
```

A narrowed callable `Top[Callable[..., object]]` should be assignable to `Callable[..., Any]`. This
is important for decorators and other patterns where we need to pass the narrowed callable to
functions expecting gradual callables.

```py
from typing import Any, Callable, TypeVar
from ty_extensions import static_assert, Top
from ty_extensions._internal import is_assignable_to

static_assert(is_assignable_to(Top[Callable[..., bool]], Callable[..., int]))

F = TypeVar("F", bound=Callable[..., Any])

def wrap(f: F) -> F:
    return f

def f(x: object):
    if callable(x):
        # x has type `Top[(...) -> object]`, which should be assignable to `Callable[..., Any]`
        wrap(x)
```

## `isinstance` parity for `typing.Callable` and `collections.abc.Callable`

`typing.Callable` is a deprecated alias for `collections.abc.Callable`. Both should narrow
identically when used as the second argument to `isinstance()`.

```py
import typing
import collections.abc

def f(x: object):
    if isinstance(x, typing.Callable):
        reveal_type(x)  # revealed: (...) -> Unknown
    else:
        reveal_type(x)  # revealed: ~Top[(...) -> object]

    if isinstance(x, collections.abc.Callable):
        reveal_type(x)  # revealed: (...) -> Unknown
    else:
        reveal_type(x)  # revealed: ~Top[(...) -> object]
```

## `Callable` special-form identity

`typing.Callable` and `collections.abc.Callable` are both modeled as special forms. Import
resolution should preserve which module the symbol comes from, even when the symbol is re-exported
through another module. These tests only check symbol resolution; class-pattern behavior is tested
separately below.

### Direct imports

```py
import collections.abc
import typing
from collections.abc import Callable as CollectionsAbcCallable
from typing import Callable as TypingCallable
from _collections_abc import Callable as _CollectionsAbcCallable

reveal_type(TypingCallable)  # revealed: <special-form 'typing.Callable'>
reveal_type(typing.Callable)  # revealed: <special-form 'typing.Callable'>
reveal_type(CollectionsAbcCallable)  # revealed: <special-form 'collections.abc.Callable'>
reveal_type(collections.abc.Callable)  # revealed: <special-form 'collections.abc.Callable'>
reveal_type(_CollectionsAbcCallable)  # revealed: <special-form 'collections.abc.Callable'>
```

### Imports proxied through another module

`typing_compat.py`:

```py
from typing import Callable
```

`collections_abc_compat.py`:

```py
from collections.abc import Callable
```

`main.py`:

```py
from collections_abc_compat import Callable as CollectionsAbcCallable
from typing_compat import Callable as TypingCallable

reveal_type(TypingCallable)  # revealed: <special-form 'typing.Callable'>
reveal_type(CollectionsAbcCallable)  # revealed: <special-form 'collections.abc.Callable'>
```

## Class-pattern behavior for `typing.Callable` and `collections.abc.Callable`

At runtime, `collections.abc.Callable` is an instance of `type` and is supported in `match`
statement class patterns; however, `typing.Callable` is not.

### `collections.abc.Callable`

```py
from collections import abc

def accepts_type(x: type): ...

accepts_type(abc.Callable)  # no diagnostic

def _(subj: None | abc.Callable[..., str]) -> None:
    match subj:
        case abc.Callable():
            reveal_type(subj)  # revealed: (...) -> str
        case _:
            reveal_type(subj)  # revealed: None

def _(subj: tuple[abc.Callable[..., int]] | tuple[None]) -> None:
    match subj:
        case [abc.Callable()]:
            reveal_type(subj[0])  # revealed: (...) -> int
```

`collections.abc.Callable` has no `__match_args__`, so it does not accept positional subpatterns:

```py
from collections import abc

def _(subj: abc.Callable[..., str]) -> None:
    match subj:
        # error: [invalid-match-pattern] "Too many positional subpatterns for `collections.abc.Callable`: expected 0, got 1"
        case abc.Callable(x): ...
```

### `typing.Callable`

```py
import typing

def accepts_type(x: type): ...

accepts_type(typing.Callable)  # error: [invalid-argument-type]

def _(subj: None | typing.Callable[..., str]) -> None:
    match subj:
        # error: [invalid-match-pattern] "`<special-form 'typing.Callable'>` cannot be used in a class pattern because it is not a type"
        case typing.Callable(): ...
        case _: ...
```
