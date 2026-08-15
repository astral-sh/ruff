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

## Recursive instance attributes widen independently established types

An inferred attribute is not a declaration: every assignment contributes to its type, even when two
attributes depend on one another.

```py
class Scalars:
    def reset(self):
        self.left = 1
        self.right = "initial"

    def update(self, flag: bool):
        if flag:
            self.left = self.right
        else:
            self.right = self.left

reveal_type(Scalars().left)  # revealed: int | str
reveal_type(Scalars().right)  # revealed: str | int
```

Collection elements introduced through either attribute remain visible to typed consumers.

```py
class Cyclic:
    def reset(self):
        self.left = [1]
        self.right = [1]

    def update(self):
        self.left = [*self.right]
        self.right = [*self.left, "added"]

def accept_int(value: int) -> None: ...

reveal_type(Cyclic().left)  # revealed: list[int] | list[int | str]
reveal_type(Cyclic().right)  # revealed: list[int] | list[int | str]
accept_int(Cyclic().left[0])  # error: [invalid-argument-type]
accept_int(Cyclic().right[0])  # error: [invalid-argument-type]
```

## Explicitly declared recursive attributes retain their upper bounds

Unlike independently inferred values, class-body annotations are actual assignment contracts.

```py
class Declared:
    left: int
    right: str

    def reset(self):
        self.left = 1
        self.right = "initial"

    def update(self):
        self.left = self.right  # error: [invalid-assignment]
        self.right = self.left  # error: [invalid-assignment]

reveal_type(Declared().left)  # revealed: int
reveal_type(Declared().right)  # revealed: str
```

## Recursive attributes written through assignment targets

Unpacking, loop targets, context managers, and comprehensions all write instance attributes without
using an ordinary attribute assignment.

```py
from contextlib import nullcontext

class Cyclic:
    def reset(self):
        self.unpacked_left = 1
        self.unpacked_right = "initial"
        self.loop_left = 1
        self.loop_right = "initial"
        self.context_left = 1
        self.context_right = "initial"
        self.comprehension_left = 1
        self.comprehension_right = "initial"

    def update(self):
        (self.unpacked_left,) = (self.unpacked_right,)
        (self.unpacked_right,) = (self.unpacked_left,)

        for self.loop_left in [self.loop_right]:
            pass
        for self.loop_right in [self.loop_left]:
            pass

        with nullcontext(self.context_right) as self.context_left:
            pass
        with nullcontext(self.context_left) as self.context_right:
            pass

        [self.comprehension_left for self.comprehension_left in [self.comprehension_right]]
        [self.comprehension_right for self.comprehension_right in [self.comprehension_left]]

reveal_type(Cyclic().unpacked_left)  # revealed: int | str
reveal_type(Cyclic().loop_left)  # revealed: int | str
reveal_type(Cyclic().context_left)  # revealed: int | str
reveal_type(Cyclic().comprehension_left)  # revealed: int | str
```

## Recursive assignments respect narrowing and unreachable branches

Filtering another attribute with `isinstance` must preserve its narrowed type. A constant-false
branch contributes no assignment.

```py
class Cyclic:
    def reset(self):
        self.left = 1
        self.right = "initial"

    def update(self):
        if isinstance(self.right, int):
            self.left = self.right
        if isinstance(self.left, str):
            self.right = self.left
        if False:
            self.left = object()

reveal_type(Cyclic().left)  # revealed: int
reveal_type(Cyclic().right)  # revealed: str
```

## Recursive assignments preserve contextual `TypedDict` inference

An explicit `TypedDict` declaration provides the expected type for a dictionary literal while its
value reads another declared attribute.

```py
from typing import TypedDict

class Payload(TypedDict):
    value: int

class Cyclic:
    left: Payload
    right: Payload

    def update(self):
        self.left = {"value": self.right["value"]}
        self.right = {"value": self.left["value"]}

reveal_type(Cyclic().left)  # revealed: Payload
reveal_type(Cyclic().right)  # revealed: Payload
```

## Recursive instance attributes normalize nested collection growth

Wrapping another independently initialized list or tuple is a valid assignment. Recursive growth
must terminate without manufacturing an assignment error.

```py
class Cyclic:
    def reset(self):
        self.list_left = [1]
        self.list_right = [1]
        self.tuple_left = ("initial",)
        self.tuple_right = (1,)

    def update(self):
        self.list_left = [self.list_right]
        self.list_right = [self.list_left]
        self.tuple_left = (self.tuple_right,)
        self.tuple_right = (self.tuple_left,)

def accept_int(value: int) -> None: ...
def accept_string(value: str) -> None: ...

accept_int(Cyclic().list_left[0])  # error: [invalid-argument-type]
accept_string(Cyclic().tuple_left[0])  # error: [invalid-argument-type]
```

