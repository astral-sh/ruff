# Narrowing for complex targets (attribute expressions, subscripts)

We support type narrowing for attributes and subscripts.

## Attribute narrowing

### Basic

```py
from ty_extensions._internal import Unknown

class C:
    x: int | None = None

c = C()

reveal_type(c.x)  # revealed: int | None

if c.x is not None:
    reveal_type(c.x)  # revealed: int
else:
    reveal_type(c.x)  # revealed: None

if c.x is not None:
    c.x = None

reveal_type(c.x)  # revealed: None

c = C()

if c.x is None:
    c.x = 1

reveal_type(c.x)  # revealed: int

class _:
    reveal_type(c.x)  # revealed: int

c = C()

class _:
    if c.x is None:
        c.x = 1
    reveal_type(c.x)  # revealed: int

# TODO: should be `int`
reveal_type(c.x)  # revealed: int | None

class D:
    x = None

def unknown() -> Unknown:
    return 1

d = D()
reveal_type(d.x)  # revealed: None | Unknown
d.x = 1
reveal_type(d.x)  # revealed: Literal[1]
d.x = unknown()
reveal_type(d.x)  # revealed: Unknown

class E:
    x: int | None = None

e = E()

if e.x is not None:
    class _:
        reveal_type(e.x)  # revealed: int
```

Narrowing can be "reset" by assigning to the attribute:

```py
c = C()

if c.x is None:
    reveal_type(c.x)  # revealed: None
    c.x = 1
    reveal_type(c.x)  # revealed: Literal[1]
    c.x = None
    reveal_type(c.x)  # revealed: None

reveal_type(c.x)  # revealed: int | None
```

Narrowing can also be "reset" by assigning to the object:

```py
c = C()

if c.x is None:
    reveal_type(c.x)  # revealed: None
    c = C()
    reveal_type(c.x)  # revealed: int | None

reveal_type(c.x)  # revealed: int | None
```

### Multiple predicates

```py
class C:
    value: str | None

def foo(c: C):
    # The truthiness check `c.value` narrows to `str & ~AlwaysFalsy`.
    # The subsequent `len(c.value)` doesn't narrow further since `str` is not narrowable by len().
    if c.value and len(c.value):  # error: [truthiness-test-of-none-union]
        reveal_type(c.value)  # revealed: str & ~AlwaysFalsy

    # error: [invalid-argument-type] "Argument to function `len` is incorrect: Expected `Sized`, found `str | None`"
    if len(c.value) and c.value:  # error: [truthiness-test-of-none-union]
        reveal_type(c.value)  # revealed: str & ~AlwaysFalsy

    if c.value is None or not len(c.value):
        reveal_type(c.value)  # revealed: str | None
    else:  # c.value is not None and len(c.value)
        # `c.value is not None` narrows to `str`, but `str` is not narrowable by len().
        reveal_type(c.value)  # revealed: str
```

### Generic class

```toml
[environment]
python-version = "3.12"
```

```py
class C[T]:
    x: T
    y: T

    def __init__(self, x: T):
        self.x = x
        self.y = x

def f(a: int | None):
    c = C(a)
    reveal_type(c.x)  # revealed: int | None
    reveal_type(c.y)  # revealed: int | None
    if c.x is not None:
        reveal_type(c.x)  # revealed: int
        # In this case, it may seem like we can narrow it down to `int`,
        # but different values ​​may be reassigned to `x` and `y` in another place.
        reveal_type(c.y)  # revealed: int | None

def g[T](c: C[T]):
    reveal_type(c.x)  # revealed: T@g
    reveal_type(c.y)  # revealed: T@g
    reveal_type(c)  # revealed: C[T@g]

    if isinstance(c.x, int):
        reveal_type(c.x)  # revealed: T@g & int
        reveal_type(c.y)  # revealed: T@g
        reveal_type(c)  # revealed: C[T@g]
    if isinstance(c.x, int) and isinstance(c.y, int):
        reveal_type(c.x)  # revealed: T@g & int
        reveal_type(c.y)  # revealed: T@g & int
        # TODO: Probably better if inferred as `C[T & int]` (mypy and pyright don't support this)
        reveal_type(c)  # revealed: C[T@g]
```

### With intermediate scopes

