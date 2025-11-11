# Implicit type aliases

Implicit type aliases are the earliest form of type alias, introduced in PEP 484. They have no
special marker, just an ordinary assignment statement.

## Basic

We support simple type aliases with no extra effort, when the "value type" of the RHS is still a
valid type for use in a type expression:

```py
MyInt = int

def f(x: MyInt):
    reveal_type(x)  # revealed: int

f(1)
```

## None

```py
MyNone = None

def g(x: MyNone):
    reveal_type(x)  # revealed: None

g(None)
```

## Unions

We also support unions in type aliases:

```py
from typing_extensions import Any, Never, Literal, LiteralString, Tuple, Annotated, Optional
from ty_extensions import Unknown

IntOrStr = int | str
IntOrStrOrBytes1 = int | str | bytes
IntOrStrOrBytes2 = (int | str) | bytes
IntOrStrOrBytes3 = int | (str | bytes)
IntOrStrOrBytes4 = IntOrStr | bytes
BytesOrIntOrStr = bytes | IntOrStr
IntOrNone = int | None
NoneOrInt = None | int
IntOrStrOrNone = IntOrStr | None
NoneOrIntOrStr = None | IntOrStr
IntOrAny = int | Any
AnyOrInt = Any | int
NoneOrAny = None | Any
AnyOrNone = Any | None
NeverOrAny = Never | Any
AnyOrNever = Any | Never
UnknownOrInt = Unknown | int
IntOrUnknown = int | Unknown
StrOrZero = str | Literal[0]
ZeroOrStr = Literal[0] | str
LiteralStringOrInt = LiteralString | int
IntOrLiteralString = int | LiteralString
NoneOrTuple = None | Tuple[int, str]
TupleOrNone = Tuple[int, str] | None
IntOrAnnotated = int | Annotated[str, "meta"]
AnnotatedOrInt = Annotated[str, "meta"] | int
IntOrOptional = int | Optional[str]
OptionalOrInt = Optional[str] | int

reveal_type(IntOrStr)  # revealed: types.UnionType
reveal_type(IntOrStrOrBytes1)  # revealed: types.UnionType
reveal_type(IntOrStrOrBytes2)  # revealed: types.UnionType
reveal_type(IntOrStrOrBytes3)  # revealed: types.UnionType
reveal_type(IntOrStrOrBytes4)  # revealed: types.UnionType
reveal_type(BytesOrIntOrStr)  # revealed: types.UnionType
reveal_type(IntOrNone)  # revealed: types.UnionType
reveal_type(NoneOrInt)  # revealed: types.UnionType
reveal_type(IntOrStrOrNone)  # revealed: types.UnionType
reveal_type(NoneOrIntOrStr)  # revealed: types.UnionType
reveal_type(IntOrAny)  # revealed: types.UnionType
reveal_type(AnyOrInt)  # revealed: types.UnionType
reveal_type(NoneOrAny)  # revealed: types.UnionType
reveal_type(AnyOrNone)  # revealed: types.UnionType
reveal_type(NeverOrAny)  # revealed: types.UnionType
reveal_type(AnyOrNever)  # revealed: types.UnionType
reveal_type(UnknownOrInt)  # revealed: types.UnionType
reveal_type(IntOrUnknown)  # revealed: types.UnionType
reveal_type(StrOrZero)  # revealed: types.UnionType
reveal_type(ZeroOrStr)  # revealed: types.UnionType
reveal_type(IntOrLiteralString)  # revealed: types.UnionType
reveal_type(LiteralStringOrInt)  # revealed: types.UnionType
reveal_type(NoneOrTuple)  # revealed: types.UnionType
reveal_type(TupleOrNone)  # revealed: types.UnionType
reveal_type(IntOrAnnotated)  # revealed: types.UnionType
reveal_type(AnnotatedOrInt)  # revealed: types.UnionType
reveal_type(IntOrOptional)  # revealed: types.UnionType
reveal_type(OptionalOrInt)  # revealed: types.UnionType

def _(
    int_or_str: IntOrStr,
    int_or_str_or_bytes1: IntOrStrOrBytes1,
    int_or_str_or_bytes2: IntOrStrOrBytes2,
    int_or_str_or_bytes3: IntOrStrOrBytes3,
    int_or_str_or_bytes4: IntOrStrOrBytes4,
    bytes_or_int_or_str: BytesOrIntOrStr,
    int_or_none: IntOrNone,
    none_or_int: NoneOrInt,
    int_or_str_or_none: IntOrStrOrNone,
    none_or_int_or_str: NoneOrIntOrStr,
    int_or_any: IntOrAny,
    any_or_int: AnyOrInt,
    none_or_any: NoneOrAny,
    any_or_none: AnyOrNone,
    never_or_any: NeverOrAny,
    any_or_never: AnyOrNever,
    unknown_or_int: UnknownOrInt,
    int_or_unknown: IntOrUnknown,
    str_or_zero: StrOrZero,
    zero_or_str: ZeroOrStr,
    literal_string_or_int: LiteralStringOrInt,
    int_or_literal_string: IntOrLiteralString,
    none_or_tuple: NoneOrTuple,
    tuple_or_none: TupleOrNone,
    int_or_annotated: IntOrAnnotated,
    annotated_or_int: AnnotatedOrInt,
    int_or_optional: IntOrOptional,
    optional_or_int: OptionalOrInt,
):
    reveal_type(int_or_str)  # revealed: int | str
    reveal_type(int_or_str_or_bytes1)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes2)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes3)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes4)  # revealed: int | str | bytes
    reveal_type(bytes_or_int_or_str)  # revealed: bytes | int | str
    reveal_type(int_or_none)  # revealed: int | None
    reveal_type(none_or_int)  # revealed: None | int
    reveal_type(int_or_str_or_none)  # revealed: int | str | None
    reveal_type(none_or_int_or_str)  # revealed: None | int | str
    reveal_type(int_or_any)  # revealed: int | Any
    reveal_type(any_or_int)  # revealed: Any | int
    reveal_type(none_or_any)  # revealed: None | Any
    reveal_type(any_or_none)  # revealed: Any | None
    reveal_type(never_or_any)  # revealed: Any
    reveal_type(any_or_never)  # revealed: Any
    reveal_type(unknown_or_int)  # revealed: Unknown | int
    reveal_type(int_or_unknown)  # revealed: int | Unknown
    reveal_type(str_or_zero)  # revealed: str | Literal[0]
    reveal_type(zero_or_str)  # revealed: Literal[0] | str
    reveal_type(literal_string_or_int)  # revealed: LiteralString | int
    reveal_type(int_or_literal_string)  # revealed: int | LiteralString
    reveal_type(none_or_tuple)  # revealed: None | tuple[int, str]
    reveal_type(tuple_or_none)  # revealed: tuple[int, str] | None
    reveal_type(int_or_annotated)  # revealed: int | str
    reveal_type(annotated_or_int)  # revealed: str | int
    reveal_type(int_or_optional)  # revealed: int | str | None
    reveal_type(optional_or_int)  # revealed: str | None | int
```

