# Cycles in implicit instance attributes

## Unpacking

See: <https://github.com/astral-sh/ty/issues/364>

```py
class Point:
    def __init__(self, x: int = 0, y: int = 0) -> None:
        self.x = x
        self.y = y

    def replace_with(self, other: "Point") -> None:
        self.x, self.y = other.x, other.y

p = Point()
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int
```

## Copying tuple elements

Copying each element preserves its type, including when the tuple's type includes the assignments
made by the copying method. Negative indices select the same elements as their positive equivalents.

```py
class Pair:
    def __init__(self, first: int, second: str):
        self.pair = (first, second)

    def copy(self):
        self.pair = (self.pair[0], self.pair[-1])
        reveal_type(self.pair)  # revealed: tuple[int, str]
```

## Copying tuple slices

A full slice preserves a tuple's length and the type at each position. Reading elements from a
reversed slice also preserves their individual types when the tuple is reassigned.

```py
class Sliced:
    def __init__(self, first: int, second: str):
        self.items = (first, second)

    def copy(self):
        self.items = self.items[:]
        reveal_type(self.items)  # revealed: tuple[int, str]

class Reversed:
    def __init__(self, first: int, second: str):
        self.items = (first, second)

    def copy(self):
        self.items = (self.items[::-1][1], self.items[::-1][0])
        reveal_type(self.items)  # revealed: tuple[int, str]
```

## Copying tuple elements through attributes

An attribute can provide another tuple to index. Repeated attribute and subscript accesses preserve
the element type when that element is used to rebuild the original tuple.

```py
class Node:
    @property
    def pair(self) -> tuple["Node"]:
        return (self,)

class Holder:
    def __init__(self, node: Node):
        self.items = (node,)

    def update(self):
        self.items = (self.items[0].pair[0].pair[0],)
        reveal_type(self.items)  # revealed: tuple[Node]
```

## Copying tuple elements through local variables

A local alias preserves the element types when an attribute is read and then reassigned. Chained
assignments can provide separate aliases for the same tuple.

```py
class Local:
    def __init__(self, first: int, second: str):
        self.items = (first, second)

    def copy(self):
        items = self.items
        first = items[0]
        second = items[1]
        self.items = (first, second)
        reveal_type(self.items)  # revealed: tuple[int, str]

class Chained:
    def __init__(self, first: int, second: str):
        self.items = (first, second)

    def copy(self):
        left = right = self.items
        self.items = (left[0], right[1])
        reveal_type(self.items)  # revealed: tuple[int, str]
```

## Copying tuple elements with union types

Each tuple element can include several types. Local aliases preserve these alternatives when the
same union is written in a different order for another element.

```py
from typing_extensions import assert_type

class Alternatives:
    def __init__(self, first: int | str, second: str | int):
        self.items = (first, second)

    def copy(self):
        items = self.items
        left = items[0]
        right = items[1]
        self.items = (left, right)
        assert_type(self.items, tuple[int | str, str | int])
```

## Copying tuple elements with an assignment expression

An assignment expression makes the same tuple available to subsequent element reads.

```py
class Named:
    def __init__(self, first: int, second: str):
        self.items = (first, second)

    def copy(self):
        self.items = ((items := self.items)[0], items[1])
        reveal_type(self.items)  # revealed: tuple[int, str]
```

## Copying a dynamic tuple element

An element annotated as `Any` retains that type when copied. Reading it does not supply a more
specific type.

```py
from typing import Any

class Dynamic:
    def __init__(self, value: Any):
        self.items = (value,)

    def copy(self):
        self.items = (self.items[0],)
        reveal_type(self.items)  # revealed: tuple[Any]
```

## Mutually dependent tuple attributes

Both attributes have initial values. Their updates swap elements from the other attribute,
preserving the initial element types. The methods can be declared before the initializer.

```py
class Mutual:
    def update_left(self):
        self.left = (self.right[1], self.right[0])

    def update_right(self):
        self.right = (self.left[1], self.left[0])

    def __init__(self, number: int, text: str):
        self.left = (number, text)
        self.right = (text, number)

    def inspect(self):
        reveal_type(self.left)  # revealed: tuple[int, str]
        reveal_type(self.right)  # revealed: tuple[str, int]
```

## Copies around a cycle with distinct initial types