```py
class C:
    def __init__(self):
        self.x: int | None = None
        self.y: int | None = None

c = C()
reveal_type(c.x)  # revealed: int | None
if c.x is not None:
    reveal_type(c.x)  # revealed: int
    reveal_type(c.y)  # revealed: int | None

if c.x is not None:
    def _():
        reveal_type(c.x)  # revealed: int | None

def _():
    if c.x is not None:
        reveal_type(c.x)  # revealed: int
```

## Subscript narrowing

### Number subscript

```py
def _(t1: tuple[int | None, int | None], t2: tuple[int, int] | tuple[None, None]):
    if t1[0] is not None:
        reveal_type(t1[0])  # revealed: int
        reveal_type(t1[1])  # revealed: int | None

    n = 0
    if t1[n] is not None:
        # Narrowing the individual element type with a non-literal subscript is not supported
        reveal_type(t1[0])  # revealed: int | None
        reveal_type(t1[n])  # revealed: int | None
        reveal_type(t1[1])  # revealed: int | None

    # However, we can still discriminate between tuples in a union using a variable index:
    if t2[n] is not None:
        reveal_type(t2)  # revealed: tuple[int, int]

    if t2[0] is not None:
        reveal_type(t2)  # revealed: tuple[int, int]
        reveal_type(t2[0])  # revealed: int
        reveal_type(t2[1])  # revealed: int
    else:
        reveal_type(t2)  # revealed: tuple[None, None]
        reveal_type(t2[0])  # revealed: None
        reveal_type(t2[1])  # revealed: None

    if t2[0] is None:
        reveal_type(t2)  # revealed: tuple[None, None]
    else:
        reveal_type(t2)  # revealed: tuple[int, int]

    if (first := t2[0]) is not None:
        reveal_type(first)  # revealed: int
        reveal_type(t2)  # revealed: tuple[int, int]
    else:
        reveal_type(first)  # revealed: None
        reveal_type(t2)  # revealed: tuple[None, None]

def _(t3: tuple[int, str] | tuple[None, None] | tuple[bool, bytes]):
    # Narrow to tuples where first element is not None
    if t3[0] is not None:
        reveal_type(t3)  # revealed: tuple[int, str] | tuple[bool, bytes]

    # Narrow to tuples where first element is None
    if t3[0] is None:
        reveal_type(t3)  # revealed: tuple[None, None]

def _(t4: tuple[bool, int] | tuple[bool, str]):
    # Both tuples have bool at index 0, which is not disjoint from True,
    # so neither gets filtered out when checking `is True`
    if t4[0] is True:
        reveal_type(t4)  # revealed: tuple[bool, int] | tuple[bool, str]

def _(t5: tuple[int, None] | tuple[None, int]):
    # Narrow on second element (index 1)
    if t5[1] is not None:
        reveal_type(t5)  # revealed: tuple[None, int]
    else:
        reveal_type(t5)  # revealed: tuple[int, None]

    # Negative index
    if t5[-1] is None:
        reveal_type(t5)  # revealed: tuple[int, None]

def _(t6: tuple[int, ...] | tuple[None, None]):
    # Variadic tuple at index 0 has element type `int` (not a union),
    # so `tuple[None, None]` gets filtered out
    if t6[0] is not None:
        reveal_type(t6)  # revealed: tuple[int, ...]

def _(t6b: tuple[int, ...] | tuple[None, ...]):
    # Both variadic: `int` is disjoint from None, `None` is not disjoint from None
    if t6b[0] is not None:
        reveal_type(t6b)  # revealed: tuple[int, ...]
    else:
        reveal_type(t6b)  # revealed: tuple[None, ...]

def _(t7: tuple[int, int] | tuple[None, None]):
    # Index out of range for both tuples - no narrowing, but errors are emitted
    # error: [index-out-of-bounds] "Index 5 is out of bounds for tuple `tuple[int, int]` with length 2"
    # error: [index-out-of-bounds] "Index 5 is out of bounds for tuple `tuple[None, None]` with length 2"
    if t7[5] is not None:
        reveal_type(t7)  # revealed: tuple[int, int] | tuple[None, None]

def _(t8: tuple[int, int, int] | tuple[None, None]):
    # Index in range for first tuple but out of range for second
    # error: [index-out-of-bounds] "Index 2 is out of bounds for tuple `tuple[None, None]` with length 2"
    if t8[2] is not None:
        reveal_type(t8)  # revealed: tuple[int, int, int] | tuple[None, None]

def _(t9: tuple[int | None, str] | tuple[str, int]):
    # When the element type is a union (like `int | None`), we can't filter
    # out the tuple.
    if t9[0] is not None:
        reveal_type(t9)  # revealed: tuple[int | None, str] | tuple[str, int]
```

