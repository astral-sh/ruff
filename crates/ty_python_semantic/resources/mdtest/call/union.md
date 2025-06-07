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
from ty_extensions import is_fully_static

def _(flag: bool):
    if flag:
        f = repr
    else:
        f = is_fully_static
    # error: [conflicting-argument-forms] "Argument is used as both a value and a type form in call"
    reveal_type(f(int))  # revealed: str | Literal[True]
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

    # Going beyond the MAX_UNION_LITERALS limit (currently 200):
    literals_256 = 16 * literals_16 + literals_16
    reveal_type(literals_256)  # revealed: int

    # Going beyond the limit when another type is already part of the union
    bool_and_literals_128 = b if flag else literals_128  # bool | Literal[0, 1, ..., 127]
    literals_128_shifted = literals_128 + 128  # Literal[128, 129, ..., 255]

    # Now union the two:
    reveal_type(bool_and_literals_128 if flag else literals_128_shifted)  # revealed: int
```

## Simplifying gradually-equivalent types

If two types are gradually equivalent, we can keep just one of them in a union:

```py
from typing import Any, Union
from ty_extensions import Intersection, Not

def _(x: Union[Intersection[Any, Not[int]], Intersection[Any, Not[int]]]):
    reveal_type(x)  # revealed: Any & ~int
```