Each attribute starts with a different element type. Repeated updates can move every initial value
around the cycle, so each attribute admits all of those types.

```py
from typing_extensions import assert_type

class V0: ...
class V1: ...
class V2: ...
class V3: ...
class V4: ...
class V5: ...
class V6: ...
class V7: ...

class Chain:
    def __init__(self):
        self.a0 = (V0(),)
        self.a1 = (V1(),)
        self.a2 = (V2(),)
        self.a3 = (V3(),)
        self.a4 = (V4(),)
        self.a5 = (V5(),)
        self.a6 = (V6(),)
        self.a7 = (V7(),)

    def update0(self):
        self.a0 = (self.a1[0],)

    def update1(self):
        self.a1 = (self.a2[0],)

    def update2(self):
        self.a2 = (self.a3[0],)

    def update3(self):
        self.a3 = (self.a4[0],)

    def update4(self):
        self.a4 = (self.a5[0],)

    def update5(self):
        self.a5 = (self.a6[0],)

    def update6(self):
        self.a6 = (self.a7[0],)

    def update7(self):
        self.a7 = (self.a0[0],)

    def inspect(self):
        assert_type(self.a0, tuple[V0 | V1 | V2 | V3 | V4 | V5 | V6 | V7])
        assert_type(self.a7, tuple[V0 | V1 | V2 | V3 | V4 | V5 | V6 | V7])
```

## Reading properties while updating a tuple

An update can read properties of the existing elements. Their declared return types determine the
new element types, even though the update contributes to the type of the tuple being read. A missing
attribute is still an error.

```py
class Item:
    @property
    def next(self) -> "Item":
        return self

    @property
    def value(self) -> int:
        return 1

class Items:
    def __init__(self, item: Item, value: int):
        self.items = (item, value)

    def copy(self):
        self.items = (self.items[0].next, self.items[0].value)
        reveal_type(self.items)  # revealed: tuple[Item, int]
        self.items[0].missing  # error: [unresolved-attribute] "Object of type `Item` has no attribute `missing`"
        self.items[0].value = "wrong"  # error: [invalid-assignment] "Cannot assign to read-only property `value`"
```

## Generic tuple attributes

The same generic class can be used with different type arguments. Copying a tuple does not mix these
specializations, including when the generic class is inherited.

```py
from typing import Generic, TypeVar

T = TypeVar("T")
S = TypeVar("S")

class Box(Generic[T]):
    def __init__(self, value: T):
        self.items = (value,)

    def copy(self):
        self.items = (self.items[0],)

class Derived(Box[tuple[S, S]]): ...

def inspect(first: Derived[int], second: Derived[str]):
    reveal_type(first.items)  # revealed: tuple[tuple[int, int]]
    reveal_type(second.items)  # revealed: tuple[tuple[str, str]]
    reveal_type((first.items[0], second.items[0]))  # revealed: tuple[tuple[int, int], tuple[str, str]]
```

A tuple element without an initial value remains unresolved. Its sibling still uses the owner's type
argument, and cannot share the type argument of a different specialization.

```py
class Partial(Generic[T]):
    def update(self, item: T):
        self.items = (self.items[0], item)

def inspect_partial(first: Partial[int], second: Partial[str]):
    reveal_type(first.items[1])  # revealed: int
    reveal_type(second.items[1])  # revealed: str
    reveal_type((first.items[1], second.items[1]))  # revealed: tuple[int, str]
```

## Rebuilding dictionaries from mappings

Reading a value from a mapping and storing it in a new dictionary preserves the value type, even
when the same attribute can hold either mapping.

```py
from collections.abc import Mapping

class Values:
    def __init__(self, items: Mapping[str, int]):
        self.items = items

    def copy(self):
        value = self.items["value"]
        self.items = {"value": value}
        reveal_type(self.items)  # revealed: dict[str, int]
```

## Rebuilding collections by iteration

A sequence of integers can be replaced with a list or set built from its elements. Subsequent
iterations still yield integers.

```py
from collections.abc import Sequence

class Lists:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def copy(self):
        for value in self.items:
            self.items = [value]
            reveal_type(self.items)  # revealed: list[int]

class Sets:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def copy(self):
        for value in self.items:
            self.items = {value}
            reveal_type(self.items)  # revealed: set[int]
```

## Rebuilding collections with comprehensions

