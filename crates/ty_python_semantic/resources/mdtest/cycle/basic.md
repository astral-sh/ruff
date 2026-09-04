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

## String accumulation in nested loops

Repeated string updates in nested loops converge to `str`, without retaining recursion placeholders.
This is a reduced regression test from Pylint's similarity report.

```py
def concatenate(lines: list[str]) -> str:
    result = ""
    for _ in lines:
        result += ""
        for _ in lines:
            result += ""
        for _ in lines:
            result += "\n"
    reveal_type(result)  # revealed: str
    return result
```

## Tuple nesting in a loop

Repeatedly wrapping a type in `tuple[...]` can grow the tuple nesting without a fixed bound.
Inference collapses that nesting to a recursion placeholder and still includes the initial `int`
type.

```py
def nest_tuples(iterations: list[int]):
    result = int
    for _ in iterations:
        result = tuple[result]
    reveal_type(result)  # revealed: <class 'int'> | <class 'tuple[Divergent]'>
```

## Tuple nesting in nested loops

Nested loops can each add tuple layers. Their recursion placeholders also converge to a single
recursive tuple alongside the initial `int` type.

```py
def nest_tuples(iterations: list[int]):
    result = int
    for _ in iterations:
        result = tuple[result]
        for _ in iterations:
            result = tuple[result]
        for _ in iterations:
            result = tuple[result]
    reveal_type(result)  # revealed: <class 'int'> | <class 'tuple[Divergent]'>
```

## Runtime union accumulation in nested loops

Runtime union values can also be accumulated in loops. Their presence does not make the result a
recursive alias: each value returned here is a `UnionType`, with no unresolved recursion
placeholders.

```toml
[rules]
unsound-return-statement = "error"
```

```py
from types import UnionType

def accumulate_unions(iterations: list[int]) -> UnionType:
    result = int | str
    for _ in iterations:
        result |= bytes
        for _ in iterations:
            result |= bytes
        for _ in iterations:
            result |= bytes
    return result
```

## Legacy union accumulation in nested loops

Repeatedly combining `int` with itself through `typing.Union` still produces `int`. Using the result
as an annotation does not admit unrelated types, even after updates in nested loops.

```py
from typing import Union

def accumulate_unions(iterations: list[int]):
    result = int
    for _ in iterations:
        result = Union[result, int]
        for _ in iterations:
            result = Union[result, int]
        for _ in iterations:
            result = Union[result, int]

    def consume(value: result):
        reveal_type(value)  # revealed: int

    consume(1)
    consume("")  # error: [invalid-argument-type]
```

## Conditional legacy union accumulation in nested loops

A conditional update in a nested loop does not change the meaning of `Union[int, int]`. Both paths
produce an annotation that accepts integers and rejects strings.

```py
from typing import Union

def accumulate_unions(iterations: list[int], flag: bool):
    result = int
    for _ in iterations:
        result = Union[result, int]
        for _ in iterations:
            result = Union[result, int]
        for _ in iterations:
            if flag:
                result = Union[result, int]

    def consume(value: result):
        reveal_type(value)  # revealed: int

    consume(1)
    consume("")  # error: [invalid-argument-type]
```

## Simultaneous legacy union updates in nested loops

Updating two union values in one assignment preserves their concrete members without introducing
recursion placeholders. The accumulated union accepts integers and strings, but rejects bytes.

```py
from typing import Union

def accumulate_unions(iterations: list[int]):
    a = int
    b = str
    for _ in iterations:
        a, b = Union[a, b], Union[b, a]
        for _ in iterations:
            a, b = Union[a, b], Union[b, a]
        for _ in iterations:
            a, b = Union[a, b], Union[b, a]

    def consume(value: a):
        reveal_type(value)  # revealed: int | str

    consume(1)
    consume("")
    consume(b"")  # error: [invalid-argument-type]
```

## Optional accumulation in nested loops