### Tagged unions of tuples (equality narrowing)

Narrow unions of tuples based on literal tag elements using `==` comparison:

```py
from typing import Literal

class A: ...
class B: ...
class C: ...

def _(x: tuple[Literal["tag1"], A] | tuple[Literal["tag2"], B, C]):
    if x[0] == "tag1":
        reveal_type(x)  # revealed: tuple[Literal["tag1"], A]
        reveal_type(x[1])  # revealed: A
    else:
        reveal_type(x)  # revealed: tuple[Literal["tag2"], B, C]
        reveal_type(x[1])  # revealed: B
        reveal_type(x[2])  # revealed: C

def _(x: tuple[Literal["tag1"], A] | tuple[Literal["tag2"], B, C]):
    if x[0] != "tag1":
        reveal_type(x)  # revealed: tuple[Literal["tag2"], B, C]
    else:
        reveal_type(x)  # revealed: tuple[Literal["tag1"], A]

def _(x: tuple[Literal["tag1"], A] | tuple[Literal["tag2"], B, C]):
    if (tag := x[0]) == "tag1":
        reveal_type(tag)  # revealed: Literal["tag1"]
        reveal_type(x)  # revealed: tuple[Literal["tag1"], A]
    else:
        reveal_type(tag)  # revealed: Literal["tag2"]
        reveal_type(x)  # revealed: tuple[Literal["tag2"], B, C]

# With int literals
def _(x: tuple[Literal[1], A] | tuple[Literal[2], B]):
    if x[0] == 1:
        reveal_type(x)  # revealed: tuple[Literal[1], A]
    else:
        reveal_type(x)  # revealed: tuple[Literal[2], B]

# With bytes literals
def _(x: tuple[Literal[b"a"], A] | tuple[Literal[b"b"], B]):
    if x[0] == b"a":
        reveal_type(x)  # revealed: tuple[Literal[b"a"], A]
    else:
        reveal_type(x)  # revealed: tuple[Literal[b"b"], B]

# Multiple tuple variants
def _(x: tuple[Literal["a"], A] | tuple[Literal["b"], B] | tuple[Literal["c"], C]):
    if x[0] == "a":
        reveal_type(x)  # revealed: tuple[Literal["a"], A]
    elif x[0] == "b":
        reveal_type(x)  # revealed: tuple[Literal["b"], B]
    else:
        reveal_type(x)  # revealed: tuple[Literal["c"], C]

# Using index 1 instead of 0
def _(x: tuple[A, Literal["tag1"]] | tuple[B, Literal["tag2"]]):
    if x[1] == "tag1":
        reveal_type(x)  # revealed: tuple[A, Literal["tag1"]]
    else:
        reveal_type(x)  # revealed: tuple[B, Literal["tag2"]]

# Works with reversed equality operands too.
def _(x: tuple[Literal["a"], A] | tuple[Literal["b"], B]):
    if "a" == x[0]:
        reveal_type(x)  # revealed: tuple[Literal["a"], A]
    else:
        reveal_type(x)  # revealed: tuple[Literal["b"], B]

# Works with reversed inequality operands too.
def _(x: tuple[Literal["a"], A] | tuple[Literal["b"], B]):
    if "a" != x[0]:
        reveal_type(x)  # revealed: tuple[Literal["b"], B]
    else:
        reveal_type(x)  # revealed: tuple[Literal["a"], A]
```

A tuple can have several literal tags. Matching a different tag rules out that tuple, while
excluding only one of its possible tags leaves it in the union:

```py
def multiple_tags(x: tuple[Literal["a"], int] | tuple[Literal["b", "c"], str]):
    if "a" == x[0]:
        reveal_type(x)  # revealed: tuple[Literal["a"], int]
    else:
        reveal_type(x)  # revealed: tuple[Literal["b", "c"], str]

    if x[0] != "b":
        reveal_type(x)  # revealed: tuple[Literal["a"], int] | tuple[Literal["b", "c"], str]
    else:
        reveal_type(x)  # revealed: tuple[Literal["b", "c"], str]
```

