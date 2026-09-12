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

## Promoting recursive values stored in attributes

A local variable keeps its literal types as it is repeatedly nested in tuples. Storing the value in
an attribute promotes the literals throughout the recursive type, so the attribute can hold other
values of those same types.

```py
class Tree:
    def __init__(self, count: int):
        value = 0
        for _ in range(count):
            value = (value, 1)
        # revealed: Literal[0] | (μ$0. tuple[$0 | Literal[0], Literal[1]])
        reveal_type(value)
        self.value = value

# revealed: int | (μ$0. tuple[$0 | int, int])
reveal_type(Tree(1).value)

def inspect(tree: Tree):
    value = tree.value
    if isinstance(value, tuple):
        reveal_type(value[0])  # revealed: int | (μ$0. tuple[$0 | int, int])
        reveal_type(value[1])  # revealed: int
        wrong: str = value[0]  # error: [invalid-assignment]
```

## Promoting class literals in recursive attributes

Class objects nested in a recursive tuple are promoted to subclass types when stored in an
attribute, while the recursive tuple structure is preserved.

```py
class Token: ...

class Classes:
    def __init__(self, count: int):
        value = Token
        for _ in range(count):
            value = (value, Token)
        self.value = value

# revealed: type[Token] | (μ$0. tuple[$0 | type[Token], type[Token]])
reveal_type(Classes(1).value)
```

## Class literals in self-referential instance attributes

An attribute can contain its own value alongside a class object and an integer. Both kinds of
literals are promoted at every level, including when the attribute is inherited.

```py
class Token: ...

class Nested:
    def update(self, other: "Nested"):
        self.value = (other.value, Token, 1)

# revealed: tuple[μ$0. tuple[$0, type[Token], int], type[Token], int]
reveal_type(Nested().value)
reveal_type(Nested().value[0][1])  # revealed: type[Token]
reveal_type(Nested().value[0][2])  # revealed: int

class Child(Nested): ...

# revealed: tuple[μ$0. tuple[$0, type[Token], int], type[Token], int]
reveal_type(Child().value)
```

## Self-referential class attributes

A classmethod can construct a tuple containing the same class attribute. Reading nested elements
preserves the recursive structure and their promoted types.

```py
class Token: ...

class Nested:
    @classmethod
    def update(cls):
        cls.value = (cls.value, Token, 1)

# revealed: tuple[μ$0. tuple[$0, type[Token], int], <class 'Token'>, int]
reveal_type(Nested.value)
reveal_type(Nested.value[0][1])  # revealed: type[Token]
reveal_type(Nested.value[0][2])  # revealed: int
```

## Copying tuple attributes

Copying tuple attributes preserves their lengths, including when two attributes are copied into each
other. Their recursive elements can still be approximated independently of their lengths.

```py
class Copies:
    def __init__(self):
        self.left = (0, 1)
        self.right = (2, 3)

    def copy(self):
        self.left = (*self.left,)

    def swap(self):
        previous = self.left
        self.left = (*self.right,)
        self.right = (*previous,)

reveal_type(len(Copies().left))  # revealed: Literal[2]
reveal_type(len(Copies().right))  # revealed: Literal[2]
```

A tuple can also start as a class default and then be copied onto the instance.

```py
class ClassDefault:
    value = (0, 1)

    def copy(self):
        self.value = (*self.value,)

reveal_type(len(ClassDefault().value))  # revealed: Literal[2]
```

## Tuple expansion through a property

A property's getter determines the tuple read from it. The tuple passed to its setter does not
define a recursive expansion of that read type.

```py
class FixedProperty:
    @property
    def value(self) -> tuple[int]:
        return (0,)

    @value.setter
    def value(self, value: tuple[int, ...]) -> None:
        pass

    def update(self):
        copied = (*self.value, 1)
        self.value = copied
        reveal_type(len(copied))  # revealed: Literal[2]

reveal_type(len(FixedProperty().value))  # revealed: Literal[1]
```

## Mutually recursive container attributes

Three attributes can refer to each other through different containers. Following their contents is
approximated with `Divergent` when the recursive equations do not settle.

```py
class Containers:
    def update(self, other: "Containers"):
        self.a = [other.b]
        self.b = {"next": other.c}
        self.c = (other.a, 1)

reveal_type(Containers().a)  # revealed: list[Divergent]
reveal_type(Containers().b)  # revealed: dict[str, tuple[list[Divergent], int]]
reveal_type(Containers().c)  # revealed: tuple[list[Divergent], int]
reveal_type(Containers().a[0]["next"][0])  # revealed: Divergent
```

## Mutually recursive attributes with initial values

Each attribute has an initial value and can contain the other attribute in a tuple. The inferred
types are equivalent to the explicit recursive aliases below.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to

