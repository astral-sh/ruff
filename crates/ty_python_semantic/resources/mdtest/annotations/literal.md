# Literal

<https://typing.python.org/en/latest/spec/literal.html#literals>

## Parameterization

```py
from typing import Literal
from enum import Enum

mode: Literal["w", "r"]
a1: Literal[26]
a2: Literal[0x1A]
a3: Literal[-4]
a4: Literal["hello world"]
a5: Literal[b"hello world"]
a6: Literal[True]
a7: Literal[None]
a8: Literal[Literal[1]]

class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

b1: Literal[Color.RED]

MissingT = Enum("MissingT", {"MISSING": "MISSING"})
b2: Literal[MissingT.MISSING]

def f():
    reveal_type(mode)  # revealed: Literal["w", "r"]
    reveal_type(a1)  # revealed: Literal[26]
    reveal_type(a2)  # revealed: Literal[26]
    reveal_type(a3)  # revealed: Literal[-4]
    reveal_type(a4)  # revealed: Literal["hello world"]
    reveal_type(a5)  # revealed: Literal[b"hello world"]
    reveal_type(a6)  # revealed: Literal[True]
    reveal_type(a7)  # revealed: None
    reveal_type(a8)  # revealed: Literal[1]
    reveal_type(b1)  # revealed: Literal[Color.RED]
    # TODO should be `Literal[MissingT.MISSING]`
    reveal_type(b2)  # revealed: @Todo(functional `Enum` syntax)

# error: [invalid-type-form]
invalid1: Literal[3 + 4]
# error: [invalid-type-form]
invalid2: Literal[4 + 3j]
# error: [invalid-type-form]
invalid3: Literal[(3, 4)]

hello = "hello"
invalid4: Literal[
    1 + 2,  # error: [invalid-type-form]
    "foo",
    hello,  # error: [invalid-type-form]
    (1, 2, 3),  # error: [invalid-type-form]
]

class NotAnEnum:
    x: int = 1

# error: [invalid-type-form]
invalid5: Literal[NotAnEnum.x]

a_list: list[int] = [1, 2, 3]
# error: [invalid-type-form]
invalid6: Literal[a_list[0]]
```

## Parameterizing with a type alias

`typing.Literal` can also be parameterized with a type alias for any literal type or union of
literal types.

### PEP 695 type alias

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal
from enum import Enum

import mod

class E(Enum):
    A = 1
    B = 2

type SingleInt = Literal[1]
type SingleStr = Literal["foo"]
type SingleBytes = Literal[b"bar"]
type SingleBool = Literal[True]
type SingleNone = Literal[None]
type SingleEnum = Literal[E.A]
type UnionLiterals = Literal[1, "foo", b"bar", True, None, E.A]
# We support this because it is an equivalent type to the following union of literals, but maybe
# we should not, because it doesn't use `Literal` form? Other type checkers do not.
type AnEnum1 = E
type AnEnum2 = Literal[E.A, E.B]
# Similarly, we support this because it is equivalent to `Literal[True, False]`.
type Bool1 = bool
type Bool2 = Literal[True, False]

def _(
    single_int: Literal[SingleInt],
    single_str: Literal[SingleStr],
    single_bytes: Literal[SingleBytes],
    single_bool: Literal[SingleBool],
    single_none: Literal[SingleNone],
    single_enum: Literal[SingleEnum],
    union_literals: Literal[UnionLiterals],
    an_enum1: Literal[AnEnum1],
    an_enum2: Literal[AnEnum2],
    bool1: Literal[Bool1],
    bool2: Literal[Bool2],
    multiple: Literal[SingleInt, SingleStr, SingleEnum],
    single_int_other_module: Literal[mod.SingleInt],
):
    reveal_type(single_int)  # revealed: Literal[1]
    reveal_type(single_str)  # revealed: Literal["foo"]
    reveal_type(single_bytes)  # revealed: Literal[b"bar"]
    reveal_type(single_bool)  # revealed: Literal[True]
    reveal_type(single_none)  # revealed: None
    reveal_type(single_enum)  # revealed: Literal[E.A]
    reveal_type(union_literals)  # revealed: Literal[1, "foo", b"bar", True, E.A] | None
    reveal_type(an_enum1)  # revealed: E
    reveal_type(an_enum2)  # revealed: E
    reveal_type(bool1)  # revealed: bool
    reveal_type(bool2)  # revealed: bool
    reveal_type(multiple)  # revealed: Literal[1, "foo", E.A]
    reveal_type(single_int_other_module)  # revealed: Literal[2]