Comprehensions preserve the element types when they replace the collection they read from. A
dictionary comprehension preserves both the keys and the corresponding values.

```py
from collections.abc import Mapping, Sequence

class Lists:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def copy(self):
        self.items = [value for value in self.items]
        reveal_type(self.items)  # revealed: list[int]

class Dictionaries:
    def __init__(self, items: Mapping[str, int]):
        self.items = items

    def copy(self):
        self.items = {key: self.items[key] for key in self.items}
        reveal_type(self.items)  # revealed: dict[str, int]
```

## Rebuilding collections with unpacking expressions

Expanding a sequence into a list preserves its element type. Expanding a mapping into a dictionary
preserves its key and value types.

```py
from collections.abc import Mapping, Sequence

class Lists:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def copy(self):
        self.items = [*self.items]
        reveal_type(self.items)  # revealed: list[int]

class Dictionaries:
    def __init__(self, items: Mapping[str, int]):
        self.items = items

    def copy(self):
        self.items = {**self.items}
        reveal_type(self.items)  # revealed: dict[str, int]
```

## Assigning comprehension elements to an attribute

A comprehension can store each element in an instance attribute. Reading that attribute to build the
replacement collection preserves the element type.

```py
from collections.abc import Sequence

class Values:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def copy(self):
        self.items = [self.item for self.item in self.items]
        reveal_type(self.items)  # revealed: list[int]
```

The comprehension assigns an instance attribute, so it does not create an attribute on the class.

```py
Values.item  # error: [unresolved-attribute]
```

## Rebuilding a collection from an attribute assigned by a loop

An attribute assigned by iteration retains its element type when another method reads it to replace
the original collection.

```py
from collections.abc import Sequence

class Values:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def load(self):
        for self.item in self.items:
            pass

    def copy(self):
        self.items = [self.item]
        reveal_type(self.items)  # revealed: list[int]
```

## Rebuilding a collection from an attribute assigned by asynchronous iteration

An asynchronous iterable supplies the type of the attribute assigned by `async for`. Another method
can use that attribute to rebuild a collection stored alongside the iterable.

```py
from collections.abc import AsyncIterable

class Values:
    def __init__(self, items: AsyncIterable[int]):
        self.state = (items, [0])

    async def load(self):
        async for self.item in self.state[0]:
            pass

    def copy(self):
        self.state = (self.state[0], [self.item])
        reveal_type(self.state)  # revealed: tuple[AsyncIterable[int], list[int]]
```

## Rebuilding a collection from an attribute assigned by a comprehension

The same dependency can cross a comprehension scope and a method boundary.

```py
from collections.abc import Sequence

class Values:
    def __init__(self, items: Sequence[int]):
        self.items = items

    def load(self):
        [None for self.item in self.items]

    def copy(self):
        self.items = [self.item]
        reveal_type(self.items)  # revealed: list[int]
```

## Rotating elements between collections

Repeated calls can move any of the three initial element types into each collection. Capturing the
collections in a tuple before reading their elements preserves those dependencies.

```py
from typing_extensions import assert_type

class Values:
    def __init__(self):
        self.x = [0]
        self.y = [""]
        self.z = [b""]

    def rotate(self):
        previous = (self.x, self.y, self.z)
        x = previous[0][0]
        y = previous[1][0]
        z = previous[2][0]
        self.x = [y]
        self.y = [z]
        self.z = [x]
        assert_type(self.x, list[int | str | bytes])
        assert_type(self.y, list[int | str | bytes])
        assert_type(self.z, list[int | str | bytes])
```

## Self-referential implicit attributes

```py
class Cyclic:
    def __init__(self, data: str | dict):  # error: [missing-type-argument]
        self.data = data

    def update(self):
        if isinstance(self.data, str):
            self.data = {"url": self.data}

# revealed: str | dict[Unknown, Unknown] | dict[str, str]
reveal_type(Cyclic("").data)
```

## Cycle normalization preserves non-gradual variadic parameters

Normalizing a recursive implicit-attribute type does not reinterpret specialized variadic parameters
as gradual:

