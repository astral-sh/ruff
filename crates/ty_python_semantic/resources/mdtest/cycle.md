# Cycles

## Recursive lambda in a loop condition

A lambda is always truthy. Determining whether the final assignment is reachable must not require
inferring the lambda's return type, which depends on that same assignment.

```py
(f := lambda: f)
while lambda: f:
    pass
f = 0
```

## Recursive lambda in a conditional

The same cycle can arise when a conditional filters the bindings visible to a recursive lambda.

```py
f = lambda: f
if not (lambda: f):
    f = 0
```

## Function signature

Deferred annotations can result in cycles in resolving a function signature:

```py
from __future__ import annotations

# error: [invalid-type-form]
def f(x: f):
    pass

reveal_type(f)  # revealed: def f(x: Unknown) -> Unknown
```

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

## Unpacking a recursively growing tuple

This is a regression test for <https://github.com/astral-sh/ty/issues/3838>.

```py
while 1:
    # error: [possibly-unresolved-reference]
    # error: [possibly-unresolved-reference]
    x = (*x, x)

while 1:
    y = (y, *y)
```

## Generic `NamedTuple` with recursive fields

This is a regression test for <https://github.com/astral-sh/ty/issues/3872>. Computing the
`NamedTuple` fields while building the class's MRO must not try to determine whether the same class
is a `TypedDict`.

```toml
[environment]
python-version = "3.14"
```

```py
from typing import NamedTuple

class Node[KT, VT](NamedTuple):
    children: tuple[Node[KT, VT], ...] | tuple[Leaf[VT], ...]

class Leaf[VT](NamedTuple):
    values: tuple[VT, ...]
```

## Literal reduction during cycle recovery

This is a regression test for <https://github.com/astral-sh/ty/issues/3851>. Constructing a union
during cycle recovery must not run redundancy checks between a literal and a protocol instance.
Resolving the protocol interface can depend on the expression inference query that is already being
recovered, which would introduce a new Salsa cycle.

```toml
[environment]
python-version = "3.14"
```

```py
from typing import Protocol, runtime_checkable

_: Any

@property
def prop(self) -> A:
    raise NotImplementedError

@runtime_checkable
class B(Protocol):
    _: A

x = 5

while isinstance(x, B):
    x = B()  # error: [call-non-callable]

type(x)
x = 2

from typing import Any, assert_type

assert_type(prop, property)

if bool:
    x = 5

while isinstance(x, B):
    x = B()  # error: [call-non-callable]

class A: ...
```

## Literal widening during cycle recovery

Once a recursively growing group of integer literals widens to `int`, later iterations must not
reintroduce individual literals. Otherwise, the inferred type continues changing and the cycle never
converges. This is a reduced regression test from SciPy's iterative sparse solvers.

```py
def solve(maxiter, a, b, c, d, e):
    iteration = 0
    stop = 0
    while iteration < maxiter:
        iteration = iteration + 1
        if iteration >= maxiter:
            stop = 7
        if a:
            stop = 6
        if b:
            stop = 5
        if c:
            stop = 4
        if d:
            stop = 3
        if e:
            stop = 2
        if stop > 0:
            break
    return stop
```

## Self-referential bare type alias

```toml
[environment]
python-version = "3.12"  # typing.TypeAliasType
```

```py
from typing import Union, TypeAliasType, Sequence, Mapping

A = list["A | None"]

def f(x: A):
    # TODO: should be `list[A | None]`?
    reveal_type(x)  # revealed: list[Divergent]
    # TODO: should be `A | None`?
    reveal_type(x[0])  # revealed: Divergent

JSONPrimitive = Union[str, int, float, bool, None]
JSONValue = TypeAliasType("JSONValue", 'Union[JSONPrimitive, Sequence["JSONValue"], Mapping[str, "JSONValue"]]')

def _(x: JSONValue):
    reveal_type(x)  # revealed: Sequence[JSONValue] | float | None | Mapping[str, JSONValue]
```

## Self-referential legacy type variables

```py
from typing import Generic, TypeVar

B = TypeVar("B", bound="Base")  # error: [missing-type-argument]

class Base(Generic[B]):
    pass
```

## Parameter default values

This is a regression test for <https://github.com/astral-sh/ty/issues/1402>. When a parameter has a
default value that references the callable itself, we currently prevent infinite recursion by simply
falling back to `Unknown` for the type of the default value, which does not have any practical
impact except for the displayed type. We could also consider inferring `Divergent` when we encounter
too many layers of nesting (instead of just one), but that would require a type traversal which
could have performance implications. So for now, we mainly make sure not to panic or stack overflow
for these seemingly rare cases.

