# Unpacking

If there are not enough or too many values when unpacking, an error will occur and the types of all
variables (if nested tuple unpacking fails, only the variables within the failed tuples) is inferred
to be `Unknown`.

## Tuple

### Simple tuple

```py
a, b, c = (1, 2, 3)
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
a, (b, c), d = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: Literal[3]
reveal_type(d)  # revealed: Literal[4]
```

### Nested tuple without unpacking

```py
a, b, c = (1, (2, 3), 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: tuple[Literal[2], Literal[3]]
reveal_type(c)  # revealed: Literal[4]
```

### Uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 3"
a, b, c = (1, 2)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
```

### Uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
a, b = (1, 2, 3)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Nested uneven unpacking (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
a, (b, c), d = (1, (2,), 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Literal[3]
```

### Nested uneven unpacking (2)

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
a, (b, c), d = (1, (2, 3, 4), 5)
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
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Literal[2]
```

### Starred expression (3)

```py
[a, *b, c] = (1, 2, 3)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: list[int]
reveal_type(c)  # revealed: Literal[3]
```

### Starred expression (4)

```py
[a, *b, c, d] = (1, 2, 3, 4, 5, 6)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: list[int]
reveal_type(c)  # revealed: Literal[5]
reveal_type(d)  # revealed: Literal[6]
```

### Starred expression (5)

```py
[a, b, *c] = (1, 2, 3, 4)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
reveal_type(c)  # revealed: list[int]
```

### Starred expression (6)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 5"
a, b, c, *d, e, f = (1,)
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: list[Unknown]
reveal_type(e)  # revealed: Unknown
reveal_type(f)  # revealed: Unknown
```

### Starred unpacking of a large tuple

For performance, ty widens inferred integer literal types to `int` in tuples with more than 64
elements. Unpacking preserves that widening: this unannotated assignment infers `int` for `first`
and `list[int]` for `rest`, including when the elements come from a list literal expansion. Widening
also applies inside nested tuple elements. Unpacking the small tuple `(0, (1,), 2)` instead infers
`Literal[0]` and `Literal[1]` for the fixed targets.

```py
# fmt: off
first, (second,), *rest = (*[
    0, (1,), 2, 3, 4, 5, 6, 7, 8, 9,
    10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
    30, 31, 32, 33, 34, 35, 36, 37, 38, 39,
    40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59,
    60, 61, 62, 63, 64,
],)
# fmt: on
reveal_type(first)  # revealed: int
reveal_type(second)  # revealed: int
reveal_type(rest)  # revealed: list[int]
```

### Non-iterable unpacking

```py
# error: "Object of type `Literal[1]` is not iterable"
a, b = 1
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Non-name unpacking target

```py
# error: [not-iterable] "Object of type `Literal[1]` is not iterable"
# error: [invalid-assignment] "Cannot assign to a subscript on an object of type `Literal[1]`"
(1[0],) = 1
```

### Custom iterator unpacking

```py
class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

a, b = Iterable()
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

a, (b, c), d = (1, Iterable(), 2)
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: int
reveal_type(c)  # revealed: int
reveal_type(d)  # revealed: Literal[2]
```

## List

### Literal unpacking

```py
a, b = [1, 2]
reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[2]
```

### Too few values in a list literal

A list literal without starred elements has a known length. If it cannot fill all targets, we report
the mismatch and infer `Unknown` for those targets, as with a tuple literal.

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
first, last = [1]
reveal_type(first)  # revealed: Unknown
reveal_type(last)  # revealed: Unknown
```

A starred target does not reduce the number of elements required by the fixed targets. On a length
mismatch, its element type is also unknown.

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 2"
first, *rest, last = [1]
reveal_type(first)  # revealed: Unknown
reveal_type(rest)  # revealed: list[Unknown]
reveal_type(last)  # revealed: Unknown
```

### Too many values in a list literal

Without a starred target, every element needs a corresponding target. Extra elements cause an error
and leave all targets with unknown types.

```py
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
first, last = [1, 2, 3]
reveal_type(first)  # revealed: Unknown
reveal_type(last)  # revealed: Unknown
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

