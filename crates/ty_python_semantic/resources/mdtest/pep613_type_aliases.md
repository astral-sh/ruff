# PEP 613 type aliases

PEP 613 type aliases are simple assignment statements, annotated with `typing.TypeAlias` to mark
them as a type alias. At runtime, they behave the same as implicit type aliases. Our support for
them is currently the same as for implicit type aliases, but we don't reproduce the full
implicit-type-alias test suite here, just some particularly interesting cases.

## Basic

### as `TypeAlias`

```py
from typing import TypeAlias

IntOrStr: TypeAlias = int | str

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

### as `typing.TypeAlias`

```py
import typing

IntOrStr: typing.TypeAlias = int | str

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

## Can be used as value

Because PEP 613 type aliases are just annotated assignments, they can be used as values, like a
legacy type expression (and unlike a PEP 695 type alias). We might prefer this wasn't allowed, but
people do use it.

```py
from typing import TypeAlias

MyExc: TypeAlias = Exception

try:
    raise MyExc("error")
except MyExc as e:
    reveal_type(e)  # revealed: Exception
```

## Can be (partially) stringified

```py
from typing import TypeAlias, Optional, TypeVar, Generic

class A: ...

OptionalA1: TypeAlias = Optional[A]
OptionalA2: TypeAlias = Optional["A"]
OptionalA3: TypeAlias = "Optional[A]"

def _(a: OptionalA1, b: OptionalA2, c: OptionalA3) -> None:
    reveal_type(a)  # revealed: A | None
    reveal_type(b)  # revealed: A | None
    reveal_type(c)  # revealed: A | None

T = TypeVar("T")

class MyGenericClass(Generic[T]): ...

MyGenericAlias1: TypeAlias = MyGenericClass[A]
MyGenericAlias2: TypeAlias = MyGenericClass["A"]
MyGenericAlias3: TypeAlias = "MyGenericClass[A]"

def _(a: MyGenericAlias1, b: MyGenericAlias2, c: MyGenericAlias3) -> None:
    reveal_type(a)  # revealed: MyGenericClass[A]
    reveal_type(b)  # revealed: MyGenericClass[A]
    reveal_type(c)  # revealed: MyGenericClass[A]
```

## Can inherit from an alias

```py
from typing import TypeAlias
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

MyList: TypeAlias = list["int"]

class Foo(MyList): ...

static_assert(is_subtype_of(Foo, list[int]))
```

## Cannot inherit from a stringified alias

```py
from typing import TypeAlias

MyList: TypeAlias = "list[int]"

# error: [invalid-base] "Invalid class base with type `str`"
class Foo(MyList): ...
```

## Unknown type in PEP 604 union

If we run into an unknown type in a PEP 604 union in the right-hand side of a PEP 613 type alias, we
still understand it as a union type, just with an unknown element.

```py
from typing import TypeAlias
from nonexistent import unknown_type  # error: [unresolved-import]

MyAlias: TypeAlias = int | unknown_type | str

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | Unknown | str
```

## Callable type in union

```py
from typing import TypeAlias, Callable

MyAlias: TypeAlias = int | Callable[[str], int]

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | ((str, /) -> int)
```

## Generic aliases

A more comprehensive set of tests can be found in
[`implicit_type_aliases.md`](./implicit_type_aliases.md). If the implementations ever diverge, we
may need to duplicate more tests here.

### Basic

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")

MyList: TypeAlias = list[T]
ListOrSet: TypeAlias = list[T] | set[T]

reveal_type(MyList)  # revealed: <class 'list[T]'>
reveal_type(ListOrSet)  # revealed: <types.UnionType special-form 'list[T] | set[T]'>

def _(list_of_int: MyList[int], list_or_set_of_str: ListOrSet[str]):
    reveal_type(list_of_int)  # revealed: list[int]
    reveal_type(list_or_set_of_str)  # revealed: list[str] | set[str]