If a type is unioned with itself in a value expression, the result is just that type. No
`types.UnionType` instance is created:

```py
IntOrInt = int | int
ListOfIntOrListOfInt = list[int] | list[int]

reveal_type(IntOrInt)  # revealed: <class 'int'>
reveal_type(ListOfIntOrListOfInt)  # revealed: <class 'list[int]'>

def _(int_or_int: IntOrInt, list_of_int_or_list_of_int: ListOfIntOrListOfInt):
    reveal_type(int_or_int)  # revealed: int
    reveal_type(list_of_int_or_list_of_int)  # revealed: list[int]
```

`NoneType` has no special or-operator behavior, so this is an error:

```py
None | None  # error: [unsupported-operator] "Operator `|` is unsupported between objects of type `None` and `None`"
```

When constructing something nonsensical like `int | 1`, we emit a diagnostic for the expression
itself, as it leads to a `TypeError` at runtime. The result of the expression is then inferred as
`Unknown`, so we permit it to be used in a type expression.

```py
IntOrOne = int | 1  # error: [unsupported-operator]

reveal_type(IntOrOne)  # revealed: Unknown

def _(int_or_one: IntOrOne):
    reveal_type(int_or_one)  # revealed: Unknown
```

If you were to somehow get hold of an opaque instance of `types.UnionType`, that could not be used
as a type expression:

```py
from types import UnionType

def f(SomeUnionType: UnionType):
    # error: [invalid-type-form] "Variable of type `UnionType` is not allowed in a type expression"
    some_union: SomeUnionType

f(int | str)
```

## `|` operator between class objects and non-class objects

Using the `|` operator between a class object and a non-class object does not create a `UnionType`
instance; it calls the relevant dunder as normal:

```py
class Foo:
    def __or__(self, other) -> str:
        return "foo"

reveal_type(Foo() | int)  # revealed: str
reveal_type(Foo() | list[int])  # revealed: str

class Bar:
    def __ror__(self, other) -> str:
        return "bar"

reveal_type(int | Bar())  # revealed: str
reveal_type(list[int] | Bar())  # revealed: str

class Invalid:
    def __or__(self, other: "Invalid") -> str:
        return "Invalid"

    def __ror__(self, other: "Invalid") -> str:
        return "Invalid"

# error: [unsupported-operator]
reveal_type(int | Invalid())  # revealed: Unknown
# error: [unsupported-operator]
reveal_type(Invalid() | list[int])  # revealed: Unknown
```

## Custom `__(r)or__` methods on metaclasses are only partially respected

A drawback of our extensive special casing of `|` operations between class objects is that
`__(r)or__` methods on metaclasses are completely disregarded if two classes are `|`'d together. We
respect the metaclass dunder if a class is `|`'d with a non-class, however:

```py
class Meta(type):
    def __or__(self, other) -> str:
        return "Meta"

class Foo(metaclass=Meta): ...
class Bar(metaclass=Meta): ...

X = Foo | Bar

# In an ideal world, perhaps we would respect `Meta.__or__` here and reveal `str`?
# But we still need to record what the elements are, since (according to the typing spec)
# `X` is still a valid type alias
reveal_type(X)  # revealed: types.UnionType

def f(obj: X):
    reveal_type(obj)  # revealed: Foo | Bar

# We do respect the metaclass `__or__` if it's used between a class and a non-class, however:

Y = Foo | 42
reveal_type(Y)  # revealed: str

Z = Bar | 56
reveal_type(Z)  # revealed: str

def g(
    arg1: Y,  # error: [invalid-type-form]
    arg2: Z,  # error: [invalid-type-form]
): ...
```

## Generic types

Implicit type aliases can also refer to generic types:

```py
from typing_extensions import TypeVar

T = TypeVar("T")

MyList = list[T]

def _(my_list: MyList[int]):
    # TODO: This should be `list[int]`
    reveal_type(my_list)  # revealed: @Todo(unknown type subscript)

ListOrTuple = list[T] | tuple[T, ...]

reveal_type(ListOrTuple)  # revealed: types.UnionType

def _(list_or_tuple: ListOrTuple[int]):
    reveal_type(list_or_tuple)  # revealed: @Todo(Generic specialization of types.UnionType)
```

## `Literal`s

We also support `typing.Literal` in implicit type aliases.

```py
from typing import Literal
from enum import Enum

IntLiteral1 = Literal[26]
IntLiteral2 = Literal[0x1A]
IntLiterals = Literal[-1, 0, 1]
NestedLiteral = Literal[Literal[1]]
StringLiteral = Literal["a"]
BytesLiteral = Literal[b"b"]
BoolLiteral = Literal[True]
MixedLiterals = Literal[1, "a", True, None]

class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

EnumLiteral = Literal[Color.RED]

def _(
    int_literal1: IntLiteral1,
    int_literal2: IntLiteral2,
    int_literals: IntLiterals,
    nested_literal: NestedLiteral,
    string_literal: StringLiteral,
    bytes_literal: BytesLiteral,
    bool_literal: BoolLiteral,
    mixed_literals: MixedLiterals,
    enum_literal: EnumLiteral,
):
    reveal_type(int_literal1)  # revealed: Literal[26]
    reveal_type(int_literal2)  # revealed: Literal[26]
    reveal_type(int_literals)  # revealed: Literal[-1, 0, 1]
    reveal_type(nested_literal)  # revealed: Literal[1]
    reveal_type(string_literal)  # revealed: Literal["a"]
    reveal_type(bytes_literal)  # revealed: Literal[b"b"]
    reveal_type(bool_literal)  # revealed: Literal[True]
    reveal_type(mixed_literals)  # revealed: Literal[1, "a", True] | None
    reveal_type(enum_literal)  # revealed: Literal[Color.RED]
```

We reject invalid uses:

```py
# error: [invalid-type-form] "Type arguments for `Literal` must be `None`, a literal value (int, bool, str, or bytes), or an enum member"
LiteralInt = Literal[int]

reveal_type(LiteralInt)  # revealed: Unknown

def _(weird: LiteralInt):
    reveal_type(weird)  # revealed: Unknown

# error: [invalid-type-form] "`Literal[26]` is not a generic class"
def _(weird: IntLiteral1[int]):
    reveal_type(weird)  # revealed: Unknown
```