### Functions

```py
class C:
    def f(self: "C"):
        def inner_a(positional=self.a):
            return
        self.a = inner_a
        # revealed: def inner_a(positional=...) -> Unknown
        reveal_type(inner_a)

        def inner_b(*, kw_only=self.b):
            return
        self.b = inner_b
        # revealed: def inner_b(*, kw_only=...) -> Unknown
        reveal_type(inner_b)

        def inner_c(positional_only=self.c, /):
            return
        self.c = inner_c
        # revealed: def inner_c(positional_only=..., /) -> Unknown
        reveal_type(inner_c)

        def inner_d(*, kw_only=self.d):
            return
        self.d = inner_d
        # revealed: def inner_d(*, kw_only=...) -> Unknown
        reveal_type(inner_d)
```

We do, however, still check assignability of the default value to the parameter type:

```py
class D:
    def f(self: "D"):
        # error: [invalid-parameter-default] "Default value of type `(a: int = ...) -> Unknown` is not assignable to annotated parameter type `int`"
        def inner_a(a: int = self.a): ...
        self.a = inner_a
```

### Lambdas

```py
class C:
    def f(self: "C"):
        self.a = lambda positional=self.a: positional
        self.b = lambda *, kw_only=self.b: kw_only
        self.c = lambda positional_only=self.c, /: positional_only
        self.d = lambda *, kw_only=self.d: kw_only

        # revealed: (positional: Unknown = ...) -> Unknown | ((positional=...) -> Divergent)
        reveal_type(self.a)

        # revealed: (*, kw_only=...) -> Unknown | ((*, kw_only=...) -> Divergent)
        reveal_type(self.b)

        # revealed: (positional_only: Unknown = ..., /) -> Unknown | ((positional_only=..., /) -> Divergent)
        reveal_type(self.c)

        # revealed: (*, kw_only=...) -> Unknown | ((*, kw_only=...) -> Divergent)
        reveal_type(self.d)
```

### Self-referential decorated functions

Resolving a decorated function's callable signature must not eagerly infer its default values.
Otherwise, a default that refers back to the decorated name can re-enter the reachability check for
an earlier assertion and prevent inference from converging. This is a regression test for
<https://github.com/astral-sh/ty/issues/4308>.

```py
f = lambda: f
assert f

@property
def f(x=lambda: f): ...
```

The same cycle must converge when the parameter and return type are annotated:

```py
g = lambda: g
assert g

@property
def g(x: object = lambda: g) -> None: ...
```

### Diagnostics for self-referential decorated functions

We reject a decorator that expects an integer instead of a function. Displaying the function's
signature in that diagnostic can infer its self-referential default value. We report the error after
function inference finishes, so diagnostic formatting does not create a cycle through the
reachability check for the earlier assertion. This is a regression test for
<https://github.com/astral-sh/ty/issues/4440>.

```py
def decorator(value: int) -> int:
    return value

f = lambda: f
assert f

# error: [invalid-argument-type] "Expected `int`, found `def f(x=...) -> Unknown`"
@decorator
def f(x=lambda: f): ...
```

### Self-referential property construction

Constructing a property explicitly has the same behavior as decorator syntax:

```py
f = lambda: f
assert f

def getter(x=lambda: f): ...

f = property(getter)
```

### Self-referential callable decorators

The cycle is not specific to properties. A decorator that returns a callable with a fixed signature
must also terminate:

```py
from collections.abc import Callable
from typing import Any

def decorator(fn: Callable[[Any], Any]) -> Callable[[Any], Any]:
    return fn

f = lambda: f
assert f

@decorator
def f(x=lambda: f): ...
```

### Self-referential ParamSpec decorators

A decorator can capture a function's parameters and return a callable with a different signature.
Capturing those parameters must not evaluate a self-referential default.

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import Callable

def decorator[**P](fn: Callable[P, None]) -> Callable[[], None]:
    return lambda: None

f = lambda: f
assert f

@decorator
def f(x=lambda: f) -> None: ...

reveal_type(f)  # revealed: () -> None
```

### Self-referential generic properties

A generic getter's annotations are inferred in its type-parameter scope. Constructing the property
must not pull its self-referential default into that inference.

```toml
[environment]
python-version = "3.12"
```

```py
f = lambda: f
assert f

@property
def f[T](value: T, callback=lambda: f) -> T:
    return value

reveal_type(f)  # revealed: property
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

## Decorated methods with implicit class attributes

This is a regression test for <https://github.com/astral-sh/ty/issues/3471>.