```

### Stringified generic alias

#### Explicitly specialized

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")
U = TypeVar("U")

TotallyStringifiedPEP613: TypeAlias = "dict[T, U]"
TotallyStringifiedPartiallySpecialized: TypeAlias = "TotallyStringifiedPEP613[U, int]"

def f(x: "TotallyStringifiedPartiallySpecialized[str]"):
    reveal_type(x)  # revealed: dict[str, int]
```

#### Unsubscripted

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")

ListAlias: TypeAlias = "list[T]"

def takes_list(value: ListAlias) -> None:
    reveal_type(value)  # revealed: list[Unknown]

takes_list([1])
```

## Class-scoped type variables

```toml
[environment]
python-version = "3.12"
```

A legacy generic alias binds its own type variables and cannot capture a type variable already bound
to its enclosing class. The restriction also applies to stringified aliases.

```py
from typing import Generic, TypeAlias, TypeVar

T = TypeVar("T")
S = TypeVar("S")

class Box(Generic[T]):
    # error: [invalid-type-form] "Type alias cannot capture class-scoped type variable `T`"
    Items: TypeAlias = list[T]
    # error: [invalid-type-form] "Type alias cannot capture class-scoped type variable `T`"
    Quoted: TypeAlias = "list[T]"

    Independent: TypeAlias = list[S]
    Concrete: TypeAlias = list[int]

reveal_type(Box.Independent[str]())  # revealed: list[str]
reveal_type(Box.Concrete())  # revealed: list[int]
```

PEP 695 `type` statements can capture the enclosing class's type parameters, but using `TypeAlias`
inside a PEP 695 class still follows the legacy alias rules.

```py
class Modern[T]:
    type Items = list[T]
    # error: [invalid-type-form] "Type alias cannot capture class-scoped type variable `T`"
    Legacy: TypeAlias = list[T]
```

The same restriction applies to class-scoped `ParamSpec` and `TypeVarTuple` parameters.

```py
from typing import Callable, ParamSpec, TypeVarTuple

P = ParamSpec("P")
Ts = TypeVarTuple("Ts")

class Callbacks(Generic[P]):
    # error: [invalid-type-form] "Type alias cannot capture class-scoped type variable `P`"
    Callback: TypeAlias = Callable[P, None]

class Tuples(Generic[*Ts]):
    # error: [invalid-type-form] "Type alias cannot capture class-scoped type variable `Ts`"
    Items: TypeAlias = tuple[*Ts]
```

## Subscripted generic alias in union

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")

Alias1: TypeAlias = list[T] | set[T]
MyAlias: TypeAlias = int | Alias1[str]

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | list[str] | set[str]
```

## Typevar-specialized dynamic types

We still recognize type aliases as being generic if a symbol of a dynamic type is explicitly
specialized with a type variable:

```py
from typing import TypeVar, TypeAlias

from unknown_module import UnknownClass  # type: ignore

T = TypeVar("T")

MyAlias1: TypeAlias = UnknownClass[T] | None

def _(a: MyAlias1[int]):
    reveal_type(a)  # revealed: Unknown | None
```

This also works with multiple type arguments:

```py
U = TypeVar("U")
V = TypeVar("V")

MyAlias2: TypeAlias = UnknownClass[T, U, V] | int

def _(a: MyAlias2[int, str, bytes]):
    reveal_type(a)  # revealed: Unknown | int
```

If we specialize with fewer or more type arguments than expected, we emit an error:

```py
def _(
    # error: [invalid-type-arguments] "No type argument provided for required type variable `V`"
    too_few: MyAlias2[int, str],
    # error: [invalid-type-arguments] "Too many type arguments: expected 3, got 4"
    too_many: MyAlias2[int, str, bytes, float],
): ...
```

We can also reference these type aliases from other type aliases:

```py
MyAlias3: TypeAlias = MyAlias1[str] | MyAlias2[int, str, bytes]

def _(c: MyAlias3):
    reveal_type(c)  # revealed: Unknown | None | int
```

