# Comparison: Intersections

## Positive contributions

If we have an intersection type `A & B` and we get a definitive true/false answer for one of the
types, we can infer that the result for the intersection type is also true/false:

```py
from typing import Literal

class Base:
    def __gt__(self, other) -> bool:
        return False

class Child1(Base):
    def __eq__(self, other) -> Literal[True]:
        return True

class Child2(Base): ...

def _(x: Base):
    c1 = Child1()

    # Create an intersection type through narrowing:
    if isinstance(x, Child1):
        if isinstance(x, Child2):
            reveal_type(x)  # revealed: Child1 & Child2

            reveal_type(x == 1)  # revealed: Literal[True]

            # Other comparison operators fall back to the base type:
            reveal_type(x > 1)  # revealed: bool
            reveal_type(x is c1)  # revealed: bool
```

## Negative contributions

Negative contributions to the intersection type only allow simplifications in a few special cases
(equality and identity comparisons).

### Equality comparisons

#### Literal strings

```py
x = "x" * 1_000_000_000
y = "y" * 1_000_000_000
reveal_type(x)  # revealed: LiteralString

if x != "abc":
    reveal_type(x)  # revealed: LiteralString & ~Literal["abc"]

    reveal_type(x == "abc")  # revealed: Literal[False]
    reveal_type("abc" == x)  # revealed: Literal[False]
    reveal_type(x == "something else")  # revealed: bool
    reveal_type("something else" == x)  # revealed: bool

    reveal_type(x != "abc")  # revealed: Literal[True]
    reveal_type("abc" != x)  # revealed: Literal[True]
    reveal_type(x != "something else")  # revealed: bool
    reveal_type("something else" != x)  # revealed: bool

    reveal_type(x == y)  # revealed: bool
    reveal_type(y == x)  # revealed: bool
    reveal_type(x != y)  # revealed: bool
    reveal_type(y != x)  # revealed: bool

    reveal_type(x >= "abc")  # revealed: bool
    reveal_type("abc" >= x)  # revealed: bool

    reveal_type(x in "abc")  # revealed: bool
    reveal_type("abc" in x)  # revealed: bool
```

A negative literal-string constraint does not exclude a runtime string with that value unless the
candidate already has known literal origin.

```py
from typing import Literal
from typing_extensions import LiteralString
from ty_extensions import Intersection, Not

def without_literal_origin(value: Intersection[str, Not[LiteralString]]) -> None:
    reveal_type(value == "hello")  # revealed: bool
    reveal_type("hello" == value)  # revealed: bool
```

A negative string-literal constraint likewise leaves the same runtime value possible, with or
without an explicit `str` constraint.

```py
def excluded_string_literal(value: Intersection[str, Not[Literal["hello"]]]) -> None:
    reveal_type(value == "hello")  # revealed: bool
    reveal_type("hello" == value)  # revealed: bool
    reveal_type(value != "hello")  # revealed: bool

def excluded_literal(value: Not[Literal["hello"]]) -> None:
    reveal_type(value == "hello")  # revealed: bool
    reveal_type("hello" == value)  # revealed: bool
    reveal_type(value != "hello")  # revealed: bool
```

#### Integers

```py
def _(x: int):
    if x != 1:
        reveal_type(x)  # revealed: int & ~Literal[1] & ~Literal[True]

        reveal_type(x != 1)  # revealed: bool
        reveal_type(x != 2)  # revealed: bool

        reveal_type(x == 1)  # revealed: bool
        reveal_type(x == 2)  # revealed: bool
```

### Identity comparisons

The type `~None` excludes the `None` object, so its identity comparisons with `None` have definite
results.

```py
def _(o: object):
    n = None

    if o is not None:
        reveal_type(o)  # revealed: ~None
        reveal_type(o is n)  # revealed: Literal[False]
        reveal_type(o is not n)  # revealed: Literal[True]
```

A single-member enum contains only one object. A value excluded from `E` cannot be `E.ONLY`, so the
branch below is unreachable and must not emit an attribute error.

```py
from enum import Enum
from ty_extensions import Not

class E(Enum):
    ONLY = 1

def f(value: Not[E]) -> None:
    if value is E.ONLY:
        reveal_type(value)  # revealed: Never
        value.does_not_exist  # no error (unreachable branch)
```

A `NewType` negation removes its static tag, not the runtime objects of its base: an integer without
that tag can still be identical to the integer passed into the `NewType` constructor.

```py
from typing import NewType

UserId = NewType("UserId", int)

def f(value: Not[UserId]) -> None:
    reveal_type(value is 1)  # revealed: bool
```

After `not isinstance(value, B)`, `value` cannot be identical to a `B` instance. This remains true
when `value` has also been narrowed to `A`, so the inner branch is unreachable.