```py
from collections.abc import Callable
from typing import TypeVar

class A: ...

T = TypeVar("T")
U = TypeVar("U", bound=A)
C = Callable[[T, U], object]

def d() -> Callable[[C[U, A]], object]:
    raise NotImplementedError

class B:
    @d()
    def m1(self, p):
        pass

    @d()
    def m2(self, p):
        self.__slots__  # error: [unresolved-attribute]
```

## Function annotation and dynamic `NamedTuple` / `NewType`

This is a regression test for <https://github.com/astral-sh/ty/issues/3485> and
<https://github.com/astral-sh/ty/issues/3682>. Type traversal during cycle recovery should not force
the lazy base of a `NewType`.

```py
class C:
    pass

def f():
    pass

def g() -> T:  # error: [unresolved-reference]
    pass

g()

from typing import NamedTuple, NewType

X = NamedTuple("X", [("x", "X")]), None  # error: [invalid-type-form]

list(X)
min(X)  # error: [invalid-argument-type]
T = f()

X = NewType("X", C)
```

The runtime callable returned by `NewType` also carries the lazy base and must use the same
cycle-safe traversal.

```py
class C: ...

def f(): ...
def g() -> T: ...

g()
from typing import NamedTuple, NewType

X = NewType("X", C)
Y = NamedTuple("Y", [("a", "Y")]), X  # error: [invalid-type-form]
min(Y)  # error: [invalid-argument-type]
T = f()
```

## Guarded instance attributes when the base is checked first

A guarded bound-method initializer remains valid, including when its receiver is explicitly
annotated, while another initializer still reports an attribute that is missing from the base class.
This reproduces <https://github.com/astral-sh/ty/issues/4076>.

`base.py`:

```py
class Base:
    existing = 1

    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
        if not hasattr(self, "existing"):
            self.missing
        if not hasattr(self, "z"):
            self.z = self.y  # error: [unresolved-attribute]

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

Checking the subclass first preserves the valid initializer and the missing-attribute diagnostic.

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
    existing = 1

    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
        if not hasattr(self, "existing"):
            self.missing
        if not hasattr(self, "z"):
            self.z = self.y  # error: [unresolved-attribute]

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
branch. That branch remains unreachable and cannot create another instance attribute. Existing
inherited class attributes also retain their usual narrowing.

```py
class Base:
    inherited = 1

class C(Base):
    def __init__(self):
        self.x = 1

    def update(self):
        if not hasattr(self, "inherited"):
            self.missing
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

## Decorator defined on a base class with constrained typevars, accessed from a subclass with decorated generic parameters

This example was minimized from
[a real issue in `robotframework`](https://github.com/astral-sh/ty/issues/2637#issuecomment-3807037935).
It created
[a complicated cycle with multiple cycle heads](https://gist.github.com/oconnor663/c996ed2cc97d172dd4b9a8d8207dc7ac),
which also involved
[a tricky Salsa behavior that comes up when a query oscillates between being a cycle head and not being one](https://gist.github.com/oconnor663/c2a7662e3d88048b691754da957121d1).

`entry.py`:

```py
from derived import Derived

Derived.decorate
# revealed: bound method <class 'Derived'>.decorate[T](item_class: type[T]) -> type[T]
reveal_type(Derived.decorate)
```

`derived.py`:

```py
from ty_extensions._internal import reveal_mro
import bases

class Derived(bases.GenericBase["Foo", "Bar"]): ...

@Derived.decorate
class Foo(bases.Foo): ...

# revealed: <class 'Foo'>
reveal_type(Foo)
# revealed: (<class 'derived.Foo'>, <class 'bases.Foo'>, <class 'object'>)
reveal_mro(Foo)

@Derived.decorate
class Bar(bases.Bar): ...

# revealed: <class 'Bar'>
reveal_type(Bar)
# revealed: (<class 'derived.Bar'>, <class 'bases.Bar'>, <class 'object'>)
reveal_mro(Bar)
```

`bases.py`:

```py
from typing import Generic, TypeVar, Type
from ty_extensions._internal import reveal_mro

T = TypeVar("T")
B1 = TypeVar("B1", bound="Foo")
B2 = TypeVar("B2", bound="Bar")

class GenericBase(Generic[B1, B2]):
    @classmethod
    def decorate(cls, item_class: Type[T]) -> Type[T]:
        return item_class

# revealed: <class 'GenericBase'>
reveal_type(GenericBase)
# revealed: (<class 'GenericBase[Unknown, Unknown]'>, typing.Generic, <class 'object'>)
reveal_mro(GenericBase)
# revealed: (<class 'GenericBase[Foo, Bar]'>, typing.Generic, <class 'object'>)
reveal_mro(GenericBase["Foo", "Bar"])

class Foo: ...
class Bar: ...
```