Here, we test some other cases that might involve `@Todo` types, which also need special handling:

```py
from typing_extensions import Callable, Concatenate, TypeAliasType

MyAlias4: TypeAlias = Callable[Concatenate[dict[str, T], ...], list[U]]

def _(c: MyAlias4[int, str]):
    reveal_type(c)  # revealed: (dict[str, int], /, *args: Any, **kwargs: Any) -> list[str]
```

## Explicit aliases using `TypeAliasType`

```py
from typing import TypeAlias, TypeVar
from typing_extensions import Callable, Concatenate, TypeAliasType

T = TypeVar("T")

MyList = TypeAliasType("MyList", list[T], type_params=(T,))

MyAlias5 = Callable[[MyList[T]], int]

def _(c: MyAlias5[int]):
    reveal_type(c)  # revealed: (MyList[int], /) -> int

K = TypeVar("K")
V = TypeVar("V")

MyDict = TypeAliasType("MyDict", dict[K, V], type_params=(K, V))

MyAlias6 = Callable[[MyDict[K, V]], int]

def _(c: MyAlias6[str, bytes]):
    reveal_type(c)  # revealed: (MyDict[str, bytes], /) -> int

ListOrDict: TypeAlias = MyList[T] | dict[str, T]

def _(x: ListOrDict[int]):
    reveal_type(x)  # revealed: list[int] | dict[str, int]

MyAlias7: TypeAlias = Callable[Concatenate[T, ...], None]

def _(c: MyAlias7[int]):
    reveal_type(c)  # revealed: (int, /, *args: Any, **kwargs: Any) -> None
```

## Imported

`alias.py`:

```py
from typing import TypeAlias

MyAlias: TypeAlias = int | str
```

`main.py`:

```py
from alias import MyAlias

def _(x: MyAlias):
    reveal_type(x)  # revealed: int | str
```

## String literal in right-hand side

```py
from typing import TypeAlias

IntOrStr: TypeAlias = "int | str"

def _(x: IntOrStr):
    reveal_type(x)  # revealed: int | str
```

## Cyclic

```py
from typing import TypeAlias, TypeVar, Union
from types import UnionType

RecursiveTuple: TypeAlias = tuple["int | RecursiveTuple", str]

def _(rec: RecursiveTuple):
    reveal_type(rec)  # revealed: RecursiveTuple

RecursiveHomogeneousTuple: TypeAlias = tuple["int | RecursiveHomogeneousTuple", ...]

def _(rec: RecursiveHomogeneousTuple):
    reveal_type(rec)  # revealed: RecursiveHomogeneousTuple

ClassInfo: TypeAlias = type | UnionType | tuple["ClassInfo", ...]
reveal_type(ClassInfo)  # revealed: <types.UnionType special-form 'type | UnionType | tuple[ClassInfo, ...]'>
```

The following alias is invalid because its cycle passes through no containing type. It falls back to
`Divergent` when used in an annotation.

```py
Unguarded: TypeAlias = "int | Unguarded"  # error: [cyclic-type-alias-definition]

def unguarded(value: Unguarded):
    reveal_type(value)  # revealed: Divergent

def my_isinstance(obj: object, classinfo: ClassInfo) -> bool:
    reveal_type(classinfo)  # revealed: ClassInfo
    return isinstance(obj, classinfo)

K = TypeVar("K")
V = TypeVar("V")
NestedDict: TypeAlias = dict[K, Union[V, "NestedDict[K, V]"]]

def _(nested: NestedDict[str, int]):
    reveal_type(nested)  # revealed: NestedDict[str, int]

T = TypeVar("T")
Even: TypeAlias = T | list["Odd[T]"]
Odd: TypeAlias = T | tuple["Even[T]"]

def invalid_even() -> Even[int]:
    return [("bad",)]  # error: [invalid-return-type]

my_isinstance(1, int)
my_isinstance(1, int | str)
my_isinstance(1, (int, str))
my_isinstance(1, (int, (str, float)))
my_isinstance(1, (int, (str | float)))
# error: [invalid-argument-type]
my_isinstance(1, 1)
# error: [invalid-argument-type]
my_isinstance(1, (int, (str, 1)))
```

