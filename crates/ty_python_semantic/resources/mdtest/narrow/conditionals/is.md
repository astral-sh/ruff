# Narrowing for `is` conditionals

## `is None`

```py
from typing import Literal

def _(x: None | Literal[1]):
    if x is None:
        reveal_type(x)  # revealed: None
    else:
        reveal_type(x)  # revealed: Literal[1]

    reveal_type(x)  # revealed: None | Literal[1]
```

## `is` for other types

```py
class A: ...

def _(x: A, y: A | None):
    if y is x:
        reveal_type(y)  # revealed: A
    else:
        reveal_type(y)  # revealed: A | None

    reveal_type(y)  # revealed: A | None
```

Identity also transfers facts about the shared object, such as whether a string is truthy.

```py
def truthy_string(value: object, text: str) -> None:
    if text:
        if value is text:
            reveal_type(value)  # revealed: str & ~AlwaysFalsy
```

## `is` with invariant generic types

A `list[int]` guarantees that values read from the list are integers. That guarantee must hold for
every reference to the same mutable list: if another reference could treat it as `list[str]`, it
could append a string that the first reference would then incorrectly read as an integer. An
identity comparison can therefore transfer the invariant type argument.

```py
def generic_type(value: object, items: list[int]) -> None:
    if value is items:
        reveal_type(value)  # revealed: list[int]
```

Incompatible invariant specializations cannot describe the same object in soundly typed code.

```py
def incompatible_generic_types(integers: list[int], strings: list[str]) -> None:
    reveal_type(integers is strings)  # revealed: Literal[False]
    if integers is strings:
        reveal_type(integers)  # revealed: Never
        reveal_type(strings)  # revealed: Never
```

## `is` with covariant generic types

Covariant specializations can describe the same object: an empty tuple belongs to both
`tuple[int, ...]` and `tuple[str, ...]`. Identity therefore remains possible and preserves both sets
of type arguments.

```py
def covariant_generic_type(value: object, items: tuple[int, ...]) -> None:
    if value is items:
        reveal_type(value)  # revealed: tuple[int, ...]

def overlapping_generic_types(integers: tuple[int, ...], strings: tuple[str, ...]) -> None:
    reveal_type(integers is strings)  # revealed: bool
    if integers is strings:
        # TODO: Ideally, these intersections would simplify to tuple[()].
        reveal_type(integers)  # revealed: tuple[int, ...] & tuple[str, ...]
        reveal_type(strings)  # revealed: tuple[str, ...] & tuple[int, ...]
```

## `is` with a `NewType`

A `NewType` constructor returns its argument unchanged, so its tag belongs to one static view rather
than the shared object. Identity can establish the underlying type without transferring that tag.

```py
from typing import NewType

UserId = NewType("UserId", int)

def discard_newtype_tag(value: object, user_id: UserId) -> None:
    if value is user_id:
        reveal_type(value)  # revealed: int
        reveal_type(user_id)  # revealed: UserId
```

## `is` with unconstrained type variables

An unconstrained type variable can hold a `NewType`. Identity therefore cannot transfer the type
variable, since doing so would also transfer the `NewType` tag.

```py
from typing import NewType, TypeVar

T = TypeVar("T")
UserId = NewType("UserId", int)

def type_variable(value: object, other: T) -> T:
    if value is other:
        reveal_type(value)  # revealed: object
        reveal_type(other)  # revealed: T@type_variable
    return other

reveal_type(type_variable(1, UserId(1)))  # revealed: UserId
```

## `is` with bounded type variables

A type variable bounded by `int` can still hold an integer `NewType`. Identity transfers its `int`
bound without transferring the type variable or its possible `NewType` tag.

```py
from typing import NewType, TypeVar

BoundedT = TypeVar("BoundedT", bound=int)
UserId = NewType("UserId", int)

def bounded_type_variable(value: object, other: BoundedT) -> BoundedT:
    if value is other:
        reveal_type(value)  # revealed: int
        reveal_type(other)  # revealed: BoundedT@bounded_type_variable
    return other

reveal_type(bounded_type_variable(1, UserId(1)))  # revealed: UserId
```

