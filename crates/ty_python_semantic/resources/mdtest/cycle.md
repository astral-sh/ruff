# Cycles

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

## Inherited instance attributes when the base is checked first

A self-referential instance assignment preserves the inherited attribute type when the base file is
checked first.

```toml
[rules]
unsound-return-statement = "error"
```

`base.py`:

```py
class Base:
    values = ["a"]

class Parent(Base):
    def __init__(self):
        if self.values:
            self.values = [*self.values]

    def get_values(self) -> list[str]:
        return self.values
```

`child.py`:

```py
from base import Parent

class Child(Parent):
    def __init__(self):
        self.values = self.values + ["b"]
```

## Inherited instance attributes when the subclass is checked first

Reversing the file order must preserve the same inherited attribute type.

```toml
[rules]
unsound-return-statement = "error"
```

`child.py`:

```py
from base import Parent

class Child(Parent):
    def __init__(self):
        self.values = self.values + ["b"]
```

`base.py`:

```py
class Base:
    values = ["a"]

class Parent(Base):
    def __init__(self):
        if self.values:
            self.values = [*self.values]

    def get_values(self) -> list[str]:
        return self.values
```

## Inherited instance initializers when the base is checked first

An independently initialized superclass instance attribute remains available to receiver aliases.

```toml
[rules]
unsound-return-statement = "error"
```

`base.py`:

```py
class Base:
    def __init__(self):
        self.values = ["a"]

class Parent(Base):
    def __init__(self):
        super().__init__()
        if self.values:
            receiver = self
            self.values = [*receiver.values]

    def get_values(self) -> list[str]:
        return self.values
```

`child.py`:

```py
from base import Parent

class Child(Parent):
    def __init__(self):
        super().__init__()
        receiver = self
        self.values = receiver.values + ["b"]
```

## Inherited instance initializers when the subclass is checked first

Reversing file order preserves an independently initialized superclass instance attribute.

```toml
[rules]
unsound-return-statement = "error"
```

`child.py`:

```py
from base import Parent

class Child(Parent):
    def __init__(self):
        super().__init__()
        receiver = self
        self.values = receiver.values + ["b"]
```

`base.py`:

```py
class Base:
    def __init__(self):
        self.values = ["a"]

class Parent(Base):
    def __init__(self):
        super().__init__()
        if self.values:
            receiver = self
            self.values = [*receiver.values]

    def get_values(self) -> list[str]:
        return self.values
```

## Same-class and final attributes when the base is checked first

A same-class initializer can follow an update through a local alias. A bare `Final` declaration
without a value does not replace an inherited initializer.

```toml
[rules]
unsound-return-statement = "error"
```

`base.py`:

```py
from typing import Final

class Parent:
    def update(self):
        previous = self.values
        self.values = [*previous]

    def __init__(self):
        self.values = ["a"]

    def get_values(self) -> list[str]:
        return self.values

class FinalBase:
    values = ["a"]

class FinalParent(FinalBase):
    def __init__(self):
        self.values: Final
        self.values = [*self.values]

    def get_values(self) -> list[str]:
        return self.values
```

`child.py`:

```py
from base import FinalParent, Parent

class Child(Parent):
    def update(self):
        self.values = [*self.values]

class FinalChild(FinalParent):
    def __init__(self):
        self.values = self.values + ["b"]
```

## Same-class and final attributes when the subclass is checked first

Reversing file order preserves both the same-class initializer and the inherited bare `Final` value.

```toml
[rules]
unsound-return-statement = "error"
```

`child.py`:

```py
from base import FinalParent, Parent

class Child(Parent):
    def update(self):
        self.values = [*self.values]

class FinalChild(FinalParent):
    def __init__(self):
        self.values = self.values + ["b"]
```

`base.py`:

```py
from typing import Final

class Parent:
    def update(self):
        previous = self.values
        self.values = [*previous]

    def __init__(self):
        self.values = ["a"]

    def get_values(self) -> list[str]:
        return self.values

class FinalBase:
    values = ["a"]

class FinalParent(FinalBase):
    def __init__(self):
        self.values: Final
        self.values = [*self.values]

    def get_values(self) -> list[str]:
        return self.values
```

## Mutually dependent attributes when the left attribute is checked first

Independently initialized attributes can depend on each other through different branches.

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str]
```

## Mutually dependent attributes when the right attribute is checked first

Checking the opposite attribute first preserves both independently initialized types.

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str]
```

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

## Mutually dependent attributes reached through an annotated local alias

A local alias can read either attribute, so both possible dependencies contribute to the same
recursive attribute group.

```py
class Example:
    def update(self, flag: bool) -> None:
        previous: list[str] = self.right if flag else self.left
        if flag:
            self.left = [*previous]
        else:
            self.right = [*previous]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
reveal_type(Example().right)  # revealed: list[str]
```

## Widening mutually dependent attributes when the left attribute is checked first

A dependent assignment can introduce a type absent from both independently initialized attributes.

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left, 1]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str | int] | list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str | int] | list[str]
```

## Widening mutually dependent attributes when the right attribute is checked first

Checking the opposite attribute first preserves the type introduced by the dependent assignment.

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str | int] | list[str]
```

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left, 1]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str | int] | list[str]
```

## Widening mutually dependent attributes with user-defined types, left first

A recursive assignment must retain a new user-defined type even when neither class is final.

`left.py`:

```py
class Original: ...
class Added: ...