```

`mod.py`:

```py
from typing import Literal

type SingleInt = Literal[2]
```

### PEP 613 type alias

```py
from typing import Literal, TypeAlias
from enum import Enum

class E(Enum):
    A = 1
    B = 2

SingleInt: TypeAlias = Literal[1]
SingleStr: TypeAlias = Literal["foo"]
SingleBytes: TypeAlias = Literal[b"bar"]
SingleBool: TypeAlias = Literal[True]
SingleNone: TypeAlias = Literal[None]
SingleEnum: TypeAlias = Literal[E.A]
UnionLiterals: TypeAlias = Literal[1, "foo", b"bar", True, None, E.A]
AnEnum1: TypeAlias = E
AnEnum2: TypeAlias = Literal[E.A, E.B]
Bool1: TypeAlias = bool
Bool2: TypeAlias = Literal[True, False]

def _(
    single_int: Literal[SingleInt],
    single_str: Literal[SingleStr],
    single_bytes: Literal[SingleBytes],
    single_bool: Literal[SingleBool],
    single_none: Literal[SingleNone],
    single_enum: Literal[SingleEnum],
    union_literals: Literal[UnionLiterals],
    # Could also not error
    an_enum1: Literal[AnEnum1],  # error: [invalid-type-form]
    an_enum2: Literal[AnEnum2],
    # Could also not error
    bool1: Literal[Bool1],  # error: [invalid-type-form]
    bool2: Literal[Bool2],
    multiple: Literal[SingleInt, SingleStr, SingleEnum],
):
    reveal_type(single_int)  # revealed: Literal[1]
    reveal_type(single_str)  # revealed: Literal["foo"]
    reveal_type(single_bytes)  # revealed: Literal[b"bar"]
    reveal_type(single_bool)  # revealed: Literal[True]
    reveal_type(single_none)  # revealed: None
    reveal_type(single_enum)  # revealed: Literal[E.A]
    reveal_type(union_literals)  # revealed: Literal[1, "foo", b"bar", True, E.A] | None
    # Could also be `E`
    reveal_type(an_enum1)  # revealed: Unknown
    reveal_type(an_enum2)  # revealed: E
    # Could also be `bool`
    reveal_type(bool1)  # revealed: Unknown
    reveal_type(bool2)  # revealed: bool
    reveal_type(multiple)  # revealed: Literal[1, "foo", E.A]
```

### Implicit type alias

```py
from typing import Literal
from enum import Enum

class E(Enum):
    A = 1
    B = 2

SingleInt = Literal[1]
SingleStr = Literal["foo"]
SingleBytes = Literal[b"bar"]
SingleBool = Literal[True]
SingleNone = Literal[None]
SingleEnum = Literal[E.A]
UnionLiterals = Literal[1, "foo", b"bar", True, None, E.A]
# For implicit type aliases, we may not want to support this. It's simpler not to, and no other
# type checker does.
AnEnum1 = E
AnEnum2 = Literal[E.A, E.B]
# For implicit type aliases, we may not want to support this.
Bool1 = bool
Bool2 = Literal[True, False]