## Stringified recursive aliases

Quoting an entire recursive alias preserves its string value at runtime and its recursive type when
used in an annotation.

```py
from typing import TypeAlias

Nested: TypeAlias = "list[Nested]"
reveal_type(Nested)  # revealed: str

def inspect(value: Nested):
    reveal_type(value)  # revealed: Nested
```

## Specializing non-generic recursive aliases

A recursive alias without type variables cannot be specialized. Quoting the entire definition does
not make the recursive reference generic. The alias still has a string value at runtime.

```py
from typing import TypeAlias

Invalid: TypeAlias = "list[Invalid[int]]"  # error: [not-subscriptable]
reveal_type(Invalid)  # revealed: str
```

## Invalid cycles

An alias cannot expand directly to itself. Adding another member to a union does not make a cycle
valid.

```py
from typing import TypeAlias, Union

# snapshot: cyclic-type-alias-definition
Itself: TypeAlias = "Itself"
```

```snapshot
error[cyclic-type-alias-definition]: Type alias `Itself` has a circular definition
 --> src/mdtest_snippet.py:4:21
  |
4 | Itself: TypeAlias = "Itself"
  |                     ^^^^^^^^
```

Adding a union member still leaves a circular definition.

```py
IntOr: TypeAlias = Union[int, "IntOr"]  # error: [cyclic-type-alias-definition] "Type alias `IntOr` has a circular definition"
```

Both direct cycles and unions use a divergent type for recovery in annotations.

```py
def inspect(itself: Itself, int_or: IntOr):
    reveal_type(itself)  # revealed: Divergent
    reveal_type(int_or)  # revealed: Divergent
```

Mutually recursive aliases are also invalid when their cycle passes through no containing type. Each
alias in the cycle receives a diagnostic.

```py
First: TypeAlias = Union[int, "Second"]  # error: [cyclic-type-alias-definition] "Type alias `First` has a circular definition"
Second: TypeAlias = Union[str, "First"]  # error: [cyclic-type-alias-definition] "Type alias `Second` has a circular definition"

def inspect_mutual(first: First, second: Second):
    reveal_type(first)  # revealed: Divergent
    reveal_type(second)  # revealed: str | Divergent
```

## Recovery from nested invalid aliases

An invalid alias uses a divergent type for recovery even when referenced inside another alias.
Operations on that divergent type do not produce additional errors.

```py
from typing import TypeAlias

Bad: TypeAlias = "Bad"  # error: [cyclic-type-alias-definition]
Wrapped: TypeAlias = list[Bad]

def inspect(values: Wrapped):
    reveal_type(values[0])  # revealed: Divergent
    values[0]()
    values[0] + 1
```

## Cycles mixing alias syntaxes

A cycle can cross module boundaries and mix PEP 613 and PEP 695 aliases. Both declarations receive a
diagnostic when the cycle passes through no containing type.

```toml
[environment]
python-version = "3.12"
```

`b.py`:

```py
from typing import TypeAlias
from a import A

B: TypeAlias = "A"  # error: [cyclic-type-alias-definition]
```

`a.py`:

```py
from b import B

type A = B  # error: [cyclic-type-alias-definition]
```

## Invalid generic cycles

Specializing a recursive reference does not guard its cycle. A generic alias cannot include a
specialization of itself as a union member. The quoted alias still has a string value at runtime.

```py
from typing import TypeAlias, TypeVar, Union

T = TypeVar("T")
Alias: TypeAlias = Union[T, "Alias[T]"]  # error: [cyclic-type-alias-definition]
Growing: TypeAlias = "T | Growing[list[T]]"  # error: [cyclic-type-alias-definition]
reveal_type(Growing)  # revealed: str
```