class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left, Added()]

    def initialize(self) -> None:
        self.left = [Original()]
        self.right = [Original()]

reveal_type(Example().left)  # revealed: list[Original | Added] | list[Original]
```

`right.py`:

```py
from left import Example, Original

def accepts_original(value: Original) -> None: ...

accepts_original(Example().right[0])  # error: [invalid-argument-type]
reveal_type(Example().right)  # revealed: list[Original | Added] | list[Original]
```

## Widening mutually dependent attributes with user-defined types, right first

Checking the opposite attribute first must not suppress the incompatible-argument diagnostic.

`right.py`:

```py
from left import Example, Original

def accepts_original(value: Original) -> None: ...

accepts_original(Example().right[0])  # error: [invalid-argument-type]
reveal_type(Example().right)  # revealed: list[Original | Added] | list[Original]
```

`left.py`:

```py
class Original: ...
class Added: ...

class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left, Added()]

    def initialize(self) -> None:
        self.left = [Original()]
        self.right = [Original()]

reveal_type(Example().left)  # revealed: list[Original | Added] | list[Original]
```

## Mutually dependent attributes introducing a collection type, left first

A recursive assignment may introduce a new collection type without creating unbounded nesting.

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = set(self.left)

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: set[str] | list[str]
```

## Mutually dependent attributes introducing a collection type, right first

Checking the opposite attribute first must preserve the independently initialized list.

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: set[str] | list[str]
```

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = set(self.left)

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

## Mutually dependent attributes introducing a collection around an atomic value

A collection introduced around another attribute's value can remain finite when the other assignment
immediately extracts its element.

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = self.right[0]
        else:
            self.right = [self.left]

    def initialize(self) -> None:
        self.left = "a"
        self.right = "a"

reveal_type(Example().left)  # revealed: str
reveal_type(Example().right)  # revealed: list[str] | str
```

## Mutually dependent attributes captured by lazy callables, defining class first

A lambda's body is executed lazily, but its return type can still depend on another attribute in the
same recursive component. Both callable alternatives must remain visible to diagnostics.

`model.py`:

```py
class Example:
    def initialize(self) -> None:
        self.left = lambda: "a"
        self.right = lambda: 1

    def update(self, flag: bool) -> None:
        if flag:
            self.left = lambda: self.right
        else:
            self.right = lambda: self.left

def accept_string(value: str) -> None: ...

reveal_type(Example().left())  # revealed: str | Divergent | (() -> int)
accept_string(Example().left())  # error: [invalid-argument-type]
```

`consumer.py`:

```py
from model import Example

def accept_int(value: int) -> None: ...

reveal_type(Example().right())  # revealed: int | Divergent | (() -> str)
accept_int(Example().right())  # error: [invalid-argument-type]
```

## Mutually dependent attributes captured by lazy callables, consumer first

Checking the other callable first must not hide either callable alternative or suppress its invalid
argument diagnostic.

`consumer.py`:

```py
from model import Example

def accept_int(value: int) -> None: ...

reveal_type(Example().right())  # revealed: int | Divergent | (() -> str)
accept_int(Example().right())  # error: [invalid-argument-type]
```

`model.py`:

```py
class Example:
    def initialize(self) -> None:
        self.left = lambda: "a"
        self.right = lambda: 1

    def update(self, flag: bool) -> None:
        if flag:
            self.left = lambda: self.right
        else:
            self.right = lambda: self.left

def accept_string(value: str) -> None: ...

reveal_type(Example().left())  # revealed: str | Divergent | (() -> int)
accept_string(Example().left())  # error: [invalid-argument-type]
```

## Mutually dependent lazy callables with finite return types

Calling the other attribute inside each lambda does not repeatedly wrap its return type. This finite
dependency must converge without introducing a divergent callable.

```py
class Example:
    def initialize(self) -> None:
        self.left = lambda: "a"
        self.right = lambda: 1

    def update(self, flag: bool) -> None:
        if flag:
            self.left = lambda: self.right()
        else:
            self.right = lambda: self.left()

def accept_int(value: int) -> None: ...

reveal_type(Example().left)  # revealed: () -> int | str
reveal_type(Example().right)  # revealed: () -> str | int
reveal_type(Example().right())  # revealed: str | int
accept_int(Example().right())  # error: [invalid-argument-type]
```

## Lazy callables capturing attributes on another class

A deferred lambda can capture an attribute on an annotated receiver belonging to another class. Its
type dependency still participates in the shared cross-class component.

```py
from __future__ import annotations

class Left:
    def initialize(self) -> None:
        self.callback = lambda: "a"

    def update(self, other: Right) -> None:
        self.callback = lambda: other.callback

class Right:
    def initialize(self) -> None:
        self.callback = lambda: 1

    def update(self, other: Left) -> None:
        self.callback = lambda: other.callback

def accept_string(value: str) -> None: ...
def accept_int(value: int) -> None: ...

accept_string(Left().callback())  # error: [invalid-argument-type]
accept_int(Right().callback())  # error: [invalid-argument-type]
```

## Lazy callable parameters that shadow the enclosing receiver

A lambda parameter named `self` shadows the method receiver. Its attribute read must not create a
false dependency on the enclosing class's attribute.