## `is` with constrained type variables

Identity transfers a constrained type variable's possible runtime types without transferring any
`NewType` tags in its constraints.

```py
from typing import NewType, TypeVar

UserId = NewType("UserId", int)
TaggedChoice = TypeVar("TaggedChoice", UserId, str)

def constrained_type_variable(value: object, other: TaggedChoice) -> None:
    if value is other:
        reveal_type(value)  # revealed: int | str
        reveal_type(other)  # revealed: TaggedChoice@constrained_type_variable
```

## Narrowing tagged unions of nominal classes by attribute identity

```py
from dataclasses import dataclass
from enum import Enum
from typing import Literal, NewType

@dataclass
class Foo:
    tag: Literal[False]

@dataclass
class Bar:
    tag: Literal[True]

@dataclass
class UnknownTag:
    tag: bool

def boolean_tags(value: Foo | Bar):
    if value.tag is True:
        reveal_type(value)  # revealed: Bar
    else:
        reveal_type(value)  # revealed: Foo

    if value.tag is not True:
        reveal_type(value)  # revealed: Foo
    else:
        reveal_type(value)  # revealed: Bar

    if True is value.tag:
        reveal_type(value)  # revealed: Bar
    else:
        reveal_type(value)  # revealed: Foo

    if True is not value.tag:
        reveal_type(value)  # revealed: Foo
    else:
        reveal_type(value)  # revealed: Bar

def ambiguous_tag(value: Foo | Bar | UnknownTag):
    if value.tag is True:
        reveal_type(value)  # revealed: Bar | UnknownTag
    else:
        reveal_type(value)  # revealed: Foo | UnknownTag

def nonsingleton_tag(value: Foo | Bar, tag: bool):
    if value.tag is tag:
        reveal_type(value)  # revealed: Foo | Bar
    else:
        reveal_type(value)  # revealed: Foo | Bar

def overwritten_tagged_union(value: Foo | Bar | bool):
    if isinstance(value, (Foo, Bar)):
        if (value := value.tag) is True:
            reveal_type(value)  # revealed: Literal[True]
        else:
            reveal_type(value)  # revealed: Literal[False]

def tagged_union_rebound_by_comparator(value: Foo | Bar | bool):
    if isinstance(value, (Foo, Bar)):
        if value.tag is (value := True):
            reveal_type(value)  # revealed: Literal[True]
        else:
            reveal_type(value)  # revealed: Literal[True]

def tagged_union_with_unrelated_assignment(value: Foo | Bar):
    if value.tag is (tag := True):
        reveal_type(value)  # revealed: Bar
        reveal_type(tag)  # revealed: Literal[True]
    else:
        reveal_type(value)  # revealed: Foo
        reveal_type(tag)  # revealed: Literal[True]

class MissingTag:
    tag: None

class PresentTag:
    tag: str

def optional_tags(value: MissingTag | PresentTag):
    if value.tag is None:
        reveal_type(value)  # revealed: MissingTag
    else:
        reveal_type(value)  # revealed: PresentTag

class Tag(Enum):
    FOO = 1
    BAR = 2

class EnumFoo:
    tag: Literal[Tag.FOO]

class EnumBar:
    tag: Literal[Tag.BAR]

def enum_tags(value: EnumFoo | EnumBar):
    if value.tag is Tag.FOO:
        reveal_type(value)  # revealed: EnumFoo
    else:
        reveal_type(value)  # revealed: EnumBar

BoolTag = NewType("BoolTag", bool)

class NewTypeTag:
    tag: BoolTag

def newtype_tags(value: Foo | Bar | NewTypeTag):
    if value.tag is True:
        reveal_type(value)  # revealed: Bar | NewTypeTag
    else:
        reveal_type(value)  # revealed: Foo | NewTypeTag

def nonsingleton_newtype_tag(value: Foo | Bar, tag: BoolTag):
    if value.tag is tag:
        reveal_type(value)  # revealed: Foo | Bar
    else:
        reveal_type(value)  # revealed: Foo | Bar

def boolean_tags_after_truthiness(value: Foo | Bar | None):
    if not value:
        return

    if value.tag is True:
        reveal_type(value)  # revealed: Bar & ~AlwaysFalsy
    else:
        reveal_type(value)  # revealed: Foo & ~AlwaysFalsy
```

