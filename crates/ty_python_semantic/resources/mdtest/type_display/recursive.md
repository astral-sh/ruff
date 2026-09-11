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
        reveal_type(recursive)  # revealed: tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])]
        def contexts[T](
            array: list[TypeOf[recursive]],
            pair: tuple[TypeOf[recursive], TypeOf[recursive]],
            union: TypeOf[recursive] | int,
            intersection: Intersection[TypeOf[recursive], T],
            complement: Not[TypeOf[recursive]],
            callback: Callable[[TypeOf[recursive]], TypeOf[recursive]],
        ):
            reveal_type(array)  # revealed: list[tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])]]
            # revealed: tuple[tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])], tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])]]
            reveal_type(pair)
            # TODO: remove redundant recursive alternatives. Expected: tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])] | int
            # revealed: tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])] | (μ$0. tuple[$0 | Literal[0]]) | int
            reveal_type(union)
            # TODO: remove redundant recursive alternatives. Expected: tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])] & T@contexts
            # revealed: μ{$0; $1 = tuple[($1 | $0) | Literal[0]]}. $1 & T@contexts
            reveal_type(intersection)
            reveal_type(complement)  # revealed: ~tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])]
            # revealed: (tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])], /) -> tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])]
            reveal_type(callback)
```

## Unions containing recursive types

A conditional expression's union follows the order of its branches, including when a branch's type
contains an anonymous recursive type inside a tuple.

```py
def outer(flag: bool, values: list[int]):
    value = 0
    for _ in values:
        value = (value,)
    # revealed: tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])] | Literal["other"]
    reveal_type((value,) if flag else "other")
    # revealed: Literal["other"] | tuple[Literal[0] | (μ$0. tuple[$0 | Literal[0]])]
    reveal_type("other" if flag else (value,))
```

## Truncated recursive bodies

Abbreviating a long union inside a tuple keeps the outer alternatives visible, even when the omitted
elements contain the recursive type.

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
    # error: [invalid-assignment] "tuple[A | B | C | ... omitted 4 union elements] | z"
    value: str = (container.value,) if flag else z()
```

## Unions of class objects

Class-object alternatives are grouped under a single `type[...]` annotation. The recursively
constructed tuple is currently approximated, losing these alternatives within its item.

```py
class A: ...
class B: ...

class Container:
    def __init__(self, left: type[B], right: type[A], choose: bool):
        self.value = left if choose else right

    def grow(self):
        self.value = (self.value,)

def inspect(container: Container):
    # TODO: retain the recursive type. Expected: type[B | A] | tuple[μ$0. tuple[$0] | type[A | B]]
    # revealed: type[B | A] | tuple[Divergent]
    reveal_type(container.value)
```