## Recursive instance attributes propagate self-growing dependencies

A self-growing attribute still participates in the cycle when another independently initialized
attribute reads it.

```py
class Cyclic:
    def reset(self):
        self.left = ["initial"]
        self.middle = [1]
        self.right = [b"initial"]

    def update(self):
        self.left = [*self.middle]
        self.middle = [*self.right]
        self.right = [self.right]

def accept_string(value: str) -> None: ...

accept_string(Cyclic().left[0])  # error: [invalid-argument-type]
```

## Recursive attributes propagate dependencies through local aliases

Aliases for an attribute value or its receiver do not hide a dependency on self-growing instance or
class attributes.

```py
class Cyclic:
    def reset(self):
        self.middle = [1]
        self.right = [b"initial"]

    def copy_value(self):
        value = self.right
        self.middle = [*value]

    def copy_receiver(self):
        receiver = self
        self.middle = [*receiver.right]

    def update(self):
        self.right = [self.right]

class ClassCyclic:
    @classmethod
    def reset(cls):
        cls.middle = [1]
        cls.right = [b"initial"]

    @classmethod
    def update(cls):
        value = cls.right
        cls.middle = [*value]
        cls.right = [cls.right]

def accept_int(value: int) -> None: ...

accept_int(Cyclic().middle[0])  # error: [invalid-argument-type]
accept_int(ClassCyclic.middle[0])  # error: [invalid-argument-type]
```

## Recursive attributes ignore unreachable dependent assignments

An overridden method and an unreachable branch do not turn standalone recursive growth into a
cross-attribute dependency.

```py
class Cyclic:
    def reset(self):
        self.values = [1]

    def update(self):
        self.values = [self.values]

    def read(self):
        self.other = [*self.values]

    def read(self):
        pass

    def never_read(self):
        if False:
            self.another = [*self.values]

reveal_type(Cyclic().values)  # revealed: list[int] | list[Divergent]
```

## Recursive instance attributes copied through collection expressions

Dictionary unpacking, comprehensions, conditional expressions, nested calls, and mapped lambdas can
each transfer another attribute's independently established type. Separate attribute pairs ensure
that recognizing one expression does not hide a failure to recognize another.

```py
class Cyclic:
    def reset(self):
        self.mapping_left = {"left": "initial"}
        self.mapping_right = {"right": "initial"}
        self.comprehension_left = [1]
        self.comprehension_right = [1]
        self.conditional_left = [1]
        self.conditional_right = [1]
        self.nested_left = [1]
        self.nested_right = [1]
        self.mapped_left = [1]
        self.mapped_right = [1]

    def update(self, flag: bool):
        self.mapping_left = {**self.mapping_right}
        self.mapping_right = {**self.mapping_left, "added": 1}
        self.comprehension_left = [value for value in self.comprehension_right]
        self.comprehension_right = [value for value in self.comprehension_left] + ["added"]
        self.conditional_left = self.conditional_right if flag else self.conditional_right
        self.conditional_right = self.conditional_left or ["added"]
        self.nested_left = list(tuple(self.nested_right))
        self.nested_right = list(tuple(self.nested_left)) + ["added"]
        self.mapped_left = list(map(lambda value: value, self.mapped_right))
        self.mapped_right = list(map(lambda value: value, self.mapped_left)) + ["added"]

def accept_int(value: int) -> None: ...
def accept_string(value: str) -> None: ...

accept_string(Cyclic().mapping_left["added"])  # error: [invalid-argument-type]
accept_string(Cyclic().mapping_right["added"])  # error: [invalid-argument-type]
accept_int(Cyclic().comprehension_left[0])  # error: [invalid-argument-type]
accept_int(Cyclic().comprehension_right[0])  # error: [invalid-argument-type]
accept_int(Cyclic().conditional_left[0])  # error: [invalid-argument-type]
accept_int(Cyclic().conditional_right[0])  # error: [invalid-argument-type]
accept_int(Cyclic().nested_left[0])  # error: [invalid-argument-type]
accept_int(Cyclic().nested_right[0])  # error: [invalid-argument-type]
accept_int(Cyclic().mapped_left[0])  # error: [invalid-argument-type]
accept_int(Cyclic().mapped_right[0])  # error: [invalid-argument-type]
```

## Recursive attributes preserve valid contextual collection widening

A copied `list[int]` can coexist with an inferred `list[object]` or a compatible union alternative.
Assignments in either direction widen the inferred attribute; neither becomes an upper bound.

