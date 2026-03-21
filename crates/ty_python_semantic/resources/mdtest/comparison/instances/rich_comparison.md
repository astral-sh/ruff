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
    def __eq__(self, other: A) -> EqReturnType:  # error: [invalid-method-override]
        return EqReturnType()

    def __ne__(self, other: A) -> NeReturnType:  # error: [invalid-method-override]
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
    def __eq__(self, other: B) -> EqReturnType:  # error: [invalid-method-override]
        return EqReturnType()

    def __ne__(self, other: B) -> NeReturnType:  # error: [invalid-method-override]
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
    def __eq__(self, other: B) -> EqReturnType:  # error: [invalid-method-override]
        return EqReturnType()

    def __ne__(self, other: B) -> NeReturnType:  # error: [invalid-method-override]
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
    def __eq__(self, other: Unrelated) -> B:  # error: [invalid-method-override]
        return B()

    def __ne__(self, other: Unrelated) -> B:  # error: [invalid-method-override]
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
    def __eq__(self, other: A) -> A:  # error: [invalid-method-override]
        return A()

    def __ne__(self, other: A) -> A:  # error: [invalid-method-override]
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
    def __eq__(self, other: A) -> EqReturnType:  # error: [invalid-method-override]
        return EqReturnType()

    def __ne__(self, other: A) -> NeReturnType:  # error: [invalid-method-override]
        return NeReturnType()

    def __lt__(self, other: A) -> LtReturnType:  # error: [invalid-method-override]
        return LtReturnType()

    def __le__(self, other: A) -> LeReturnType:  # error: [invalid-method-override]
        return LeReturnType()

    def __gt__(self, other: A) -> GtReturnType:  # error: [invalid-method-override]
        return GtReturnType()

    def __ge__(self, other: A) -> GeReturnType:  # error: [invalid-method-override]
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
    def __lt__(self, other: int) -> B:  # error: [invalid-method-override]
        return B()

    def __gt__(self, other: int) -> B:  # error: [invalid-method-override]
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
    def __eq__(self, other: int) -> A:  # error: [invalid-method-override]
        return A()

    def __ne__(self, other: int) -> A:  # error: [invalid-method-override]
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

# error: [unsupported-operator] "Operator `<` is not supported between objects of type `A` and `object`"
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

# error: [unsupported-operator] "Operator `<` is not supported between objects of type `Literal[1]` and `complex`"
reveal_type(1 < 2j)  # revealed: Unknown
# error: [unsupported-operator] "Operator `<=` is not supported between objects of type `Literal[1]` and `complex`"
reveal_type(1 <= 2j)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>` is not supported between objects of type `Literal[1]` and `complex`"
reveal_type(1 > 2j)  # revealed: Unknown
# error: [unsupported-operator] "Operator `>=` is not supported between objects of type `Literal[1]` and `complex`"
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

## Diagnostics where classes have the same name

We use the fully qualified names of classes to disambiguate them where necessary:

`a.py`:

```py
class Foo: ...
```

`b.py`:

```py
class Foo: ...
```

`main.py`:

```py
import a
import b

# error: [unsupported-operator] "Operator `<` is not supported between objects of type `a.Foo` and `b.Foo`"
a.Foo() < b.Foo()
```

## TypeVar Comparisons

TypeVars with bounds support comparison operations if the bound type supports them.

### TypeVar with `float` bound

Since `float` is treated as `int | float` in type annotations, TypeVars bounded by `float` should
support all comparison operations that both `int` and `float` support:

```py
from typing import TypeVar, Generic

T = TypeVar("T", bound=float)

class Range(Generic[T]):
    min: T
    max: T

    def __init__(self, min: T, max: T) -> None:
        self.min = min
        self.max = max

    def __contains__(self, value: T) -> bool:
        return self.min <= value <= self.max

def compare_float_bound(a: T, b: T) -> bool:
    return a <= b

def compare_with_literal(a: T) -> bool:
    return a <= 1.0
```

### TypeVar with `int` bound

TypeVars bounded by `int` should support comparison operations:

```py
from typing import TypeVar

U = TypeVar("U", bound=int)

def compare_int_bound(a: U, b: U) -> bool:
    return a <= b
```

### TypeVar with `str` bound

TypeVars bounded by `str` should support comparison operations:

```py
from typing import TypeVar

V = TypeVar("V", bound=str)

def compare_str_bound(a: V, b: V) -> bool:
    return a <= b
```

### Constrained TypeVar comparisons

Constrained TypeVars support comparisons if all constraints support the operation:

```py
from typing import TypeVar

W = TypeVar("W", int, str)

def compare_constrained(a: W, b: W) -> bool:
    # Both int and str support ==
    return a == b

X = TypeVar("X", int, str)

def compare_constrained_lt(a: X, b: X) -> bool:
    # Both int and str support <
    return a < b