```py
class A: ...
class B: ...

def f(value: object, other_b: B) -> None:
    if isinstance(value, A) and not isinstance(value, B):
        if value is other_b:
            reveal_type(value)  # revealed: Never
            value.does_not_exist  # no error (unreachable branch)
```

## Non-boolean comparison results

Rich comparisons preserve their declared return types after narrowing an operand to an intersection.
A return type that is disjoint from `bool` does not make the comparison unreachable, and an `int`
return type is not narrowed to its `bool` subtype.

```py
class Comparison:
    def __eq__(self, other: object) -> str:  # error: [invalid-method-override]
        return "equal"

    def __ne__(self, other: object) -> bytes:  # error: [invalid-method-override]
        return b"different"

    def __lt__(self, other: object) -> int:
        return 42

    def __contains__(self, other: object) -> str:
        return "contained"

class Excluded: ...

def compare(value: Comparison):
    if not isinstance(value, Excluded):
        reveal_type(value == 0)  # revealed: str
        reveal_type(value != 0)  # revealed: bytes
        reveal_type(value < 0)  # revealed: int
```

Membership tests still convert their result to `bool`, even when `__contains__` returns another
type.

```py
def membership(value: Comparison):
    if not isinstance(value, Excluded):
        reveal_type(0 in value)  # revealed: bool
        reveal_type(0 not in value)  # revealed: bool
```

A comparison that always raises still has type `Never` when another component inherits
`object.__eq__`.

```py
from typing_extensions import Never

class NonReturning:
    def __eq__(self, other: object) -> Never:
        raise RuntimeError

def never_returns(value: NonReturning):
    if isinstance(value, Excluded):
        reveal_type(value == 0)  # revealed: Never
```

## Conditionally defined comparison methods

A conditional comparison method can fall back to the inherited `object` method. Narrowing its
receiver preserves both the custom result and the boolean fallback.

```py
def enabled() -> bool:
    return True

class Conditional:
    if enabled():
        def __eq__(self, other: object) -> str:  # error: [invalid-method-override]
            return "equal"

class Excluded: ...

def compare(value: Conditional):
    if not isinstance(value, Excluded):
        reveal_type(value == 0)  # revealed: str | bool
```

The conditional method must not be discarded in favor of a reflected method that returns a different
boolean literal. The left operand can return `True` without calling the right operand's method.

```py
from typing_extensions import Literal

class ConditionalTrue:
    if enabled():
        def __eq__(self, other: object) -> Literal[True]:
            return True

class ReflectedFalse:
    def __eq__(self, other: object) -> Literal[False]:
        return False

def reflected(left: ConditionalTrue, right: ReflectedFalse):
    if not isinstance(left, Excluded):
        reveal_type(left == right)  # revealed: bool
```

An intersection with another class that defines a boolean comparison still permits either result of
the conditional method.

```py
class BooleanComparison:
    def __eq__(self, other: object) -> bool:
        return False

def positive(left: ConditionalTrue):
    if isinstance(left, BooleanComparison):
        reveal_type(left == 0)  # revealed: bool
```

The same applies when both classes define their comparison methods conditionally.

```py
class ConditionalBoolean:
    if enabled():
        def __eq__(self, other: object) -> bool:
            return False

def both_conditional(left: ConditionalTrue):
    if isinstance(left, ConditionalBoolean):
        reveal_type(left == 0)  # revealed: bool
```

## Comparison methods returning `Self`

A comparison method annotated with `Self` returns the full intersection receiver, just like an
explicit method call. Excluding a class from the receiver also excludes it from the comparison
result.

```py
from __future__ import annotations
from typing_extensions import Self

class Index:
    def __eq__(self, other: object) -> Self:  # error: [invalid-method-override]
        return self

    def __lt__(self, other: object) -> Self:
        return self

    def __gt__(self, other: object) -> Index:
        return Index()

class MultiIndex: ...

def equality(index: Index):
    if not isinstance(index, MultiIndex):
        reveal_type(index.__eq__(""))  # revealed: Index & ~MultiIndex
        reveal_type(index == "")  # revealed: Index & ~MultiIndex
```

An `and` expression can return either `False` or the comparison result; it does not convert the
comparison result to a boolean.

```py
def conjunction(index: Index):
    reveal_type(not isinstance(index, MultiIndex) and index == "")  # revealed: Literal[False] | (Index & ~MultiIndex)
```

Positive intersection components are preserved as well. An inherited `object.__eq__` on another
component does not restrict a custom comparison's result to `bool`.

```py
def positive(index: Index):
    if isinstance(index, MultiIndex):
        reveal_type(index == "")  # revealed: Index & MultiIndex
```

