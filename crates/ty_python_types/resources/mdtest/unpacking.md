# Unpacking

If there are not enough or too many values when unpacking, an error will occur and the types of all
variables (if nested tuple unpacking fails, only the variables within the failed tuples) is inferred
to be `Unknown`.

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
# error: [invalid-assignment] "Not enough values to unpack: Expected 3"
(a, b, c) = (1, 2)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
(a, b) = (1, 2, 3)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Nested uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
(a, (b, c), d) = (1, (2,), 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Literal[3]
```

### Nested uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
(a, (b, c), d) = (1, (2, 3, 4), 5)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Literal[5]
```

### Starred expression (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
[a, *b, c, d] = (1, 2)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
[a, *b, c] = (1, 2)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: list[Never]
reveal_type(c)  # revealed: Literal[2]
```

### Starred expression (3)

```py
[a, *b, c] = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: list[Literal[2]]
reveal_type(c)  # revealed: Literal[3]
```

### Starred expression (4)

```py
[a, *b, c, d] = (1, 2, 3, 4, 5, 6)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: list[Literal[2, 3, 4]]
reveal_type(c)  # revealed: Literal[5]
reveal_type(d)  # revealed: Literal[6]
```

### Starred expression (5)

```py
[a, b, *c] = (1, 2, 3, 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: list[Literal[3, 4]]
```

### Starred expression (6)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 5"
(a, b, c, *d, e, f) = (1,)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: list[Unknown]
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

## List

### Literal unpacking

```py
a, b = [1, 2]
reveal_type(a)  # revealed: Unknown | int
reveal_type(b)  # revealed: Unknown | int
```

### Simple unpacking

```py
def _(value: list[int]):
    a, b = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

### Nested unpacking

```py
def _(value: list[list[int]]):
    a, (b, c) = value
    reveal_type(a)  # revealed: list[int]
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: int
```

### Invalid nested unpacking

```py
def _(value: list[int]):
    # error: [not-iterable] "Object of type `int` is not iterable"
    a, (b, c) = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
```

### Starred expression

```py
def _(value: list[int]):
    a, *b, c = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[int]
    reveal_type(c)  # revealed: int
```

## Homogeneous tuples

### Simple unpacking

```py
def _(value: tuple[int, ...]):
    a, b = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

### Nested unpacking

```py
def _(value: tuple[tuple[int, ...], ...]):
    a, (b, c) = value
    reveal_type(a)  # revealed: tuple[int, ...]
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: int
```

### Invalid nested unpacking

```py
def _(value: tuple[int, ...]):
    # error: [not-iterable] "Object of type `int` is not iterable"
    a, (b, c) = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
```

### Starred expression

```py
def _(value: tuple[int, ...]):
    a, *b, c = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[int]
    reveal_type(c)  # revealed: int
```

## Mixed tuples

```toml
[environment]
python-version = "3.11"
```

### Simple unpacking (1)

```py
def _(value: tuple[int, *tuple[str, ...]]):
    a, b = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
```

### Simple unpacking (2)

```py
def _(value: tuple[int, int, *tuple[str, ...]]):
    a, b = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
```

### Simple unpacking (3)

```py
def _(value: tuple[int, *tuple[str, ...], int]):
    a, b, c = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: str
    reveal_type(c)  # revealed: int
```

### Invalid unpacked

```py
def _(value: tuple[int, int, int, *tuple[str, ...]]):
    # error: [invalid-assignment] "Too many values to unpack: Expected 2"
    a, b = value
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
```

### Nested unpacking

```py
def _(value: tuple[str, *tuple[tuple[int, ...], ...]]):
    a, (b, c) = value
    reveal_type(a)  # revealed: str
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: int
```

### Invalid nested unpacking

```py
def _(value: tuple[str, *tuple[int, ...]]):
    # error: [not-iterable] "Object of type `int` is not iterable"
    a, (b, c) = value
    reveal_type(a)  # revealed: str
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
```

### Starred expression (1)

```py
def _(value: tuple[int, *tuple[str, ...]]):
    a, *b, c = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[str]
    reveal_type(c)  # revealed: str
```

### Starred expression (2)

```py
def _(value: tuple[int, *tuple[str, ...], int]):
    a, *b, c = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[str]
    reveal_type(c)  # revealed: int
```

### Starred expression (3)

```py
def _(value: tuple[int, *tuple[str, ...], int]):
    a, *b, c, d = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[str]
    reveal_type(c)  # revealed: str
    reveal_type(d)  # revealed: int
```

### Starred expression (4)

```py
def _(value: tuple[int, int, *tuple[str, ...], int]):
    a, *b, c = value
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[int | str]
    reveal_type(c)  # revealed: int
```

## Tuple subclasses

A tuple subclass inherits its heterogeneous unpacking behaviour from its tuple superclass.

```toml
[environment]
python-version = "3.11"
```

```py
class I0: ...
class I1: ...
class I2: ...
class HeterogeneousTupleSubclass(tuple[I0, I1, I2]): ...

def f(x: HeterogeneousTupleSubclass):
    a, b, c = x
    reveal_type(a)  # revealed: I0
    reveal_type(b)  # revealed: I1
    reveal_type(c)  # revealed: I2

    d, e = x  # error: [invalid-assignment] "Too many values to unpack: Expected 2"

    reveal_type(d)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown

    f, g, h, i = x  # error: [invalid-assignment] "Not enough values to unpack: Expected 4"

    reveal_type(f)  # revealed: Unknown
    reveal_type(g)  # revealed: Unknown
    reveal_type(h)  # revealed: Unknown
    reveal_type(i)  # revealed: Unknown

    [j, *k] = x
    reveal_type(j)  # revealed: I0
    reveal_type(k)  # revealed: list[I1 | I2]

    [l, m, *n] = x
    reveal_type(l)  # revealed: I0
    reveal_type(m)  # revealed: I1
    reveal_type(n)  # revealed: list[I2]

    [o, p, q, *r] = x
    reveal_type(o)  # revealed: I0
    reveal_type(p)  # revealed: I1
    reveal_type(q)  # revealed: I2
    reveal_type(r)  # revealed: list[Never]

    # error: [invalid-assignment] "Not enough values to unpack: Expected at least 4"
    [s, t, u, v, *w] = x
    reveal_type(s)  # revealed: Unknown
    reveal_type(t)  # revealed: Unknown
    reveal_type(u)  # revealed: Unknown
    reveal_type(v)  # revealed: Unknown
    reveal_type(w)  # revealed: list[Unknown]

class MixedTupleSubclass(tuple[I0, *tuple[I1, ...], I2]): ...

def f(x: MixedTupleSubclass):
    (a,) = x  # error: [invalid-assignment] "Too many values to unpack: Expected 1"
    reveal_type(a)  # revealed: Unknown

    c, d = x
    reveal_type(c)  # revealed: I0
    reveal_type(d)  # revealed: I2

    e, f, g = x
    reveal_type(e)  # revealed: I0
    reveal_type(f)  # revealed: I1
    reveal_type(g)  # revealed: I2

    h, i, j, k = x
    reveal_type(h)  # revealed: I0
    reveal_type(i)  # revealed: I1
    reveal_type(j)  # revealed: I1
    reveal_type(k)  # revealed: I2

    [l, *m] = x
    reveal_type(l)  # revealed: I0
    reveal_type(m)  # revealed: list[I1 | I2]

    [n, o, *p] = x
    reveal_type(n)  # revealed: I0

    # TODO: `I1 | I2` might be better here? (https://github.com/astral-sh/ty/issues/947)
    reveal_type(o)  # revealed: I1

    reveal_type(p)  # revealed: list[I1 | I2]

    [o, p, q, *r] = x
    reveal_type(o)  # revealed: I0

    # TODO: `I1 | I2` might be better for both of these? (https://github.com/astral-sh/ty/issues/947)
    reveal_type(p)  # revealed: I1
    reveal_type(q)  # revealed: I1

    reveal_type(r)  # revealed: list[I1 | I2]

    s, *t, u = x
    reveal_type(s)  # revealed: I0
    reveal_type(t)  # revealed: list[I1]
    reveal_type(u)  # revealed: I2

    aa, bb, *cc, dd = x
    reveal_type(aa)  # revealed: I0
    reveal_type(bb)  # revealed: I1
    reveal_type(cc)  # revealed: list[I1]
    reveal_type(dd)  # revealed: I2
```

## String

### Simple unpacking

```py
a, b = "ab"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: Literal["b"]
```

### Uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 3"
a, b, c = "ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
a, b = "abc"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Starred expression (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
(a, *b, c, d) = "ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
(a, b, *c, d) = "a"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: list[Unknown]
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
(a, *b, c) = "ab"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: list[Never]
reveal_type(c)  # revealed: Literal["b"]
```

### Starred expression (3)

```py
(a, *b, c) = "abc"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: list[Literal["b"]]
reveal_type(c)  # revealed: Literal["c"]
```

### Starred expression (4)

```py
(a, *b, c, d) = "abcdef"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: list[Literal["b", "c", "d"]]
reveal_type(c)  # revealed: Literal["e"]
reveal_type(d)  # revealed: Literal["f"]
```

### Starred expression (5)

```py
(a, b, *c) = "abcd"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: Literal["b"]
reveal_type(c)  # revealed: list[Literal["c", "d"]]
```

### Starred expression (6)

```py
from typing_extensions import LiteralString

def _(s: LiteralString):
    a, b, *c = s
    reveal_type(a)  # revealed: LiteralString
    reveal_type(b)  # revealed: LiteralString
    reveal_type(c)  # revealed: list[LiteralString]
```

### Unicode

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
(a, b) = "é"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Unicode escape (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
(a, b) = "\u9e6c"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Unicode escape (2)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
(a, b) = "\U0010ffff"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Surrogates

```py
(a, b) = "\ud800\udfff"

reveal_type(a)  # revealed: Literal["�"]
reveal_type(b)  # revealed: Literal["�"]
```

### Very long literal

```py
string = "very long stringgggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"

a, *b = string
reveal_type(a)  # revealed: LiteralString
reveal_type(b)  # revealed: list[LiteralString]
```

## Bytes

### Simple unpacking

```py
a, b = b"ab"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: Literal[98]
```

### Uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 3"
a, b, c = b"ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
a, b = b"abc"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Starred expression (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
(a, *b, c, d) = b"ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
(a, b, *c, d) = b"a"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: list[Unknown]
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
(a, *b, c) = b"ab"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: list[Never]
reveal_type(c)  # revealed: Literal[98]
```

### Starred expression (3)

```py
(a, *b, c) = b"abc"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: list[Literal[98]]
reveal_type(c)  # revealed: Literal[99]
```

### Starred expression (4)

```py
(a, *b, c, d) = b"abcdef"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: list[Literal[98, 99, 100]]
reveal_type(c)  # revealed: Literal[101]
reveal_type(d)  # revealed: Literal[102]
```

### Starred expression (5)

```py
(a, b, *c) = b"abcd"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: Literal[98]
reveal_type(c)  # revealed: list[Literal[99, 100]]
```

### Very long literal

```py
too_long = b"very long bytes stringggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"

a, *b = too_long
reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: list[int]
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
    reveal_type(c)  # revealed: bytes | Literal["b"]
```

### Starred expression

```py
def _(arg: tuple[int, bytes, int] | tuple[int, int, str, int, bytes]):
    a, *b, c = arg
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: list[bytes] | list[int | str]
    reveal_type(c)  # revealed: int | bytes
```

### Size mismatch (1)

```py
def _(arg: tuple[int, bytes, int] | tuple[int, int, str, int, bytes]):
    # error: [invalid-assignment] "Too many values to unpack: Expected 2"
    # error: [invalid-assignment] "Too many values to unpack: Expected 2"
    a, b = arg
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
```

### Size mismatch (2)

```py
def _(arg: tuple[int, bytes] | tuple[int, str]):
    # error: [invalid-assignment] "Not enough values to unpack: Expected 3"
    # error: [invalid-assignment] "Not enough values to unpack: Expected 3"
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
    reveal_type(a)  # revealed: int | Literal["a"]
    reveal_type(b)  # revealed: int | Literal["b"]
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
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
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
    reveal_type(b)  # revealed: list[int]
```

### Unbound context manager expression

```py
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

# error: [invalid-assignment] "Not enough values to unpack: Expected 3"
with ContextManager() as (a, b, c):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
```

## Comprehension

Unpacking in a comprehension.

### Same types

```py
def _(arg: tuple[tuple[int, int], tuple[int, int]]):
    # revealed: tuple[int, int]
    [reveal_type((a, b)) for a, b in arg]
```

### Mixed types (1)

```py
def _(arg: tuple[tuple[int, int], tuple[int, str]]):
    # revealed: tuple[int, int | str]
    [reveal_type((a, b)) for a, b in arg]
```

### Mixed types (2)

```py
def _(arg: tuple[tuple[int, str], tuple[str, int]]):
    # revealed: tuple[int | str, str | int]
    [reveal_type((a, b)) for a, b in arg]
```

### Mixed types (3)

```py
def _(arg: tuple[tuple[int, int, int], tuple[int, str, bytes], tuple[int, int, str]]):
    # revealed: tuple[int, int | str, int | bytes | str]
    [reveal_type((a, b, c)) for a, b, c in arg]
```

### Same literal values

```py
# revealed: tuple[Literal[1, 3], Literal[2, 4]]
[reveal_type((a, b)) for a, b in ((1, 2), (3, 4))]
```

### Mixed literal values (1)

```py
# revealed: tuple[Literal[1, "a"], Literal[2, "b"]]
[reveal_type((a, b)) for a, b in ((1, 2), ("a", "b"))]
```

### Mixed literals values (2)

```py
# error: "Object of type `Literal[1]` is not iterable"
# error: "Object of type `Literal[2]` is not iterable"
# error: "Object of type `Literal[4]` is not iterable"
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
# revealed: tuple[Unknown | Literal[3, 5], Unknown | Literal["a", "b"]]
[reveal_type((a, b)) for a, b in (1, 2, (3, "a"), 4, (5, "b"), "c")]
```

### Custom iterator (1)

```py
class Iterator:
    def __next__(self) -> tuple[int, int]:
        return (1, 2)

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

# revealed: tuple[int, int]
[reveal_type((a, b)) for a, b in Iterable()]
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
    # revealed: tuple[int | bytes, str | bytes]
    [reveal_type((a, b)) for a, b in arg]
```

## Empty

Unpacking an empty tuple or list shouldn't raise any diagnostics.

```py
[] = []
() = ()
[] = ()
() = []
```