Invalid generic aliases use a divergent type for recovery with or without type arguments.

```py
def inspect(bare: Alias, specialized: Alias[int], growing: Growing[int]):
    reveal_type(bare)  # revealed: Divergent
    reveal_type(specialized)  # revealed: Divergent
    reveal_type(growing)  # revealed: Divergent
```

The same restriction applies when generic aliases refer to each other, even when neither is used in
an annotation.

```py
First: TypeAlias = Union[T, "Second[T]"]  # error: [cyclic-type-alias-definition]
Second: TypeAlias = Union[str, "First[T]"]  # error: [cyclic-type-alias-definition]
```

Putting the recursive reference inside a container guards the cycle and preserves specialization.

```py
Valid: TypeAlias = Union[T, list["Valid[T]"]]

valid: Valid[int] = [1, [2]]
invalid: Valid[int] = ["bad"]  # error: [invalid-assignment]
```

## A cycle guarded by another alias

A direct reference to another alias is valid when the return path passes through a container. The
intermediate alias preserves its type argument.

```py
from typing import TypeAlias, TypeVar

T = TypeVar("T")
Forward: TypeAlias = "Container[T]"
Container: TypeAlias = tuple[T, "Forward[T] | None"]

valid: Forward[int] = (1, (2, None))
invalid: Forward[int] = (1, ("bad", None))  # error: [invalid-assignment]
```

## Direct and mutual recursion

An alias can combine a direct recursive member with a reference to another alias, provided both
paths pass through a containing type.

```py
from typing import TypeAlias, Union

Tree: TypeAlias = Union[list["Tree"], "Branch"]
Branch: TypeAlias = tuple[Tree]

valid: Tree = [([],)]
invalid: Tree = 1  # error: [invalid-assignment]
```

## Type parameters used only in recursive references

A PEP 613 alias remains generic when its type variable appears only in a recursive reference.
Specialized variable annotations still reject invalid nested values.

```py
from __future__ import annotations
from typing import TypeAlias, TypeVar

T = TypeVar("T")
NestedDict: TypeAlias = dict[str, "NestedDict[T]"]

valid: NestedDict[int] = {"nested": {}}
invalid: NestedDict[int] = {"nested": b"wrong"}  # error: [invalid-assignment]

def inspect(value: NestedDict[int]):
    local: NestedDict[int] = value
    reveal_type(local["nested"])  # revealed: NestedDict[int]
    invalid_local: NestedDict[int] = {"nested": {"leaf": 1}}  # error: [invalid-assignment]
```

The type variable also binds the alias when its entire definition is quoted.

```py
QuotedDict: TypeAlias = "dict[str, QuotedDict[T]]"
reveal_type(QuotedDict)  # revealed: str

def inspect_quoted(value: QuotedDict[int]):
    reveal_type(value["nested"])  # revealed: QuotedDict[int]
```

## Mutually recursive stringified aliases

Two quoted aliases can refer to each other. Each retains its string value, while their annotations
describe the alternating containers.

```py
from typing import TypeAlias

First: TypeAlias = "list[Second]"
Second: TypeAlias = "tuple[First]"
reveal_type(First)  # revealed: str
reveal_type(Second)  # revealed: str

def inspect(value: First):
    reveal_type(value[0])  # revealed: tuple[First]
```

## Materialization of self-referential generic PEP 613 type aliases

```py
from typing import TypeAlias, TypeVar, Union
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import is_subtype_of

K = TypeVar("K")
V = TypeVar("V")

NestedDict: TypeAlias = dict[K, Union[V, "NestedDict[K, V]"]]

static_assert(is_subtype_of(Bottom[NestedDict[str, int]], Top[NestedDict[str, int]]))
```

## Conditionally imported

```toml
[environment]
python-version = "3.9"
```

