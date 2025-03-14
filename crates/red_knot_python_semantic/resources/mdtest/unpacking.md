# Unpacking

If there are not enough or too many values ​​when unpacking, an error will occur and the types of
all variables (if nested tuple unpacking fails, only the variables within the failed tuples) is
inferred to be `Unknown`.

## Tuple

### Simple tuple

```py
(a, b, c) = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
```

### Simple list

```py
[a, b, c] = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
```

### Simple mixed

```py
[a, (b, c), d] = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
reveal_type(d)  # revealed: Literal[4]
```

### Multiple assignment

```py
a, b = c = 1, 2
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: tuple[Literal[1], Literal[2]]
```

### Nested tuple with unpacking

```py
(a, (b, c), d) = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
reveal_type(d)  # revealed: Literal[4]
```

### Nested tuple without unpacking

```py
(a, b, c) = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: tuple[Literal[2], Literal[3]]
reveal_type(c)  # revealed: Literal[4]
```

### Uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 3, got 2)"
(a, b, c) = (1, 2)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack (expected 2, got 3)"
(a, b) = (1, 2, 3)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Nested uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 2, got 1)"
(a, (b, c), d) = (1, (2,), 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Literal[3]
```

### Nested uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack (expected 2, got 3)"
(a, (b, c), d) = (1, (2, 3, 4), 5)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Literal[5]
```

### Starred expression (1)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 3 or more, got 2)"
[a, *b, c, d] = (1, 2)
reveal_type(a)  # revealed: Unknown
# TODO: Should be list[Any] once support for assigning to starred expression is added
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
[a, *b, c] = (1, 2)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[Any] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo(starred unpacking)
reveal_type(c)  # revealed: Literal[2]
```

### Starred expression (3)

```py
[a, *b, c] = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo(starred unpacking)
reveal_type(c)  # revealed: Literal[3]
```

### Starred expression (4)

```py
[a, *b, c, d] = (1, 2, 3, 4, 5, 6)
reveal_type(a)  # revealed: Literal[1]
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo(starred unpacking)
reveal_type(c)  # revealed: Literal[5]
reveal_type(d)  # revealed: Literal[6]
```

### Starred expression (5)

```py
[a, b, *c] = (1, 2, 3, 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(c)  # revealed: @Todo(starred unpacking)
```

### Starred expression (6)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 5 or more, got 1)"
(a, b, c, *d, e, f) = (1,)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
reveal_type(e)  # revealed: Unknown
reveal_type(f)  # revealed: Unknown
```

### Non-iterable unpacking

```py
# error: "Object of type `Literal[1]` is not iterable"
a, b = 1
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Custom iterator unpacking

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

(a, b) = Iterable()
reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: int
```

### Custom iterator unpacking nested

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

(a, (b, c), d) = (1, Iterable(), 2)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
reveal_type(d)  # revealed: Literal[2]
```

## String

### Simple unpacking

```py
a, b = "ab"
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
```

### Uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 3, got 2)"
a, b, c = "ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack (expected 2, got 3)"
a, b = "abc"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Starred expression (1)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 3 or more, got 2)"
(a, *b, c, d) = "ab"
reveal_type(a)  # revealed: Unknown
# TODO: Should be list[LiteralString] once support for assigning to starred expression is added
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 3 or more, got 1)"
(a, b, *c, d) = "a"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
(a, *b, c) = "ab"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[Any] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo(starred unpacking)
reveal_type(c)  # revealed: LiteralString
```

### Starred expression (3)

```py
(a, *b, c) = "abc"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[LiteralString] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo(starred unpacking)
reveal_type(c)  # revealed: LiteralString
```

### Starred expression (4)

```py
(a, *b, c, d) = "abcdef"
reveal_type(a)  # revealed: LiteralString
# TODO: Should be list[LiteralString] once support for assigning to starred expression is added
reveal_type(b)  # revealed: @Todo(starred unpacking)
reveal_type(c)  # revealed: LiteralString
reveal_type(d)  # revealed: LiteralString
```

### Starred expression (5)

```py
(a, b, *c) = "abcd"
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
# TODO: Should be list[int] once support for assigning to starred expression is added
reveal_type(c)  # revealed: @Todo(starred unpacking)
```

### Unicode

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 2, got 1)"
(a, b) = "é"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Unicode escape (1)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 2, got 1)"
(a, b) = "\u9e6c"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Unicode escape (2)