```py
class Example:
    def initialize(self) -> None:
        self.left = lambda: "a"
        self.right = lambda: 1

    def update(self, flag: bool) -> None:
        if flag:
            self.left = lambda self: self.right
        else:
            self.right = lambda self: self.left

reveal_type(Example().left)  # revealed: (() -> str) | ((self) -> Unknown)
reveal_type(Example().right)  # revealed: (() -> int) | ((self) -> Unknown)
```

## Recursive attributes that repeatedly introduce a callable

A callable that returns the previous attribute value produces an unbounded recursive type without
preventing inference from terminating.

```py
from collections.abc import Callable
from typing import TypeVar

T = TypeVar("T")

def wrap(value: T) -> Callable[[], T]:
    return lambda: value

class Example:
    def initialize(self) -> None:
        self.value = 1

    def update(self) -> None:
        self.value = wrap(self.value)

reveal_type(Example().value)  # revealed: int | (() -> Divergent)
```

## Recursive attributes in callable parameter types

Recursive growth in a callable's parameter type is normalized just like growth in its return type.

```py
from collections.abc import Callable
from typing import TypeVar

T = TypeVar("T")

def wrap(value: T) -> Callable[[T], int]:
    return lambda _: 1

class Example:
    def initialize(self) -> None:
        self.value = 1

    def update(self) -> None:
        self.value = wrap(self.value)

reveal_type(Example().value)  # revealed: int | ((Divergent, /) -> int)
```

## Recursive attributes nested in a class object

Recursion remains guarded when the attribute value appears beneath both a list and a class object.

```py
from typing import TypeVar

T = TypeVar("T")

def wrap(value: T) -> list[type[T]]:
    return [type(value)]

class Example:
    def initialize(self) -> None:
        self.value = 1

    def update(self) -> None:
        self.value = wrap(self.value)

reveal_type(Example().value)  # revealed: int | list[Divergent]
```

## Recursive attributes nested in a structural protocol

Protocol specializations preserve the recursive marker without repeatedly expanding their type
argument.

```py
from typing import Protocol, TypeVar

T = TypeVar("T")

class Wrapper(Protocol[T]):
    def wrap(self, value: T) -> T: ...

def wrap(value: T) -> Wrapper[T]:
    raise NotImplementedError

class Example:
    def initialize(self) -> None:
        self.value = 1

    def update(self) -> None:
        self.value = wrap(self.value)

reveal_type(Example().value)  # revealed: int | Wrapper[Divergent]
```

## Recursive attributes that repeatedly wrap a projected element

Recursion can expand an extracted element without ever containing the preceding complete attribute
type as a nested value.

```py
class Example:
    def initialize(self) -> None:
        self.value = ["a"]

    def update(self) -> None:
        self.value = [(self.value[0],)]

reveal_type(Example().value)  # revealed: list[str] | list[Divergent]
```

## Recursive attributes that wrap comprehension elements

An eagerly executed comprehension can introduce the same unbounded element recursion from a nested
inference scope.

```py
class Example:
    def initialize(self) -> None:
        self.value = ["a"]

    def update(self) -> None:
        self.value = [(item,) for item in self.value]

reveal_type(Example().value)  # revealed: list[str] | list[Divergent]
```

## Mutually recursive attributes with alternating collection types

Different attributes can grow the same recursive type through alternating list and tuple
constructors.

```py
class Example:
    def initialize(self) -> None:
        self.left = 1
        self.right = "a"

    def update(self, flag: bool) -> None:
        if flag:
            self.left = [self.right]
        else:
            self.right = (self.left,)

def accept_string(value: str) -> None: ...

reveal_type(Example().left)  # revealed: int | list[Divergent] | list[str]
reveal_type(Example().right)  # revealed: str | tuple[Divergent] | tuple[int]
accept_string(Example().right[0])  # error: [invalid-argument-type]
```

## Finite mutually dependent attributes that wrap an initially nested value

Wrapping another attribute and then extracting the result reaches a finite fixed point even when the
independently initialized values already contain the same collection constructor.

```py
class Example:
    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

    def update(self, flag: bool) -> None:
        if flag:
            self.left = [self.right]
        else:
            self.right = self.left[0]

reveal_type(Example().left)  # revealed: list[str] | list[list[str] | str]
reveal_type(Example().right)  # revealed: list[str] | str
```

## Finite mutually dependent attributes that unwrap several collections

A cycle can introduce several nested collections through different attributes and then remove all of
them again without constructing an infinitely recursive type.

```py
class Example:
    def initialize(self) -> None:
        self.left = ["a"]
        self.middle = ["a"]
        self.right = ["a"]

    def update(self, mode: int) -> None:
        if mode == 0:
            self.left = [self.middle]
        elif mode == 1:
            self.middle = [self.right]
        else:
            self.right = self.left[0][0]

reveal_type(Example().left)  # revealed: list[str] | list[list[str] | list[list[str] | str]]
reveal_type(Example().middle)  # revealed: list[str] | list[list[str] | str]
reveal_type(Example().right)  # revealed: list[str] | str
```

## Finite mutually dependent attributes that wrap several times per assignment

A finite cycle can add several collection layers in each of two assignments before a third
assignment removes those layers. Its propagated element type must remain available to diagnostics.