```py
class Cyclic:
    def reset(self, flag: bool):
        self.left = [object()]
        self.right = [1]
        if flag:
            self.union_left = [object()]
        else:
            self.union_left = ["initial"]
        self.union_right = [1]

    def update(self, flag: bool):
        if flag:
            self.left = [*self.right]
            self.union_left = [*self.union_right]
        else:
            self.right = [*self.left]
            self.union_right = [*self.union_left]

def accept_int(value: int) -> None: ...

accept_int(Cyclic().right[0])  # error: [invalid-argument-type]
accept_int(Cyclic().union_right[0])  # error: [invalid-argument-type]
```

## Recursive attribute dependencies inside lambda scopes

Inferring a lambda's return type can read another attribute even though calling the lambda is
deferred. Nested lambdas and comprehensions can also capture the enclosing method's receiver.

```py
class Cyclic:
    def reset(self):
        self.left = lambda: "initial"
        self.right = lambda: 1
        self.nested_left = lambda: "initial"
        self.nested_right = lambda: 1

    def update(self, flag: bool):
        if flag:
            self.left = lambda: self.right
            self.nested_left = lambda: lambda: self.nested_right
        else:
            self.right = lambda: self.left
            self.nested_right = lambda: [self.nested_left for _ in [0]][0]

def accept_string(value: str) -> None: ...

accept_string(Cyclic().left())  # error: [invalid-argument-type]
accept_string(Cyclic().nested_left())  # error: [invalid-argument-type]
```

## Recursive attributes across classes when definitions are checked first

Assignments across two classes contribute to a shared fixed point rather than treating either
independent initializer as a declaration.

`model.py`:

```py
class Left:
    def reset(self):
        self.values = [1]

    def update(self, other: "Right"):
        self.values = [*other.values]

class Right:
    def reset(self):
        self.values = [1]

    def update(self, other: Left):
        self.values = [*other.values, "added"]
```

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

reveal_type(Left().values)  # revealed: list[int] | list[int | str]
reveal_type(Right().values)  # revealed: list[int] | list[int | str]
accept_int(Left().values[0])  # error: [invalid-argument-type]
accept_int(Right().values[0])  # error: [invalid-argument-type]
```

## Recursive attributes across classes when uses are checked first

Checking the consumer before the class definitions preserves both widened types and real downstream
diagnostics.

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

reveal_type(Left().values)  # revealed: list[int] | list[int | str]
reveal_type(Right().values)  # revealed: list[int] | list[int | str]
accept_int(Left().values[0])  # error: [invalid-argument-type]
accept_int(Right().values[0])  # error: [invalid-argument-type]
```

`model.py`:

```py
class Left:
    def reset(self):
        self.values = [1]

    def update(self, other: "Right"):
        self.values = [*other.values]

class Right:
    def reset(self):
        self.values = [1]

    def update(self, other: Left):
        self.values = [*other.values, "added"]
```

## Recursive attributes across classes through an intermediate field

An attribute chain beginning with `self` can still read a different instance. Checking the consumer
first must retain the element introduced through that intermediate field.

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

accept_int(Left().values[0])  # error: [invalid-argument-type]
accept_int(Right().values[0])  # error: [invalid-argument-type]
```

`model.py`:

```py
class Left:
    holder: "Right"

    def reset(self):
        self.values = [1]

    def update(self):
        self.values = [*self.holder.values]

class Right:
    holder: Left

    def reset(self):
        self.values = [1]

    def update(self):
        self.values = [*self.holder.values, "added"]
```

## Recursive attributes across classes through conditional intermediate fields

Both branches of a conditional receiver can point to another class. Checking the consumer first must
retain both initializer types and the additional assigned element.

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

accept_int(Left().values[0])  # error: [invalid-argument-type]
accept_int(Right().values[0])  # error: [invalid-argument-type]
```

`model.py`:

```py
class Left:
    holder: "Right"
    alternate: "Right"

    def reset(self):
        self.values = [1]

    def update(self, flag: bool):
        self.values = [*(self.holder if flag else self.alternate).values]

class Right:
    holder: Left
    alternate: Left

    def reset(self):
        self.values = [1]

    def update(self, flag: bool):
        self.values = [*(self.holder if flag else self.alternate).values, "added"]
```

## Recursive attributes across conditionally selected receivers

A conditional expression can choose between two parameters belonging to another class, including
when the classes use different attribute names.

```py
class Left:
    def reset(self):
        self.values = [1]
        self.left = [1]

    def update(self, other: "Right", alternate: "Right", flag: bool):
        self.values = [*(other if flag else alternate).values]
        self.left = [*(other if flag else alternate).right]

class Right:
    def reset(self):
        self.values = [1]
        self.right = [1]

    def update(self, other: Left, alternate: Left, flag: bool):
        self.values = [*(other if flag else alternate).values, "added"]
        self.right = [*(other if flag else alternate).left, "added"]

def accept_int(value: int) -> None: ...

accept_int(Left().values[0])  # error: [invalid-argument-type]
accept_int(Right().values[0])  # error: [invalid-argument-type]
accept_int(Left().left[0])  # error: [invalid-argument-type]
accept_int(Right().right[0])  # error: [invalid-argument-type]
```