## `is` in chained comparisons

```py
def _(x: bool, y: bool):
    if y is x is False:  # Interpreted as `(y is x) and (x is False)`
        reveal_type(x)  # revealed: Literal[False]
        reveal_type(y)  # revealed: bool
    else:
        # The negation of the clause above is (y is not x) or (x is not False)
        # So we can't narrow the type of x or y here, because each arm of the `or` could be true
        reveal_type(x)  # revealed: bool
        reveal_type(y)  # revealed: bool
```

## `is` in elif clause

```py
from typing import Literal

def _(x: None | Literal[1, True]):
    if x is None:
        reveal_type(x)  # revealed: None
    elif x is True:
        reveal_type(x)  # revealed: Literal[True]
    else:
        reveal_type(x)  # revealed: Literal[1]
```

## `is` for enums

```py
from enum import Enum
from typing import Literal

class Answer(Enum):
    NO = 0
    YES = 1

def _(answer: Answer):
    if answer is Answer.NO:
        reveal_type(answer)  # revealed: Literal[Answer.NO]
    else:
        reveal_type(answer)  # revealed: Literal[Answer.YES]

class Single(Enum):
    VALUE = 1

def _(x: Single | int):
    if x is Single.VALUE:
        reveal_type(x)  # revealed: Single
    else:
        reveal_type(x)  # revealed: int

def _(x: list[int] | Literal[Answer.NO]):
    if x is Answer.NO:
        return
    reveal_type(x)  # revealed: list[int]
```

## `is` for `EllipsisType`

```py
from types import EllipsisType

def _(x: int | EllipsisType):
    if x is ...:
        reveal_type(x)  # revealed: EllipsisType
    else:
        reveal_type(x)  # revealed: int
```

## Assignment expressions

```py
from typing import Literal

def f() -> Literal[1, 2] | None: ...

if (x := f()) is None:
    reveal_type(x)  # revealed: None
else:
    reveal_type(x)  # revealed: Literal[1, 2]

value = f()
if result := (value is None):
    reveal_type(value)  # revealed: None
    reveal_type(result)  # revealed: Literal[True]
else:
    reveal_type(value)  # revealed: Literal[1, 2]
    reveal_type(result)  # revealed: Literal[False]

value = f()
if value := (value is None):
    reveal_type(value)  # revealed: Literal[True]
else:
    reveal_type(value)  # revealed: Literal[False]
```

## `is` with two narrowable operands

Both operands should be narrowed when both are narrowable expressions.

```py
from typing import Literal

def _(t: Literal[True], tn: Literal[True] | None):
    if tn is t:
        reveal_type(tn)  # revealed: Literal[True]
    if t is tn:
        reveal_type(tn)  # revealed: Literal[True]
```

Both operands should also be narrowed in chained comparisons:

```py
from typing import Literal

def _(a: Literal[1], b: Literal[1, 2], c: Literal[1, 2, 3]):
    if a is b is c:
        reveal_type(b)  # revealed: Literal[1]
        reveal_type(c)  # revealed: Literal[1]
```

When a generic class object is compared with an exact class object, the exact class object is not
widened to the generic type. The intersection is retained because it preserves the relationship
between the class object and `T`:

```toml
[environment]
python-version = "3.12"
```