## List and tuple literals

### Starred targets

Unpacking a list literal assigns each element to its corresponding target.

```py
first: int
first, *rest = [1, "wrong"]
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[str]
```

The starred target can also precede the fixed targets:

```py
*rest, last = ["one", "two", 3]
reveal_type(rest)  # revealed: list[str]
reveal_type(last)  # revealed: Literal[3]
```

A starred target between fixed targets excludes both the prefix and the suffix from its element
type:

```py
[first, *rest, last] = [1, "two", "three", 4]
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[str]
reveal_type(last)  # revealed: Literal[4]
```

### Empty starred targets

When the fixed targets consume every element, the starred target receives an empty list. As with an
empty list literal, the element type is unknown, allowing values to be added later.

```py
first, *rest, last = [1, 2]
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[Unknown]
reveal_type(last)  # revealed: Literal[2]
rest.append(3)

(*empty,) = []
reveal_type(empty)  # revealed: list[Unknown]
```

### Nested list literals

Element positions are preserved when list literals are nested inside other list or tuple literals.

```py
(first, *rest), *outer_rest, (last,) = [[1, "two"], False, [3]]
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[str]
reveal_type(outer_rest)  # revealed: list[bool]
reveal_type(last)  # revealed: Literal[3]
```

The same nested lists retain their element positions when the outer literal is a tuple.

```py
(first, *rest), *outer_rest, (last,) = ([1, "two"], False, [3])
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[str]
reveal_type(outer_rest)  # revealed: list[bool]
reveal_type(last)  # revealed: Literal[3]
```

Unpacking another iterable alongside a list literal does not affect the literal's element types.

```py
def nested(values: list[int]):
    (first, *rest), (other,) = ([1, "two"], values)
    reveal_type(first)  # revealed: Literal[1]
    reveal_type(rest)  # revealed: list[str]
    reveal_type(other)  # revealed: int
```

If a nested list has too few elements, only the targets unpacked from that list get unknown types.
The sibling target retains its corresponding element's type.

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 2"
(first, *rest, last), other = [[1], 2]
reveal_type(first)  # revealed: Unknown
reveal_type(rest)  # revealed: list[Unknown]
reveal_type(last)  # revealed: Unknown
reveal_type(other)  # revealed: Literal[2]
```

A starred outer target does not hide a length mismatch inside either kind of literal.

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
(first, last), *rest = ([1],)
reveal_type(first)  # revealed: Unknown
reveal_type(last)  # revealed: Unknown

# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
(first, last), *rest = [[1]]
reveal_type(first)  # revealed: Unknown
reveal_type(last)  # revealed: Unknown
```

### Incompatible targets

An incompatible element still causes an error for its corresponding target.

```py
first: int
# error: [invalid-assignment] "Object of type `Literal["wrong"]` is not assignable to `int`"
first, *rest = ["wrong", 1]
reveal_type(rest)  # revealed: list[int]
```

The starred target is checked against the list of collected elements.

```py
numbers: list[int]
# error: [invalid-assignment] "Object of type `list[str]` is not assignable to `list[int]`"
first, *numbers = [1, "wrong"]
```

### Capture-list inference

A starred target receives a new list. Inferred literal element types are promoted, as in a list
literal, so additional values of the same type can be appended.

```py
first, *rest = [1, "two"]
rest.append("three")
reveal_type(rest)  # revealed: list[str]

first, *rest = (1, "two")
rest.append("three")
reveal_type(rest)  # revealed: list[str]

first, *rest = (1,)
rest.append("three")
reveal_type(rest)  # revealed: list[Unknown]
```

The collected elements are also compatible with an explicitly annotated list.

```py
strings: list[str]
first, *strings = [1, "two"]
first, *strings = [1]
```

Singleton values follow the same inference rules as in a list literal.