Repeatedly applying `Optional` to `int` produces `int | None`. The resulting annotation accepts
integers and `None`, but rejects strings.

```py
from typing import Optional

def accumulate_optional(iterations: list[int]):
    result = int
    for _ in iterations:
        result = Optional[result]
        for _ in iterations:
            result = Optional[result]
        for _ in iterations:
            result = Optional[result]

    def consume(value: result):
        reveal_type(value)  # revealed: int | None

    consume(1)
    consume(None)
    consume("")  # error: [invalid-argument-type]
```

## Runtime unions containing unrelated recursion markers

A runtime union can contain a recursion marker from an already inferred alias. That marker belongs
to the alias, not to the loop, and does not prevent the loop's own placeholders from being removed.

```toml
[rules]
unsound-return-statement = "error"
```

```py
from typing import Union
from types import UnionType

D = Union["D", "D"]
R = Union[D, int]
reveal_type(R)  # revealed: <types.UnionType special-form>

def accumulate_unions(iterations: list[int]) -> UnionType:
    result = R
    for _ in iterations:
        result |= bytes
        for _ in iterations:
            result |= bytes
        for _ in iterations:
            result |= bytes
    return result
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

## Mutually recursive tuple and union aliases

Mutually recursive tuple and union aliases converge even when the union also references itself. This
is a regression test for <https://github.com/astral-sh/ty/issues/4443>.

```py
from typing import Union

a = tuple["b"]
b = Union["a", "b", int]
```

## Mutually recursive tuple and union aliases with multiple concrete members

Adding another concrete member to the recursive union also converges. The union keeps both concrete
members when its recursive part is normalized.

TODO: Eliminate redundant tuple members retained from intermediate recursive expansions.

```py
from typing import Union

a = tuple["b"]
b = Union["a", "b", int, str]

def f(x: b):
    reveal_type(x)  # revealed: tuple[Divergent] | tuple[int | str] | int | str
```

## Chained assignments of recursive union aliases

A chained assignment gives both names the same recursive union. Each alias accepts tuples and
integers, but rejects unrelated types.

```py
from typing import Union

A = tuple["B"]
B = C = Union["A", "B", int]

def consume(first: B, second: C):
    # TODO: eliminate the redundant `tuple[int]` member.
    reveal_type(first)  # revealed: tuple[Divergent] | tuple[int] | int
    reveal_type(second)  # revealed: tuple[Divergent] | tuple[int] | int

consume(1, (1,))
consume(1.5, 1)  # error: [invalid-argument-type]
consume(1, 1.5)  # error: [invalid-argument-type]
```

## Unpacking assignments of recursive union aliases

A recursive union can be unpacked from a tuple without changing its members or the other unpacked
value's type.

```py
from typing import Union

A = tuple["B"]
B, C = (Union["A", "B", int], str)

reveal_type(C)  # revealed: <class 'str'>

def consume(value: B):
    # TODO: eliminate the redundant `tuple[int]` member.
    reveal_type(value)  # revealed: tuple[Divergent] | tuple[int] | int

consume(1)
consume((1,))
consume(1.5)  # error: [invalid-argument-type]
```

## Mutually recursive tuple and optional aliases

A recursive optional alias includes `None` alongside its tuple member. The self-reference does not
admit unrelated types.

```py
from typing import Optional

A = tuple["B"]
B = Optional["A | B"]

def consume(value: B):
    # TODO: coalesce the repeated tuple alternatives.
    reveal_type(value)  # revealed: tuple[Divergent] | tuple[Divergent] | None

consume(None)
consume((None,))
consume(1.5)  # error: [invalid-argument-type]
```

## Mutually recursive union and tuple aliases in reverse order

The recursive union also converges when it is defined before the tuple alias.

```py
from typing import Union

b = Union["a", "b", int, str]
a = tuple["b"]

def f(x: b):
    reveal_type(x)  # revealed: tuple[Divergent] | tuple[int | str] | int | str