Enum literals are supported as tuple tags, including `IntEnum` literals:

```py
from enum import Enum, IntEnum
from typing import Literal

class Tag(Enum):
    A = 1
    B = 2

def _(x: tuple[Literal[Tag.A], int] | tuple[Literal[Tag.B], str]):
    if x[0] == Tag.A:
        reveal_type(x)  # revealed: tuple[Literal[Tag.A], int]
    else:
        reveal_type(x)  # revealed: tuple[Literal[Tag.B], str]

class IntTag(IntEnum):
    A = 1
    B = 2

def _(x: tuple[Literal[IntTag.A], int] | tuple[Literal[IntTag.B], str]):
    if x[0] == IntTag.A:
        reveal_type(x)  # revealed: tuple[Literal[IntTag.A], int]
    else:
        reveal_type(x)  # revealed: tuple[Literal[IntTag.B], str]
```

An `IntEnum` member compares equal to its integer value. A tuple whose tags are `IntTag.A` or `1`
therefore always matches `1`, and is excluded from the other branch:

```py
def enum_tag_equal_to_integer(
    x: tuple[Literal[IntTag.A, 1], int] | tuple[Literal[1], str] | tuple[Literal[2], bytes],
):
    if x[0] == 1:
        reveal_type(x)  # revealed: tuple[Literal[IntTag.A, 1], int] | tuple[Literal[1], str]
    else:
        reveal_type(x)  # revealed: tuple[Literal[2], bytes]
```

An enum can customize `__ne__` independently of `__eq__`. An ambiguous inequality keeps tuples whose
tag is that enum member in both branches, even when its literal type differs from the comparison
value:

```py
class NeverUnequal(Enum):
    A = 1
    B = 2

    def __ne__(self, other: object) -> bool:
        return False

def custom_inequality(
    x: tuple[Literal[NeverUnequal.A], int] | tuple[Literal["a"], str] | tuple[Literal["b"], bytes],
):
    if "a" != x[0]:
        reveal_type(x)  # revealed: tuple[Literal[NeverUnequal.A], int] | tuple[Literal["b"], bytes]
    else:
        reveal_type(x)  # revealed: tuple[Literal[NeverUnequal.A], int] | tuple[Literal["a"], str]
```

An ambiguous tag keeps its tuple in both branches. Other tuples can still be excluded when their
literal tags make the comparison always true or always false:

```py
def _(x: tuple[Literal["tag1"], A] | tuple[str, B] | tuple[Literal["tag2"], C]):
    if x[0] == "tag1":
        reveal_type(x)  # revealed: tuple[Literal["tag1"], A] | tuple[str, B]
    else:
        reveal_type(x)  # revealed: tuple[str, B] | tuple[Literal["tag2"], C]
```

This also applies when a tag is a union of literal and non-literal types. The non-literal
alternative can compare equal to the tag being checked:

```py
class MatchesAnything:
    def __eq__(self, other: object) -> bool:
        return True

def nonliteral_tag_union(
    x: tuple[Literal["a"], int] | tuple[Literal["b"] | MatchesAnything, str] | tuple[Literal["c"], bytes],
):
    if x[0] == "a":
        reveal_type(x)  # revealed: tuple[Literal["a"], int] | tuple[Literal["b"] | MatchesAnything, str]
    else:
        reveal_type(x)  # revealed: tuple[Literal["b"] | MatchesAnything, str] | tuple[Literal["c"], bytes]
```

An `int` tag can contain a subclass with custom equality, so it can match a string literal. This
preserves the tuple with that tag without preventing narrowing of the literal tags:

```py
def integer_tag(x: tuple[int, A] | tuple[Literal["a"], B] | tuple[Literal["b"], C]):
    if x[0] == "a":
        reveal_type(x)  # revealed: tuple[int, A] | tuple[Literal["a"], B]
    else:
        reveal_type(x)  # revealed: tuple[int, A] | tuple[Literal["b"], C]
```

If the index is out of bounds for any tuple in the union, we also skip narrowing (a diagnostic will
be emitted elsewhere for the out-of-bounds access):