class Pair:
    def seed(self):
        self.left = 1
        self.right = "start"

    def update(self, other: "Pair"):
        self.left = (other.right, 1)
        self.right = (other.left, "end")

type Left = int | tuple[Right, int]
type Right = str | tuple[Left, str]

static_assert(is_equivalent_to(TypeOf[Pair().left], Left))
static_assert(is_equivalent_to(TypeOf[Pair().right], Right))
```

## Multiple paths through recursive attributes

These four attributes have initial values, and the tuple assigned to `d` refers to both `a` and `b`.
TODO: Infer types equivalent to the explicit recursive aliases below.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to

class Ring:
    def seed(self):
        self.a = 1
        self.b = "b"
        self.c = True
        self.d = 1.0
    def step(self, other: "Ring"):
        self.a = (other.b,)
        self.b = [other.c]
        self.c = {"d": other.d}
        self.d = (other.a, other.b)

type A = int | tuple[B]
type B = str | list[C]
type C = bool | dict[str, D]
type D = float | tuple[A, B]

static_assert(is_equivalent_to(TypeOf[Ring().a], A))  # error: [static-assert-error]
static_assert(is_equivalent_to(TypeOf[Ring().b], B))  # error: [static-assert-error]
static_assert(is_equivalent_to(TypeOf[Ring().c], C))  # error: [static-assert-error]
static_assert(is_equivalent_to(TypeOf[Ring().d], D))  # error: [static-assert-error]
```

## Mutually recursive attributes with the same type

Two attributes can refer to each other and have the same recursive type. Each is displayed with one
tuple layer unfolded around the recursive type.

```py
class Same:
    def __init__(self):
        self.left = 0
        self.right = 0

    def update(self, other: "Same"):
        self.left = (other.right,)
        self.right = (other.left,)

# revealed: int | tuple[μ$0. tuple[$0] | int]
reveal_type(Same().left)
# revealed: int | tuple[μ$0. tuple[$0] | int]
reveal_type(Same().right)
```

## Self-reference and mutual references

An attribute can refer both to itself and to another recursively defined attribute. The inferred
types preserve the initial values at each level and are equivalent to the explicit aliases below.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to

class Branches:
    def seed(self):
        self.a = 1
        self.b = "b"
    def step(self, other: "Branches"):
        self.a = (other.a, other.b)
        self.b = (other.a,)

# revealed: int | tuple[μ$0. tuple[$0, tuple[$0] | str] | int, μ{$0; $1 = tuple[$1, $0] | int}. tuple[$1] | str]
reveal_type(Branches().a)
# revealed: str | tuple[μ$0. tuple[$0, tuple[$0] | str] | int]
reveal_type(Branches().b)

type A = int | tuple[A, B]
type B = str | tuple[A]
static_assert(is_equivalent_to(TypeOf[Branches().b], B))
static_assert(is_equivalent_to(TypeOf[Branches().a], A))
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

## Shared mutually recursive attributes

Several attributes can share recursive dependencies and gradual items. Their fixed tuple items
retain their types when the attributes are materialized.

```py
from typing import Any
from ty_extensions import Top
from ty_extensions._internal import TypeOf

class Graph:
    def set_0(self, other: "Graph", extra: Any):
        self.x0 = (other.x1, other.x2, other.x3, other.x4, other.x5, other.x6, other.x7, 1, extra)
    def set_1(self, other: "Graph", extra: Any):
        self.x1 = (other.x0, other.x2, other.x3, other.x4, other.x5, other.x6, other.x7, 1, extra)
    def set_2(self, other: "Graph", extra: Any):
        self.x2 = (other.x0, other.x1, other.x3, other.x4, other.x5, other.x6, other.x7, 1, extra)
    def set_3(self, other: "Graph", extra: Any):
        self.x3 = (other.x0, other.x1, other.x2, other.x4, other.x5, other.x6, other.x7, 1, extra)
    def set_4(self, other: "Graph", extra: Any):
        self.x4 = (other.x0, other.x1, other.x2, other.x3, other.x5, other.x6, other.x7, 1, extra)
    def set_5(self, other: "Graph", extra: Any):
        self.x5 = (other.x0, other.x1, other.x2, other.x3, other.x4, other.x6, other.x7, 1, extra)
    def set_6(self, other: "Graph", extra: Any):
        self.x6 = (other.x0, other.x1, other.x2, other.x3, other.x4, other.x5, other.x7, 1, extra)
    def set_7(self, other: "Graph", extra: Any):
        self.x7 = (other.x0, other.x1, other.x2, other.x3, other.x4, other.x5, other.x6, 1, extra)

def inspect(graph: Graph):
    def bound(value: Top[TypeOf[graph.x0]]):
        reveal_type(value[7])  # revealed: int
        wrong: str = value[7]  # error: [invalid-assignment]
```
