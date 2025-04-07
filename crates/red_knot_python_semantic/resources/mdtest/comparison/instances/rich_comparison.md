# Comparison: Rich Comparison

Rich comparison operations (`==`, `!=`, `<`, `<=`, `>`, `>=`) in Python are implemented through
double-underscore methods that allow customization of comparison behavior.

For references, see:

- <https://docs.python.org/3/reference/datamodel.html#object.__lt__>
- <https://snarky.ca/unravelling-rich-comparison-operators/>

## Rich Comparison Dunder Implementations For Same Class

Classes can support rich comparison by implementing dunder methods like `__eq__`, `__ne__`, etc. The
most common case involves implementing these methods for the same type:

```py
from __future__ import annotations

class EqReturnType: ...
class NeReturnType: ...
class LtReturnType: ...
class LeReturnType: ...
class GtReturnType: ...
class GeReturnType: ...

class A:
    def __eq__(self, other: A) -> EqReturnType:
        return EqReturnType()

    def __ne__(self, other: A) -> NeReturnType:
        return NeReturnType()

    def __lt__(self, other: A) -> LtReturnType:
        return LtReturnType()

    def __le__(self, other: A) -> LeReturnType:
        return LeReturnType()

    def __gt__(self, other: A) -> GtReturnType:
        return GtReturnType()

    def __ge__(self, other: A) -> GeReturnType:
        return GeReturnType()

reveal_type(A() == A())  # revealed: EqReturnType
reveal_type(A() != A())  # revealed: NeReturnType
reveal_type(A() < A())  # revealed: LtReturnType
reveal_type(A() <= A())  # revealed: LeReturnType
reveal_type(A() > A())  # revealed: GtReturnType
reveal_type(A() >= A())  # revealed: GeReturnType
```

## Rich Comparison Dunder Implementations for Other Class

In some cases, classes may implement rich comparison dunder methods for comparisons with a different
type:

```py
from __future__ import annotations

class EqReturnType: ...
class NeReturnType: ...
class LtReturnType: ...
class LeReturnType: ...
class GtReturnType: ...
class GeReturnType: ...

class A:
    def __eq__(self, other: B) -> EqReturnType:
        return EqReturnType()

    def __ne__(self, other: B) -> NeReturnType:
        return NeReturnType()

    def __lt__(self, other: B) -> LtReturnType:
        return LtReturnType()

    def __le__(self, other: B) -> LeReturnType:
        return LeReturnType()

    def __gt__(self, other: B) -> GtReturnType:
        return GtReturnType()

    def __ge__(self, other: B) -> GeReturnType:
        return GeReturnType()

class B: ...

reveal_type(A() == B())  # revealed: EqReturnType
reveal_type(A() != B())  # revealed: NeReturnType
reveal_type(A() < B())  # revealed: LtReturnType
reveal_type(A() <= B())  # revealed: LeReturnType
reveal_type(A() > B())  # revealed: GtReturnType
reveal_type(A() >= B())  # revealed: GeReturnType
```

## Reflected Comparisons

Fallback to the right-hand side’s comparison methods occurs when the left-hand side does not define
them. Note: class `B` has its own `__eq__` and `__ne__` methods to override those of `object`, but
these methods will be ignored here because they require a mismatched operand type.