```py
class Example:
    def initialize(self) -> None:
        self.left = 1
        self.middle = "a"
        self.right = b"a"

    def update(self, mode: int) -> None:
        if mode == 0:
            self.left = [[[self.middle]]]
        elif mode == 1:
            self.middle = [[[self.right]]]
        else:
            self.right = self.left[0][0][0][0][0][0] if isinstance(self.left, list) else False

def accept_initial(value: bytes | bool) -> None: ...

# revealed: int | list[list[list[str | list[list[list[bytes | str | bool]]]]]]
reveal_type(Example().left)
accept_initial(Example().right)  # error: [invalid-argument-type]
reveal_type(Example().right)  # revealed: bytes | str | bool
```

## Finite mutually dependent attributes that unwrap a collection

Wrapping one attribute and immediately unwrapping it in the other remains finite, even when a new
element type appears after the first iteration.

```py
class Example:
    def initialize(self) -> None:
        self.left = 1
        self.right = "a"

    def update(self, flag: bool) -> None:
        if flag:
            self.left = [self.right]
        else:
            self.right = self.left[0] if isinstance(self.left, list) else b"a"

reveal_type(Example().left)  # revealed: int | list[str | bytes]
reveal_type(Example().right)  # revealed: str | bytes
```

## Mutually dependent attributes in comprehensions, left first

Eager comprehension scopes retain dependencies on the enclosing method's receiver.

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [item for item in self.right]
        else:
            self.right = [item for item in self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str]
```

## Mutually dependent attributes in comprehensions, right first

Checking the opposite attribute first must preserve both comprehension element types.

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str]
```

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [item for item in self.right]
        else:
            self.right = [item for item in self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

## Mutually dependent attributes captured by comprehension bodies

An attribute accessed only inside a comprehension body still belongs to the enclosing method's
recursive attribute group.

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [self.right[0] for _ in [0]]
        else:
            self.right = [self.left[0] for _ in [0]]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
reveal_type(Example().right)  # revealed: list[str]
```

## Mutually dependent attributes captured through local aliases, left first

A comprehension can capture an enclosing local variable that aliases another attribute in the same
recursive component.

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            previous = self.right
            self.left = [previous[0] for _ in [0]]
        else:
            previous = self.left
            self.right = [previous[0] for _ in [0]]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str]
```

## Mutually dependent attributes captured through local aliases, right first

Reading the opposite attribute first must not change dependencies captured through a local alias.

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[str]
```

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            previous = self.right
            self.left = [previous[0] for _ in [0]]
        else:
            previous = self.left
            self.right = [previous[0] for _ in [0]]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[str]
```

## Differently initialized attributes when the left attribute is checked first

Mutually dependent attributes retain both independently initialized types.

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = [1]

reveal_type(Example().left)  # revealed: list[str | int] | list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[int | str] | list[int]
```

## Differently initialized attributes when the right attribute is checked first

Checking the opposite attribute first preserves both independently initialized types.

`right.py`:

```py
from left import Example

reveal_type(Example().right)  # revealed: list[int | str] | list[int]
```

`left.py`:

```py
class Example:
    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = [1]

reveal_type(Example().left)  # revealed: list[str | int] | list[str]
```

## Three mutually dependent attributes with different initial types

Three differently initialized attributes converge without dropping any of their initial types.

```py
class Example:
    def initialize(self) -> None:
        self.left = ["a"]
        self.middle = [1]
        self.right = [b"a"]

    def update(self) -> None:
        self.left = [*self.middle]
        self.middle = [*self.right]
        self.right = [*self.left]

    def get_left(self) -> list[str]:
        return self.left  # error: [invalid-return-type]

reveal_type(Example().left)  # revealed: list[str] | list[int | bytes | str]
```

## Mutually dependent class attributes when the left attribute is checked first

Independently initialized class attributes can depend on each other through different branches.

`left.py`:

```py
class Example:
    @classmethod
    def update(cls, flag: bool) -> None:
        if flag:
            cls.left = [*cls.right]
        else:
            cls.right = [*cls.left]

    @classmethod
    def initialize(cls) -> None:
        cls.left = ["a"]
        cls.right = ["a"]

reveal_type(Example.left)  # revealed: list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example.right)  # revealed: list[str]
```

## Mutually dependent class attributes when the right attribute is checked first

Checking the opposite class attribute first preserves both independently initialized types.

`right.py`:

```py
from left import Example

reveal_type(Example.right)  # revealed: list[str]
```

`left.py`:

```py
class Example:
    @classmethod
    def update(cls, flag: bool) -> None:
        if flag:
            cls.left = [*cls.right]
        else:
            cls.right = [*cls.left]

    @classmethod
    def initialize(cls) -> None:
        cls.left = ["a"]
        cls.right = ["a"]

reveal_type(Example.left)  # revealed: list[str]
```

## Mutually dependent class attributes with different initial types

Class attributes with different initial types converge together across class methods.

```py
class Example:
    @classmethod
    def initialize(cls) -> None:
        cls.left = ["a"]
        cls.right = [1]

    @classmethod
    def update(cls, flag: bool) -> None:
        if flag:
            cls.left = [*cls.right]
        else:
            cls.right = [*cls.left]

reveal_type(Example.left)  # revealed: list[str] | list[int | str]
```

## Mutually dependent class attributes on a differently specialized receiver

A class method can read a recursive class attribute through a class object with a different generic
specialization.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    @classmethod
    def update(cls, other: "type[Example[int]]", flag: bool) -> None:
        if flag:
            cls.left = [*other.right]
        else:
            cls.right = [*cls.left]

    @classmethod
    def initialize(cls, value: T) -> None:
        cls.left = [value]
        cls.right = [value]

reveal_type(Example[str].left)  # revealed: list[int] | list[str]
reveal_type(Example[str].right)  # revealed: list[int | str] | list[str]
```

## Mutually dependent class attributes with a metaclass data descriptor

A metaclass data descriptor takes precedence over provisional class-attribute values.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type | None = None) -> list[int]:
        return [1]

    def __set__(self, instance: object, value: list[int]) -> None:
        pass