Reflected comparisons bind `Self` to the right-hand receiver. In contrast, a concrete return
annotation does not inherit constraints from the receiver: `__gt__` can return a different `Index`.

```py
def reflected(index: Index):
    if not isinstance(index, MultiIndex):
        reveal_type(0 > index)  # revealed: Index & ~MultiIndex
        reveal_type(index > 0)  # revealed: Index
```

## Comparison results containing `Self`

Receiver binding also applies when `Self` occurs inside the comparison's return type, rather than
being the entire return type.

```py
from typing_extensions import Self

class Comparison:
    def __eq__(self, other: object) -> tuple[Self]:  # error: [invalid-method-override]
        return (self,)

class Excluded: ...

def equality(value: Comparison):
    if not isinstance(value, Excluded):
        reveal_type(value.__eq__(0))  # revealed: tuple[Comparison & ~Excluded]
        reveal_type(value == 0)  # revealed: tuple[Comparison & ~Excluded]
```

## Reflected comparisons with narrowed receivers

Excluding an unrelated class from the left operand does not prevent the right operand from being a
subclass with a reflected comparison method. Since the runtime classes of these operands are not
known exactly, either method can supply the result.

```py
class Base:
    def __lt__(self, other: object) -> int:
        return 42

class Child(Base):
    def __gt__(self, other: object) -> str:
        return "reflected"

class Excluded: ...

def compare(left: Base, right: Child):
    if not isinstance(left, Excluded):
        reveal_type(left < right)  # revealed: int | str
```

## NewTypes in intersection comparisons

Narrowing a `NewType` of `float` preserves the comparison operations supported by its base type,
including comparisons where both operands are intersections.

```py
from typing import NewType

Float = NewType("Float", float)

class Excluded: ...

def compare(left: Float, right: Float):
    if not isinstance(left, Excluded) and not isinstance(right, Excluded):
        reveal_type(left < right)  # revealed: bool
```

## Comparisons with multiple union return types

A comparison can have different union return types on its positive intersection components. When
distributing those unions exceeds the complexity limit, we keep a wider union of their return types
instead of losing their non-boolean results.

```py
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...

class First:
    def __lt__(self, other: object) -> A | B | C:
        return A()

class Second:
    def __lt__(self, other: object) -> D | E:
        return D()

def compare(value: First):
    if isinstance(value, Second):
        reveal_type(value < 0)  # revealed: A | B | C | D | E
```

## Diagnostics

### Unsupported operators for positive contributions

Raise an error if the given operator is unsupported for all positive contributions to the
intersection type:

```py
class NonContainer1: ...
class NonContainer2: ...

def _(x: object):
    if isinstance(x, NonContainer1):
        if isinstance(x, NonContainer2):
            reveal_type(x)  # revealed: NonContainer1 & NonContainer2

            # snapshot: unsupported-operator
            reveal_type(2 in x)  # revealed: bool
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:10:25
   |
10 |             reveal_type(2 in x)  # revealed: bool
   |                         -^^^^-
   |                         |    |
   |                         |    Has type `NonContainer1 & NonContainer2`
   |                         Has type `Literal[2]`
```

Do not raise an error if at least one of the positive contributions to the intersection type support
the operator:

```py
class Container:
    def __contains__(self, x) -> bool:
        return False

def _(x: object):
    if isinstance(x, NonContainer1):
        if isinstance(x, Container):
            if isinstance(x, NonContainer2):
                reveal_type(x)  # revealed: NonContainer1 & Container & NonContainer2
                reveal_type(2 in x)  # revealed: bool
```

Do also raise an error if the intersection has no positive contributions at all, unless the operator
is supported on `object`:

```py
def _(x: object):
    if not isinstance(x, NonContainer1):
        reveal_type(x)  # revealed: ~NonContainer1

        # snapshot: unsupported-operator
        reveal_type(2 in x)  # revealed: bool

        reveal_type(2 is x)  # revealed: bool
        reveal_type(x == 0)  # revealed: bool
        x < 0  # error: [unsupported-operator]
```

```snapshot
error[unsupported-operator]: Unsupported `in` operation
  --> src/mdtest_snippet.py:26:21
   |
26 |         reveal_type(2 in x)  # revealed: bool
   |                     -^^^^-
   |                     |    |
   |                     |    Has type `~NonContainer1`
   |                     Has type `Literal[2]`
```

### Unsupported operators for negative contributions

Do *not* raise an error if any of the negative contributions to the intersection type are
unsupported for the given operator:

```py
class Container:
    def __contains__(self, x) -> bool:
        return False

class NonContainer: ...

def _(x: object):
    if isinstance(x, Container):
        if not isinstance(x, NonContainer):
            reveal_type(x)  # revealed: Container & ~NonContainer

            # No error here!
            reveal_type(2 in x)  # revealed: bool
```