## Recursive attributes across flow-narrowed receivers

Built-in narrowing and imported `TypeIs` and `TypeGuard` predicates identify the other participant
in a recursive dependency, even when its declared parameter type is `object`.

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

accept_int(Left().builtin[0])  # error: [invalid-argument-type]
accept_int(Right().builtin[0])  # error: [invalid-argument-type]
accept_int(Left().type_is[0])  # error: [invalid-argument-type]
accept_int(Right().type_is[0])  # error: [invalid-argument-type]
accept_int(Left().type_guard[0])  # error: [invalid-argument-type]
accept_int(Right().type_guard[0])  # error: [invalid-argument-type]
```

`guards.py`:

```py
from typing import TYPE_CHECKING, TypeGuard
from typing_extensions import TypeIs

if TYPE_CHECKING:
    from model import Right

def is_right(value: object) -> TypeIs["Right"]:
    return hasattr(value, "sentinel")

def is_right_guard(value: object) -> TypeGuard["Right"]:
    return hasattr(value, "sentinel")
```

`model.py`:

```py
from guards import is_right, is_right_guard

class Left:
    def reset(self):
        self.builtin = [1]
        self.type_is = [1]
        self.type_guard = [1]

    def update(self, other: object):
        if isinstance(other, Right):
            self.builtin = [*other.builtin]
        if is_right(other):
            self.type_is = [*other.type_is]
        if is_right_guard(other):
            self.type_guard = [*other.type_guard]

class Right:
    sentinel = True

    def reset(self):
        self.builtin = [1]
        self.type_is = [1]
        self.type_guard = [1]

    def update(self, other: Left):
        self.builtin = [*other.builtin, "added"]
        self.type_is = [*other.type_is, "added"]
        self.type_guard = [*other.type_guard, "added"]
```

## Recursive attributes across specialized generic receivers

Specializing a generic field, property, method, or inherited property identifies the concrete
receiver whose attributes participate in a cycle.

`model.py`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Holder(Generic[T]):
    item: T

    @property
    def current(self) -> T:
        return self.item

    def get(self) -> T:
        return self.item

class ConcreteHolder(Holder["Right"]): ...

class Left:
    def reset(self):
        self.field = [1]
        self.property = [1]
        self.method = [1]
        self.inherited = [1]

    def update(self, holder: Holder["Right"], inherited: ConcreteHolder):
        self.field = [*holder.item.field]
        self.property = [*holder.current.property]
        self.method = [*holder.get().method]
        self.inherited = [*inherited.current.inherited]

class Right:
    def reset(self):
        self.field = [1]
        self.property = [1]
        self.method = [1]
        self.inherited = [1]

    def update(self, other: Left):
        self.field = [*other.field, "added"]
        self.property = [*other.property, "added"]
        self.method = [*other.method, "added"]
        self.inherited = [*other.inherited, "added"]
```

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

accept_int(Left().field[0])  # error: [invalid-argument-type]
accept_int(Right().field[0])  # error: [invalid-argument-type]
accept_int(Left().property[0])  # error: [invalid-argument-type]
accept_int(Right().property[0])  # error: [invalid-argument-type]
accept_int(Left().method[0])  # error: [invalid-argument-type]
accept_int(Right().method[0])  # error: [invalid-argument-type]
accept_int(Left().inherited[0])  # error: [invalid-argument-type]
accept_int(Right().inherited[0])  # error: [invalid-argument-type]
```

## Recursive attributes across imported factory receivers

An imported classmethod can return a concrete receiver, while an imported generic factory derives
that receiver from its argument. Local callable aliases, unpacked arguments, and generic callable
instances preserve the same dependencies between attributes in separate files.

`consumer.py`:

```py
from left import Left
from right import Right

def accept_int(value: int) -> None: ...

accept_int(Left().classmethod[0])  # error: [invalid-argument-type]
accept_int(Right().classmethod[0])  # error: [invalid-argument-type]
accept_int(Left().generic[0])  # error: [invalid-argument-type]
accept_int(Right().generic[0])  # error: [invalid-argument-type]
accept_int(Left().alias[0])  # error: [invalid-argument-type]
accept_int(Right().alias[0])  # error: [invalid-argument-type]
accept_int(Left().starred[0])  # error: [invalid-argument-type]
accept_int(Right().starred[0])  # error: [invalid-argument-type]
accept_int(Left().keywords[0])  # error: [invalid-argument-type]
accept_int(Right().keywords[0])  # error: [invalid-argument-type]
accept_int(Left().callable[0])  # error: [invalid-argument-type]
accept_int(Right().callable[0])  # error: [invalid-argument-type]
```

`left.py`:

```py
from right import CallableFactory, Factory, Right, identity