class Meta(type):
    right = Descriptor()

class Example(metaclass=Meta):
    @classmethod
    def initialize(cls) -> None:
        cls.left = ["a"]
        cls.right = ["a"]  # error: [invalid-assignment]

    @classmethod
    def update(cls, flag: bool) -> None:
        if flag:
            cls.left = [*cls.right]
        else:
            cls.right = [*cls.left]  # error: [invalid-assignment]

    @classmethod
    def get_left(cls) -> list[str]:
        return cls.left  # error: [invalid-return-type]

reveal_type(Example.left)  # revealed: list[str] | list[int]
reveal_type(Example.right)  # revealed: list[int]
```

## Mutually dependent class attributes with a metaclass declaration

A metaclass declaration takes precedence over provisional class-attribute values.

```py
class Meta(type):
    right: list[int]

class Example(metaclass=Meta):
    @classmethod
    def initialize(cls) -> None:
        cls.left = ["a"]
        cls.right = ["a"]

    @classmethod
    def update(cls, flag: bool) -> None:
        if flag:
            cls.left = [*cls.right]
        else:
            cls.right = [*cls.left]

    @classmethod
    def get_left(cls) -> list[str]:
        return cls.left  # error: [invalid-return-type]

reveal_type(Example.left)  # revealed: list[str] | list[int]
reveal_type(Example.right)  # revealed: list[int]
```

## Mutually dependent attributes that widen an initial value

Widening one attribute invalidates provisional values for other attributes that depend on it.

```py
class Example:
    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

    def update(self, flag: bool) -> None:
        if flag:
            self.left = self.right
        else:
            self.right = self.left + [1]

    def get_left(self) -> list[str]:
        return self.left  # error: [invalid-return-type]

reveal_type(Example().left)  # revealed: list[str] | list[int | str]
reveal_type(Example().right)  # revealed: list[str] | list[int | str]
```

## Mutually dependent attributes with a class-body declaration

A class-body declaration takes precedence over a provisional instance-attribute value.

```py
class Example:
    right: list[int]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]  # error: [invalid-assignment]

    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left]  # error: [invalid-assignment]

    def get_left(self) -> list[str]:
        return self.left  # error: [invalid-return-type]

reveal_type(Example().left)  # revealed: list[str] | list[int]
reveal_type(Example().right)  # revealed: list[int]
```

## Mutually dependent attributes with an inherited data descriptor

An inherited data descriptor retains precedence over provisional instance-attribute values.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type | None = None) -> list[int]:
        return [1]

    def __set__(self, instance: object, value: list[int]) -> None:
        pass

class Base:
    right = Descriptor()

class Example(Base):
    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]  # error: [invalid-assignment]

    def update(self, flag: bool) -> None:
        if flag:
            self.left = [*self.right]
        else:
            self.right = [*self.left]  # error: [invalid-assignment]

    def get_left(self) -> list[str]:
        return self.left  # error: [invalid-return-type]

reveal_type(Example().left)  # revealed: list[str] | list[int]
reveal_type(Example().right)  # revealed: list[int]
```

## Mutually dependent attributes on different instances of the same class

A provisional value for `self.right` cannot replace a value assigned to another instance.

```toml
[rules]
unsound-return-statement = "error"
```

```py
class Example:
    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

    def update(self, other: "Example", flag: bool) -> None:
        if flag:
            other.right = [1]
            self.left = [*other.right]
        else:
            self.right = [*self.left]

    def get_left(self) -> list[str]:
        return self.left  # error: [invalid-return-type]

reveal_type(Example().left)  # revealed: list[str] | list[int]
```

## Mutually dependent attributes on differently specialized instances, left first

A recursive attribute read through another instance uses that instance's generic specialization,
even when the attribute on the current instance is checked first.

`left.py`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    def update(self, other: "Example[int]", flag: bool) -> None:
        if flag:
            self.left = [*other.right]
        else:
            self.right = [*self.left]

    def __init__(self, item: T) -> None:
        self.left = [item]
        self.right = [item]

reveal_type(Example("a").left)  # revealed: list[int] | list[str]
```

`right.py`:

```py
from left import Example

reveal_type(Example("a").right)  # revealed: list[int | str] | list[str]
```

## Mutually dependent attributes on differently specialized instances, right first

Checking the other attribute first must retain both the current instance's type argument and the
type argument of the other instance.

`right.py`:

```py
from left import Example