```py
try:
    # this fails at runtime, but we don't emit an error for it
    # because typeshed has removed its <3.10 branches for the stdlib
    from typing import TypeAlias
except ImportError:
    from typing_extensions import TypeAlias

MyAlias: TypeAlias = int

def _(x: MyAlias):
    reveal_type(x)  # revealed: int
```

## PEP-613 aliases in stubs are deferred

Although the right-hand side of a PEP-613 alias is a value expression, inference of this value is
deferred in a stub file, allowing for forward references:

`stub.pyi`:

```pyi
from typing import TypeAlias

MyAlias: TypeAlias = A | B

class A: ...
class B: ...
```

`module.py`:

```py
import stub

def f(x: stub.MyAlias): ...

f(stub.A())
f(stub.B())

class Unrelated: ...

# error: [invalid-argument-type]
f(Unrelated())
```

## Invalid position

`typing.TypeAlias` must be used as the sole annotation in an annotated assignment. Use in any other
context is an error.

```py
from typing import TypeAlias

# error: [invalid-type-form]
def _(x: TypeAlias):
    reveal_type(x)  # revealed: Unknown

# error: [invalid-type-form]
y: list[TypeAlias] = []
```

## Right-hand side is required

```py
from typing import TypeAlias

# error: [invalid-type-form]
Empty: TypeAlias
```

## Simple syntactic validation

We do full validation of the right-hand side of a type alias.

```toml
[environment]
python-version = "3.11"
```

```py
from typing_extensions import Annotated, Literal, TypeAlias

GoodTypeAlias: TypeAlias = Annotated[int, (1, 3.14, lambda x: x)]
GoodTypeAlias: TypeAlias = tuple[int, *tuple[str, ...]]

var1 = 3

# typing conformance cases:
BadTypeAlias1: TypeAlias = eval("".join(map(chr, [105, 110, 116])))  # error: [invalid-type-form]
BadTypeAlias2: TypeAlias = [int, str]  # error: [invalid-type-form]
BadTypeAlias3: TypeAlias = ((int, str),)  # error: [invalid-type-form]
BadTypeAlias4: TypeAlias = [int for i in range(1)]  # error: [invalid-type-form]
BadTypeAlias5: TypeAlias = {"a": "b"}  # error: [invalid-type-form]
BadTypeAlias6: TypeAlias = (lambda: int)()  # error: [invalid-type-form]
BadTypeAlias7: TypeAlias = [int][0]  # error: [invalid-type-form]
BadTypeAlias8: TypeAlias = int if 1 < 3 else str  # error: [invalid-type-form]
BadTypeAlias9: TypeAlias = var1  # error: [invalid-type-form]
BadTypeAlias10: TypeAlias = True  # error: [invalid-type-form]
BadTypeAlias11: TypeAlias = 1  # error: [invalid-type-form]
BadTypeAlias12: TypeAlias = list or set  # error: [invalid-type-form]
BadTypeAlias13: TypeAlias = f"{'int'}"  # error: [invalid-type-form]

# bonus ones from Alex:
#
# error:[invalid-type-form]
BadTypeAlias14: TypeAlias = Literal[3.14]
# error: [invalid-type-form]
BadTypeAlias15: TypeAlias = Literal[-3.14]
# error: [unsupported-operator]
BadTypeAlias16: TypeAlias = list["int" | "str"]
```

A tuple alias reports both a misplaced ellipsis and multiple unpacked variadic tuples, even though
both errors point to the same specialization.

```py
# error: [invalid-type-form] "`...` can only be used as the second element"
# error: [invalid-type-form] "Multiple unpacked variadic tuples are not allowed"
BadTuple: TypeAlias = tuple[int, ..., *tuple[str, ...], *tuple[bytes, ...]]
```

## No type qualifiers

The right-hand side of a type alias definition is a [type expression], not an annotation expression.
Type qualifiers like `ClassVar` and `Final` are only valid in annotation expressions, so they cannot
appear in type alias definitions:

```py
from typing_extensions import ClassVar, Final, Required, NotRequired, ReadOnly, TypeAlias, Unpack
from dataclasses import InitVar

bad1: TypeAlias = ClassVar[str]  # error: [invalid-type-form]
bad2: TypeAlias = ClassVar  # error: [invalid-type-form]
bad3: TypeAlias = Final[int]  # error: [invalid-type-form]
bad4: TypeAlias = Final  # error: [invalid-type-form]
bad5: TypeAlias = Required[int]  # error: [invalid-type-form]
bad6: TypeAlias = NotRequired[int]  # error: [invalid-type-form]
bad7: TypeAlias = ReadOnly[int]  # error: [invalid-type-form]
bad9: TypeAlias = InitVar[int]  # error: [invalid-type-form]
bad10: TypeAlias = InitVar  # error: [invalid-type-form]

differently_bad: TypeAlias = Unpack[tuple[int, ...]]  # snapshot: invalid-type-form
```

```snapshot
error[invalid-type-form]: `Unpack` is not allowed in type alias values
  --> src/mdtest_snippet.py:14:30
   |
14 | differently_bad: TypeAlias = Unpack[tuple[int, ...]]  # snapshot: invalid-type-form
   |                              ^^^^^^^^^^^^^^^^^^^^^^^
info: See the following page for a reference on valid type expressions:
info: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
```

## `Self`

`Self` is not allowed in an explicit type alias value, even when the alias is defined in a class
body. Runtime-expression positions, such as `Annotated` metadata, are not part of the alias value's
type expression.

TODO: Reject `Self` introduced indirectly through runtime-expression forms such as `TypeOf[value]`.

```py
from typing_extensions import Annotated, Self, TypeAlias, cast

class C:
    # error: [invalid-type-form] "`Self` cannot be used in a type alias"
    Alias: TypeAlias = tuple[Self]

    # error: [invalid-type-form] "`Self` cannot be used in a type alias"
    Simplified: TypeAlias = object | Self

    # error: [invalid-type-form] "`Self` cannot be used in a type alias"
    Stringified: TypeAlias = "tuple[Self]"

    Metadata: TypeAlias = Annotated[int, cast(Self, object())]
```

The restriction also applies to recursive aliases. Using an invalid alias more than once does not
repeat its diagnostic.

```py
class Node:
    # error: [invalid-type-form] "`Self` cannot be used in a type alias"
    Tree: TypeAlias = tuple[Self, "Node.Tree"]

def first(value: Node.Tree): ...
def second(value: Node.Tree): ...
```

## Disabled `invalid-type-form` `Self` fallback

Rejected aliases recover as `Unknown` even when the diagnostic is disabled:

```toml
[rules]
invalid-type-form = "ignore"
```

```py
from typing_extensions import Self, TypeAlias

class C:
    Inner: TypeAlias = Self
    Tuple: TypeAlias = tuple[Self]
    Stringified: TypeAlias = "tuple[Self, int]"

    def takes(self, value: Inner) -> None:
        reveal_type(value)  # revealed: Unknown

    def takes_tuple(self, value: Tuple) -> None:
        reveal_type(value)  # revealed: tuple[Unknown]

    def takes_stringified(self, value: Stringified) -> None:
        reveal_type(value)  # revealed: Unknown

    def invalid_attribute(self) -> Self:
        self.attribute: TypeAlias = Self
        return self.attribute  # error: [invalid-return-type]

C().takes(1)
```

## Recursive `TypeIs` and `TypeGuard` aliases don't stack overflow

```py
from typing import TypeAlias
from typing_extensions import TypeGuard, TypeIs

RecursiveIs: TypeAlias = TypeIs["RecursiveIs"]
RecursiveGuard: TypeAlias = TypeGuard["RecursiveGuard"]

AliasIs: TypeAlias = RecursiveIs
AliasGuard: TypeAlias = RecursiveGuard
```

[type expression]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