```py
optional: list[int | None]
first, *optional = [1, None]
first, *optional = (1, None)
```

Explicit literal annotations are preserved when constructing the collected list.

```py
from typing import Literal

def explicit_literal(value: Literal["one", "two"]):
    first, *rest = [1, value]
    reveal_type(rest)  # revealed: list[Literal["one", "two"]]
    first, *rest = (1, value)
    reveal_type(rest)  # revealed: list[Literal["one", "two"]]
```

### Collected tuple elements

Homogeneous tuple literals of different lengths are promoted to a variable-length tuple element
type, as in an ordinary list literal.

```py
rest: list[tuple[int, ...]]
first, *rest = [(1,), (2,), (3, 4)]
reveal_type(first)  # revealed: tuple[Literal[1]]
reveal_type(rest)  # revealed: list[tuple[int, ...]]

first, *rest = ((1,), (2,), (3, 4))
reveal_type(first)  # revealed: tuple[Literal[1]]
reveal_type(rest)  # revealed: list[tuple[int, ...]]
```

A tuple from a variable retains its annotated shape and prevents tuple-size promotion for the
collected elements.

```py
def annotated_tuple(value: tuple[int, int]):
    first, *rest = [0, (1,), value]
    reveal_type(rest)  # revealed: list[tuple[int] | tuple[int, int]]
    first, *rest = (0, (1,), value)
    reveal_type(rest)  # revealed: list[tuple[int] | tuple[int, int]]
```

Tuple literals collected between multiple expansions remain eligible for tuple-size promotion.

```py
def expanded_tuples(values: list[str]):
    first, *rest, last = (0, *values, (1,), *values, (2, 3), False)
    reveal_type(first)  # revealed: Literal[0]
    reveal_type(rest)  # revealed: list[str | tuple[int, ...]]
    reveal_type(last)  # revealed: Literal[False]

    first, *rest, last = [0, *values, (1,), *values, (2, 3), False]
    reveal_type(first)  # revealed: Literal[0]
    reveal_type(rest)  # revealed: list[str | tuple[int, ...]]
    reveal_type(last)  # revealed: Literal[False]
```

### Starred expressions on the right-hand side

A starred element can contribute an unknown number of values. The literal's AST length does not
determine whether it can fill the targets.

```py
def unpack(values: list[int]):
    first, *rest, last = [*values]
    reveal_type(first)  # revealed: int
    reveal_type(rest)  # revealed: list[int]
    reveal_type(last)  # revealed: int
```

Known elements before and after a starred expression keep their positions, for both tuple and list
literals. Only the starred target collects the elements supplied by `values`.

```py
def fixed_ends(values: list[str]):
    first: int
    first, *rest, last = (1, *values, 2)
    reveal_type(first)  # revealed: Literal[1]
    reveal_type(rest)  # revealed: list[str]
    reveal_type(last)  # revealed: Literal[2]

    first, *rest, last = [1, *values, 2]
    reveal_type(first)  # revealed: Literal[1]
    reveal_type(rest)  # revealed: list[str]
    reveal_type(last)  # revealed: Literal[2]
```

### Ambiguous positions around an expansion

When an expansion may be empty, a fixed target can receive either one of its elements or a value
from the other side of the expansion. We combine those possibilities without losing the types of the
unambiguous targets.

```py
def ambiguous(values: list[str]):
    first, second, *rest = (0, *values, 1)
    reveal_type(first)  # revealed: Literal[0]
    reveal_type(second)  # revealed: str | Literal[1]
    reveal_type(rest)  # revealed: list[str | int]

    first, second, *rest = [0, *values, 1]
    reveal_type(first)  # revealed: Literal[0]
    reveal_type(second)  # revealed: str | Literal[1]
    reveal_type(rest)  # revealed: list[str | int]
```

### Unpacking literal expansions

Expanding a literal preserves its elements and length. These assignments fail even though their
right-hand sides contain starred expressions.

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 2"
first, *rest, last = (*(1,),)
reveal_type(first)  # revealed: Unknown
reveal_type(rest)  # revealed: list[Unknown]
reveal_type(last)  # revealed: Unknown