reveal_type(Example("a").right)  # revealed: list[int | str] | list[str]
```

`left.py`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    def update(self, other: "Example[int]", flag: bool) -> None:
        if flag:
            self.left = [*other.right]
        else:
            self.right = [*self.left]

    def __init__(self, item: T) -> None:
        self.left = [item]
        self.right = [item]

reveal_type(Example("a").left)  # revealed: list[int] | list[str]
```

## Mutually dependent attributes on an aliased specialized instance

A local alias retains the other instance's specialization while participating in the recursive
attribute component.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    def update(self, other: "Example[int]", flag: bool) -> None:
        if flag:
            source = other
            self.left = [*source.right]
        else:
            self.right = [*self.left]

    def __init__(self, item: T) -> None:
        self.left = [item]
        self.right = [item]

reveal_type(Example("a").left)  # revealed: list[int] | list[str]
reveal_type(Example("a").right)  # revealed: list[int | str] | list[str]
```

## Mutually dependent attributes captured from a specialized instance

An eager comprehension can capture a receiver other than the enclosing method's first parameter. Its
recursive attribute read still uses the captured receiver's specialization.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    def update(self, other: "Example[int]", flag: bool) -> None:
        if flag:
            self.left = [other.right[0] for _ in [0]]
        else:
            self.right = [self.left[0] for _ in [0]]

    def __init__(self, item: T) -> None:
        self.left = [item]
        self.right = [item]

reveal_type(Example("a").left)  # revealed: list[int] | list[str]
reveal_type(Example("a").right)  # revealed: list[int | str] | list[str]
```

## Mutually dependent attributes preserve assignments on another instance

A value assigned directly to another instance must take precedence over the symbolic component's
value for that instance.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    def update(self, other: "Example[str]", flag: bool) -> None:
        if flag:
            other.right = [1]
            self.left = [*other.right]
        else:
            self.right = [*self.left]

    def __init__(self, item: T) -> None:
        self.left = [item]
        self.right = [item]

def accept_string(value: str) -> None: ...

reveal_type(Example("a").left)  # revealed: list[int] | list[str]
accept_string(Example("a").left[0])  # error: [invalid-argument-type]
```

## Mutually dependent attributes preserve assignments through receiver aliases

Assigning through a local alias of another instance retains the same concrete value as assigning
through the original receiver.

```py
class Example:
    def update(self, other: "Example", flag: bool) -> None:
        if flag:
            source = other
            source.right = [1]
            self.left = [*source.right]
        else:
            self.right = [*self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

def accept_string(value: str) -> None: ...

reveal_type(Example().left)  # revealed: list[int] | list[str]
accept_string(Example().left[0])  # error: [invalid-argument-type]
```

## Captured attributes preserve assignments on another instance

An eager comprehension must retain a value assigned to its captured receiver in the enclosing
method.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Example(Generic[T]):
    def update(self, other: "Example[str]", flag: bool) -> None:
        if flag:
            other.right = [1]
            self.left = [other.right[0] for _ in [0]]
        else:
            self.right = [self.left[0] for _ in [0]]

    def __init__(self, item: T) -> None:
        self.left = [item]
        self.right = [item]

def accept_string(value: str) -> None: ...

reveal_type(Example("a").left)  # revealed: list[int] | list[str]
accept_string(Example("a").left[0])  # error: [invalid-argument-type]
```

## Mutually dependent attributes distinguish unrelated receivers

Another class can have an attribute with the same name without its value being replaced by the
current class's provisional attribute.

```py
class Other:
    def __init__(self) -> None:
        self.right = [b"a"]

class Example:
    def update(self, other: Other, flag: bool) -> None:
        if flag:
            self.left = [*other.right]
        else:
            self.right = [*self.left]

    def initialize(self) -> None:
        self.left = ["a"]
        self.right = ["a"]

reveal_type(Example().left)  # revealed: list[bytes] | list[str]
reveal_type(Example().right)  # revealed: list[bytes | str] | list[str]
```

## Finite attributes that read a deeply nested value from another class

A same-named attribute on an unrelated receiver does not prove that the current class has a
recursively growing attribute. Its finite nested value must remain available to diagnostics.

```py
class Other:
    def __init__(self) -> None:
        self.right = [[[1]]]

class Example:
    def __init__(self) -> None:
        self.left = "a"
        self.right = "a"

    def update(self, other: Other, flag: bool) -> None:
        if flag:
            self.left = [other.right]
        else:
            self.right = [self.left]

def accept_string(value: str) -> None: ...

reveal_type(Example().left)  # revealed: str | list[list[list[list[int]]]]
reveal_type(Example().right)  # revealed: str | list[str | list[list[list[list[int]]]]]
accept_string(Example().right[0])  # error: [invalid-argument-type]
```

## Finite cross-class attributes distinguish unrelated receiver contributions

A deeply nested value from a third class must not be mistaken for constructor growth on a separate
finite cycle. The resulting concrete value must remain available to diagnostics.

```py
from __future__ import annotations

class Foreign:
    def initialize(self) -> None:
        self.values = [[[[1]]]]

class Left:
    def initialize(self) -> None:
        self.values = "left"

    def update(self, other: Right) -> None:
        self.values = [other.values]

class Right:
    def initialize(self) -> None:
        self.values = "right"

    def update(self, other: Left, foreign: Foreign, choose: bool) -> None:
        if choose:
            self.values = other.values[0]
        else:
            self.values = [foreign.values]

def accept_string(value: str) -> None: ...

reveal_type(Left().values)  # revealed: str | list[str | list[list[list[list[list[int]]]]]]
reveal_type(Right().values)  # revealed: str | list[list[list[list[list[int]]]]]
accept_string(Left().values[0])  # error: [invalid-argument-type]
```

## Mutually dependent attributes on different classes, defining class first

Attributes on two classes can form one recursive dependency. A value introduced on either side must
remain available to diagnostics when the class definitions are checked first.

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Left:
    def update(self, other: Right) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
```

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

accept_original(Right().values[0])  # error: [invalid-argument-type]
reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
```

## Mutually dependent attributes on different classes, consumer first

Checking the consumer first must not discard the type introduced by the other class or suppress the
resulting invalid-argument diagnostic.

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

accept_original(Right().values[0])  # error: [invalid-argument-type]
reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
```

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Left:
    def update(self, other: Right) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
```

## Cross-class recursive attributes with different names

An attribute can depend on a differently named attribute owned by another class. Both members must
retain values introduced anywhere in their shared component.

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Left:
    def update(self, other: Right) -> None:
        self.left = [*other.right]

    def initialize(self) -> None:
        self.left = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.right = [*other.left, Added()]

    def initialize(self) -> None:
        self.right = [Original()]

def accept_original(value: Original) -> None: ...

reveal_type(Left().left)  # revealed: list[Original | Added] | list[Original]
reveal_type(Right().right)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().right[0])  # error: [invalid-argument-type]
```

## Cross-class recursive attributes accessed through receiver aliases

A local alias of an annotated receiver must resolve to the same owning class as the original
parameter when discovering the recursive component.

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Left:
    def update(self, other: Right) -> None:
        source = other
        self.values = [*source.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        source = other
        self.values = [*source.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

def accept_original(value: Original) -> None: ...

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

## Cross-class recursive attributes with specialized generic receivers

The shared component is independent of receiver specialization, but every attribute read must apply
the actual type arguments of the class being accessed.

```py
from __future__ import annotations
from typing import Generic, TypeVar