```py
from __future__ import annotations

class EqReturnType: ...
class NeReturnType: ...
class LtReturnType: ...
class LeReturnType: ...
class GtReturnType: ...
class GeReturnType: ...

class A:
    def __eq__(self, other: B) -> EqReturnType:
        return EqReturnType()

    def __ne__(self, other: B) -> NeReturnType:
        return NeReturnType()

    def __lt__(self, other: B) -> LtReturnType:
        return LtReturnType()

    def __le__(self, other: B) -> LeReturnType:
        return LeReturnType()

    def __gt__(self, other: B) -> GtReturnType:
        return GtReturnType()

    def __ge__(self, other: B) -> GeReturnType:
        return GeReturnType()

class Unrelated: ...

class B:
    # To override builtins.object.__eq__ and builtins.object.__ne__
    # TODO these should emit an invalid override diagnostic
    def __eq__(self, other: Unrelated) -> B:
        return B()

    def __ne__(self, other: Unrelated) -> B:
        return B()

# Because `object.__eq__` and `object.__ne__` accept `object` in typeshed,
# this can only happen with an invalid override of these methods,
# but we still support it.
reveal_type(B() == A())  # revealed: EqReturnType
reveal_type(B() != A())  # revealed: NeReturnType

reveal_type(B() < A())  # revealed: GtReturnType
reveal_type(B() <= A())  # revealed: GeReturnType

reveal_type(B() > A())  # revealed: LtReturnType
reveal_type(B() >= A())  # revealed: LeReturnType

class C:
    def __gt__(self, other: C) -> EqReturnType:
        return EqReturnType()

    def __ge__(self, other: C) -> NeReturnType:
        return NeReturnType()

reveal_type(C() < C())  # revealed: EqReturnType
reveal_type(C() <= C())  # revealed: NeReturnType
```

## Reflected Comparisons with Subclasses

When subclasses override comparison methods, these overridden methods take precedence over those in
the parent class. Class `B` inherits from `A` and redefines comparison methods to return types other
than `A`.

```py
from __future__ import annotations

class EqReturnType: ...
class NeReturnType: ...
class LtReturnType: ...
class LeReturnType: ...
class GtReturnType: ...
class GeReturnType: ...

class A:
    def __eq__(self, other: A) -> A:
        return A()

    def __ne__(self, other: A) -> A:
        return A()

    def __lt__(self, other: A) -> A:
        return A()

    def __le__(self, other: A) -> A:
        return A()

    def __gt__(self, other: A) -> A:
        return A()

    def __ge__(self, other: A) -> A:
        return A()

class B(A):
    def __eq__(self, other: A) -> EqReturnType:
        return EqReturnType()

    def __ne__(self, other: A) -> NeReturnType:
        return NeReturnType()

    def __lt__(self, other: A) -> LtReturnType:
        return LtReturnType()

    def __le__(self, other: A) -> LeReturnType:
        return LeReturnType()

    def __gt__(self, other: A) -> GtReturnType:
        return GtReturnType()

    def __ge__(self, other: A) -> GeReturnType:
        return GeReturnType()

reveal_type(A() == B())  # revealed: EqReturnType
reveal_type(A() != B())  # revealed: NeReturnType

reveal_type(A() < B())  # revealed: GtReturnType
reveal_type(A() <= B())  # revealed: GeReturnType

reveal_type(A() > B())  # revealed: LtReturnType
reveal_type(A() >= B())  # revealed: LeReturnType
```

## Reflected Comparisons with Subclass But Falls Back to LHS

In the case of a subclass, the right-hand side has priority. However, if the overridden dunder
method has an mismatched type to operand, the comparison will fall back to the left-hand side.

```py
from __future__ import annotations

class A:
    def __lt__(self, other: A) -> A:
        return A()

    def __gt__(self, other: A) -> A:
        return A()

class B(A):
    def __lt__(self, other: int) -> B:
        return B()

    def __gt__(self, other: int) -> B:
        return B()

reveal_type(A() < B())  # revealed: A
reveal_type(A() > B())  # revealed: A
```

## Operations involving instances of classes inheriting from `Any`

`Any` and `Unknown` represent a set of possible runtime objects, wherein the bounds of the set are
unknown. Whether the left-hand operand's dunder or the right-hand operand's reflected dunder depends
on whether the right-hand operand is an instance of a class that is a subclass of the left-hand
operand's class and overrides the reflected dunder. In the following example, because of the
unknowable nature of `Any`/`Unknown`, we must consider both possibilities: `Any`/`Unknown` might
resolve to an unknown third class that inherits from `X` and overrides `__gt__`; but it also might
not. Thus, the correct answer here for the `reveal_type` is `int | Unknown`.