```py
def _(x: tuple[A, Literal["a"]] | tuple[B]):
    # error: [index-out-of-bounds]
    if x[1] == "a":
        # Can't narrow because index 1 is out of bounds for second tuple
        reveal_type(x)  # revealed: tuple[A, Literal["a"]] | tuple[B]
    else:
        reveal_type(x)  # revealed: tuple[A, Literal["a"]] | tuple[B]
```

We can still narrow tuples when non-tuple types are present in the union:

```py
def _(x: tuple[Literal["tag1"], A] | tuple[Literal["tag2"], B] | list[int]):
    if x[0] == "tag1":
        # A list of ints could have int subclasses in it,
        # and int subclasses could have custom `__eq__` methods such that they
        # compare equal to `"tag1"`, so `list[int]` cannot be narrowed out of this
        # union.
        reveal_type(x)  # revealed: tuple[Literal["tag1"], A] | list[int]
```

### Tuple unions and truthiness

A truthiness check on one tuple element can narrow the other elements.

```py
from typing import Literal

def truthy(value: tuple[Literal[True], int] | tuple[Literal[False], str]):
    if value[0]:
        reveal_type(value[1])  # revealed: int
    else:
        reveal_type(value[1])  # revealed: str
```

Negating the check selects the tuple with the falsy element. Negative indices work too:

```py
def negated(value: tuple[Literal[True], int] | tuple[Literal[False], str]):
    if not value[-2]:
        reveal_type(value[1])  # revealed: str
```

Calling `bool` on the element has the same effect as checking its truthiness directly:

```py
def explicit_bool(value: tuple[Literal[True], int] | tuple[Literal[False], str]):
    if bool(value[0]):
        reveal_type(value[1])  # revealed: int
```

The checked element can also be a union of literals that are all truthy or all falsy:

```py
def literal_tags(value: tuple[Literal[0, ""], str] | tuple[Literal[1, "x"], int]):
    if value[0]:
        reveal_type(value[1])  # revealed: int
    else:
        reveal_type(value[1])  # revealed: str
```

A `bool` element can be either true or false, so its tuple remains possible in both branches:

```py
def ambiguous(value: tuple[bool, int] | tuple[Literal[False], str]):
    if value[0]:
        reveal_type(value)  # revealed: tuple[bool, int]
    else:
        reveal_type(value)  # revealed: tuple[bool, int] | tuple[Literal[False], str]
```

### Tuple unions and instance checks

An `isinstance` check on one element narrows the other elements in both branches.

```py
def instance(value: tuple[int, str] | tuple[str, int]):
    if isinstance(value[0], str):
        reveal_type(value[1])  # revealed: int
    else:
        reveal_type(value[1])  # revealed: str
```

An `int | str` element can pass or fail the `str` check, so its tuple remains possible in both
branches:

```py
def overlapping(value: tuple[int | str, bytes] | tuple[str, int]):
    if isinstance(value[0], str):
        reveal_type(value)  # revealed: tuple[int | str, bytes] | tuple[str, int]
    else:
        reveal_type(value)  # revealed: tuple[int | str, bytes]
```

### Tuple unions and subclass checks

Subclass checks also narrow the tuple containing the checked class.

```py
def subclass(value: tuple[type[int], str] | tuple[type[str], int]):
    if issubclass(value[0], str):
        reveal_type(value[1])  # revealed: int
    else:
        reveal_type(value[1])  # revealed: str
```

### Tuple unions and `TypeIs`

A user-defined `TypeIs` check narrows the tuple containing its argument.

```py
from typing_extensions import TypeIs

def is_string(value: object) -> TypeIs[str]:
    return isinstance(value, str)

def check(value: tuple[int, str] | tuple[str, int]):
    if is_string(value[0]):
        reveal_type(value[1])  # revealed: int
    else:
        reveal_type(value[1])  # revealed: str
```

The checked element can be passed by keyword:

```py
def keyword(value: tuple[int, str] | tuple[str, int]):
    if is_string(value=value[0]):
        reveal_type(value[1])  # revealed: int
```

### Tuple unions and `TypeGuard`

`TypeGuard` can widen its argument's type, so it does not eliminate tuples from the union.

