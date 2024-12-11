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

class A:
    def __eq__(self, other: A) -> int:
        return 42

    def __ne__(self, other: A) -> float:
        return 42.0

    def __lt__(self, other: A) -> str:
        return "42"

    def __le__(self, other: A) -> bytes:
        return b"42"

    def __gt__(self, other: A) -> list:
        return [42]

    def __ge__(self, other: A) -> set:
        return {42}

reveal_type(A() == A())  # revealed: int
reveal_type(A() != A())  # revealed: float
reveal_type(A() < A())  # revealed: str
reveal_type(A() <= A())  # revealed: bytes
reveal_type(A() > A())  # revealed: list
reveal_type(A() >= A())  # revealed: set
```

## Rich Comparison Dunder Implementations for Other Class

In some cases, classes may implement rich comparison dunder methods for comparisons with a different
type:

```py
from __future__ import annotations

class A:
    def __eq__(self, other: B) -> int:
        return 42

    def __ne__(self, other: B) -> float:
        return 42.0

    def __lt__(self, other: B) -> str:
        return "42"

    def __le__(self, other: B) -> bytes:
        return b"42"

    def __gt__(self, other: B) -> list:
        return [42]

    def __ge__(self, other: B) -> set:
        return {42}

class B: ...

reveal_type(A() == B())  # revealed: int
reveal_type(A() != B())  # revealed: float
reveal_type(A() < B())  # revealed: str
reveal_type(A() <= B())  # revealed: bytes
reveal_type(A() > B())  # revealed: list
reveal_type(A() >= B())  # revealed: set
```

## Reflected Comparisons

Fallback to the right-hand side’s comparison methods occurs when the left-hand side does not define
them. Note: class `B` has its own `__eq__` and `__ne__` methods to override those of `object`, but
these methods will be ignored here because they require a mismatched operand type.

```py
from __future__ import annotations

class A:
    def __eq__(self, other: B) -> int:
        return 42

    def __ne__(self, other: B) -> float:
        return 42.0

    def __lt__(self, other: B) -> str:
        return "42"

    def __le__(self, other: B) -> bytes:
        return b"42"

    def __gt__(self, other: B) -> list:
        return [42]

    def __ge__(self, other: B) -> set:
        return {42}

class B:
    # To override builtins.object.__eq__ and builtins.object.__ne__
    # TODO these should emit an invalid override diagnostic
    def __eq__(self, other: str) -> B:
        return B()

    def __ne__(self, other: str) -> B:
        return B()

# TODO: should be `int` and `float`.
# Need to check arg type and fall back to `rhs.__eq__` and `rhs.__ne__`.
#
# Because `object.__eq__` and `object.__ne__` accept `object` in typeshed,
# this can only happen with an invalid override of these methods,
# but we still support it.
reveal_type(B() == A())  # revealed: B
reveal_type(B() != A())  # revealed: B

reveal_type(B() < A())  # revealed: list
reveal_type(B() <= A())  # revealed: set

reveal_type(B() > A())  # revealed: str
reveal_type(B() >= A())  # revealed: bytes

class C:
    def __gt__(self, other: C) -> int:
        return 42

    def __ge__(self, other: C) -> float:
        return 42.0

reveal_type(C() < C())  # revealed: int
reveal_type(C() <= C())  # revealed: float
```

## Reflected Comparisons with Subclasses

When subclasses override comparison methods, these overridden methods take precedence over those in
the parent class. Class `B` inherits from `A` and redefines comparison methods to return types other
than `A`.

```py
from __future__ import annotations

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
    def __eq__(self, other: A) -> int:
        return 42

    def __ne__(self, other: A) -> float:
        return 42.0

    def __lt__(self, other: A) -> str:
        return "42"

    def __le__(self, other: A) -> bytes:
        return b"42"

    def __gt__(self, other: A) -> list:
        return [42]

    def __ge__(self, other: A) -> set:
        return {42}

reveal_type(A() == B())  # revealed: int
reveal_type(A() != B())  # revealed: float

reveal_type(A() < B())  # revealed: list
reveal_type(A() <= B())  # revealed: set

reveal_type(A() > B())  # revealed: str
reveal_type(A() >= B())  # revealed: bytes
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

# TODO: should be `A`, need to check argument type and fall back to LHS method
reveal_type(A() < B())  # revealed: B
reveal_type(A() > B())  # revealed: B
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

# TODO: it should be `bool`, need to check arg type and fall back to `is` and `is not`
reveal_type(A() == A())  # revealed: A
reveal_type(A() != A())  # revealed: A
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

# TODO: should be Unknown and emit diagnostic,
# need to check arg type and should be failed
reveal_type(1 < 2j)  # revealed: bool
reveal_type(1 <= 2j)  # revealed: bool
reveal_type(1 > 2j)  # revealed: bool
reveal_type(1 >= 2j)  # revealed: bool

def f(x: bool, y: int):
    reveal_type(x < y)  # revealed: bool
    reveal_type(y < x)  # revealed: bool
    reveal_type(4.2 < x)  # revealed: bool
    reveal_type(x < 4.2)  # revealed: bool
```
