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
from typing_extensions import Any, Never, Literal, LiteralString, Tuple, Annotated, Optional, Union, Callable, TypeVar
from ty_extensions import Unknown

T = TypeVar("T")

IntOrStr = int | str
IntOrStrOrBytes1 = int | str | bytes
IntOrStrOrBytes2 = (int | str) | bytes
IntOrStrOrBytes3 = int | (str | bytes)
IntOrStrOrBytes4 = IntOrStr | bytes
IntOrStrOrBytes5 = int | Union[str, bytes]
IntOrStrOrBytes6 = Union[int, str] | bytes
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
IntOrTypeOfStr = int | type[str]
TypeOfStrOrInt = type[str] | int
IntOrCallable = int | Callable[[str], bytes]
CallableOrInt = Callable[[str], bytes] | int
TypeVarOrInt = T | int
IntOrTypeVar = int | T
TypeVarOrNone = T | None
NoneOrTypeVar = None | T

reveal_type(IntOrStr)  # revealed: <types.UnionType special-form 'int | str'>
reveal_type(IntOrStrOrBytes1)  # revealed: <types.UnionType special-form 'int | str | bytes'>
reveal_type(IntOrStrOrBytes2)  # revealed: <types.UnionType special-form 'int | str | bytes'>
reveal_type(IntOrStrOrBytes3)  # revealed: <types.UnionType special-form 'int | str | bytes'>
reveal_type(IntOrStrOrBytes4)  # revealed: <types.UnionType special-form 'int | str | bytes'>
reveal_type(IntOrStrOrBytes5)  # revealed: <types.UnionType special-form 'int | str | bytes'>
reveal_type(IntOrStrOrBytes6)  # revealed: <types.UnionType special-form 'int | str | bytes'>
reveal_type(BytesOrIntOrStr)  # revealed: <types.UnionType special-form 'bytes | int | str'>
reveal_type(IntOrNone)  # revealed: <types.UnionType special-form 'int | None'>
reveal_type(NoneOrInt)  # revealed: <types.UnionType special-form 'None | int'>
reveal_type(IntOrStrOrNone)  # revealed: <types.UnionType special-form 'int | str | None'>
reveal_type(NoneOrIntOrStr)  # revealed: <types.UnionType special-form 'None | int | str'>
reveal_type(IntOrAny)  # revealed: <types.UnionType special-form 'int | Any'>
reveal_type(AnyOrInt)  # revealed: <types.UnionType special-form 'Any | int'>
reveal_type(NoneOrAny)  # revealed: <types.UnionType special-form 'None | Any'>
reveal_type(AnyOrNone)  # revealed: <types.UnionType special-form 'Any | None'>
reveal_type(NeverOrAny)  # revealed: <types.UnionType special-form 'Any'>
reveal_type(AnyOrNever)  # revealed: <types.UnionType special-form 'Any'>
reveal_type(UnknownOrInt)  # revealed: <types.UnionType special-form 'Unknown | int'>
reveal_type(IntOrUnknown)  # revealed: <types.UnionType special-form 'int | Unknown'>
reveal_type(StrOrZero)  # revealed: <types.UnionType special-form 'str | Literal[0]'>
reveal_type(ZeroOrStr)  # revealed: <types.UnionType special-form 'Literal[0] | str'>
reveal_type(IntOrLiteralString)  # revealed: <types.UnionType special-form 'int | LiteralString'>
reveal_type(LiteralStringOrInt)  # revealed: <types.UnionType special-form 'LiteralString | int'>
reveal_type(NoneOrTuple)  # revealed: <types.UnionType special-form 'None | tuple[int, str]'>
reveal_type(TupleOrNone)  # revealed: <types.UnionType special-form 'tuple[int, str] | None'>
reveal_type(IntOrAnnotated)  # revealed: <types.UnionType special-form 'int | str'>
reveal_type(AnnotatedOrInt)  # revealed: <types.UnionType special-form 'str | int'>
reveal_type(IntOrOptional)  # revealed: <types.UnionType special-form 'int | str | None'>
reveal_type(OptionalOrInt)  # revealed: <types.UnionType special-form 'str | None | int'>
reveal_type(IntOrTypeOfStr)  # revealed: <types.UnionType special-form 'int | type[str]'>
reveal_type(TypeOfStrOrInt)  # revealed: <types.UnionType special-form 'type[str] | int'>
reveal_type(IntOrCallable)  # revealed: <types.UnionType special-form 'int | ((str, /) -> bytes)'>
reveal_type(CallableOrInt)  # revealed: <types.UnionType special-form '((str, /) -> bytes) | int'>
reveal_type(TypeVarOrInt)  # revealed: <types.UnionType special-form 'T@TypeVarOrInt | int'>
reveal_type(IntOrTypeVar)  # revealed: <types.UnionType special-form 'int | T@IntOrTypeVar'>
reveal_type(TypeVarOrNone)  # revealed: <types.UnionType special-form 'T@TypeVarOrNone | None'>
reveal_type(NoneOrTypeVar)  # revealed: <types.UnionType special-form 'None | T@NoneOrTypeVar'>

def _(
    int_or_str: IntOrStr,
    int_or_str_or_bytes1: IntOrStrOrBytes1,
    int_or_str_or_bytes2: IntOrStrOrBytes2,
    int_or_str_or_bytes3: IntOrStrOrBytes3,
    int_or_str_or_bytes4: IntOrStrOrBytes4,
    int_or_str_or_bytes5: IntOrStrOrBytes5,
    int_or_str_or_bytes6: IntOrStrOrBytes6,
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
    int_or_type_of_str: IntOrTypeOfStr,
    type_of_str_or_int: TypeOfStrOrInt,
    int_or_callable: IntOrCallable,
    callable_or_int: CallableOrInt,
    type_var_or_int: TypeVarOrInt,
    int_or_type_var: IntOrTypeVar,
    type_var_or_none: TypeVarOrNone,
    none_or_type_var: NoneOrTypeVar,
):
    reveal_type(int_or_str)  # revealed: int | str
    reveal_type(int_or_str_or_bytes1)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes2)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes3)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes4)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes5)  # revealed: int | str | bytes
    reveal_type(int_or_str_or_bytes6)  # revealed: int | str | bytes
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
    reveal_type(int_or_type_of_str)  # revealed: int | type[str]
    reveal_type(type_of_str_or_int)  # revealed: type[str] | int
    reveal_type(int_or_callable)  # revealed: int | ((str, /) -> bytes)
    reveal_type(callable_or_int)  # revealed: ((str, /) -> bytes) | int
    reveal_type(type_var_or_int)  # revealed: Unknown | int
    reveal_type(int_or_type_var)  # revealed: int | Unknown
    reveal_type(type_var_or_none)  # revealed: Unknown | None
    reveal_type(none_or_type_var)  # revealed: None | Unknown
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
None | None  # error: [unsupported-operator] "Operator `|` is not supported between two objects of type `None`"
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
reveal_type(X)  # revealed: <types.UnionType special-form 'Foo | Bar'>

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