```py
# error: [invalid-assignment] "Not enough values to unpack (expected 2, got 1)"
(a, b) = "\U0010ffff"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Surrogates

```py
(a, b) = "\ud800\udfff"

reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: LiteralString
```

## Union

### Same types

Union of two tuples of equal length and each element is of the same type.

```py
def _(arg: tuple[int, int] | tuple[int, int]):
    (a, b) = arg
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

### Mixed types (1)

Union of two tuples of equal length and one element differs in its type.

```py
def _(arg: tuple[int, int] | tuple[int, str]):
    a, b = arg
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int | str
```

### Mixed types (2)

Union of two tuples of equal length and both the element types are different.

```py
def _(arg: tuple[int, str] | tuple[str, int]):
    a, b = arg
    reveal_type(a)  # revealed: int | str
    reveal_type(b)  # revealed: str | int
```

### Mixed types (3)

Union of three tuples of equal length and various combination of element types:

1. All same types
1. One different type
1. All different types

```py
def _(arg: tuple[int, int, int] | tuple[int, str, bytes] | tuple[int, int, str]):
    a, b, c = arg
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int | str
    reveal_type(c)  # revealed: int | bytes | str
```

### Nested

```py
from typing import Literal

def _(arg: tuple[int, tuple[str, bytes]] | tuple[tuple[int, bytes], Literal["ab"]]):
    a, (b, c) = arg
    reveal_type(a)  # revealed: int | tuple[int, bytes]
    reveal_type(b)  # revealed: str
    reveal_type(c)  # revealed: bytes | LiteralString
```

### Starred expression

```py
def _(arg: tuple[int, bytes, int] | tuple[int, int, str, int, bytes]):
    a, *b, c = arg
    reveal_type(a)  # revealed: int
    # TODO: Should be `list[bytes | int | str]`
    reveal_type(b)  # revealed: @Todo(starred unpacking)
    reveal_type(c)  # revealed: int | bytes
```

### Size mismatch (1)

```py
def _(arg: tuple[int, bytes, int] | tuple[int, int, str, int, bytes]):
    # error: [invalid-assignment] "Too many values to unpack (expected 2, got 3)"
    # error: [invalid-assignment] "Too many values to unpack (expected 2, got 5)"
    a, b = arg
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
```

### Size mismatch (2)

```py
def _(arg: tuple[int, bytes] | tuple[int, str]):
    # error: [invalid-assignment] "Not enough values to unpack (expected 3, got 2)"
    # error: [invalid-assignment] "Not enough values to unpack (expected 3, got 2)"
    a, b, c = arg
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
```

### Same literal types

```py
def _(flag: bool):
    if flag:
        value = (1, 2)
    else:
        value = (3, 4)

    a, b = value
    reveal_type(a)  # revealed: Literal[1, 3]
    reveal_type(b)  # revealed: Literal[2, 4]
```

### Mixed literal types

```py
def _(flag: bool):
    if flag:
        value = (1, 2)
    else:
        value = ("a", "b")

    a, b = value
    reveal_type(a)  # revealed: Literal[1, "a"]
    reveal_type(b)  # revealed: Literal[2, "b"]
```

### Typing literal

```py
from typing import Literal

def _(arg: tuple[int, int] | Literal["ab"]):
    a, b = arg
    reveal_type(a)  # revealed: int | LiteralString
    reveal_type(b)  # revealed: int | LiteralString
```

### Custom iterator (1)

```py
class Iterator:
    def __next__(self) -> tuple[int, int] | tuple[int, str]:
        return (1, 2)

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

((a, b), c) = Iterable()
reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: int | str
reveal_type(c)  # revealed: tuple[int, int] | tuple[int, str]
```

### Custom iterator (2)

```py
class Iterator:
    def __next__(self) -> bytes:
        return b""

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

def _(arg: tuple[int, str] | Iterable):
    a, b = arg
    reveal_type(a)  # revealed: int | bytes
    reveal_type(b)  # revealed: str | bytes
```

## For statement

Unpacking in a `for` statement.