T = TypeVar("T")

class Left(Generic[T]):
    def update(self, other: Right[int]) -> None:
        self.values = [*other.values]

    def __init__(self, item: T) -> None:
        self.values = [item]

class Right(Generic[T]):
    def update(self, other: Left[str]) -> None:
        self.values = [*other.values]

    def __init__(self, item: T) -> None:
        self.values = [item]

def accept_string(value: str) -> None: ...

reveal_type(Left("x").values)  # revealed: list[str | int] | list[str]
reveal_type(Right(1).values)  # revealed: list[int | str] | list[int]
accept_string(Left("x").values[0])  # error: [invalid-argument-type]
```

## Recursive attributes shared by three classes

Discovering a component must follow attribute dependencies transitively instead of handling only
pairs of directly connected classes.

```py
from __future__ import annotations

class Original: ...
class Added: ...

class First:
    def update(self, other: Second) -> None:
        self.first = [*other.second]

    def initialize(self) -> None:
        self.first = [Original()]

class Second:
    def update(self, other: Third) -> None:
        self.second = [*other.third]

    def initialize(self) -> None:
        self.second = [Original()]

class Third:
    def update(self, other: First) -> None:
        self.third = [*other.first, Added()]

    def initialize(self) -> None:
        self.third = [Original()]

def accept_original(value: Original) -> None: ...

reveal_type(First().first)  # revealed: list[Original | Added] | list[Original]
reveal_type(Second().second)  # revealed: list[Original | Added] | list[Original]
reveal_type(Third().third)  # revealed: list[Original | Added] | list[Original]
accept_original(Third().third[0])  # error: [invalid-argument-type]
```

## Cross-class recursive attributes inherited by a subclass

An annotation naming a subclass must identify the base class that actually defines the recursive
attribute, including when the consumer is checked first.

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

accept_original(Right().values[0])  # error: [invalid-argument-type]
reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
```

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Left:
    def update(self, other: RightChild) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

class RightChild(Right): ...

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
```

## Cross-class recursive attributes with union-annotated receivers, defining class first

Each class named by a receiver's union annotation can contribute to a recursive attribute. Checking
the defining classes first must preserve values introduced on either side of that dependency.

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Foreign:
    def initialize(self) -> None:
        self.values = [Original()]

class Left:
    def update(self, other: Right | Foreign) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

def accept_original(value: Original) -> None: ...

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Left().values[0])  # error: [invalid-argument-type]
```

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

## Cross-class recursive attributes with union-annotated receivers, consumer first

Checking the consumer before the classes must not discard the value introduced through the
union-annotated receiver or suppress either invalid-argument diagnostic.

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Foreign:
    def initialize(self) -> None:
        self.values = [Original()]

class Left:
    def update(self, other: Right | Foreign) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

def accept_original(value: Original) -> None: ...

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Left().values[0])  # error: [invalid-argument-type]
```

## Cross-class recursive attributes with aliased union receivers

A type alias for a union receiver names the same possible attribute owners as the union itself.
Resolving the alias must retain the shared component and its concrete invalid-argument diagnostic.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Foreign:
    def initialize(self) -> None:
        self.values = [Original()]

type Receiver = Right | Foreign

class Left:
    def update(self, other: Receiver) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

def accept_original(value: Original) -> None: ...

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Left().values[0])  # error: [invalid-argument-type]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

## Cross-class recursive attributes through conditional receiver aliases

A receiver alias can refer to multiple classes on different branches. Every reachable owner must
contribute an edge to the component, rather than only the first annotated receiver.

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

accept_original(Right().values[0])  # error: [invalid-argument-type]
reveal_type(Right().values)  # revealed: list[Original] | list[Original | Added]
```

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Foreign:
    def initialize(self) -> None:
        self.values = [Original()]