(This test is referenced from `mdtest/binary/instances.md`)

```py
from does_not_exist import Foo  # error: [unresolved-import]

reveal_type(Foo)  # revealed: Unknown

class X:
    def __lt__(self, other: object) -> int:
        return 42

class Y(Foo): ...

# TODO: Should be `int | Unknown`; see above discussion.
reveal_type(X() < Y())  # revealed: int
```

## Equality and Inequality Fallback

This test confirms that `==` and `!=` comparisons default to identity comparisons (`is`, `is not`)
when argument types do not match the method signature.

Please refer to the [docs](https://docs.python.org/3/reference/datamodel.html#object.__eq__)

```py
from __future__ import annotations

class A:
    # TODO both these overrides should emit invalid-override diagnostic
    def __eq__(self, other: int) -> A:
        return A()

    def __ne__(self, other: int) -> A:
        return A()

reveal_type(A() == A())  # revealed: bool
reveal_type(A() != A())  # revealed: bool
```

## Object Comparisons with Typeshed

```py
class A: ...

reveal_type(A() == object())  # revealed: bool
reveal_type(A() != object())  # revealed: bool
reveal_type(object() == A())  # revealed: bool
reveal_type(object() != A())  # revealed: bool

# error: [unsupported-operator] "Operator `<` is not supported for types `A` and `object`"
# revealed: Unknown
reveal_type(A() < object())
```

## Numbers Comparison with typeshed

```py
reveal_type(1 == 1.0)  # revealed: bool
reveal_type(1 != 1.0)  # revealed: bool
reveal_type(1 < 1.0)  # revealed: bool
reveal_type(1 <= 1.0)  # revealed: bool
reveal_type(1 > 1.0)  # revealed: bool
reveal_type(1 >= 1.0)  # revealed: bool

reveal_type(1 == 2j)  # revealed: bool
reveal_type(1 != 2j)  # revealed: bool

# error: [unsupported-operator] "Operator `<` is not supported for types `int` and `complex`, in comparing `Literal[1]` with `complex`"
reveal_type(1 < 2j)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<=` is not supported for types `int` and `complex`, in comparing `Literal[1]` with `complex`"
reveal_type(1 <= 2j)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>` is not supported for types `int` and `complex`, in comparing `Literal[1]` with `complex`"
reveal_type(1 > 2j)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>=` is not supported for types `int` and `complex`, in comparing `Literal[1]` with `complex`"
reveal_type(1 >= 2j)  # revealed: Unknown

def f(x: bool, y: int):
    reveal_type(x < y)  # revealed: bool
    reveal_type(y < x)  # revealed: bool
    reveal_type(4.2 < x)  # revealed: bool
    reveal_type(x < 4.2)  # revealed: bool
```

## Chained comparisons with objects that don't implement `__bool__` correctly

<!-- snapshot-diagnostics -->

Python implicitly calls `bool` on the comparison result of preceding elements (but not for the last
element) of a chained comparison.

```py
class NotBoolable:
    __bool__: int = 3

class Comparable:
    def __lt__(self, item) -> NotBoolable:
        return NotBoolable()

    def __gt__(self, item) -> NotBoolable:
        return NotBoolable()

# error: [unsupported-bool-conversion]
10 < Comparable() < 20
# error: [unsupported-bool-conversion]
10 < Comparable() < Comparable()

Comparable() < Comparable()  # fine
```

## Callables as comparison dunders

```py
from typing import Literal

class AlwaysTrue:
    def __call__(self, other: object) -> Literal[True]:
        return True

class A:
    __eq__: AlwaysTrue = AlwaysTrue()
    __lt__: AlwaysTrue = AlwaysTrue()

reveal_type(A() == A())  # revealed: Literal[True]
reveal_type(A() < A())  # revealed: Literal[True]
reveal_type(A() > A())  # revealed: Literal[True]
```