```py
class Y:
    def __init__(self) -> None: ...

class Z(Y):
    def __init__(self, x: int) -> None: ...

def narrow[T: (Y, Z)](klass: type[T]) -> None:
    if klass is Y:
        reveal_type(klass)  # revealed: type[T@narrow] & <class 'Y'>
        reveal_type(Y)  # revealed: <class 'Y'> & type[T@narrow]

    if klass is Z:
        reveal_type(klass)  # revealed: <class 'Z'>
        reveal_type(Z)  # revealed: <class 'Z'>

def construct[T: (Y, Z)](klass: type[T]) -> T:
    if klass is Y:
        return Y()
    raise AssertionError

class Generic[T]: ...
class Specialized(Generic[int]): ...

def narrow_generic_alias[T: (Generic[int], Specialized)](klass: type[T]) -> None:
    if klass is Generic[int]:
        reveal_type(klass)  # revealed: type[T@narrow_generic_alias] & <class 'Generic[int]'>
        reveal_type(Generic[int])  # revealed: <class 'Generic[int]'>
```

## Narrowing with a constrained `TypeVar`

The `is` check below can discard `int` because it cannot be `None` or `...`. The `is not` check
cannot discard either remaining type: depending on the current constraint, either value could differ
from `other`.

```py
from types import EllipsisType
from typing import TypeVar

T = TypeVar("T", None, EllipsisType)

def takes_singleton(value: None | EllipsisType) -> None: ...
def f(value: int | None | EllipsisType, other: T) -> None:
    if value is other:
        takes_singleton(value)
    if value is not other:
        reveal_type(value)  # revealed: int | (None & ~T@f) | (EllipsisType & ~T@f)
```

## `is` with a negated `NewType`

Excluding a `NewType` removes its invisible tag, not the runtime objects accepted by its
constructor. An identity comparison preserves that negation without making a reachable branch
disappear.

```py
from typing import Literal, NewType, TypeVar
from ty_extensions import Intersection, Not

UserId = NewType("UserId", int)

def excluded_newtype(value: Not[UserId], other: UserId) -> None:
    if value is other:
        reveal_type(value)  # revealed: int & ~UserId
        reveal_type(other)  # revealed: UserId

    if other is value:
        reveal_type(value)  # revealed: int & ~UserId
```

A type variable can hide the same static negation in its upper bound. The reachable branch must
preserve that type variable.

```py
ExcludedBound = TypeVar("ExcludedBound", bound=Intersection[int, Not[UserId]])

def excluded_newtype_in_bound(
    value: ExcludedBound,
    other: UserId,
    without_one: Intersection[ExcludedBound, Not[Literal[1]]],
) -> None:
    if value is other:
        reveal_type(value)  # revealed: ExcludedBound@excluded_newtype_in_bound

    if without_one is other:
        reveal_type(without_one)  # revealed: ExcludedBound@excluded_newtype_in_bound & ~Literal[1]
```

The same runtime overlap remains reachable when both operands are unions, while genuinely
incompatible alternatives are removed.

```py
def excluded_newtype_in_unions(
    value: Intersection[int, Not[UserId]] | None,
    other: UserId | bytes,
) -> None:
    if value is other:
        reveal_type(value)  # revealed: int & ~UserId
        reveal_type(other)  # revealed: UserId
```

Unlike a negated `NewType`, a negated runtime class genuinely rules out identity with its instances.

```py
def excluded_runtime_class(not_int: Not[int], other: UserId) -> None:
    if not_int is other:
        reveal_type(not_int)  # revealed: Never
        reveal_type(other)  # revealed: Never
```

## `is` with string types

Identity comparisons preserve existing `LiteralString` narrowing and do not make negated string
literal comparisons unreachable.

```py
from typing import Literal
from typing_extensions import LiteralString
from ty_extensions import Not

def literal_string(value: object, text: LiteralString) -> None:
    if value is text:
        reveal_type(value)  # revealed: LiteralString

def negated_string_literal(value: Not[Literal["hello"]]) -> None:
    if value is "hello":
        reveal_type(value)  # revealed: ~Literal["hello"]
```