# error: [invalid-assignment] "Not enough values to unpack: Expected at least 2"
first, *rest, last = [*[1]]
reveal_type(first)  # revealed: Unknown
reveal_type(rest)  # revealed: list[Unknown]
reveal_type(last)  # revealed: Unknown
```

A dictionary literal with a single key supplies one element.

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 2"
first, *rest, last = (*{"key": 1},)
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 2"
first, *rest, last = [*{"key": 1}]
```

### Unpacking a named expression

A named expression preserves the structure of its value when unpacked immediately. The bound list
itself still has an ordinary list type.

```py
first: int
first, *rest = (items := (1, "two"))
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[str]

first, *rest = (items := [1, "two"])
reveal_type(first)  # revealed: Literal[1]
reveal_type(rest)  # revealed: list[str]
reveal_type(items)  # revealed: list[int | str]
```

### Aliases in unpacked values

Collected lists retain aliases in their element types.

```toml
[environment]
python-version = "3.12"
```

```py
type Element = int | str

def aliases(value: Element):
    first, *rest = (value, value)
    reveal_type(first)  # revealed: int | str
    reveal_type(rest)  # revealed: list[Element]

    first, *rest = [value, value]
    reveal_type(first)  # revealed: int | str
    reveal_type(rest)  # revealed: list[Element]
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
    reveal_type(r)  # revealed: list[Unknown]

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
    reveal_type(o)  # revealed: I1 | I2
    reveal_type(p)  # revealed: list[I1 | I2]

    [o, p, q, *r] = x
    reveal_type(o)  # revealed: I0
    reveal_type(p)  # revealed: I1 | I2
    reveal_type(q)  # revealed: I1 | I2
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
a, *b, c, d = "ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
a, b, *c, d = "a"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: list[Unknown]
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
a, *b, c = "ab"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Literal["b"]
```

### Starred expression (3)

```py
a, *b, c = "abc"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: list[str]
reveal_type(c)  # revealed: Literal["c"]
```

### Starred expression (4)

```py
a, *b, c, d = "abcdef"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: list[str]
reveal_type(c)  # revealed: Literal["e"]
reveal_type(d)  # revealed: Literal["f"]
```

### Starred expression (5)

```py
a, b, *c = "abcd"
reveal_type(a)  # revealed: Literal["a"]
reveal_type(b)  # revealed: Literal["b"]
reveal_type(c)  # revealed: list[str]
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
a, b = "é"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Unicode escape (1)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
a, b = "\u9e6c"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Unicode escape (2)

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected 2"
a, b = "\U0010ffff"

reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
```

### Surrogates

```py
a, b = "\ud800\udfff"

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
a, *b, c, d = b"ab"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Unknown
reveal_type(d)  # revealed: Unknown
```

```py
# error: [invalid-assignment] "Not enough values to unpack: Expected at least 3"
a, b, *c, d = b"a"
reveal_type(a)  # revealed: Unknown
reveal_type(b)  # revealed: Unknown
reveal_type(c)  # revealed: list[Unknown]
reveal_type(d)  # revealed: Unknown
```

### Starred expression (2)

```py
a, *b, c = b"ab"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: list[Unknown]
reveal_type(c)  # revealed: Literal[98]
```

### Starred expression (3)

```py
a, *b, c = b"abc"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: list[int]
reveal_type(c)  # revealed: Literal[99]
```

### Starred expression (4)

```py
a, *b, c, d = b"abcdef"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: list[int]
reveal_type(c)  # revealed: Literal[101]
reveal_type(d)  # revealed: Literal[102]
```

### Starred expression (5)

```py
a, b, *c = b"abcd"
reveal_type(a)  # revealed: Literal[97]
reveal_type(b)  # revealed: Literal[98]
reveal_type(c)  # revealed: list[int]
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
    a, b = arg
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

(a, b), c = Iterable()
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
