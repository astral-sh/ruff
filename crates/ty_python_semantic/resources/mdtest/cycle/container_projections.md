# Recursive container projections

## Reconstructing a tuple from an element

Reading the first element and constructing another pair preserves the first element's type. The
second element remains a string throughout the recursive assignments, including when the whole tuple
is copied by slicing.

```py
class Pair:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Pair"):
        self.value = (other.value[0], "next")

    def copy_from(self, other: "Pair"):
        self.value = other.value[:]

reveal_type(Pair().value)  # revealed: tuple[int, str]
reveal_type(Pair().value[0])  # revealed: int
reveal_type(Pair().value[1])  # revealed: str
```

## Indexing through two tuple levels

Reading the element inside both tuples and rebuilding both levels preserves the nested tuple's type.
Each indexing operation removes one level of nesting.

```py
class Container:
    def __init__(self):
        self.value = ((0,),)

    def update(self):
        self.value = ((self.value[0][0],),)

reveal_type(Container().value)  # revealed: tuple[tuple[int]]
```

## Nesting a projected element

Nesting just one element produces a recursive element type. It does not make the other element
recursive or change the length of the outer tuple.

```py
class Nested:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Nested"):
        self.value = (other.value[0], (other.value[1],))

reveal_type(Nested().value[0])  # revealed: int
reveal_type(Nested().value[1])  # revealed: μa0. tuple[a0] | str
```

## Checking subscripts in a recursive assignment

Recursive assignments do not permit invalid indices or a slice with a zero step.

```py
class Checked:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Checked"):
        self.value = (other.value[0], "next")
        other.value[2]  # error: [index-out-of-bounds]
        other.value[::0]  # error: [zero-stepsize-in-slice]
        other.value["key"]  # error: [invalid-argument-type]

reveal_type(Checked().value)  # revealed: tuple[int, str]
```

## Iterating over tuples

Both the initial pair and every later one-element tuple contain integers.

```py
class Container:
    def __init__(self):
        self.value = (0, 1)

    def update(self, other: "Container"):
        for item in other.value:
            self.value = (item,)

reveal_type(Container().value)  # revealed: tuple[int, int] | tuple[int]
```

## Iterating through two tuple levels

The outer loop yields a tuple, and the inner loop yields its integer element. Rebuilding both tuple
levels preserves the original nested tuple type.

```py
class Container:
    def __init__(self):
        self.value = ((0,),)

    def update(self):
        for row in self.value:
            for item in row:
                self.value = ((item,),)

reveal_type(Container().value)  # revealed: tuple[tuple[int]]
```

## Iterating over lists

The first iteration reads an integer from a list. Later iterations read integers from the tuples
assigned by the loop.

```py
class Container:
    def __init__(self):
        self.value = [0]

    def update(self, other: "Container"):
        for item in other.value:
            self.value = (item,)

reveal_type(Container().value)  # revealed: list[int] | tuple[int]
```

## Iterating over sets

Set elements remain integers when iteration feeds them into a recursively assigned tuple.

```py
class Container:
    def __init__(self):
        self.value = {0}

    def update(self, other: "Container"):
        for item in other.value:
            self.value = (item,)

reveal_type(Container().value)  # revealed: set[int] | tuple[int]
```

## Iterating over dictionaries

Iterating over a dictionary reads its keys. Its string values do not contribute to the recursively
assigned tuples.

```py
class Container:
    def __init__(self):
        self.value = {0: "value"}

    def update(self, other: "Container"):
        for item in other.value:
            self.value = (item,)

reveal_type(Container().value)  # revealed: dict[int, str] | tuple[int]
```

## Iterating over a user-defined iterable

```toml
[environment]
python-version = "3.12"
```

A user-defined iterator's element type is also preserved through recursive assignments.

```py
from typing import Iterator

class Values[T]:
    def __init__(self, item: T):
        self.item = item

    def __iter__(self) -> Iterator[T]:
        yield self.item

class Container:
    def __init__(self):
        self.value = Values(0)

    def update(self, other: "Container"):
        for item in other.value:
            self.value = (item,)

reveal_type(Container().value)  # revealed: Values[int] | tuple[int]
```

## Iterating through indexed access

```toml
[environment]
python-version = "3.12"
```

An object can support iteration through `__getitem__` without defining `__iter__`. Its declared item
type contributes to the recursively assigned tuple in the same way.

```py
class Values[T]:
    def __init__(self, item: T):
        self.item = item

    def __getitem__(self, index: int) -> T:
        return self.item

class Container:
    def __init__(self):
        self.value = Values(0)

    def update(self, other: "Container"):
        for item in other.value:
            self.value = (item,)

reveal_type(Container().value)  # revealed: Values[int] | tuple[int]
```

## Unpacking a recursive pair

Unpacking and rebuilding a pair preserves the type at each position.

```py
class Pair:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Pair"):
        first, second = other.value
        self.value = (first, second)

reveal_type(Pair().value)  # revealed: tuple[int, str]
```

## Unpacking through two tuple levels

Each unpacking assignment removes one tuple level, so the inner item remains an integer when both
levels are reconstructed.

```py
class Container:
    def __init__(self):
        self.value = ((0,),)

    def update(self):
        (row,) = self.value
        (item,) = row
        self.value = ((item,),)

reveal_type(Container().value)  # revealed: tuple[tuple[int]]
```

## Unpacking a recursive tuple with a starred target

A starred target consumes the middle elements without mixing them into the first and last positions.

```py
class Triple:
    def __init__(self):
        self.value = (0, "start", True)

    def update(self, other: "Triple"):
        first, *middle, last = other.value
        reveal_type(middle)  # revealed: list[str]
        self.value = (first, "next", last)

reveal_type(Triple().value)  # revealed: tuple[int, str, bool]
```

## Iterating asynchronously over the containing instance

An asynchronous iterator yields instances of its class. Assigning those instances back to an
inferred attribute preserves that class type, including when the attribute initially stores `self`.

```py
from typing import AsyncIterator

class Container:
    def __init__(self):
        self.items = self

    async def __aiter__(self) -> AsyncIterator["Container"]:
        yield self

    async def update(self, other: "Container"):
        async for item in other.items:
            self.items = item

reveal_type(Container().items)  # revealed: Container
```
