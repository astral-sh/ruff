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
        reveal_type(recursive)  # revealed: tuple[(μ$0. tuple[$0 | Literal[0]]) | Literal[0]]
        def contexts[T](
            array: list[TypeOf[recursive]],
            pair: tuple[TypeOf[recursive], TypeOf[recursive]],
            union: TypeOf[recursive] | int,
            intersection: Intersection[TypeOf[recursive], T],
            complement: Not[TypeOf[recursive]],
            callback: Callable[[TypeOf[recursive]], TypeOf[recursive]],
        ):
            reveal_type(array)  # revealed: list[μ$0. tuple[$0 | Literal[0]]]
            reveal_type(pair)  # revealed: tuple[μ$0. tuple[$0 | Literal[0]], μ$0. tuple[$0 | Literal[0]]]
            reveal_type(union)  # revealed: (μ$0. tuple[$0 | Literal[0]]) | int
            reveal_type(intersection)  # revealed: (μ$0. tuple[$0 | Literal[0]]) & T@contexts
            reveal_type(complement)  # revealed: ~(μ$0. tuple[$0 | Literal[0]])
            reveal_type(callback)  # revealed: (μ$0. tuple[$0 | Literal[0]], /) -> μ$0. tuple[$0 | Literal[0]]
```

## Unions containing recursive types

A union containing an anonymous recursive type uses a consistent display order, including when the
recursive type is nested inside a tuple. Reversing the conditional's branches preserves that order.

```py
def outer(flag: bool, values: list[int]):
    value = 0
    for _ in values:
        value = (value,)
    # revealed: tuple[(μ$0. tuple[$0 | Literal[0]]) | Literal[0]] | Literal["other"]
    reveal_type((value,) if flag else "other")
    # revealed: tuple[(μ$0. tuple[$0 | Literal[0]]) | Literal[0]] | Literal["other"]
    reveal_type("other" if flag else (value,))
```

## Truncated recursive bodies

Abbreviating a long recursive body in a diagnostic preserves the order of the surrounding union,
even when the omitted alternatives contain all references to its binder.

```py
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
class F: ...
class z: ...

class Container:
    def __init__(self, value: A | B | C | D | E | F):
        self.value = value

    def grow(self):
        self.value = (self.value,)

def inspect(container: Container, flag: bool):
    # error: [invalid-assignment] "tuple[μ$0. tuple[$0] | A | B | ... omitted 4 union elements] | z"
    value: str = (container.value,) if flag else z()
```

## Unions of class objects

Class-object alternatives inside a recursive type are displayed in a consistent order, including
when they are grouped under a single `type[...]` annotation.

```py
class A: ...
class B: ...

class Container:
    def __init__(self, left: type[B], right: type[A], choose: bool):
        self.value = left if choose else right

    def grow(self):
        self.value = (self.value,)

def inspect(container: Container):
    reveal_type(container.value)  # revealed: μ$0. tuple[$0] | type[A | B]
```
