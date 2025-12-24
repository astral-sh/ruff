# Unions in calls

## Union of return types

```py
def _(flag: bool):
    if flag:
        def f() -> int:
            return 1
    else:
        def f() -> str:
            return "foo"
    reveal_type(f())  # revealed: int | str
```

## Calling with an unknown union

```py
from nonexistent import f  # error: [unresolved-import] "Cannot resolve imported module `nonexistent`"

def coinflip() -> bool:
    return True

if coinflip():
    def f() -> int:
        return 1

reveal_type(f())  # revealed: Unknown | int
```

## Non-callable elements in a union

Calling a union with a non-callable element should emit a diagnostic.

```py
def _(flag: bool):
    if flag:
        f = 1
    else:
        def f() -> int:
            return 1
    x = f()  # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    reveal_type(x)  # revealed: Unknown | int
```

## Multiple non-callable elements in a union

Calling a union with multiple non-callable elements should mention all of them in the diagnostic.

```py
def _(flag: bool, flag2: bool):
    if flag:
        f = 1
    elif flag2:
        f = "foo"
    else:
        def f() -> int:
            return 1
    # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    # error: [call-non-callable] "Object of type `Literal["foo"]` is not callable"
    # revealed: Unknown | int
    reveal_type(f())
```

## All non-callable union elements

Calling a union with no callable elements can emit a simpler diagnostic.

```py
def _(flag: bool):
    if flag:
        f = 1
    else:
        f = "foo"

    x = f()  # error: [call-non-callable] "Object of type `Literal[1, "foo"]` is not callable"
    reveal_type(x)  # revealed: Unknown
```

## Mismatching signatures

Calling a union where the arguments don't match the signature of all variants.

```py
def f1(a: int) -> int:
    return a

def f2(a: str) -> str:
    return a

def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # error: [invalid-argument-type] "Argument to function `f2` is incorrect: Expected `str`, found `Literal[3]`"
    x = f(3)
    reveal_type(x)  # revealed: int | str
```

## Any non-callable variant

```py
def f1(a: int): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = "This is a string literal"

    # error: [call-non-callable] "Object of type `Literal["This is a string literal"]` is not callable"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## Union of binding errors

```py
def f1(): ...
def f2(): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # error: [too-many-positional-arguments] "Too many positional arguments to function `f1`: expected 0, got 1"
    # error: [too-many-positional-arguments] "Too many positional arguments to function `f2`: expected 0, got 1"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## One not-callable, one wrong argument

```py
class C: ...

def f1(): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = C()

    # error: [too-many-positional-arguments] "Too many positional arguments to function `f1`: expected 0, got 1"
    # error: [call-non-callable] "Object of type `C` is not callable"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## Union including a special-cased function

```py
def _(flag: bool):
    if flag:
        f = str
    else:
        f = repr
    reveal_type(str("string"))  # revealed: Literal["string"]
    reveal_type(repr("string"))  # revealed: Literal["'string'"]
    reveal_type(f("string"))  # revealed: Literal["string", "'string'"]
```

## Unions with literals and negations

```py
from typing import Literal
from ty_extensions import Not, AlwaysFalsy, static_assert, is_subtype_of, is_assignable_to

static_assert(is_subtype_of(Literal["a", ""], Literal["a", ""] | Not[AlwaysFalsy]))
static_assert(is_subtype_of(Not[AlwaysFalsy], Literal["", "a"] | Not[AlwaysFalsy]))
static_assert(is_subtype_of(Literal["a", ""], Not[AlwaysFalsy] | Literal["a", ""]))
static_assert(is_subtype_of(Not[AlwaysFalsy], Not[AlwaysFalsy] | Literal["a", ""]))

static_assert(is_subtype_of(Literal["a", ""], Literal["a", ""] | Not[Literal[""]]))
static_assert(is_subtype_of(Not[Literal[""]], Literal["a", ""] | Not[Literal[""]]))
static_assert(is_subtype_of(Literal["a", ""], Not[Literal[""]] | Literal["a", ""]))
static_assert(is_subtype_of(Not[Literal[""]], Not[Literal[""]] | Literal["a", ""]))

