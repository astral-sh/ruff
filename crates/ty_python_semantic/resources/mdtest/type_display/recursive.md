# Display of recursive types

```toml
[environment]
python-version = "3.12"
```

## Binder precedence

A recursive binder extends over its whole body and binds less tightly than union and intersection.
It needs no parentheses at the top level or within brackets and parameter lists, where the enclosing
delimiters already mark its boundary. As an operand of union, intersection, or negation, it is
parenthesized to keep those operations outside the binder.

The loop builds a recursive tuple. Narrowing out the initial integer exposes one tuple layer; its
element type still contains the recursive binder.

```py
from typing import Callable
from ty_extensions import Intersection, Not
from ty_extensions._internal import TypeOf

def show(n: int):
    recursive = 0
    for _ in range(n):
        recursive = (recursive,)
    if isinstance(recursive, tuple):
        reveal_type(recursive)  # revealed: tuple[(μa0. tuple[a0 | Literal[0]]) | Literal[0]]
        def contexts[T](
            array: list[TypeOf[recursive]],
            pair: tuple[TypeOf[recursive], TypeOf[recursive]],
            union: TypeOf[recursive] | int,
            intersection: Intersection[TypeOf[recursive], T],
            complement: Not[TypeOf[recursive]],
            callback: Callable[[TypeOf[recursive]], TypeOf[recursive]],
        ):
            reveal_type(array)  # revealed: list[μa0. tuple[a0 | Literal[0]]]
            reveal_type(pair)  # revealed: tuple[μa0. tuple[a0 | Literal[0]], μa0. tuple[a0 | Literal[0]]]
            reveal_type(union)  # revealed: (μa0. tuple[a0 | Literal[0]]) | int
            reveal_type(intersection)  # revealed: (μa0. tuple[a0 | Literal[0]]) & T@contexts
            reveal_type(complement)  # revealed: ~(μa0. tuple[a0 | Literal[0]])
            reveal_type(callback)  # revealed: (μa0. tuple[a0 | Literal[0]], /) -> μa0. tuple[a0 | Literal[0]]
```

## Unions containing recursive types

A union containing an anonymous recursive type uses a consistent display order, including when the
recursive type is nested inside a tuple. Reversing the conditional's branches preserves that order.

```py
def outer(flag: bool, values: list[int]):
    value = 0
    for _ in values:
        value = (value,)
    # revealed: tuple[(μa0. tuple[a0 | Literal[0]]) | Literal[0]] | Literal["other"]
    reveal_type((value,) if flag else "other")
    # revealed: tuple[(μa0. tuple[a0 | Literal[0]]) | Literal[0]] | Literal["other"]
    reveal_type("other" if flag else (value,))
```