class Left:
    def reset(self):
        self.classmethod = [1]
        self.generic = [1]
        self.alias = [1]
        self.starred = [1]
        self.keywords = [1]
        self.callable = [1]

    def update(self):
        self.classmethod = [*Factory.make().classmethod]
        self.generic = [*identity(Right()).generic]
        local_identity = identity
        self.alias = [*local_identity(Right()).alias]
        self.starred = [*identity(*(Right(),)).starred]
        self.keywords = [*identity(**{"value": Right()}).keywords]
        self.callable = [*CallableFactory(Right())().callable]
```

`right.py`:

```py
from typing import TYPE_CHECKING, Generic, TypeVar

if TYPE_CHECKING:
    from left import Left

class Right:
    def reset(self):
        self.classmethod = [1]
        self.generic = [1]
        self.alias = [1]
        self.starred = [1]
        self.keywords = [1]
        self.callable = [1]

    def update(self, other: "Left"):
        self.classmethod = [*other.classmethod, "added"]
        self.generic = [*other.generic, "added"]
        self.alias = [*other.alias, "added"]
        self.starred = [*other.starred, "added"]
        self.keywords = [*other.keywords, "added"]
        self.callable = [*other.callable, "added"]

T = TypeVar("T")

def identity(value: T) -> T:
    return value

class Factory:
    @classmethod
    def make(cls) -> Right:
        return Right()

class CallableFactory(Generic[T]):
    def __init__(self, value: T) -> None:
        self.value = value

    def __call__(self) -> T:
        return self.value
```

## Recursive attributes across explicitly cast receivers

Direct, renamed, and module-qualified `typing.cast` calls identify a recursive receiver without
treating the independently inferred attribute as an annotated declaration.

`consumer.py`:

```py
from model import Left, Right

def accept_int(value: int) -> None: ...

accept_int(Left().direct[0])  # error: [invalid-argument-type]
accept_int(Right().direct[0])  # error: [invalid-argument-type]
accept_int(Left().renamed[0])  # error: [invalid-argument-type]
accept_int(Right().renamed[0])  # error: [invalid-argument-type]
accept_int(Left().qualified[0])  # error: [invalid-argument-type]
accept_int(Right().qualified[0])  # error: [invalid-argument-type]
```

`model.py`:

```py
import typing
from typing import cast
from typing import cast as coerce

class Left:
    def reset(self):
        self.direct = [1]
        self.renamed = [1]
        self.qualified = [1]

    def update(self, other: object):
        direct = cast(Right, other)
        renamed = coerce(Right, other)
        qualified = typing.cast(Right, other)
        self.direct = [*direct.direct]
        self.renamed = [*renamed.renamed]
        self.qualified = [*qualified.qualified]

class Right:
    def reset(self):
        self.direct = [1]
        self.renamed = [1]
        self.qualified = [1]

    def update(self, other: Left):
        self.direct = [*other.direct, "added"]
        self.renamed = [*other.renamed, "added"]
        self.qualified = [*other.qualified, "added"]
```

## Recursive attributes initialized from independently typed values

Local aliases, class and method annotations, inferred class attributes, and typed properties can all
establish independent initial values. Each attribute pair remains a separate recursive dependency.

```py
class Left:
    declared: int = 1
    inferred = 1

    @property
    def typed(self) -> int:
        return 1

    def reset(self):
        first = 1
        initial = first
        self.alias = [initial]
        self.annotated = [self.declared]
        self.default: int = 1
        self.method = [self.default]
        self.class_value = [self.inferred]
        self.property_value = [self.typed]

    def update(self, other: "Right"):
        self.alias = [*other.alias]
        self.annotated = [*other.annotated]
        self.method = [*other.method]
        self.class_value = [*other.class_value]
        self.property_value = [*other.property_value]

class Right:
    declared: int = 1
    inferred = 1

    @property
    def typed(self) -> int:
        return 1

    def reset(self):
        first = 1
        initial = first
        self.alias = [initial]
        self.annotated = [self.declared]
        self.default: int = 1
        self.method = [self.default]
        self.class_value = [self.inferred]
        self.property_value = [self.typed]

    def update(self, other: Left):
        self.alias = [*other.alias, "added"]
        self.annotated = [*other.annotated, "added"]
        self.method = [*other.method, "added"]
        self.class_value = [*other.class_value, "added"]
        self.property_value = [*other.property_value, "added"]

def accept_int(value: int) -> None: ...