## `|` unions in stubs and `TYPE_CHECKING` blocks

In runtime contexts, `|` unions are only permitted on Python 3.10+. But in suites of code that are
never executed at runtime (stub files, `if TYPE_CHECKING` blocks, and stringified annotations), they
are permitted even if the target version is set to Python 3.9 or earlier.

```toml
[environment]
python-version = "3.9"
```

`bar.pyi`:

```pyi
Z = int | str
GLOBAL_CONSTANT: Z
```

`foo.py`:

```py
from typing import TYPE_CHECKING
from bar import GLOBAL_CONSTANT

reveal_type(GLOBAL_CONSTANT)  # revealed: int | str

if TYPE_CHECKING:
    class ItsQuiteCloudyInManchester:
        X = int | str

        def f(obj: X):
            reveal_type(obj)  # revealed: int | str

    # TODO: we currently only understand code as being inside a `TYPE_CHECKING` block
    # if a whole *scope* is inside the `if TYPE_CHECKING` block
    # (like the `ItsQuiteCloudyInManchester` class above); this is a false-positive
    Y = int | str  # error: [unsupported-operator]

    def g(obj: Y):
        # TODO: should be `int | str`
        reveal_type(obj)  # revealed: Unknown

Y = list["int | str"]

def g(obj: Y):
    reveal_type(obj)  # revealed: list[int | str]
```

## Generic implicit type aliases

### Functionality

Implicit type aliases can also be generic:

```py
from typing_extensions import TypeVar, ParamSpec, Callable, Union, Annotated

T = TypeVar("T")
U = TypeVar("U")

P = ParamSpec("P")

MyList = list[T]
MyDict = dict[T, U]
MyType = type[T]
IntAndType = tuple[int, T]
Pair = tuple[T, T]
Sum = tuple[T, U]
ListOrTuple = list[T] | tuple[T, ...]
ListOrTupleLegacy = Union[list[T], tuple[T, ...]]
MyCallable = Callable[P, T]
AnnotatedType = Annotated[T, "tag"]
TransparentAlias = T
MyOptional = T | None

reveal_type(MyList)  # revealed: <class 'list[T@MyList]'>
reveal_type(MyDict)  # revealed: <class 'dict[T@MyDict, U@MyDict]'>
reveal_type(MyType)  # revealed: <special-form 'type[T@MyType]'>
reveal_type(IntAndType)  # revealed: <class 'tuple[int, T@IntAndType]'>
reveal_type(Pair)  # revealed: <class 'tuple[T@Pair, T@Pair]'>
reveal_type(Sum)  # revealed: <class 'tuple[T@Sum, U@Sum]'>
reveal_type(ListOrTuple)  # revealed: <types.UnionType special-form 'list[T@ListOrTuple] | tuple[T@ListOrTuple, ...]'>
# revealed: <types.UnionType special-form 'list[T@ListOrTupleLegacy] | tuple[T@ListOrTupleLegacy, ...]'>
reveal_type(ListOrTupleLegacy)
reveal_type(MyCallable)  # revealed: <typing.Callable special-form '(**P@MyCallable) -> T@MyCallable'>
reveal_type(AnnotatedType)  # revealed: <special-form 'typing.Annotated[T@AnnotatedType, <metadata>]'>
reveal_type(TransparentAlias)  # revealed: TypeVar
reveal_type(MyOptional)  # revealed: <types.UnionType special-form 'T@MyOptional | None'>

def _(
    list_of_ints: MyList[int],
    dict_str_to_int: MyDict[str, int],
    subclass_of_int: MyType[int],
    int_and_str: IntAndType[str],
    pair_of_ints: Pair[int],
    int_and_bytes: Sum[int, bytes],
    list_or_tuple: ListOrTuple[int],
    list_or_tuple_legacy: ListOrTupleLegacy[int],
    my_callable: MyCallable[[str, bytes], int],
    annotated_int: AnnotatedType[int],
    # error: [invalid-type-form] "A type variable itself cannot be specialized"
    transparent_alias: TransparentAlias[int],
    optional_int: MyOptional[int],
):
    reveal_type(list_of_ints)  # revealed: list[int]
    reveal_type(dict_str_to_int)  # revealed: dict[str, int]
    reveal_type(subclass_of_int)  # revealed: type[int]
    reveal_type(int_and_str)  # revealed: tuple[int, str]
    reveal_type(pair_of_ints)  # revealed: tuple[int, int]
    reveal_type(int_and_bytes)  # revealed: tuple[int, bytes]
    reveal_type(list_or_tuple)  # revealed: list[int] | tuple[int, ...]
    reveal_type(list_or_tuple_legacy)  # revealed: list[int] | tuple[int, ...]
    reveal_type(my_callable)  # revealed: (str, bytes, /) -> int
    reveal_type(annotated_int)  # revealed: int
    reveal_type(transparent_alias)  # revealed: Unknown
    reveal_type(optional_int)  # revealed: int | None
```

Generic implicit type aliases can be partially specialized:

```py
DictStrTo = MyDict[str, U]

reveal_type(DictStrTo)  # revealed: <class 'dict[str, U@DictStrTo]'>

def _(
    dict_str_to_int: DictStrTo[int],
):
    reveal_type(dict_str_to_int)  # revealed: dict[str, int]
```

Using specializations of generic implicit type aliases in other implicit type aliases works as
expected:

```py
IntsOrNone = MyList[int] | None
IntsOrStrs = Pair[int] | Pair[str]
ListOfPairs = MyList[Pair[str]]
ListOrTupleOfInts = ListOrTuple[int]
AnnotatedInt = AnnotatedType[int]
SubclassOfInt = MyType[int]
CallableIntToStr = MyCallable[[int], str]

reveal_type(IntsOrNone)  # revealed: <types.UnionType special-form 'list[int] | None'>
reveal_type(IntsOrStrs)  # revealed: <types.UnionType special-form 'tuple[int, int] | tuple[str, str]'>
reveal_type(ListOfPairs)  # revealed: <class 'list[tuple[str, str]]'>
reveal_type(ListOrTupleOfInts)  # revealed: <types.UnionType special-form 'list[int] | tuple[int, ...]'>
reveal_type(AnnotatedInt)  # revealed: <special-form 'typing.Annotated[int, <metadata>]'>
reveal_type(SubclassOfInt)  # revealed: <special-form 'type[int]'>
reveal_type(CallableIntToStr)  # revealed: <typing.Callable special-form '(int, /) -> str'>

def _(
    ints_or_none: IntsOrNone,
    ints_or_strs: IntsOrStrs,
    list_of_pairs: ListOfPairs,
    list_or_tuple_of_ints: ListOrTupleOfInts,
    annotated_int: AnnotatedInt,
    subclass_of_int: SubclassOfInt,
    callable_int_to_str: CallableIntToStr,
):
    reveal_type(ints_or_none)  # revealed: list[int] | None
    reveal_type(ints_or_strs)  # revealed: tuple[int, int] | tuple[str, str]
    reveal_type(list_of_pairs)  # revealed: list[tuple[str, str]]
    reveal_type(list_or_tuple_of_ints)  # revealed: list[int] | tuple[int, ...]
    reveal_type(annotated_int)  # revealed: int
    reveal_type(subclass_of_int)  # revealed: type[int]
    reveal_type(callable_int_to_str)  # revealed: (int, /) -> str
```

A generic implicit type alias can also be used in another generic implicit type alias:

```py
from typing_extensions import Any

B = TypeVar("B", bound=int)

MyOtherList = MyList[T]
MyOtherType = MyType[T]
TypeOrList = MyType[B] | MyList[B]

reveal_type(MyOtherList)  # revealed: <class 'list[T@MyOtherList]'>
reveal_type(MyOtherType)  # revealed: <special-form 'type[T@MyOtherType]'>
reveal_type(TypeOrList)  # revealed: <types.UnionType special-form 'type[B@TypeOrList] | list[B@TypeOrList]'>

def _(
    list_of_ints: MyOtherList[int],
    subclass_of_int: MyOtherType[int],
    type_or_list: TypeOrList[Any],
):
    reveal_type(list_of_ints)  # revealed: list[int]
    reveal_type(subclass_of_int)  # revealed: type[int]
    reveal_type(type_or_list)  # revealed: type[Any] | list[Any]
```

If a generic implicit type alias is used unspecialized in a type expression, we use the default
specialization. For type variables without defaults, this is `Unknown`:

```py
def _(
    list_unknown: MyList,
    dict_unknown: MyDict,
    subclass_of_unknown: MyType,
    int_and_unknown: IntAndType,
    pair_of_unknown: Pair,
    unknown_and_unknown: Sum,
    list_or_tuple: ListOrTuple,
    list_or_tuple_legacy: ListOrTupleLegacy,
    my_callable: MyCallable,
    annotated_unknown: AnnotatedType,
    optional_unknown: MyOptional,
):
    reveal_type(list_unknown)  # revealed: list[Unknown]
    reveal_type(dict_unknown)  # revealed: dict[Unknown, Unknown]
    reveal_type(subclass_of_unknown)  # revealed: type[Unknown]
    reveal_type(int_and_unknown)  # revealed: tuple[int, Unknown]
    reveal_type(pair_of_unknown)  # revealed: tuple[Unknown, Unknown]
    reveal_type(unknown_and_unknown)  # revealed: tuple[Unknown, Unknown]
    reveal_type(list_or_tuple)  # revealed: list[Unknown] | tuple[Unknown, ...]
    reveal_type(list_or_tuple_legacy)  # revealed: list[Unknown] | tuple[Unknown, ...]
    reveal_type(my_callable)  # revealed: (...) -> Unknown
    reveal_type(annotated_unknown)  # revealed: Unknown
    reveal_type(optional_unknown)  # revealed: Unknown | None
```

For a type variable with a default, we use the default type:

```py
T_default = TypeVar("T_default", default=int)

MyListWithDefault = list[T_default]

def _(
    list_of_str: MyListWithDefault[str],
    list_of_int: MyListWithDefault,
    list_of_str_or_none: MyListWithDefault[str] | None,
    list_of_int_or_none: MyListWithDefault | None,
):
    reveal_type(list_of_str)  # revealed: list[str]
    reveal_type(list_of_int)  # revealed: list[int]
    reveal_type(list_of_str_or_none)  # revealed: list[str] | None
    reveal_type(list_of_int_or_none)  # revealed: list[int] | None
```

(Generic) implicit type aliases can be used as base classes:

```py
from typing_extensions import Generic
from ty_extensions import reveal_mro

class GenericBase(Generic[T]):
    pass

ConcreteBase = GenericBase[int]

class Derived1(ConcreteBase):
    pass

# revealed: (<class 'Derived1'>, <class 'GenericBase[int]'>, typing.Generic, <class 'object'>)
reveal_mro(Derived1)

GenericBaseAlias = GenericBase[T]

class Derived2(GenericBaseAlias[int]):
    pass
```

### Imported aliases

Generic implicit type aliases can be imported from other modules and specialized:

`my_types.py`:

```py
from typing_extensions import TypeVar

T = TypeVar("T", default=str)

MyList = list[T]
```

`main.py`:

```py
from my_types import MyList
import my_types as mt

def _(
    list_of_ints1: MyList[int],
    list_of_ints2: mt.MyList[int],
    list_of_str: mt.MyList,
    list_of_str_or_none: mt.MyList | None,
):
    reveal_type(list_of_ints1)  # revealed: list[int]
    reveal_type(list_of_ints2)  # revealed: list[int]
    reveal_type(list_of_str)  # revealed: list[str]
    reveal_type(list_of_str_or_none)  # revealed: list[str] | None
```