```py
from typing import Any, Callable, Generic, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of

T = TypeVar("T")
flag: bool

class C(Generic[T]):
    def method(self, *args: T, **kwargs: T) -> None: ...

c = C[Any]()

class Recursive:
    def __init__(self, other: "Recursive"):
        self.callback = c.method if flag else other.callback

def check(value: Recursive):
    reveal_type(value.callback)  # revealed: bound method C[Any].method(*args: Any, **kwargs: Any) -> None
    static_assert(is_subtype_of(TypeOf[value.callback], Callable[[], None]))
```

## Guarded instance attributes when the base is checked first

A guarded bound-method initializer remains valid, including when its receiver is explicitly
annotated, while another initializer still reports an attribute that is missing from the base class.
Calling the initialized method returns `str`. This reproduces
<https://github.com/astral-sh/ty/issues/4076>.

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
        if not hasattr(self, "z"):
            self.z = self.y  # error: [unresolved-attribute]

reveal_type(Base().x())  # revealed: str

class Annotated:
    def __init__(self: "Annotated"):
        if not hasattr(self, "value"):
            self.value = self.__str__
            self.missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Annotated, Base

class Child(Base):
    x = Base.__str__

    def z(self): ...
    def y(self): ...

class AnnotatedChild(Annotated):
    value = Annotated.__str__
```

## Guarded instance attributes when the subclass is checked first

Checking the subclass first preserves the valid initializer, its inferred return type, and the
missing-attribute diagnostic.

`child.py`:

```py
from base import Annotated, Base

class Child(Base):
    x = Base.__str__

    def z(self): ...
    def y(self): ...

class AnnotatedChild(Annotated):
    value = Annotated.__str__
```

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
        if not hasattr(self, "z"):
            self.z = self.y  # error: [unresolved-attribute]

reveal_type(Base().x())  # revealed: str

class Annotated:
    def __init__(self: "Annotated"):
        if not hasattr(self, "value"):
            self.value = self.__str__
            self.missing  # error: [unresolved-attribute]
```

## Named protocol guards when the base is checked first

Here, a runtime-checkable protocol with a read-only `x` property checks the same member presence as
`hasattr(self, "x")`. Whether the protocol is named does not change the initializer's reachability.
The initialized instance also satisfies the protocol outside the initializer.

`base.py`:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> object: ...

class Base:
    def __init__(self):
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]

def accepts_x(value: HasX) -> None: ...

accepts_x(Base())
reveal_type(Base().x())  # revealed: str
```

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

## Named protocol guards when the subclass is checked first

Checking the subclass first preserves the reachable initializer and its missing-attribute error.

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

`base.py`:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> object: ...

class Base:
    def __init__(self):
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]

def accepts_x(value: HasX) -> None: ...

accepts_x(Base())
reveal_type(Base().x())  # revealed: str
```

## Nested and compound attribute guards when the base is checked first

An unrelated condition can appear outside an attribute guard, inside it, or on either side of a
compound condition without making a guarded initializer invalid. Previous narrowing of the receiver
must also preserve real diagnostics in the guarded branch.

`base.py`:

```py
class Marker: ...

class Base:
    def __init__(self, enabled: bool):
        if enabled:
            if not hasattr(self, "outer"):
                self.outer = self.__str__
        if not hasattr(self, "inner"):
            if enabled:
                self.inner = self.__str__
        if enabled and not hasattr(self, "leading"):
            self.leading = self.__str__
        if not hasattr(self, "trailing") and enabled:
            self.trailing = self.__str__
        if self is not None:
            if not hasattr(self, "nonnull"):
                self.nonnull = self.__str__
                self.nonnull_missing  # error: [unresolved-attribute]
        if not hasattr(self, "other"):
            if not hasattr(self, "unrelated"):
                self.unrelated = self.__str__
                self.unrelated_missing  # error: [unresolved-attribute]
        if isinstance(self, Marker):
            if not hasattr(self, "narrowed"):
                self.narrowed = self.__str__
                self.narrowed_missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Base, Marker

class Child(Base, Marker):
    outer = Base.__str__
    inner = Base.__str__
    leading = Base.__str__
    trailing = Base.__str__
    nonnull = Base.__str__
    unrelated = Base.__str__
    narrowed = Base.__str__
```

## Nested and compound attribute guards when the subclass is checked first

Checking the subclass first must preserve the same nested and compound guarded initializers.

`child.py`:

```py
from base import Base, Marker

class Child(Base, Marker):
    outer = Base.__str__
    inner = Base.__str__
    leading = Base.__str__
    trailing = Base.__str__
    nonnull = Base.__str__
    unrelated = Base.__str__
    narrowed = Base.__str__
```