## `is` with `NewType`s

### Distinct `NewType`s with the same base

Distinct `NewType` tags are mutually exclusive, so their types are disjoint even when they have the
same concrete base. Their constructors still return their arguments unchanged: an identity
comparison can succeed, but each operand retains only its own tag.

```py
from typing import NewType
from ty_extensions import Intersection

class Foo: ...
class FooSub(Foo): ...

FooNewType1 = NewType("FooNewType1", Foo)
FooNewType2 = NewType("FooNewType2", Foo)

def same_base(foo1: FooNewType1, foo2: FooNewType2) -> None:
    reveal_type(foo1 is foo2)  # revealed: bool
    if foo1 is foo2:
        reveal_type(foo1)  # revealed: FooNewType1
        reveal_type(foo2)  # revealed: FooNewType2

def union(value: FooNewType1 | None, other: FooNewType2) -> None:
    if value is other:
        reveal_type(value)  # revealed: FooNewType1

def intersection(left: Intersection[FooNewType1, FooSub], right: FooNewType2) -> None:
    if left is right:
        reveal_type(right)  # revealed: FooNewType2 & FooSub
```

### `NewType`s in `TypeVar` bounds and constraints

`NewType`s inside `TypeVar` bounds and constraints can likewise refer to the same runtime object.
Comparing distinct type variables is not always false, but a successful comparison preserves each
operand's own type variable and tag.

```py
from typing import NewType, TypeVar

class Foo: ...

FooNewType1 = NewType("FooNewType1", Foo)
FooNewType2 = NewType("FooNewType2", Foo)
FooNewType3 = NewType("FooNewType3", Foo)
FooNewType4 = NewType("FooNewType4", Foo)

BoundedT = TypeVar("BoundedT", bound=FooNewType1)
BoundedU = TypeVar("BoundedU", bound=FooNewType2)

def bounded_typevars(left: BoundedT, right: BoundedU) -> None:
    reveal_type(left is right)  # revealed: bool
    if left is right:
        # These are the same object, so substituting `left` for `right` in a return would be
        # sound. But `BoundedT & BoundedU` is still empty because their `NewType` tags differ;
        # inferring that intersection could incorrectly make reachable code disappear.
        reveal_type(left)  # revealed: BoundedT@bounded_typevars
        reveal_type(right)  # revealed: BoundedU@bounded_typevars

ConstrainedT = TypeVar("ConstrainedT", FooNewType1, FooNewType2)
ConstrainedU = TypeVar("ConstrainedU", FooNewType3, FooNewType4)

def constrained_typevars(left: ConstrainedT, right: ConstrainedU) -> None:
    reveal_type(left is right)  # revealed: bool
    if left is right:
        reveal_type(left)  # revealed: ConstrainedT@constrained_typevars
        reveal_type(right)  # revealed: ConstrainedU@constrained_typevars
```

A type variable bounded by a `NewType` also carries that `NewType` tag. Identity cannot transfer the
type variable to an untagged value, but can establish the underlying runtime class.

```py
def object_with_bounded_newtype(value: object, tagged: BoundedT) -> None:
    if value is tagged:
        reveal_type(value)  # revealed: Foo
        reveal_type(tagged)  # revealed: BoundedT@object_with_bounded_newtype
```

Every constraint below is a `NewType` based on `EllipsisType`. Although their tags are mutually
exclusive, all of these values refer to the same `...` object. An `is not` check therefore removes
the singleton alternative, making a subsequent `is` check unreachable.

```py
from types import EllipsisType
from typing import NewType, TypeVar
from typing_extensions import assert_never

SingletonA = NewType("SingletonA", EllipsisType)
SingletonB = NewType("SingletonB", EllipsisType)
SingletonC = NewType("SingletonC", EllipsisType)

SingletonT = TypeVar("SingletonT", SingletonA, SingletonB)

def same_singleton(first: SingletonA, second: SingletonB) -> None:
    reveal_type(first is second)  # revealed: Literal[True]
    if first is second:
        reveal_type(first)  # revealed: SingletonA
        reveal_type(second)  # revealed: SingletonB

def contradictory_singleton_comparisons(value: SingletonC | int, other: SingletonT) -> None:
    if value is not other:
        reveal_type(value)  # revealed: int
        if value is other:
            assert_never(value)
```