accept_int(Left().alias[0])  # error: [invalid-argument-type]
accept_int(Right().alias[0])  # error: [invalid-argument-type]
accept_int(Left().annotated[0])  # error: [invalid-argument-type]
accept_int(Right().annotated[0])  # error: [invalid-argument-type]
accept_int(Left().method[0])  # error: [invalid-argument-type]
accept_int(Right().method[0])  # error: [invalid-argument-type]
accept_int(Left().class_value[0])  # error: [invalid-argument-type]
accept_int(Right().class_value[0])  # error: [invalid-argument-type]
accept_int(Left().property_value[0])  # error: [invalid-argument-type]
accept_int(Right().property_value[0])  # error: [invalid-argument-type]
```

## Acyclic instance attributes on another receiver

Reading another instance's attribute does not make an otherwise acyclic assignment an error.

```py
class Source:
    def __init__(self):
        self.values = [1]

class Target:
    def reset(self):
        self.values = [0]

    def update(self, source: Source):
        self.values = [*source.values]

reveal_type(Target().values)  # revealed: list[int]
```

## Recursive instance attributes without an independent value

When no assignment establishes the attribute independently, existing cycle recovery still applies.

```py
class Rootless:
    def update(self):
        self.values = [*self.values]

reveal_type(Rootless().values)  # revealed: list[Divergent]
```

## Recursive singleton attributes preserve independently introduced elements

An attribute can contribute a concrete element to its own recursive assignment, with or without a
separate initializer. Other classes must also see that independently introduced element.

```py
class Original: ...
class Added: ...

class Rooted:
    def reset(self):
        self.values = [Original()]

    def update(self):
        self.values = [*self.values, Added()]

class Rootless:
    def update(self):
        self.values = [*self.values, Added()]

class Concatenated:
    def update(self):
        self.values = self.values + [Added()]

class United:
    def update(self):
        self.values = self.values | {Added()}

class Consumer:
    def reset(self):
        self.values = [Original()]

    def update(self, other: Rootless):
        self.values = [*other.values]

def accept_original(value: Original) -> None: ...

reveal_type(Rooted().values)  # revealed: list[Original] | list[Original | Added]
reveal_type(Rootless().values)  # revealed: list[Added]
reveal_type(Consumer().values)  # revealed: list[Original] | list[Added]
accept_original(Rooted().values[0])  # error: [invalid-argument-type]
accept_original(Rootless().values[0])  # error: [invalid-argument-type]
accept_original(Concatenated().values[0])  # error: [invalid-argument-type]
accept_original(next(iter(United().values)))  # error: [invalid-argument-type]
accept_original(Consumer().values[0])  # error: [invalid-argument-type]
```

## Recursive attributes retain independently introduced elements

Without an independent initial collection, direct construction, local aliases, constructor
arguments, unary literals, and class methods can still establish an individual element's type.

```py
class Original: ...

class Added:
    def __init__(self, value: int = 0): ...
    @classmethod
    def create(cls) -> "Added":
        return cls()

class Direct:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left, Added()]

class Aliased:
    def update(self):
        candidate = Added()
        self.left = [*self.right]
        self.right = [*self.left, candidate]

class Constructed:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left, Added(1)]

class Unary:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left, -1]

class ClassMethod:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left, Added.create()]

def accept_original(value: Original) -> None: ...