`base.py`:

```py
class Marker: ...

class Base:
    def __init__(self, enabled: bool):
        if enabled:
            if not hasattr(self, "outer"):
                self.outer = self.__str__
        if not hasattr(self, "inner"):
            if enabled:
                self.inner = self.__str__
        if enabled and not hasattr(self, "leading"):
            self.leading = self.__str__
        if not hasattr(self, "trailing") and enabled:
            self.trailing = self.__str__
        if self is not None:
            if not hasattr(self, "nonnull"):
                self.nonnull = self.__str__
                self.nonnull_missing  # error: [unresolved-attribute]
        if not hasattr(self, "other"):
            if not hasattr(self, "unrelated"):
                self.unrelated = self.__str__
                self.unrelated_missing  # error: [unresolved-attribute]
        if isinstance(self, Marker):
            if not hasattr(self, "narrowed"):
                self.narrowed = self.__str__
                self.narrowed_missing  # error: [unresolved-attribute]
```

## Class attributes independently establish presence

A class attribute is present before an initializer runs, including when it is inherited. Assigning
to the same name inside a negative guard does not make that branch reachable. This applies to both
`hasattr` and named protocols with a read-only `object` property.

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> object: ...

class Base:
    x = 1

    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.missing
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing

class Child(Base):
    def initialize(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.missing
```

## Class attributes establish presence through aliased protocol members

A read-only protocol property typed as an alias of `object` imposes the same presence requirement as
`object` itself. The class attribute makes the negative guard unreachable, even when that branch
assigns to the same attribute.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, runtime_checkable

type Top = object

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> Top: ...

class C:
    x = 1

    def __init__(self):
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing
```

## Unreachable and deleted class attributes do not prevent guarded initialization

A class attribute that was never assigned or was deleted cannot make a later instance initializer
unreachable.

`base.py`:

```py
class Base:
    if False:
        unreachable = 1

    deleted = 1
    del deleted

    def __init__(self):
        if not hasattr(self, "unreachable"):
            self.unreachable = self.__str__
        if not hasattr(self, "deleted"):
            self.deleted = self.__str__
```

`child.py`:

```py
from base import Base

class Child(Base):
    unreachable = Base.__str__
    deleted = Base.__str__
```

## Guarded instance attributes after a call when the base is checked first

A call before a guarded initializer must not make its validity or later diagnostics depend on file
order.

`base.py`:

```py
def prepare() -> None: ...

class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            prepare()
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

## Guarded instance attributes after a call when the subclass is checked first

Checking the subclass first must preserve the same guarded assignment and genuine missing-attribute
diagnostic after the intervening call.

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

`base.py`:

```py
def prepare() -> None: ...

class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            prepare()
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]
```

## Non-returning initializers do not define instance attributes

An assignment whose initializer never returns cannot make its target attribute present.

```py
from typing import NoReturn

def fail() -> NoReturn:
    raise RuntimeError

class C:
    def initialize(self):
        if not hasattr(self, "x"):
            self.x = fail()  # error: [invalid-assignment]

C().x  # error: [unresolved-attribute]
```

## Assignments in the opposite guard branch do not initialize an attribute

Assigning an existing attribute when `hasattr` succeeds does not initialize it in the opposite
branch. That branch remains unreachable and cannot create another instance attribute.

```py
class C:
    def __init__(self):
        self.x = 1

    def update(self):
        if hasattr(self, "x"):
            self.x = 2
        else:
            self.y = self.missing

C().y  # error: [unresolved-attribute]
```

## Contradictory attribute guards do not initialize an attribute

An impossible inner `hasattr` branch cannot create an instance attribute.

```py
class C:
    def initialize(self):
        if hasattr(self, "x"):
            if not hasattr(self, "x"):
                self.x = self.missing

C().x  # error: [unresolved-attribute]
```

## Lazy cached property behind `hasattr`

This pattern used to panic with "too many cycle iterations".

```py
class Cached:
    def get(self) -> int:
        return 0

    @property
    def metadata(self) -> int:
        if not hasattr(self, "_metadata"):
            self._metadata = self.get()
        return self._metadata

reveal_type(Cached().metadata)  # revealed: int
```
