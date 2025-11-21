# Comprehensions

## Basic comprehensions

```py
# revealed: int
[reveal_type(x) for x in range(3)]

class Row:
    def __next__(self) -> range:
        return range(3)

class Table:
    def __iter__(self) -> Row:
        return Row()

# revealed: tuple[int, range]
[reveal_type((cell, row)) for row in Table() for cell in row]

# revealed: int
{reveal_type(x): 0 for x in range(3)}

# revealed: int
{0: reveal_type(x) for x in range(3)}
```

## Nested comprehension

```py
# revealed: tuple[int, int]
[[reveal_type((x, y)) for x in range(3)] for y in range(3)]
```

## Comprehension referencing outer comprehension

```py
class Row:
    def __next__(self) -> range:
        return range(3)

class Table:
    def __iter__(self) -> Row:
        return Row()

# revealed: tuple[int, range]
[[reveal_type((cell, row)) for cell in row] for row in Table()]
```

## Comprehension with unbound iterable

Iterating over an unbound iterable yields `Unknown`:

```py
# error: [unresolved-reference] "Name `x` used when not defined"
# revealed: Unknown
[reveal_type(z) for z in x]

# error: [not-iterable] "Object of type `int` is not iterable"
# revealed: tuple[int, Unknown]
[reveal_type((x, z)) for x in range(3) for z in x]

# error: [unresolved-reference] "Name `foo` used when not defined"
foo
foo = [
    # revealed: tuple[int, Unknown]
    reveal_type((x, z))
    for x in range(3)
    # error: [unresolved-reference] "Name `foo` used when not defined"
    for z in [foo]
]

baz = [
    # revealed: tuple[int, Unknown]
    reveal_type((x, z))
    for x in range(3)
    # error: [unresolved-reference] "Name `baz` used when not defined"
    for z in [baz]
]
```

## Starred expressions

Starred expressions must be iterable

```py
class NotIterable: ...

# This is fine:
x = [*range(3)]

# error: [not-iterable] "Object of type `NotIterable` is not iterable"
y = [*NotIterable()]
```

## Async comprehensions

### Basic

```py
class AsyncIterator:
    async def __anext__(self) -> int:
        return 42

class AsyncIterable:
    def __aiter__(self) -> AsyncIterator:
        return AsyncIterator()

async def _():
    # revealed: int
    [reveal_type(x) async for x in AsyncIterable()]
```

### Invalid async comprehension

This tests that we understand that `async` comprehensions do *not* work according to the synchronous
iteration protocol

```py
async def _():
    # error: [not-iterable] "Object of type `range` is not async-iterable"
    # revealed: Unknown
    [reveal_type(x) async for x in range(3)]
```

## Comprehension expression types

The type of the comprehension expression itself should reflect the inferred element type:

```py
from typing import TypedDict, Literal

# revealed: list[int | Unknown]
reveal_type([x for x in range(10)])

# revealed: set[int | Unknown]
reveal_type({x for x in range(10)})

# revealed: dict[int | Unknown, str | Unknown]
reveal_type({x: str(x) for x in range(10)})

# revealed: list[tuple[int, Unknown | str] | Unknown]
reveal_type([(x, y) for x in range(5) for y in ["a", "b", "c"]])

squares: list[int | None] = [x**2 for x in range(10)]
reveal_type(squares)  # revealed: list[int | None]
```

Inference for comprehensions takes the type context into account:

```py
# Without type context:
reveal_type([x for x in [1, 2, 3]])  # revealed: list[Unknown | int]
reveal_type({x: "a" for x in [1, 2, 3]})  # revealed: dict[Unknown | int, str | Unknown]
reveal_type({str(x): x for x in [1, 2, 3]})  # revealed: dict[str | Unknown, Unknown | int]
reveal_type({x for x in [1, 2, 3]})  # revealed: set[Unknown | int]

# With type context:
xs: list[int] = [x for x in [1, 2, 3]]
reveal_type(xs)  # revealed: list[int]

ys: dict[int, str] = {x: str(x) for x in [1, 2, 3]}
reveal_type(ys)  # revealed: dict[int, str]

zs: set[int] = {x for x in [1, 2, 3]}
```

This also works for nested comprehensions:

```py
table = [[(x, y) for x in range(3)] for y in range(3)]
reveal_type(table)  # revealed: list[list[tuple[int, int] | Unknown] | Unknown]

table_with_content: list[list[tuple[int, int, str | None]]] = [[(x, y, None) for x in range(3)] for y in range(3)]
reveal_type(table_with_content)  # revealed: list[list[tuple[int, int, str | None]]]
```

The type context is propagated down into the comprehension:

```py
class Person(TypedDict):
    name: str

# TODO: This should not error.
# error: [invalid-assignment]
persons: list[Person] = [{"name": n} for n in ["Alice", "Bob"]]
reveal_type(persons)  # revealed: list[Person]

# TODO: This should be an invalid-key error.
# error: [invalid-assignment]
invalid: list[Person] = [{"misspelled": n} for n in ["Alice", "Bob"]]
```

We promote literals to avoid overly-precise types in invariant positions:

```py
reveal_type([x for x in ("a", "b", "c")])  # revealed: list[str | Unknown]
reveal_type({x for x in (1, 2, 3)})  # revealed: set[int | Unknown]
reveal_type({k: 0 for k in ("a", "b", "c")})  # revealed: dict[str | Unknown, int | Unknown]
```

Type context can prevent this promotion from happening:

```py
list_of_literals: list[Literal["a", "b", "c"]] = [x for x in ("a", "b", "c")]
reveal_type(list_of_literals)  # revealed: list[Literal["a", "b", "c"]]

dict_with_literal_keys: dict[Literal["a", "b", "c"], int] = {k: 0 for k in ("a", "b", "c")}
reveal_type(dict_with_literal_keys)  # revealed: dict[Literal["a", "b", "c"], int]

dict_with_literal_values: dict[str, Literal[1, 2, 3]] = {str(k): k for k in (1, 2, 3)}
reveal_type(dict_with_literal_values)  # revealed: dict[str, Literal[1, 2, 3]]

set_with_literals: set[Literal[1, 2, 3]] = {k for k in (1, 2, 3)}
reveal_type(set_with_literals)  # revealed: set[Literal[1, 2, 3]]
```