def _(
    single_int: Literal[SingleInt],
    single_str: Literal[SingleStr],
    single_bytes: Literal[SingleBytes],
    single_bool: Literal[SingleBool],
    single_none: Literal[SingleNone],
    single_enum: Literal[SingleEnum],
    union_literals: Literal[UnionLiterals],
    an_enum1: Literal[AnEnum1],  # error: [invalid-type-form]
    an_enum2: Literal[AnEnum2],
    bool1: Literal[Bool1],  # error: [invalid-type-form]
    bool2: Literal[Bool2],
    multiple: Literal[SingleInt, SingleStr, SingleEnum],
):
    reveal_type(single_int)  # revealed: Literal[1]
    reveal_type(single_str)  # revealed: Literal["foo"]
    reveal_type(single_bytes)  # revealed: Literal[b"bar"]
    reveal_type(single_bool)  # revealed: Literal[True]
    reveal_type(single_none)  # revealed: None
    reveal_type(single_enum)  # revealed: Literal[E.A]
    reveal_type(union_literals)  # revealed: Literal[1, "foo", b"bar", True, E.A] | None
    reveal_type(an_enum1)  # revealed: Unknown
    reveal_type(an_enum2)  # revealed: E
    reveal_type(bool1)  # revealed: Unknown
    reveal_type(bool2)  # revealed: bool
    reveal_type(multiple)  # revealed: Literal[1, "foo", E.A]
```

## Shortening unions of literals

When a Literal is parameterized with more than one value, it’s treated as exactly to equivalent to
the union of those types.

```py
from typing import Literal

def x(
    a1: Literal[Literal[Literal[1, 2, 3], "foo"], 5, None],
    a2: Literal["w"] | Literal["r"],
    a3: Literal[Literal["w"], Literal["r"], Literal[Literal["w+"]]],
    a4: Literal[True] | Literal[1, 2] | Literal["foo"],
):
    reveal_type(a1)  # revealed: Literal[1, 2, 3, 5, "foo"] | None
    reveal_type(a2)  # revealed: Literal["w", "r"]
    reveal_type(a3)  # revealed: Literal["w", "r", "w+"]
    reveal_type(a4)  # revealed: Literal[True, 1, 2, "foo"]
```

## Display of heterogeneous unions of literals

```py
from typing import Literal, Union

def foo(x: int) -> int:
    return x + 1

def bar(s: str) -> str:
    return s

class A: ...
class B: ...

def union_example(
    x: Union[
        # unknown type
        # error: [unresolved-reference]
        y,
        Literal[-1],
        Literal["A"],
        Literal[b"A"],
        Literal[b"\x00"],
        Literal[b"\x07"],
        Literal[0],
        Literal[1],
        Literal["B"],
        Literal["foo"],
        Literal["bar"],
        Literal["B"],
        Literal[True],
        None,
    ],
):
    reveal_type(x)  # revealed: Unknown | Literal[-1, 0, 1, "A", "B", "foo", "bar", b"A", b"\x00", b"\x07", True] | None
```

## Detecting Literal outside typing and typing_extensions

Only Literal that is defined in typing and typing_extension modules is detected as the special
Literal.

`other.pyi`:

```pyi
from typing import _SpecialForm

Literal: _SpecialForm
```

```py
from other import Literal

# TODO: can we add a subdiagnostic here saying something like:
#
#     `other.Literal` and `typing.Literal` have similar names, but are different symbols and don't have the same semantics
#
# ?
#
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
# error: [invalid-type-form] "Invalid subscript of object of type `_SpecialForm` in type expression"
a1: Literal[26]

def f():
    reveal_type(a1)  # revealed: Unknown
```

## Detecting typing_extensions.Literal

```py
from typing_extensions import Literal

a1: Literal[26]

def f():
    reveal_type(a1)  # revealed: Literal[26]
```

## Invalid

```py
from typing import Literal

# error: [invalid-type-form] "`typing.Literal` requires at least one argument when used in a type expression"
def _(x: Literal):
    reveal_type(x)  # revealed: Unknown
```