### Same types

```py
def _(arg: tuple[tuple[int, int], tuple[int, int]]):
    for a, b in arg:
        reveal_type(a)  # revealed: int
        reveal_type(b)  # revealed: int
```

### Mixed types (1)

```py
def _(arg: tuple[tuple[int, int], tuple[int, str]]):
    for a, b in arg:
        reveal_type(a)  # revealed: int
        reveal_type(b)  # revealed: int | str
```

### Mixed types (2)

```py
def _(arg: tuple[tuple[int, str], tuple[str, int]]):
    for a, b in arg:
        reveal_type(a)  # revealed: int | str
        reveal_type(b)  # revealed: str | int
```

### Mixed types (3)

```py
def _(arg: tuple[tuple[int, int, int], tuple[int, str, bytes], tuple[int, int, str]]):
    for a, b, c in arg:
        reveal_type(a)  # revealed: int
        reveal_type(b)  # revealed: int | str
        reveal_type(c)  # revealed: int | bytes | str
```

### Same literal values

```py
for a, b in ((1, 2), (3, 4)):
    reveal_type(a)  # revealed: Literal[1, 3]
    reveal_type(b)  # revealed: Literal[2, 4]
```

### Mixed literal values (1)

```py
for a, b in ((1, 2), ("a", "b")):
    reveal_type(a)  # revealed: Literal[1, "a"]
    reveal_type(b)  # revealed: Literal[2, "b"]
```

### Mixed literals values (2)

```py
# error: "Object of type `Literal[1]` is not iterable"
# error: "Object of type `Literal[2]` is not iterable"
# error: "Object of type `Literal[4]` is not iterable"
# error: [invalid-assignment] "Not enough values to unpack (expected 2, got 1)"
for a, b in (1, 2, (3, "a"), 4, (5, "b"), "c"):
    reveal_type(a)  # revealed: Unknown | Literal[3, 5]
    reveal_type(b)  # revealed: Unknown | Literal["a", "b"]
```

### Custom iterator (1)

```py
class Iterator:
    def __next__(self) -> tuple[int, int]:
        return (1, 2)

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

for a, b in Iterable():
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

### Custom iterator (2)

```py
class Iterator:
    def __next__(self) -> bytes:
        return b""

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

def _(arg: tuple[tuple[int, str], Iterable]):
    for a, b in arg:
        reveal_type(a)  # revealed: int | bytes
        reveal_type(b)  # revealed: str | bytes
```

## With statement

Unpacking in a `with` statement.

### Same types

```py
class ContextManager:
    def __enter__(self) -> tuple[int, int]:
        return (1, 2)

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

with ContextManager() as (a, b):
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

### Mixed types

```py
class ContextManager:
    def __enter__(self) -> tuple[int, str]:
        return (1, "a")

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

with ContextManager() as (a, b):
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
```

### Nested

```py
class ContextManager:
    def __enter__(self) -> tuple[int, tuple[str, bytes]]:
        return (1, ("a", b"bytes"))

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

with ContextManager() as (a, (b, c)):
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
    reveal_type(c)  # revealed: bytes
```

### Starred expression

```py
class ContextManager:
    def __enter__(self) -> tuple[int, int, int]:
        return (1, 2, 3)

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

with ContextManager() as (a, *b):
    reveal_type(a)  # revealed: int
    # TODO: Should be list[int] once support for assigning to starred expression is added
    reveal_type(b)  # revealed: @Todo(starred unpacking)
```

### Unbound context manager expression

```py
# TODO: should only be one diagnostic
# error: [unresolved-reference] "Name `nonexistant` used when not defined"
# error: [unresolved-reference] "Name `nonexistant` used when not defined"
# error: [unresolved-reference] "Name `nonexistant` used when not defined"
with nonexistant as (x, y):
    reveal_type(x)  # revealed: Unknown
    reveal_type(y)  # revealed: Unknown
```

### Invalid unpacking

```py
class ContextManager:
    def __enter__(self) -> tuple[int, str]:
        return (1, "a")

    def __exit__(self, *args) -> None:
        pass

# error: [invalid-assignment] "Not enough values to unpack (expected 3, got 2)"
with ContextManager() as (a, b, c):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
```
