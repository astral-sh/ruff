# Narrowing for `callable()`

## Basic narrowing

The `callable()` builtin returns `TypeIs[Callable[..., object]]`, which narrows the type to the
intersection with `Top[Callable[..., object]]`. The `Top[...]` wrapper indicates this is a fully
static type representing the top materialization of a gradual callable.

Since all callable types are subtypes of `Top[Callable[..., object]]`, intersections with `Top[...]`
simplify to just the original callable type.

```py
from typing import Any, Callable

def f(x: Callable[..., Any] | None):
    if callable(x):
        # The intersection simplifies because `(...) -> Any` is a subtype of
        # `Top[(...) -> object]` - all callables are subtypes of the top materialization.
        reveal_type(x)  # revealed: (...) -> Any
    else:
        # Since `(...) -> Any` is a subtype of `Top[(...) -> object]`, the intersection
        # with the negation is empty (Never), leaving just None.
        reveal_type(x)  # revealed: None
```

## Narrowing with other callable types

```py
from typing import Any, Callable

def g(x: Callable[[int], str] | None):
    if callable(x):
        # All callables are subtypes of `Top[(...) -> object]`, so the intersection simplifies.
        reveal_type(x)  # revealed: (int, /) -> str
    else:
        reveal_type(x)  # revealed: None

def h(x: Callable[..., int] | None):
    if callable(x):
        reveal_type(x)  # revealed: (...) -> int
    else:
        reveal_type(x)  # revealed: None
```

## Narrowing from object

```py
def f(x: object):
    if callable(x):
        reveal_type(x)  # revealed: Top[(...) -> object]
    else:
        reveal_type(x)  # revealed: ~Top[(...) -> object]
```

## Calling narrowed callables

### Strict generic narrowing mode

```toml
[analysis]
strict-generic-narrowing = true
```

In strict generic narrowing mode, an `isinstance(.., Callable)` check intersects the type with
`Top[Callable[..., object]]`. This type represents the set of all possible callable types
(including, e.g., functions that take no arguments and functions that require arguments). While such
objects *are* callable (they pass `callable()`), no specific set of arguments can be guaranteed to
be valid.

```py
import typing as t

def call_with_args(y: object, a: int, b: str) -> object:
    if isinstance(y, t.Callable):
        # error: [call-top-callable]
        return y(a, b)
    return None
```

If a top-callable is part of an intersection, it should still contribute its return type even when
the other intersection elements are not callable:

```py
def resolve(value: str):
    if callable(value):
        reveal_type(value)  # revealed: str & Top[(...) -> object]
        # error: [call-top-callable]
        reveal_type(value())  # revealed: object
```

### Relaxed generic narrowing mode

```toml
[analysis]
strict-generic-narrowing = false
```

In relaxed generic narrowing mode, an `isinstance(.., Callable)` check narrows the type to
`Callable[..., Unknown]` which is callable with any arguments and returns an unknown type:

```py
from typing import Callable

def call_with_args(y: object):
    if isinstance(y, Callable):
        reveal_type(y)  # revealed: (...) -> Unknown

        y()
        y(1, "foo")
        y(1, "foo", keyword_arg="bar")
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
        reveal_type(first)  # revealed: Any & Top[(...) -> object]
    else:
        reveal_type(first)  # revealed: (Any & ~Top[(...) -> object]) | None

    if callable(second := getattr(foo, "func", None)):
        reveal_type(second)  # revealed: Any & Top[(...) -> object]
    else:
        reveal_type(second)  # revealed: (Any & ~Top[(...) -> object]) | None
```

## Assignability of narrowed callables

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

At runtime, `collections.abc.Callable` is supported in `match` statement class patterns, however
`typing.Callable` is not.

### `collections.abc.Callable`

```py
from collections import abc

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

def _(subj: None | typing.Callable[..., str]) -> None:
    match subj:
        # error: [invalid-match-pattern] "`<special-form 'typing.Callable'>` cannot be used in a class pattern because it is not a type"
        case typing.Callable(): ...
        case _: ...
```
