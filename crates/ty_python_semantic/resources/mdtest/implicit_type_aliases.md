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
from typing_extensions import Any, Never
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

When constructing something non-sensical like `int | 1`, we could ideally emit a diagnostic for the
expression itself, as it leads to a `TypeError` at runtime. No other type checker supports this, so
for now we only emit an error when it is used in a type expression:

```py
IntOrOne = int | 1

# error: [invalid-type-form] "Variable of type `Literal[1]` is not allowed in a type expression"
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

## Stringified annotations?

From the [typing spec on type aliases](https://typing.python.org/en/latest/spec/aliases.html):

> Type aliases may be as complex as type hints in annotations – anything that is acceptable as a
> type hint is acceptable in a type alias

However, no other type checker seems to support stringified annotations in implicit type aliases. We
currently also do not support them:

```py
AliasForStr = "str"

# error: [invalid-type-form] "Variable of type `Literal["str"]` is not allowed in a type expression"
def _(s: AliasForStr):
    reveal_type(s)  # revealed: Unknown

IntOrStr = int | "str"

# error: [invalid-type-form] "Variable of type `Literal["str"]` is not allowed in a type expression"
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