```

### TypeVar with `complex` bound

`complex` is treated as `int | float | complex`. Since `complex` doesn't support ordering
comparisons like `<` and `<=`, only equality comparisons should work:

```py
from typing import TypeVar

Y = TypeVar("Y", bound=complex)

def compare_complex_eq(a: Y, b: Y) -> bool:
    return a == b
```

## Literal Types in Comparison Methods

Classes can define comparison methods that accept literal types. We should preserve the literal type
when checking these comparisons.

### Integer Literals

```py
from typing import Literal

class Money:
    def __gt__(self, other: Literal[0]) -> bool:
        return True

    def __lt__(self, other: Literal[0]) -> bool:
        return True

m = Money()

# Instance on left, literal on right
reveal_type(m > 0)  # revealed: bool
reveal_type(m < 0)  # revealed: bool

# Direct method calls should also work
reveal_type(m.__gt__(0))  # revealed: bool
reveal_type(m.__lt__(0))  # revealed: bool

# Comparison with general int should fail (only Literal[0] is accepted)
def check_int_fails(x: int, m: Money):
    # error: [unsupported-operator] "Operator `>` is not supported between objects of type `Money` and `int`"
    m > x
```

Reflected operators for integer literals (literal on left, instance on right). For `100 > t`, Python
tries `int.__gt__(100, t)` first (returns NotImplemented), then falls back to
`Threshold.__lt__(t, 100)`:

```py
from typing import Literal

class Threshold:
    # Called when literal is on the left: `100 > t` falls back to `t.__lt__(100)`
    def __lt__(self, other: Literal[100]) -> bool:
        return True
    # Called when literal is on the left: `100 < t` falls back to `t.__gt__(100)`
    def __gt__(self, other: Literal[100]) -> bool:
        return True

t = Threshold()

# Literal on left, instance on right (uses reflected/swapped operators)
reveal_type(100 > t)  # revealed: bool
reveal_type(100 < t)  # revealed: bool

# General int should fail
def check_int_reflected_fails(y: int, t: Threshold):
    # error: [unsupported-operator] "Operator `>` is not supported between objects of type `int` and `Threshold`"
    y > t
```

### String Literals

```py
from typing import Literal

class Command:
    def __gt__(self, other: Literal["quit"]) -> bool:
        return True

cmd = Command()

# Instance on left, literal on right
reveal_type(cmd > "quit")  # revealed: bool

# Direct method call
reveal_type(cmd.__gt__("quit"))  # revealed: bool

# Comparison with general str should fail
def check_str_fails(s: str, cmd: Command):
    # error: [unsupported-operator] "Operator `>` is not supported between objects of type `Command` and `str`"
    cmd > s
```

Reflected operators for string literals:

```py
from typing import Literal

class Keyword:
    # Called when literal is on the left: `"match" > kw` falls back to `kw.__lt__("match")`
    def __lt__(self, other: Literal["match"]) -> bool:
        return True

kw = Keyword()

# Literal on left, instance on right
reveal_type("match" > kw)  # revealed: bool

# General str should fail
def check_str_reflected_fails(s: str, kw: Keyword):
    # error: [unsupported-operator] "Operator `>` is not supported between objects of type `str` and `Keyword`"
    s > kw
```

### Bytes Literals

```py
from typing import Literal

class Header:
    def __gt__(self, other: Literal[b"HTTP"]) -> bool:
        return True

h = Header()

# Instance on left, literal on right
reveal_type(h > b"HTTP")  # revealed: bool

# Direct method call
reveal_type(h.__gt__(b"HTTP"))  # revealed: bool

# Comparison with general bytes should fail
def check_bytes_fails(b: bytes, h: Header):
    # error: [unsupported-operator] "Operator `>` is not supported between objects of type `Header` and `bytes`"
    h > b
```

Reflected operators for bytes literals:

```py
from typing import Literal

class Magic:
    # Called when literal is on the left: `b"\x89PNG" > m` falls back to `m.__lt__(b"\x89PNG")`
    def __lt__(self, other: Literal[b"\x89PNG"]) -> bool:
        return True

m = Magic()

# Literal on left, instance on right
reveal_type(b"\x89PNG" > m)  # revealed: bool

# General bytes should fail
def check_bytes_reflected_fails(data: bytes, m: Magic):
    # error: [unsupported-operator] "Operator `>` is not supported between objects of type `bytes` and `Magic`"
    data > m
```

### Union Types with Literals

Union types containing literals should also work:

```py
from typing import Literal, Union

class Money2:
    def __gt__(self, other: Union["Money2", Literal[0]]) -> bool:
        return True

m2 = Money2()
reveal_type(m2 > 0)  # revealed: bool
reveal_type(m2 > Money2())  # revealed: bool
```