## `Annotated`

Basic usage:

```py
from typing import Annotated

MyAnnotatedInt = Annotated[int, "some metadata", 1, 2, 3]

def _(annotated_int: MyAnnotatedInt):
    reveal_type(annotated_int)  # revealed: int
```

Usage with generics:

```py
from typing import TypeVar

T = TypeVar("T")

Deprecated = Annotated[T, "deprecated attribute"]

class C:
    old: Deprecated[int]

# TODO: Should be `int`
reveal_type(C().old)  # revealed: @Todo(Generic specialization of typing.Annotated)
```

If the metadata argument is missing, we emit an error (because this code fails at runtime), but
still use the first element as the type, when used in annotations:

```py
# error: [invalid-type-form] "Special form `typing.Annotated` expected at least 2 arguments (one type and at least one metadata element)"
WronglyAnnotatedInt = Annotated[int]

def _(wrongly_annotated_int: WronglyAnnotatedInt):
    reveal_type(wrongly_annotated_int)  # revealed: int
```

## `Optional`

Starting with Python 3.14, `Optional[int]` creates an instance of `typing.Union`, which is an alias
for `types.UnionType`. We only support this new behavior and do not attempt to model the details of
the pre-3.14 behavior:

```py
from typing import Optional

MyOptionalInt = Optional[int]

reveal_type(MyOptionalInt)  # revealed: types.UnionType

def _(optional_int: MyOptionalInt):
    reveal_type(optional_int)  # revealed: int | None
```

A special case is `Optional[None]`, which is equivalent to `None`:

```py
JustNone = Optional[None]

reveal_type(JustNone)  # revealed: None

def _(just_none: JustNone):
    reveal_type(just_none)  # revealed: None
```

Invalid uses:

```py
# error: [invalid-type-form] "`typing.Optional` requires exactly one argument"
Optional[int, str]
```

## `LiteralString`, `NoReturn`, `Never`

```py
from typing_extensions import LiteralString, NoReturn, Never

MyLiteralString = LiteralString
MyNoReturn = NoReturn
MyNever = Never

reveal_type(MyLiteralString)  # revealed: typing.LiteralString
reveal_type(MyNoReturn)  # revealed: typing.NoReturn
reveal_type(MyNever)  # revealed: typing.Never

def _(
    ls: MyLiteralString,
    nr: MyNoReturn,
    nv: MyNever,
):
    reveal_type(ls)  # revealed: LiteralString
    reveal_type(nr)  # revealed: Never
    reveal_type(nv)  # revealed: Never
```

## `Tuple`

```py
from typing import Tuple

IntAndStr = Tuple[int, str]

def _(int_and_str: IntAndStr):
    reveal_type(int_and_str)  # revealed: tuple[int, str]
```

## Stringified annotations?

From the [typing spec on type aliases](https://typing.python.org/en/latest/spec/aliases.html):

> Type aliases may be as complex as type hints in annotations – anything that is acceptable as a
> type hint is acceptable in a type alias

However, no other type checker seems to support stringified annotations in implicit type aliases. We
currently also do not support them, and we detect places where these attempted unions cause runtime
errors:

```py
AliasForStr = "str"

# error: [invalid-type-form] "Variable of type `Literal["str"]` is not allowed in a type expression"
def _(s: AliasForStr):
    reveal_type(s)  # revealed: Unknown

IntOrStr = int | "str"  # error: [unsupported-operator]

reveal_type(IntOrStr)  # revealed: Unknown

def _(int_or_str: IntOrStr):
    reveal_type(int_or_str)  # revealed: Unknown
```

We *do* support stringified annotations if they appear in a position where a type expression is
syntactically expected:

```py
ListOfInts = list["int"]

def _(list_of_ints: ListOfInts):
    reveal_type(list_of_ints)  # revealed: list[int]
```

## Recursive

### Old union syntax

```py
from typing import Union

Recursive = list[Union["Recursive", None]]

def _(r: Recursive):
    reveal_type(r)  # revealed: list[Divergent]
```

### New union syntax

```toml
[environment]
python-version = "3.12"
```

```py
Recursive = list["Recursive" | None]

def _(r: Recursive):
    reveal_type(r)  # revealed: list[Divergent]
```