```

## Conditional recursive union values

Mutually recursive aliases also converge when a conditional expression produces a union value on one
branch.

```py
from typing import Union

a = tuple["b"]
b = Union["a", "b", int] if a else str
```

## Mutually recursive aliases through two unions

Aliases can form a chain through multiple unions. Inference converges without expanding the tuple on
every pass through the chain.

```py
from typing import Union

a = tuple["b"]
b = Union["c", "b", int]
c = Union["a", "c", str]
```

## Recursive tuple aliases with concrete elements

Normalizing the recursive tuple element preserves the other element's type and the tuple's length.

```py
from typing import Union

a = tuple[str, "b"]
b = Union["a", "b", int]

def f(x: b):
    reveal_type(x)  # revealed: tuple[str, Divergent] | tuple[str, int] | int
```

## Recursive tuple aliases containing another recursive alias

A tuple element can refer to an already inferred recursive alias. Normalizing a different recursive
element preserves that alias's tuple structure, so an integer is still invalid in its place.

```py
from typing import Union

Other = tuple["Other"]
a = tuple[Other, "b"]
b = Union["a", "b", int]

def consume(x: b):
    reveal_type(x)  # revealed: tuple[tuple[Divergent], Divergent] | tuple[tuple[Divergent], int] | int

consume((1, 1))  # error: [invalid-argument-type]
```

## Recursive aliases using legacy tuples

`typing.Tuple` supports the same recursive aliases as `tuple`, including homogeneous tuples.

```py
from typing import Tuple, Union

a = Tuple["b", ...]
b = Union["a", "b", int]

def f(x: b):
    reveal_type(x)  # revealed: tuple[Divergent, ...] | tuple[int, ...] | int
```

## Recursive aliases using imported tuple constructors

An imported alias for the built-in tuple constructor has the same behavior as `tuple`.

```py
from builtins import tuple as TupleConstructor
from typing import Union

a = TupleConstructor["b"]
b = Union["a", "b", int]

def f(x: b):
    reveal_type(x)  # revealed: tuple[Divergent] | tuple[int] | int
```

## Recursive generic tuple aliases

A recursive generic tuple alias can be specialized without rejecting the resulting alias as an
invalid type form. Specialized recursive generic aliases are not yet fully supported, so the type
parameter currently falls back to a `@Todo` type.

```py
from typing import TypeVar

T = TypeVar("T")
Callee = tuple[T, "A"]
A = Callee[int]

def f(x: A):
    # revealed: tuple[@Todo(specialized recursive generic type alias), Divergent]
    reveal_type(x)
```

## Self-referential union aliases

A direct self-reference contributes no additional members to a union. Removing it keeps both
single-member and multi-member unions precise.

```py
from typing import Union

One = Union["One", int]
Many = Union["Many", int, str]

def f(x: One, y: Many):
    reveal_type(x)  # revealed: int
    reveal_type(y)  # revealed: int | str

f("", 0)  # error: [invalid-argument-type]
f(0, b"")  # error: [invalid-argument-type]
```

## Self-referential optional aliases

Without a concrete member, a self-referential optional alias reduces to `None`:

```py
from typing import Optional

Alias = Optional["Alias"]
reveal_type(Alias)  # revealed: None

def consume(value: Alias):
    reveal_type(value)  # revealed: None

consume(None)
consume(1)  # error: [invalid-argument-type]
```

## Mutually recursive optional aliases

Mutual references between optional aliases also reduce to `None` when neither adds a concrete
member:

```py
from typing import Optional

A = Optional["B"]
B = Optional["A"]
reveal_type(A)  # revealed: None
reveal_type(B)  # revealed: None

def consume(first: A, second: B):
    reveal_type(first)  # revealed: None
    reveal_type(second)  # revealed: None

consume(None, None)
consume(1, None)  # error: [invalid-argument-type]
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