```py
from typing_extensions import TypeGuard

def guard_string(value: object) -> TypeGuard[str]:
    return isinstance(value, str)

def check(value: tuple[int, str] | tuple[str, int]):
    if guard_string(value[0]):
        reveal_type(value)  # revealed: tuple[int, str] | tuple[str, int]
```

### Tuple unions with `Any` elements

An `Any` element can pass or fail a truthiness or `isinstance` check, so its tuple remains possible
in both branches.

```py
from typing import Any, Literal

def check(value: tuple[Any, bytes] | tuple[Literal[False], str]):
    if value[0]:
        reveal_type(value)  # revealed: tuple[Any, bytes]
    else:
        reveal_type(value)  # revealed: tuple[Any, bytes] | tuple[Literal[False], str]
    if isinstance(value[0], str):
        reveal_type(value)  # revealed: tuple[Any, bytes]
    else:
        reveal_type(value)  # revealed: tuple[Any, bytes] | tuple[Literal[False], str]
```

### Tuple unions with other types

The check can rule out a tuple while leaving other types in the union. It does not narrow those
other types.

```py
def check(value: list[str] | tuple[int, int]):
    if isinstance(value[0], str):
        reveal_type(value)  # revealed: list[str]
    else:
        reveal_type(value)  # revealed: list[str] | tuple[int, int]
```

### Tuple unions with variable-length tuples

`tuple[str, ...]` supplies a string at every valid index, so the check can distinguish it from
`tuple[int, int]`.

```py
def check(value: tuple[str, ...] | tuple[int, int]):
    if isinstance(value[0], str):
        reveal_type(value)  # revealed: tuple[str, ...]
    else:
        reveal_type(value)  # revealed: tuple[int, int]
```

### Tuple unions with unknown indices

When the index is not known, the check cannot identify which tuple is present.

```py
from typing import Literal

def check(value: tuple[Literal[False], str] | tuple[str, Literal[False]], index: int):
    if value[index]:
        reveal_type(value)  # revealed: tuple[Literal[False], str] | tuple[str, Literal[False]]
    if isinstance(value[index], str):
        reveal_type(value)  # revealed: tuple[Literal[False], str] | tuple[str, Literal[False]]
```

### Tuple unions with out-of-bounds indices

If the index is out of bounds for any tuple in the union, we report an error and leave the union
unchanged.

```py
def check(value: tuple[int, str] | tuple[str]):
    # error: [index-out-of-bounds]
    if isinstance(value[1], str):
        reveal_type(value)  # revealed: tuple[int, str] | tuple[str]
    # error: [index-out-of-bounds]
    if value[1]:
        reveal_type(value)  # revealed: tuple[int, str] | tuple[str]
```

### Tuple checks that assign to another name

Assigning the checked element to another name leaves the tuple unchanged, but currently prevents
narrowing the tuple.

```py
def element(value: tuple[str, int] | tuple[int, str]):
    if isinstance(item := value[0], str):
        # TODO: Narrow to `int`.
        reveal_type(value[1])  # revealed: int | str
```

### Tuple checks that reassign the tuple

Assigning a checked element back to the tuple's name replaces the tuple. The check narrows the
assigned element without restoring the old tuple type.

```py
def instance(flag: bool):
    value = ("x",) if flag else (1,)
    if isinstance(value := value[0], str):
        reveal_type(value)  # revealed: Literal["x"]
    else:
        reveal_type(value)  # revealed: Literal[1]
```

The same applies to user-defined `TypeIs` checks:

```py
from typing_extensions import TypeIs

def is_string(value: object) -> TypeIs[str]:
    return isinstance(value, str)

def check(flag: bool):
    value = ("x",) if flag else (1,)
    if is_string(value := value[0]):
        reveal_type(value)  # revealed: Literal["x"]
    else:
        reveal_type(value)  # revealed: Literal[1]
```

### Tuple reassignment in an index

Python reads the tuple before evaluating its index. The index expression below then replaces `value`
with `(False,)`. The truthiness check uses the original tuple and does not narrow the replacement.

```py
from typing import Literal

def check(value: tuple[Literal[True]] | tuple[Literal[False]]):
    if value[(value := (False,)) and 0]:
        reveal_type(value)  # revealed: tuple[Literal[False]]
```

### Tuple checks with assignments in later arguments

An assignment in a later argument can replace the tuple after its element has been read. The check
does not narrow the replacement value.