### In stringified annotations

Generic implicit type aliases can be specialized in stringified annotations:

```py
from typing_extensions import TypeVar

T = TypeVar("T")

MyList = list[T]

def _(
    list_of_ints: "MyList[int]",
):
    reveal_type(list_of_ints)  # revealed: list[int]
```

### Tuple unpacking

```toml
[environment]
python-version = "3.11"
```

```py
from typing import TypeVar

T = TypeVar("T")
U = TypeVar("U")
V = TypeVar("V")

X = tuple[T, *tuple[U, ...], V]
Y = X[T, tuple[int, str, U], bytes]

def g(obj: Y[bool, range]):
    reveal_type(obj)  # revealed: tuple[bool, *tuple[tuple[int, str, range], ...], bytes]
```

### Error cases

A generic alias that is already fully specialized cannot be specialized again:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, TypeVar, TypedDict

ListOfInts = list[int]

# error: [not-subscriptable] "Cannot subscript non-generic type: `<class 'list[int]'>` is already specialized"
def _(doubly_specialized: ListOfInts[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown

type ListOfInts2 = list[int]
# error: [not-subscriptable] "Cannot subscript non-generic type alias: `list[int]` is already specialized"
DoublySpecialized = ListOfInts2[int]

def _(doubly_specialized: DoublySpecialized):
    reveal_type(doubly_specialized)  # revealed: Unknown

# error: [not-subscriptable] "Cannot subscript non-generic type: `<class 'list[int]'>` is already specialized"
List = list[int][int]

def _(doubly_specialized: List):
    reveal_type(doubly_specialized)  # revealed: Unknown

Tuple = tuple[int, str]

# error: [not-subscriptable] "Cannot subscript non-generic type: `<class 'tuple[int, str]'>` is already specialized"
def _(doubly_specialized: Tuple[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown

T = TypeVar("T")

class LegacyProto(Protocol[T]):
    pass

LegacyProtoInt = LegacyProto[int]

# error: [not-subscriptable] "Cannot subscript non-generic type: `<class 'LegacyProto[int]'>` is already specialized"
def _(doubly_specialized: LegacyProtoInt[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown

class Proto[T](Protocol):
    pass

ProtoInt = Proto[int]

# error: [not-subscriptable] "Cannot subscript non-generic type: `<class 'Proto[int]'>` is already specialized"
def _(doubly_specialized: ProtoInt[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown

# TODO: TypedDict is just a function object at runtime, we should emit an error
class LegacyDict(TypedDict[T]):
    x: T

# TODO: should be a `not-subscriptable` error
LegacyDictInt = LegacyDict[int]

# TODO: should be a `not-subscriptable` error
def _(doubly_specialized: LegacyDictInt[int]):
    # TODO: should be `Unknown`
    reveal_type(doubly_specialized)  # revealed: @Todo(Inference of subscript on special form)

class Dict[T](TypedDict):
    x: T

DictInt = Dict[int]

# error: [not-subscriptable] "Cannot subscript non-generic type: `<class 'Dict[int]'>` is already specialized"
def _(doubly_specialized: DictInt[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown

Union = list[str] | list[int]

# error: [not-subscriptable] "Cannot subscript non-generic type: `<types.UnionType special-form 'list[str] | list[int]'>` is already specialized"
def _(doubly_specialized: Union[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown

type MyListAlias[T] = list[T]
MyListOfInts = MyListAlias[int]

# error: [not-subscriptable] "Cannot subscript non-generic type alias: Double specialization is not allowed"
def _(doubly_specialized: MyListOfInts[int]):
    reveal_type(doubly_specialized)  # revealed: Unknown
```

Specializing a generic implicit type alias with an incorrect number of type arguments also results
in an error:

```py
from typing_extensions import TypeVar

T = TypeVar("T")
U = TypeVar("U")

MyList = list[T]
MyDict = dict[T, U]

def _(
    # error: [invalid-type-arguments] "Too many type arguments: expected 1, got 2"
    list_too_many_args: MyList[int, str],
    # error: [invalid-type-arguments] "No type argument provided for required type variable `U`"
    dict_too_few_args: MyDict[int],
):
    reveal_type(list_too_many_args)  # revealed: list[Unknown]
    reveal_type(dict_too_few_args)  # revealed: dict[Unknown, Unknown]
```

Trying to specialize a non-name node results in an error:

```py
from ty_extensions import TypeOf

IntOrStr = int | str

def this_does_not_work() -> TypeOf[IntOrStr]:
    raise NotImplementedError()

def _(
    # error: [not-subscriptable] "Cannot subscript non-generic type"
    specialized: this_does_not_work()[int],
):
    reveal_type(specialized)  # revealed: Unknown
```

Similarly, if you try to specialize a union type without a binding context, we emit an error:

```py
# error: [not-subscriptable] "Cannot subscript non-generic type"
x: (list[T] | set[T])[int]

def _():
    # TODO: `list[Unknown] | set[Unknown]` might be better
    reveal_type(x)  # revealed: Unknown
```

### Multiple definitions

#### Shadowed definitions

When a generic type alias shadows a definition from an outer scope, the inner definition is used:

```py
from typing_extensions import TypeVar

T = TypeVar("T")

MyAlias = list[T]

def outer():
    MyAlias = set[T]

    def _(x: MyAlias[int]):
        reveal_type(x)  # revealed: set[int]
```

#### Statically known conditions

```py
from typing_extensions import TypeVar

T = TypeVar("T")

if True:
    MyAlias1 = list[T]
else:
    MyAlias1 = set[T]

if False:
    MyAlias2 = list[T]
else:
    MyAlias2 = set[T]

def _(
    x1: MyAlias1[int],
    x2: MyAlias2[int],
):
    reveal_type(x1)  # revealed: list[int]
    reveal_type(x2)  # revealed: set[int]
```

#### Statically unknown conditions

If several definitions are visible, we emit an error:

```py
from typing_extensions import TypeVar

T = TypeVar("T")

def flag() -> bool:
    return True

if flag():
    MyAlias = list[T]
else:
    MyAlias = set[T]

# It is questionable whether this should be supported or not. It might also be reasonable to
# emit an error here (e.g. "Invalid subscript of object of type `<class 'list[T@MyAlias]'> |
# <class 'set[T@MyAlias]'>` in type expression"). If we ever choose to do so, the revealed
# type should probably be `Unknown`.
def _(x: MyAlias[int]):
    reveal_type(x)  # revealed: list[int] | set[int]
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

reveal_type(C().old)  # revealed: int
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

reveal_type(MyOptionalInt)  # revealed: <types.UnionType special-form 'int | None'>

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

reveal_type(MyLiteralString)  # revealed: <special-form 'typing.LiteralString'>
reveal_type(MyNoReturn)  # revealed: <special-form 'typing.NoReturn'>
reveal_type(MyNever)  # revealed: <special-form 'typing.Never'>

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

We support implicit type aliases using `typing.Tuple`:

```py
from typing import Tuple

IntAndStr = Tuple[int, str]
SingleInt = Tuple[int]
Ints = Tuple[int, ...]
EmptyTuple = Tuple[()]

def _(int_and_str: IntAndStr, single_int: SingleInt, ints: Ints, empty_tuple: EmptyTuple):
    reveal_type(int_and_str)  # revealed: tuple[int, str]
    reveal_type(single_int)  # revealed: tuple[int]
    reveal_type(ints)  # revealed: tuple[int, ...]
    reveal_type(empty_tuple)  # revealed: tuple[()]
```

Invalid uses cause diagnostics:

```py
from typing import Tuple

# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
Invalid = Tuple[int, 1]

def _(invalid: Invalid):
    reveal_type(invalid)  # revealed: tuple[int, Unknown]
```

## `Union`

We support implicit type aliases using `typing.Union`:

```py
from typing import Union

IntOrStr = Union[int, str]
IntOrStrOrBytes = Union[int, Union[str, bytes]]

reveal_type(IntOrStr)  # revealed: <types.UnionType special-form 'int | str'>
reveal_type(IntOrStrOrBytes)  # revealed: <types.UnionType special-form 'int | str | bytes'>

def _(
    int_or_str: IntOrStr,
    int_or_str_or_bytes: IntOrStrOrBytes,
):
    reveal_type(int_or_str)  # revealed: int | str
    reveal_type(int_or_str_or_bytes)  # revealed: int | str | bytes
```

If a single type is given, no `types.UnionType` instance is created:

```py
JustInt = Union[int]

reveal_type(JustInt)  # revealed: <class 'int'>

def _(just_int: JustInt):
    reveal_type(just_int)  # revealed: int
```

An empty `typing.Union` leads to a `TypeError` at runtime, so we emit an error. We still infer
`Never` when used as a type expression, which seems reasonable for an empty union:

```py
# error: [invalid-type-form] "`typing.Union` requires at least one type argument"
EmptyUnion = Union[()]

reveal_type(EmptyUnion)  # revealed: <types.UnionType special-form 'Never'>

def _(empty: EmptyUnion):
    reveal_type(empty)  # revealed: Never
```

Other invalid uses are also caught:

```py
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
Invalid = Union[str, 1]

def _(
    invalid: Invalid,
):
    reveal_type(invalid)  # revealed: str | Unknown
```

## `type[…]` and `Type[…]`

### `type[…]`

We support implicit type aliases using `type[…]`:

```py
from typing import Any, Union, Protocol, TypeVar, Generic

T = TypeVar("T")

class A: ...
class B: ...
class G(Generic[T]): ...

class P(Protocol):
    def method(self) -> None: ...

SubclassOfA = type[A]
SubclassOfAny = type[Any]
SubclassOfAOrB1 = type[A | B]
SubclassOfAOrB2 = type[A] | type[B]
SubclassOfAOrB3 = Union[type[A], type[B]]
SubclassOfG = type[G]
SubclassOfGInt = type[G[int]]
SubclassOfP = type[P]

reveal_type(SubclassOfA)  # revealed: <special-form 'type[A]'>
reveal_type(SubclassOfAny)  # revealed: <special-form 'type[Any]'>
reveal_type(SubclassOfAOrB1)  # revealed: <special-form 'type[A | B]'>
reveal_type(SubclassOfAOrB2)  # revealed: <types.UnionType special-form 'type[A] | type[B]'>
reveal_type(SubclassOfAOrB3)  # revealed: <types.UnionType special-form 'type[A] | type[B]'>
reveal_type(SubclassOfG)  # revealed: <special-form 'type[G[Unknown]]'>
reveal_type(SubclassOfGInt)  # revealed: <special-form 'type[G[int]]'>
reveal_type(SubclassOfP)  # revealed: <special-form 'type[P]'>

def _(
    subclass_of_a: SubclassOfA,
    subclass_of_any: SubclassOfAny,
    subclass_of_a_or_b1: SubclassOfAOrB1,
    subclass_of_a_or_b2: SubclassOfAOrB2,
    subclass_of_a_or_b3: SubclassOfAOrB3,
    subclass_of_g: SubclassOfG,
    subclass_of_g_int: SubclassOfGInt,
    subclass_of_p: SubclassOfP,
):
    reveal_type(subclass_of_a)  # revealed: type[A]
    reveal_type(subclass_of_a())  # revealed: A

    reveal_type(subclass_of_any)  # revealed: type[Any]
    reveal_type(subclass_of_any())  # revealed: Any

    reveal_type(subclass_of_a_or_b1)  # revealed: type[A] | type[B]
    reveal_type(subclass_of_a_or_b1())  # revealed: A | B

    reveal_type(subclass_of_a_or_b2)  # revealed: type[A] | type[B]
    reveal_type(subclass_of_a_or_b2())  # revealed: A | B

    reveal_type(subclass_of_a_or_b3)  # revealed: type[A] | type[B]
    reveal_type(subclass_of_a_or_b3())  # revealed: A | B

    reveal_type(subclass_of_g)  # revealed: type[G[Unknown]]
    reveal_type(subclass_of_g())  # revealed: G[Unknown]

    reveal_type(subclass_of_g_int)  # revealed: type[G[int]]
    reveal_type(subclass_of_g_int())  # revealed: G[int]

    reveal_type(subclass_of_p)  # revealed: type[P]
```

Using `type[]` with a union type alias distributes the `type[]` over the union elements:

```py
from typing import Union

class C: ...
class D: ...

UnionAlias1 = C | D
UnionAlias2 = Union[C, D]

SubclassOfUnionAlias1 = type[UnionAlias1]
SubclassOfUnionAlias2 = type[UnionAlias2]

reveal_type(SubclassOfUnionAlias1)  # revealed: <special-form 'type[C | D]'>
reveal_type(SubclassOfUnionAlias2)  # revealed: <special-form 'type[C | D]'>

def _(
    subclass_of_union_alias1: SubclassOfUnionAlias1,
    subclass_of_union_alias2: SubclassOfUnionAlias2,
):
    reveal_type(subclass_of_union_alias1)  # revealed: type[C] | type[D]
    reveal_type(subclass_of_union_alias1())  # revealed: C | D

    reveal_type(subclass_of_union_alias2)  # revealed: type[C] | type[D]
    reveal_type(subclass_of_union_alias2())  # revealed: C | D
```

Invalid uses result in diagnostics:

```py
from typing import Literal

# error: [invalid-type-form]
InvalidSubclassOf1 = type[1]

# TODO: This should be an error
InvalidSubclassOfLiteral = type[Literal[42]]

def _(
    invalid_subclass_of_1: InvalidSubclassOf1,
    invalid_subclass_of_literal: InvalidSubclassOfLiteral,
):
    reveal_type(invalid_subclass_of_1)  # revealed: type[Unknown]
    # TODO: this should be `type[Unknown]` or `Unknown`
    reveal_type(invalid_subclass_of_literal)  # revealed: <class 'int'>
```

### `Type[…]`

The same also works for `typing.Type[…]`:

```py
from typing import Any, Union, Protocol, TypeVar, Generic, Type

T = TypeVar("T")

class A: ...
class B: ...
class G(Generic[T]): ...

class P(Protocol):
    def method(self) -> None: ...

SubclassOfA = Type[A]
SubclassOfAny = Type[Any]
SubclassOfAOrB1 = Type[A | B]
SubclassOfAOrB2 = Type[A] | Type[B]
SubclassOfAOrB3 = Union[Type[A], Type[B]]
SubclassOfG = Type[G]
SubclassOfGInt = Type[G[int]]
SubclassOfP = Type[P]

reveal_type(SubclassOfA)  # revealed: <special-form 'type[A]'>
reveal_type(SubclassOfAny)  # revealed: <special-form 'type[Any]'>
reveal_type(SubclassOfAOrB1)  # revealed: <special-form 'type[A | B]'>
reveal_type(SubclassOfAOrB2)  # revealed: <types.UnionType special-form 'type[A] | type[B]'>
reveal_type(SubclassOfAOrB3)  # revealed: <types.UnionType special-form 'type[A] | type[B]'>
reveal_type(SubclassOfG)  # revealed: <special-form 'type[G[Unknown]]'>
reveal_type(SubclassOfGInt)  # revealed: <special-form 'type[G[int]]'>
reveal_type(SubclassOfP)  # revealed: <special-form 'type[P]'>

def _(
    subclass_of_a: SubclassOfA,
    subclass_of_any: SubclassOfAny,
    subclass_of_a_or_b1: SubclassOfAOrB1,
    subclass_of_a_or_b2: SubclassOfAOrB2,
    subclass_of_a_or_b3: SubclassOfAOrB3,
    subclass_of_g: SubclassOfG,
    subclass_of_g_int: SubclassOfGInt,
    subclass_of_p: SubclassOfP,
):
    reveal_type(subclass_of_a)  # revealed: type[A]
    reveal_type(subclass_of_a())  # revealed: A

    reveal_type(subclass_of_any)  # revealed: type[Any]
    reveal_type(subclass_of_any())  # revealed: Any

    reveal_type(subclass_of_a_or_b1)  # revealed: type[A] | type[B]
    reveal_type(subclass_of_a_or_b1())  # revealed: A | B

    reveal_type(subclass_of_a_or_b2)  # revealed: type[A] | type[B]
    reveal_type(subclass_of_a_or_b2())  # revealed: A | B

    reveal_type(subclass_of_a_or_b3)  # revealed: type[A] | type[B]
    reveal_type(subclass_of_a_or_b3())  # revealed: A | B

    reveal_type(subclass_of_g)  # revealed: type[G[Unknown]]
    reveal_type(subclass_of_g())  # revealed: G[Unknown]

    reveal_type(subclass_of_g_int)  # revealed: type[G[int]]
    reveal_type(subclass_of_g_int())  # revealed: G[int]

    reveal_type(subclass_of_p)  # revealed: type[P]
```

Invalid uses result in diagnostics:

```py
# error: [invalid-type-form]
InvalidSubclass = Type[1]
```

## Other `typing` special forms

The following special forms from the `typing` module are also supported in implicit type aliases:

```py
from typing import List, Dict, Set, FrozenSet, ChainMap, Counter, DefaultDict, Deque, OrderedDict

MyList = List[str]
MySet = Set[str]
MyDict = Dict[str, int]
MyFrozenSet = FrozenSet[str]
MyChainMap = ChainMap[str, int]
MyCounter = Counter[str]
MyDefaultDict = DefaultDict[str, int]
MyDeque = Deque[str]
MyOrderedDict = OrderedDict[str, int]

reveal_type(MyList)  # revealed: <class 'list[str]'>
reveal_type(MySet)  # revealed: <class 'set[str]'>
reveal_type(MyDict)  # revealed: <class 'dict[str, int]'>
reveal_type(MyFrozenSet)  # revealed: <class 'frozenset[str]'>
reveal_type(MyChainMap)  # revealed: <class 'ChainMap[str, int]'>
reveal_type(MyCounter)  # revealed: <class 'Counter[str]'>
reveal_type(MyDefaultDict)  # revealed: <class 'defaultdict[str, int]'>
reveal_type(MyDeque)  # revealed: <class 'deque[str]'>
reveal_type(MyOrderedDict)  # revealed: <class 'OrderedDict[str, int]'>

def _(
    my_list: MyList,
    my_set: MySet,
    my_dict: MyDict,
    my_frozen_set: MyFrozenSet,
    my_chain_map: MyChainMap,
    my_counter: MyCounter,
    my_default_dict: MyDefaultDict,
    my_deque: MyDeque,
    my_ordered_dict: MyOrderedDict,
):
    reveal_type(my_list)  # revealed: list[str]
    reveal_type(my_set)  # revealed: set[str]
    reveal_type(my_dict)  # revealed: dict[str, int]
    reveal_type(my_frozen_set)  # revealed: frozenset[str]
    reveal_type(my_chain_map)  # revealed: ChainMap[str, int]
    reveal_type(my_counter)  # revealed: Counter[str]
    reveal_type(my_default_dict)  # revealed: defaultdict[str, int]
    reveal_type(my_deque)  # revealed: deque[str]
    reveal_type(my_ordered_dict)  # revealed: OrderedDict[str, int]
```

All of them are supported in unions:

```py
NoneOrList = None | List[str]
NoneOrSet = None | Set[str]
NoneOrDict = None | Dict[str, int]
NoneOrFrozenSet = None | FrozenSet[str]
NoneOrChainMap = None | ChainMap[str, int]
NoneOrCounter = None | Counter[str]
NoneOrDefaultDict = None | DefaultDict[str, int]
NoneOrDeque = None | Deque[str]
NoneOrOrderedDict = None | OrderedDict[str, int]

ListOrNone = List[int] | None
SetOrNone = Set[int] | None
DictOrNone = Dict[str, int] | None
FrozenSetOrNone = FrozenSet[int] | None
ChainMapOrNone = ChainMap[str, int] | None
CounterOrNone = Counter[str] | None
DefaultDictOrNone = DefaultDict[str, int] | None
DequeOrNone = Deque[str] | None
OrderedDictOrNone = OrderedDict[str, int] | None

reveal_type(NoneOrList)  # revealed: <types.UnionType special-form 'None | list[str]'>
reveal_type(NoneOrSet)  # revealed: <types.UnionType special-form 'None | set[str]'>
reveal_type(NoneOrDict)  # revealed: <types.UnionType special-form 'None | dict[str, int]'>
reveal_type(NoneOrFrozenSet)  # revealed: <types.UnionType special-form 'None | frozenset[str]'>
reveal_type(NoneOrChainMap)  # revealed: <types.UnionType special-form 'None | ChainMap[str, int]'>
reveal_type(NoneOrCounter)  # revealed: <types.UnionType special-form 'None | Counter[str]'>
reveal_type(NoneOrDefaultDict)  # revealed: <types.UnionType special-form 'None | defaultdict[str, int]'>
reveal_type(NoneOrDeque)  # revealed: <types.UnionType special-form 'None | deque[str]'>
reveal_type(NoneOrOrderedDict)  # revealed: <types.UnionType special-form 'None | OrderedDict[str, int]'>

reveal_type(ListOrNone)  # revealed: <types.UnionType special-form 'list[int] | None'>
reveal_type(SetOrNone)  # revealed: <types.UnionType special-form 'set[int] | None'>
reveal_type(DictOrNone)  # revealed: <types.UnionType special-form 'dict[str, int] | None'>
reveal_type(FrozenSetOrNone)  # revealed: <types.UnionType special-form 'frozenset[int] | None'>
reveal_type(ChainMapOrNone)  # revealed: <types.UnionType special-form 'ChainMap[str, int] | None'>
reveal_type(CounterOrNone)  # revealed: <types.UnionType special-form 'Counter[str] | None'>
reveal_type(DefaultDictOrNone)  # revealed: <types.UnionType special-form 'defaultdict[str, int] | None'>
reveal_type(DequeOrNone)  # revealed: <types.UnionType special-form 'deque[str] | None'>
reveal_type(OrderedDictOrNone)  # revealed: <types.UnionType special-form 'OrderedDict[str, int] | None'>

def _(
    none_or_list: NoneOrList,
    none_or_set: NoneOrSet,
    none_or_dict: NoneOrDict,
    none_or_frozen_set: NoneOrFrozenSet,
    none_or_chain_map: NoneOrChainMap,
    none_or_counter: NoneOrCounter,
    none_or_default_dict: NoneOrDefaultDict,
    none_or_deque: NoneOrDeque,
    none_or_ordered_dict: NoneOrOrderedDict,
    list_or_none: ListOrNone,
    set_or_none: SetOrNone,
    dict_or_none: DictOrNone,
    frozen_set_or_none: FrozenSetOrNone,
    chain_map_or_none: ChainMapOrNone,
    counter_or_none: CounterOrNone,
    default_dict_or_none: DefaultDictOrNone,
    deque_or_none: DequeOrNone,
    ordered_dict_or_none: OrderedDictOrNone,
):
    reveal_type(none_or_list)  # revealed: None | list[str]
    reveal_type(none_or_set)  # revealed: None | set[str]
    reveal_type(none_or_dict)  # revealed: None | dict[str, int]
    reveal_type(none_or_frozen_set)  # revealed: None | frozenset[str]
    reveal_type(none_or_chain_map)  # revealed: None | ChainMap[str, int]
    reveal_type(none_or_counter)  # revealed: None | Counter[str]
    reveal_type(none_or_default_dict)  # revealed: None | defaultdict[str, int]
    reveal_type(none_or_deque)  # revealed: None | deque[str]
    reveal_type(none_or_ordered_dict)  # revealed: None | OrderedDict[str, int]

    reveal_type(list_or_none)  # revealed: list[int] | None
    reveal_type(set_or_none)  # revealed: set[int] | None
    reveal_type(dict_or_none)  # revealed: dict[str, int] | None
    reveal_type(frozen_set_or_none)  # revealed: frozenset[int] | None
    reveal_type(chain_map_or_none)  # revealed: ChainMap[str, int] | None
    reveal_type(counter_or_none)  # revealed: Counter[str] | None
    reveal_type(default_dict_or_none)  # revealed: defaultdict[str, int] | None
    reveal_type(deque_or_none)  # revealed: deque[str] | None
    reveal_type(ordered_dict_or_none)  # revealed: OrderedDict[str, int] | None
```

Invalid uses result in diagnostics:

```py
from typing import List, Dict

# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
InvalidList = List[1]

# error: [invalid-type-form] "`typing.List` requires exactly one argument"
ListTooManyArgs = List[int, str]

# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
InvalidDict1 = Dict[1, str]

# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
InvalidDict2 = Dict[str, 2]

# error: [invalid-type-form] "`typing.Dict` requires exactly two arguments, got 1"
DictTooFewArgs = Dict[str]

# error: [invalid-type-form] "`typing.Dict` requires exactly two arguments, got 3"
DictTooManyArgs = Dict[str, int, float]

def _(
    invalid_list: InvalidList,
    list_too_many_args: ListTooManyArgs,
    invalid_dict1: InvalidDict1,
    invalid_dict2: InvalidDict2,
    dict_too_few_args: DictTooFewArgs,
    dict_too_many_args: DictTooManyArgs,
):
    reveal_type(invalid_list)  # revealed: list[Unknown]
    reveal_type(list_too_many_args)  # revealed: list[Unknown]
    reveal_type(invalid_dict1)  # revealed: dict[Unknown, str]
    reveal_type(invalid_dict2)  # revealed: dict[str, Unknown]
    reveal_type(dict_too_few_args)  # revealed: dict[str, Unknown]
    reveal_type(dict_too_many_args)  # revealed: dict[Unknown, Unknown]
```

## `Callable[...]`

We support implicit type aliases using `Callable[...]`:

```py
from typing import Callable, Union

CallableNoArgs = Callable[[], None]
BasicCallable = Callable[[int, str], bytes]
GradualCallable = Callable[..., str]

reveal_type(CallableNoArgs)  # revealed: <typing.Callable special-form '() -> None'>
reveal_type(BasicCallable)  # revealed: <typing.Callable special-form '(int, str, /) -> bytes'>
reveal_type(GradualCallable)  # revealed: <typing.Callable special-form '(...) -> str'>

def _(
    callable_no_args: CallableNoArgs,
    basic_callable: BasicCallable,
    gradual_callable: GradualCallable,
):
    reveal_type(callable_no_args)  # revealed: () -> None
    reveal_type(basic_callable)  # revealed: (int, str, /) -> bytes
    reveal_type(gradual_callable)  # revealed: (...) -> str
```

Nested callables work as expected:

```py
TakesCallable = Callable[[Callable[[int], str]], bytes]
ReturnsCallable = Callable[[int], Callable[[str], bytes]]

def _(takes_callable: TakesCallable, returns_callable: ReturnsCallable):
    reveal_type(takes_callable)  # revealed: ((int, /) -> str, /) -> bytes
    reveal_type(returns_callable)  # revealed: (int, /) -> (str, /) -> bytes
```

Invalid uses result in diagnostics:

```py
# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
InvalidCallable1 = Callable[[int]]

# error: [invalid-type-form] "The first argument to `Callable` must be either a list of types, ParamSpec, Concatenate, or `...`"
InvalidCallable2 = Callable[int, str]

reveal_type(InvalidCallable1)  # revealed: <typing.Callable special-form '(...) -> Unknown'>
reveal_type(InvalidCallable2)  # revealed: <typing.Callable special-form '(...) -> Unknown'>

def _(invalid_callable1: InvalidCallable1, invalid_callable2: InvalidCallable2):
    reveal_type(invalid_callable1)  # revealed: (...) -> Unknown
    reveal_type(invalid_callable2)  # revealed: (...) -> Unknown
```

## Stringified annotations

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
from typing import Union, List, Dict, Annotated, Callable

ListOfInts1 = list["int"]
ListOfInts2 = List["int"]
StrOrStyle = Union[str, "Style"]
SubclassOfStyle = type["Style"]
DictStrToStyle = Dict[str, "Style"]
AnnotatedStyle = Annotated["Style", "metadata"]
CallableStyleToStyle = Callable[["Style"], "Style"]

class Style: ...

def _(
    list_of_ints1: ListOfInts1,
    list_of_ints2: ListOfInts2,
    str_or_style: StrOrStyle,
    subclass_of_style: SubclassOfStyle,
    dict_str_to_style: DictStrToStyle,
    annotated_style: AnnotatedStyle,
    callable_style_to_style: CallableStyleToStyle,
):
    reveal_type(list_of_ints1)  # revealed: list[int]
    reveal_type(list_of_ints2)  # revealed: list[int]
    reveal_type(str_or_style)  # revealed: str | Style
    reveal_type(subclass_of_style)  # revealed: type[Style]
    reveal_type(dict_str_to_style)  # revealed: dict[str, Style]
    reveal_type(annotated_style)  # revealed: Style
    reveal_type(callable_style_to_style)  # revealed: (Style, /) -> Style
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
from typing import List, Dict

RecursiveList1 = list["RecursiveList1" | None]
RecursiveList2 = List["RecursiveList2" | None]
RecursiveDict1 = dict[str, "RecursiveDict1" | None]
RecursiveDict2 = Dict[str, "RecursiveDict2" | None]
RecursiveDict3 = dict["RecursiveDict3", int]
RecursiveDict4 = Dict["RecursiveDict4", int]

def _(
    recursive_list1: RecursiveList1,
    recursive_list2: RecursiveList2,
    recursive_dict1: RecursiveDict1,
    recursive_dict2: RecursiveDict2,
    recursive_dict3: RecursiveDict3,
    recursive_dict4: RecursiveDict4,
):
    reveal_type(recursive_list1)  # revealed: list[Divergent]
    reveal_type(recursive_list2)  # revealed: list[Divergent]
    reveal_type(recursive_dict1)  # revealed: dict[str, Divergent]
    reveal_type(recursive_dict2)  # revealed: dict[str, Divergent]
    reveal_type(recursive_dict3)  # revealed: dict[Divergent, int]
    reveal_type(recursive_dict4)  # revealed: dict[Divergent, int]
```

### Self-referential generic implicit type aliases

```py
from typing import TypeVar

T = TypeVar("T")

NestedDict = dict[str, "NestedDict[T] | T"]
NestedList = list["NestedList[T] | None"]

def _(
    nested_dict_int: NestedDict[int],
    nested_list_str: NestedList[str],
):
    reveal_type(nested_dict_int)  # revealed: dict[str, Divergent]
    reveal_type(nested_list_str)  # revealed: list[Divergent]
```