class Left:
    def initialize(self) -> None:
        self.values = [Original()]

    def update(self, other: Right, foreign: Foreign, choose: bool) -> None:
        if choose:
            source = foreign
        else:
            source = other
        self.values = [*source.values]

class Right:
    def initialize(self) -> None:
        self.values = [Original()]

    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

reveal_type(Left().values)  # revealed: list[Original] | list[Original | Added]
```

## Cross-class recursive attributes after writes through conditional receivers

A conditional receiver can be assigned before its attribute is captured by an eager comprehension.
The concrete local assignment must remain visible to every class in the recursive component.

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

accept_original(Right().values[0])  # error: [invalid-argument-type]
reveal_type(Right().values)  # revealed: list[Added | Original] | list[Original]
```

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Foreign:
    def initialize(self) -> None:
        self.values = [Added()]

class Left:
    def update(self, other: Right, foreign: Foreign, choose: bool) -> None:
        source = other if choose else foreign
        source.values = [Added()]
        self.values = [source.values[0] for _ in [0]]

    def initialize(self) -> None:
        self.values = [Original()]

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

reveal_type(Left().values)  # revealed: list[Added] | list[Original]
```

## Cross-class receiver annotations with recursive defaults, defining class first

Discovering the class named by a receiver annotation must not infer an unrelated parameter default
before the shared attribute component has been established.

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

def make_right(value: object) -> Right:
    return Right()

class Left:
    def update(self, other: Right = make_right(Right().values[0])) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
```

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

## Cross-class receiver annotations with recursive defaults, consumer first

Checking the consumer first must produce the same component even when the other class's method has a
default value that reads the recursive attribute.

`consumer.py`:

```py
from model import Original, Right

def accept_original(value: Original) -> None: ...

reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

`model.py`:

```py
from __future__ import annotations

class Original: ...
class Added: ...

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]

def make_right(value: object) -> Right:
    return Right()

class Left:
    def update(self, other: Right = make_right(Right().values[0])) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
```

## Cross-class recursive attributes defined in separate modules

Recursive components can cross a module boundary. Each class's assignments must be discovered
through its own tracked query, and the consumer must retain values introduced by either module.

`consumer.py`:

```py
from left import Left
from right import Right
from shared import Original

def accept_original(value: Original) -> None: ...

reveal_type(Left().values)  # revealed: list[Original | Added] | list[Original]
reveal_type(Right().values)  # revealed: list[Original | Added] | list[Original]
accept_original(Right().values[0])  # error: [invalid-argument-type]
```

`left.py`:

```py
from __future__ import annotations
from typing import TYPE_CHECKING
from shared import Original

if TYPE_CHECKING:
    from right import Right

class Left:
    def update(self, other: Right) -> None:
        self.values = [*other.values]

    def initialize(self) -> None:
        self.values = [Original()]
```

`right.py`:

```py
from __future__ import annotations
from typing import TYPE_CHECKING
from shared import Added, Original

if TYPE_CHECKING:
    from left import Left

class Right:
    def update(self, other: Left) -> None:
        self.values = [*other.values, Added()]

    def initialize(self) -> None:
        self.values = [Original()]
```

`shared.py`:

```py
class Original: ...
class Added: ...
```

## Final annotations that establish an attribute type

An initialized bare `Final` infers its value, while `Final[T]` retains its explicit declared type.

```py
from typing import Final

class Initialized:
    def __init__(self):
        self.values: Final = ["a"]

class Declared:
    def __init__(self):
        self.values: Final[list[str]]
        self.values = ["a"]

reveal_type(Initialized().values)  # revealed: list[str]
reveal_type(Declared().values)  # revealed: list[str]
```

## Same-named attributes on unrelated receivers

An independently initialized attribute must not replace a different receiver's same-named attribute.

```py
class Other:
    def __init__(self):
        self.values = [1]

class Example:
    def __init__(self):
        self.values = ["a"]

    def update(self, other: Other):
        self.values = [*other.values]

reveal_type(Example().values)  # revealed: list[str] | list[int]
```

## Inherited attributes remain stable across assignment forms

Inherited attribute inference does not depend on how an assignment accesses or binds the previous
value.

```toml
[rules]
unsound-return-statement = "error"
```

```py
class Base:
    values = ["a"]

class Child(Base):
    def aliased(self) -> None:
        first = self.values
        second = first
        self.values = second + ["b"]

    def augmented_alias(self) -> None:
        previous = self.values
        previous += ["b"]
        self.values = previous

    def named_alias(self) -> None:
        (previous := self.values)
        self.values = previous + ["b"]

    def unpacked(self) -> None:
        (self.values,) = (self.values + ["b"],)

    def loop_target(self) -> None:
        for self.values in [self.values + ["b"]]:
            pass

    def get_values(self) -> list[str]:
        return self.values
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