reveal_type(Direct().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(Aliased().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(Constructed().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(Unary().right)  # revealed: list[int] | list[Unknown | int]
reveal_type(ClassMethod().right)  # revealed: list[Added] | list[Unknown | Added]
accept_original(Direct().left[0])  # error: [invalid-argument-type]
accept_original(Direct().right[0])  # error: [invalid-argument-type]
accept_original(Aliased().left[0])  # error: [invalid-argument-type]
accept_original(Aliased().right[0])  # error: [invalid-argument-type]
accept_original(Constructed().left[0])  # error: [invalid-argument-type]
accept_original(Constructed().right[0])  # error: [invalid-argument-type]
accept_original(Unary().left[0])  # error: [invalid-argument-type]
accept_original(Unary().right[0])  # error: [invalid-argument-type]
accept_original(ClassMethod().left[0])  # error: [invalid-argument-type]
accept_original(ClassMethod().right[0])  # error: [invalid-argument-type]
```

## Recursive attributes retain elements through comprehensions and iterators

Conditional and nested comprehensions, generator expressions, captured local aliases (including
unpacked, loop, context-manager, and lambda-default bindings), composed iterator operations, and
constant-condition branches all preserve independently introduced elements.

```py
from contextlib import nullcontext
from itertools import chain

class Original: ...
class Added: ...

class Conditional:
    def update(self, flag: bool):
        self.left = [*self.right]
        self.right = [value for value in [*self.left, Added()]] if flag else [value for value in [*self.left, Added()]]

class Generator:
    def update(self):
        self.left = [*self.right]
        self.right = list(value for value in [*self.left, Added()])

class Captured:
    def update(self):
        self.left = [*self.right]
        source = [*self.left, Added()]
        self.right = [value for value in source]

class CapturedGenerator:
    def update(self):
        self.left = [*self.right]
        source = [*self.left, Added()]
        self.right = list(value for value in source)

class CapturedUnpacked:
    def update(self):
        self.left = [*self.right]
        (source,) = ([*self.left, Added()],)
        self.right = [value for value in source]

class CapturedLoop:
    def update(self):
        self.left = [*self.right]
        for source in [[*self.left, Added()]]:
            self.right = [value for value in source]

class CapturedContext:
    def update(self):
        self.left = [*self.right]
        with nullcontext([*self.left, Added()]) as source:
            self.right = [value for value in source]

class LocallyImportedContext:
    def update(self):
        from contextlib import nullcontext

        self.left = [*self.right]
        with nullcontext([*self.left, Added()]) as source:
            self.right = [value for value in source]

class CapturedDefault:
    def update(self):
        self.left = [*self.right]
        source = [*self.left, Added()]
        self.right = [value for value in (lambda captured=source: captured)()]

class Nested:
    def update(self):
        self.left = [*self.right]
        self.right = [value for group in [[*self.left, Added()]] for value in group]

class Composed:
    def update(self):
        self.left = [*self.right]
        self.right = list(reversed(list(chain(self.left, [Added()]))))[:]

class SelectedTrue:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left, Added()] if True else [*self.left]

class SelectedFalse:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left] if False else [*self.left, Added()]

def accept_original(value: Original) -> None: ...

reveal_type(Conditional().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(Generator().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(Captured().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(CapturedGenerator().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(CapturedUnpacked().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(CapturedLoop().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(CapturedContext().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(LocallyImportedContext().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(CapturedDefault().right)  # revealed: list[Unknown | Added]
reveal_type(Nested().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(Composed().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(SelectedTrue().right)  # revealed: list[Added] | list[Unknown | Added]
reveal_type(SelectedFalse().right)  # revealed: list[Added] | list[Unknown | Added]
accept_original(Conditional().left[0])  # error: [invalid-argument-type]
accept_original(Conditional().right[0])  # error: [invalid-argument-type]
accept_original(Generator().left[0])  # error: [invalid-argument-type]
accept_original(Generator().right[0])  # error: [invalid-argument-type]
accept_original(Captured().left[0])  # error: [invalid-argument-type]
accept_original(Captured().right[0])  # error: [invalid-argument-type]
accept_original(CapturedGenerator().left[0])  # error: [invalid-argument-type]
accept_original(CapturedGenerator().right[0])  # error: [invalid-argument-type]
accept_original(CapturedUnpacked().left[0])  # error: [invalid-argument-type]
accept_original(CapturedUnpacked().right[0])  # error: [invalid-argument-type]
accept_original(CapturedLoop().left[0])  # error: [invalid-argument-type]
accept_original(CapturedLoop().right[0])  # error: [invalid-argument-type]
accept_original(CapturedContext().left[0])  # error: [invalid-argument-type]
accept_original(CapturedContext().right[0])  # error: [invalid-argument-type]
accept_original(LocallyImportedContext().left[0])  # error: [invalid-argument-type]
accept_original(LocallyImportedContext().right[0])  # error: [invalid-argument-type]
accept_original(CapturedDefault().left[0])  # error: [invalid-argument-type]
accept_original(CapturedDefault().right[0])  # error: [invalid-argument-type]
accept_original(Nested().left[0])  # error: [invalid-argument-type]
accept_original(Nested().right[0])  # error: [invalid-argument-type]
accept_original(Composed().left[0])  # error: [invalid-argument-type]
accept_original(Composed().right[0])  # error: [invalid-argument-type]
accept_original(SelectedTrue().left[0])  # error: [invalid-argument-type]
accept_original(SelectedTrue().right[0])  # error: [invalid-argument-type]
accept_original(SelectedFalse().left[0])  # error: [invalid-argument-type]
accept_original(SelectedFalse().right[0])  # error: [invalid-argument-type]
```

## Recursive mappings retain independently introduced entries

Unpacking, identity and transformed comprehensions, captured local aliases, keyword constructors,
generated entries, and independent values paired with recursive keys all preserve their known
mapping components.

```py
class Original: ...
class Added: ...

class Unpacked:
    def update(self):
        additional = {"added": Added()}
        self.left = {**self.right, **{"other": Added()}}
        self.right = {**self.left, **additional}

class Copied:
    def update(self):
        self.left = {**self.right}
        self.right = {key: value for key, value in {**self.left, "added": Added()}.items()}

class Captured:
    def update(self):
        self.left = {**self.right}
        source = {**self.left, "added": Added()}
        self.right = {key: value for key, value in source.items()}

class Transformed:
    def update(self):
        self.left = {**self.right}
        self.right = {key.upper(): value for key, value in {**self.left, "added": Added()}.items()}

class Constructed:
    def update(self):
        self.left = {**self.right}
        self.right = dict(self.left, added=Added())

class IndependentValue:
    def update(self):
        self.left = {**self.right}
        self.right = {key: Added() for key in self.left}

class Generator:
    def update(self):
        self.left = {**self.right}
        self.right = dict((key, value) for key, value in {**self.left, "added": Added()}.items())

def accept_original(value: Original) -> None: ...

reveal_type(Unpacked().right)  # revealed: dict[Unknown | str, Added]
reveal_type(Copied().right)  # revealed: dict[Unknown | str, Added]
reveal_type(Captured().right)  # revealed: dict[Unknown | str, Added]
reveal_type(Transformed().right)  # revealed: dict[Unknown | str, Added]
reveal_type(Constructed().right)  # revealed: dict[str, Added]
reveal_type(IndependentValue().right)  # revealed: dict[Unknown, Added]
reveal_type(Generator().right)  # revealed: dict[Unknown | str, Added]
accept_original(next(iter(Unpacked().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Unpacked().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Copied().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Copied().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Captured().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Captured().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Transformed().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Transformed().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Constructed().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Constructed().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(IndependentValue().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(IndependentValue().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Generator().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Generator().right.values())))  # error: [invalid-argument-type]
```

## Recursive collections preserve independently added operands

Collection concatenation, mapping and set unions, operator helpers, and bound collection methods
retain independently supplied elements.

```py
from operator import add

class Original: ...
class Added: ...

class Lists:
    def update(self):
        self.left = [*self.right]
        self.right = [*self.left] + [Added()]

class Mappings:
    def update(self):
        self.left = {**self.right}
        self.right = self.left | {"added": Added()}

class Sets:
    def update(self):
        self.left = {*self.right}
        self.right = self.left | {Added()}

class Operator:
    def update(self):
        self.left = [*self.right]
        self.right = add([*self.left], [Added()])

class Method:
    def update(self):
        self.left = {*self.right}
        self.right = self.left.union({Added()})

def accept_original(value: Original) -> None: ...

reveal_type(Lists().right)  # revealed: list[Added] | list[Unknown | Added] | list[Unknown]
reveal_type(Mappings().right)  # revealed: dict[str, Added] | dict[Unknown | str, Added]
reveal_type(Sets().right)  # revealed: set[Added] | set[Unknown | Added]
reveal_type(Operator().right)  # revealed: list[Added] | list[Unknown | Added] | list[Unknown]
reveal_type(Method().right)  # revealed: set[Unknown | Added]
accept_original(Lists().left[0])  # error: [invalid-argument-type]
accept_original(Lists().right[0])  # error: [invalid-argument-type]
accept_original(next(iter(Mappings().left.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Mappings().right.values())))  # error: [invalid-argument-type]
accept_original(next(iter(Sets().left)))  # error: [invalid-argument-type]
accept_original(next(iter(Sets().right)))  # error: [invalid-argument-type]
accept_original(Operator().left[0])  # error: [invalid-argument-type]
accept_original(Operator().right[0])  # error: [invalid-argument-type]
accept_original(next(iter(Method().left)))  # error: [invalid-argument-type]
accept_original(next(iter(Method().right)))  # error: [invalid-argument-type]
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

## Guarded implicit attributes when the base is checked first

An attribute initialized behind a negative `hasattr` check cannot establish its own absence while
the initializer is being inferred. This is a regression test for
<https://github.com/astral-sh/ty/issues/4076>.

`base.py`:

```py
class Base:
    existing = 1

    def __init__(self) -> None:
        if not hasattr(self, "value"):
            self.value = self.__str__
        if not hasattr(self, "existing"):
            self.missing
        if not hasattr(self, "other"):
            self.other = self.missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Base

class Child(Base):
    value = Base.__str__

    def other(self): ...
    def missing(self): ...
```

## Guarded implicit attributes when the subclass is checked first

Checking the subclass before its guarded base initializer must not change whether the assignment is
valid.

`child.py`:

```py
from base import Base

class Child(Base):
    value = Base.__str__

    def other(self): ...
    def missing(self): ...
```

`base.py`:

```py
class Base:
    existing = 1

    def __init__(self) -> None:
        if not hasattr(self, "value"):
            self.value = self.__str__
        if not hasattr(self, "existing"):
            self.missing
        if not hasattr(self, "other"):
            self.other = self.missing  # error: [unresolved-attribute]
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