def _(
    a: Literal["a", ""] | Not[AlwaysFalsy],
    b: Literal["a", ""] | Not[Literal[""]],
    c: Literal[""] | Not[Literal[""]],
    d: Not[Literal[""]] | Literal[""],
    e: Literal["a"] | Not[Literal["a"]],
    f: Literal[b"b"] | Not[Literal[b"b"]],
    g: Not[Literal[b"b"]] | Literal[b"b"],
    h: Literal[42] | Not[Literal[42]],
    i: Not[Literal[42]] | Literal[42],
):
    reveal_type(a)  # revealed: Literal[""] | ~AlwaysFalsy
    reveal_type(b)  # revealed: object
    reveal_type(c)  # revealed: object
    reveal_type(d)  # revealed: object
    reveal_type(e)  # revealed: object
    reveal_type(f)  # revealed: object
    reveal_type(g)  # revealed: object
    reveal_type(h)  # revealed: object
    reveal_type(i)  # revealed: object
```

## Cannot use an argument as both a value and a type form

```py
from ty_extensions import is_singleton

def _(flag: bool):
    if flag:
        f = repr
    else:
        f = is_singleton
    # error: [conflicting-argument-forms] "Argument is used as both a value and a type form in call"
    reveal_type(f(int))  # revealed: str | Literal[False]
```

## Size limit on unions of literals

Beyond a certain size, large unions of literal types collapse to their nearest super-type (`int`,
`bytes`, `str`).

```py
from typing import Literal

def _(literals_2: Literal[0, 1], b: bool, flag: bool):
    literals_4 = 2 * literals_2 + literals_2  # Literal[0, 1, 2, 3]
    literals_16 = 4 * literals_4 + literals_4  # Literal[0, 1, .., 15]
    literals_64 = 4 * literals_16 + literals_4  # Literal[0, 1, .., 63]
    literals_128 = 2 * literals_64 + literals_2  # Literal[0, 1, .., 127]
    literals_256 = 2 * literals_128 + literals_2  # Literal[0, 1, .., 255]

    # Going beyond the MAX_NON_RECURSIVE_UNION_LITERALS limit (currently 256):
    reveal_type(literals_256 if flag else 256)  # revealed: int

    # Going beyond the limit when another type is already part of the union
    bool_and_literals_128 = b if flag else literals_128  # bool | Literal[0, 1, ..., 127]
    literals_128_shifted = literals_128 + 128  # Literal[128, 129, ..., 255]
    literals_256_shifted = literals_256 + 256  # Literal[256, 257, ..., 511]

    # Now union the two:
    two = bool_and_literals_128 if flag else literals_128_shifted
    # revealed: bool | Literal[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255]
    reveal_type(two)
    reveal_type(two if flag else literals_256_shifted)  # revealed: int
```

Recursively defined literal union types are widened earlier than non-recursively defined types for
faster convergence.

```py
class RecursiveAttr:
    def __init__(self):
        self.i = 0

    def update(self):
        self.i = self.i + 1

reveal_type(RecursiveAttr().i)  # revealed: Unknown | int

# Here are some recursive but saturating examples. Because it's difficult to statically determine whether literal unions saturate or diverge,
# we widen them early, even though they may actually be convergent.
class RecursiveAttr2:
    def __init__(self):
        self.i = 0

    def update(self):
        self.i = (self.i + 1) % 9

reveal_type(RecursiveAttr2().i)  # revealed: Unknown | Literal[0, 1, 2, 3, 4, 5, 6, 7, 8]

class RecursiveAttr3:
    def __init__(self):
        self.i = 0

    def update(self):
        self.i = (self.i + 1) % 10

# Going beyond the MAX_RECURSIVE_UNION_LITERALS limit:
reveal_type(RecursiveAttr3().i)  # revealed: Unknown | int
```

## Simplifying gradually-equivalent types

If two types are gradually equivalent, we can keep just one of them in a union:

```py
from typing import Any, Union
from ty_extensions import Intersection, Not

def _(x: Union[Intersection[Any, Not[int]], Intersection[Any, Not[int]]]):
    reveal_type(x)  # revealed: Any & ~int
```

## Bidirectional Type Inference

```toml
[environment]
python-version = "3.12"
```

Type inference accounts for parameter type annotations across all signatures in a union.

```py
from typing import TypedDict, overload

class T(TypedDict):
    x: int

def _(flag: bool):
    if flag:
        def f(x: T) -> int:
            return 1
    else:
        def f(x: dict[str, int]) -> int:
            return 1
    x = f({"x": 1})
    reveal_type(x)  # revealed: int

    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `T`, found `dict[str, int] & dict[Unknown | str, Unknown | int]`"
    f({"y": 1})
```