```py
from typing_extensions import TypeIs

def is_string(value: object, other: object) -> TypeIs[str]:
    return isinstance(value, str)

def positional(flag: bool):
    value = ("x",) if flag else (1,)
    if is_string(value[0], value := 0):
        reveal_type(value)  # revealed: Literal[0]
    else:
        reveal_type(value)  # revealed: Literal[0]
```

This also applies when the later argument is passed by keyword:

```py
def keyword(flag: bool):
    value = ("x",) if flag else (1,)
    if is_string(value=value[0], other=(value := 0)):
        reveal_type(value)  # revealed: Literal[0]
```

An assignment expression in a comprehension also replaces the tuple, because it assigns to the name
in the containing function:

```py
def comprehension(flag: bool):
    value = ("x",) if flag else (1,)
    if is_string(value[0], [(value := 0) for _ in (0,)]):
        reveal_type(value)  # revealed: int
```

Replacing an object prevents narrowing its tuple attribute based on a check of the old object:

```py
class Container:
    value: tuple[str, int] | tuple[int, str] = (1, "new")

def attribute(container: Container):
    if is_string(container.value[0], container := Container()):
        reveal_type(container.value)  # revealed: tuple[str, int] | tuple[int, str]
```

### Tuple tags with non-literal comparators

A boolean comparison value can match either boolean tag value, but cannot match a string literal:

```py
from typing import Literal

def boolean_comparator(value: tuple[bool, int] | tuple[Literal["other"], str], other: bool):
    if value[0] == other:
        reveal_type(value)  # revealed: tuple[bool, int]
    else:
        reveal_type(value)  # revealed: tuple[bool, int] | tuple[Literal["other"], str]
```

When the comparison value is a union of literals, a matching tuple can have any of those tags. An
unequal comparison can retain every tuple because the comparison value is not fixed:

```py
def union_comparator(
    value: tuple[Literal["a"], int] | tuple[Literal["b"], str] | tuple[Literal["c"], bytes],
    other: Literal["a", "b"],
):
    if other != value[0]:
        reveal_type(value)  # revealed: tuple[Literal["a"], int] | tuple[Literal["b"], str] | tuple[Literal["c"], bytes]
    else:
        reveal_type(value)  # revealed: tuple[Literal["a"], int] | tuple[Literal["b"], str]
```

An intersection can restrict an enum comparison value to some of its members. A tuple with the whole
enum as its tag remains possible, while an excluded member cannot match:

```py
from enum import Enum
from ty_extensions import Intersection, Not

class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

def intersection_comparator(
    value: tuple[Literal[Color.RED], int] | tuple[Color, str] | tuple[Literal[Color.GREEN], bytes],
    other: Intersection[Color, Not[Literal[Color.RED]]],
):
    if value[0] == other:
        reveal_type(value)  # revealed: tuple[Color, str] | tuple[Literal[Color.GREEN], bytes]
    else:
        reveal_type(value)  # revealed: tuple[Literal[Color.RED], int] | tuple[Color, str] | tuple[Literal[Color.GREEN], bytes]
```

### PEP 695 type aliases

Tuple narrowing also works when the union is defined via a PEP 695 type alias:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

class A: ...
class B: ...

type TaggedTuple = tuple[Literal["a"], A] | tuple[Literal["b"], B]

def test_equality_narrowing(x: TaggedTuple):
    if x[0] == "a":
        reveal_type(x)  # revealed: tuple[Literal["a"], A]
    else:
        reveal_type(x)  # revealed: tuple[Literal["b"], B]

type NullableTuple = tuple[int, int] | tuple[None, None]

def test_is_narrowing(t: NullableTuple):
    if t[0] is not None:
        reveal_type(t)  # revealed: tuple[int, int]
    else:
        reveal_type(t)  # revealed: tuple[None, None]

# Nested type aliases (an alias referring to another alias) also work:
type InnerTagged = tuple[Literal["a"], A] | tuple[Literal["b"], B]
type OuterTagged = InnerTagged

def test_nested_equality_narrowing(x: OuterTagged):
    if x[0] == "a":
        reveal_type(x)  # revealed: tuple[Literal["a"], A]
    else:
        reveal_type(x)  # revealed: tuple[Literal["b"], B]

type InnerNullable = tuple[int, int] | tuple[None, None]
type OuterNullable = InnerNullable