### Narrowing an object to the generic base of a `NewType`

Identity does not transfer a `NewType` tag, but it preserves the invariant type arguments of the
underlying generic type.

```py
from typing import NewType

UserIds = NewType("UserIds", list[int])

def preserve_generic_base(value: object, user_ids: UserIds) -> None:
    if value is user_ids:
        reveal_type(value)  # revealed: list[int]
        reveal_type(user_ids)  # revealed: UserIds
```

### Comparing `NewType`s with literals

Calls to `NewType` return their arguments unchanged. Comparisons with `bool` and `int` literals can
therefore succeed. Identity transfers the literal value to the tagged operand, but does not transfer
its `NewType` tag back to the literal.

```py
from typing import Literal, NewType

BoolNewType = NewType("BoolNewType", bool)
IntNewType = NewType("IntNewType", int)

def literals(true: Literal[True], b: BoolNewType, forty_two: Literal[42], i: IntNewType) -> None:
    if b is true:
        reveal_type(true)  # revealed: Literal[True]
        reveal_type(b)  # revealed: BoolNewType & Literal[True]
    if i is forty_two:
        reveal_type(forty_two)  # revealed: Literal[42]
        reveal_type(i)  # revealed: IntNewType & Literal[42]
```

### `is not` with singleton `NewType`s

Both `NewType`s below are based on `EllipsisType`, which contains only the `...` object. The
`is not` branch therefore removes the `NewType` alternative.

```py
from types import EllipsisType
from typing import NewType

SingletonA = NewType("SingletonA", EllipsisType)
SingletonB = NewType("SingletonB", EllipsisType)

def singleton_is_not(value: SingletonA | int, other: SingletonB) -> None:
    if value is not other:
        reveal_type(value)  # revealed: int

    if value is other:
        reveal_type(value)  # revealed: SingletonA
        reveal_type(other)  # revealed: SingletonB
```

### Comparisons that are always false

An identity comparison is still always false when the two runtime types are distinct final classes.

```py
from typing import NewType, final

@final
class A: ...

@final
class B: ...

ANewType = NewType("ANewType", A)
BNewType = NewType("BNewType", B)

def disjoint_bases(a: ANewType, b: BNewType) -> None:
    reveal_type(a is b)  # revealed: Literal[False]
```

## `is` where the other operand is a call expression

```py
from typing import Literal, final

def foo() -> Literal[42]:
    return 42

def f(x: object):
    if x is foo():
        reveal_type(x)  # revealed: Literal[42]
    else:
        reveal_type(x)  # revealed: object

    if x is not foo():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: Literal[42]

    if foo() is x:
        reveal_type(x)  # revealed: Literal[42]
    else:
        reveal_type(x)  # revealed: object

    if foo() is not x:
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: Literal[42]

def bar() -> int:
    return 42

def g(x: object):
    if x is bar():
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: object

    if x is not bar():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: int

@final
class FinalClass: ...

def baz() -> FinalClass:
    return FinalClass()

def h(x: object):
    if x is baz():
        reveal_type(x)  # revealed: FinalClass
    else:
        reveal_type(x)  # revealed: object

    if x is not baz():
        reveal_type(x)  # revealed: object
    else:
        reveal_type(x)  # revealed: FinalClass

def spam() -> None:
    return None

def h(x: object):
    if x is spam():
        reveal_type(x)  # revealed: None
    else:
        # `else` narrowing can occur because `spam()` returns a singleton type
        reveal_type(x)  # revealed: ~None

    if x is not spam():
        reveal_type(x)  # revealed: ~None
    else:
        reveal_type(x)  # revealed: None
```