def test_nested_is_narrowing(t: OuterNullable):
    if t[0] is not None:
        reveal_type(t)  # revealed: tuple[int, int]
    else:
        reveal_type(t)  # revealed: tuple[None, None]
```

### String subscript

```py
def _(d: dict[str, str | None]):
    if d["a"] is not None:
        reveal_type(d["a"])  # revealed: str
        reveal_type(d["b"])  # revealed: str | None
```

## Combined attribute and subscript narrowing

```py
class C:
    def __init__(self):
        self.x: tuple[int | None, int | None] = (None, None)

class D:
    def __init__(self):
        self.c: tuple[C] | None = None

d = D()
if d.c is not None and d.c[0].x[0] is not None:
    reveal_type(d.c[0].x[0])  # revealed: int
```

## Narrowing with negative subscripts

Narrowing should work with negative subscripts like `x[-1]`:

```py
def _(x: list[int | None]):
    if x[-1] is not None:
        reveal_type(x[-1])  # revealed: int

def _(x: list[str | None]):
    if x[-1] is None:
        reveal_type(x[-1])  # revealed: None
    else:
        reveal_type(x[-1])  # revealed: str
```

Nested negative subscripts should also work:

```py
def _(x: list[list[int | None]]):
    if x[-1][-1] is not None:
        reveal_type(x[-1][-1])  # revealed: int
```

Mixed positive and negative subscripts:

```py
def _(x: list[list[int | None]]):
    if x[0][-1] is not None:
        reveal_type(x[0][-1])  # revealed: int

    if x[-1][0] is not None:
        reveal_type(x[-1][0])  # revealed: int
```

Attribute access combined with negative subscripts:

```py
class Container:
    items: list[int | None]

def _(c: Container):
    if c.items[-1] is not None:
        reveal_type(c.items[-1])  # revealed: int
```

Multiple conditions in an `and` chain:

```py
def _(x: list[int | None]):
    # Narrowing should persist through `and` chains
    if x[-1] is not None and x[-1] > 0:
        reveal_type(x[-1])  # revealed: int
```

Negative indices with tuples:

```py
def _(t: tuple[int, str, None] | tuple[None, None, int]):
    if t[-1] is not None:
        reveal_type(t)  # revealed: tuple[None, None, int]
    else:
        reveal_type(t)  # revealed: tuple[int, str, None]

    if t[-3] is not None:
        reveal_type(t)  # revealed: tuple[int, str, None]
```

## Narrowing with explicit positive subscripts

Narrowing should work with explicit positive subscripts like `x[+1]`:

```py
def _(x: list[int | None]):
    if x[+0] is not None:
        reveal_type(x[+0])  # revealed: int

    if x[+1] is not None:
        reveal_type(x[+1])  # revealed: int
```

## Narrowing with boolean subscripts

Narrowing should work with boolean subscripts like `x[True]` and `x[False]`. We treat `bool`
subscripts the same as `int` subscripts because `True` always has the same hash and index value as
`1`, and `False` always has the same hash and index value as `0`:

```py
def _(x: tuple[object, object]):
    if isinstance(x[True], str):
        reveal_type(x[True])  # revealed: str
        reveal_type(x[1])  # revealed: str

def _(x: list[int | None]):
    # x[True] is equivalent to x[1]
    if x[True] is not None:
        reveal_type(x[True])  # revealed: int

    # x[False] is equivalent to x[0]
    if x[False] is not None:
        reveal_type(x[False])  # revealed: int
```

Combined with other subscript types:

```py
def _(x: list[list[int | None]]):
    if x[True][-1] is not None:
        reveal_type(x[True][-1])  # revealed: int

    if x[False][0] is not None:
        reveal_type(x[False][0])  # revealed: int
```

## Narrowing with bytes literal subscripts

Narrowing should work with bytes literal subscripts like `x[b"key"]`:

```py
def _(d: dict[bytes, str | None]):
    if d[b"key"] is not None:
        reveal_type(d[b"key"])  # revealed: str
        reveal_type(d[b"other"])  # revealed: str | None
```

Combined with attribute access:

```py
class Container:
    data: dict[bytes, int | None]

def _(c: Container):
    if c.data[b"key"] is not None:
        reveal_type(c.data[b"key"])  # revealed: int
```
