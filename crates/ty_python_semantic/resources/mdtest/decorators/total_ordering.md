# `functools.total_ordering`

The `@functools.total_ordering` decorator allows a class to define a single comparison method (like
`__lt__`), and the decorator automatically generates the remaining comparison methods (`__le__`,
`__gt__`, `__ge__`). Defining `__eq__` is optional, as it can be inherited from `object`.

## Basic usage

When a class defines `__eq__` and `__lt__`, the decorator synthesizes `__le__`, `__gt__`, and
`__ge__`:

```py
from functools import total_ordering

@total_ordering
class Student:
    def __init__(self, grade: int):
        self.grade = grade

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Student):
            return NotImplemented
        return self.grade == other.grade

    def __lt__(self, other: "Student") -> bool:
        return self.grade < other.grade

s1 = Student(85)
s2 = Student(90)

# User-defined comparison methods work as expected.
reveal_type(s1 == s2)  # revealed: bool
reveal_type(s1 < s2)  # revealed: bool

# Synthesized comparison methods are available.
reveal_type(s1 <= s2)  # revealed: bool
reveal_type(s1 > s2)  # revealed: bool
reveal_type(s1 >= s2)  # revealed: bool
```

## Signature derived from source ordering method

When the source ordering method accepts a broader type (like `object`) for its `other` parameter,
the synthesized comparison methods should use the same signature. This allows comparisons with types
other than the class itself:

```py
from functools import total_ordering

@total_ordering
class Comparable:
    def __init__(self, value: int):
        self.value = value

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Comparable):
            return self.value == other.value
        if isinstance(other, int):
            return self.value == other
        return NotImplemented

    def __lt__(self, other: object) -> bool:
        if isinstance(other, Comparable):
            return self.value < other.value
        if isinstance(other, int):
            return self.value < other
        return NotImplemented

a = Comparable(10)
b = Comparable(20)

# Comparisons with the same type work.
reveal_type(a <= b)  # revealed: bool
reveal_type(a >= b)  # revealed: bool

# Comparisons with `int` also work because `__lt__` accepts `object`.
reveal_type(a <= 15)  # revealed: bool
reveal_type(a >= 5)  # revealed: bool
```

## Multiple ordering methods with different signatures

When multiple ordering methods are defined with different signatures, the decorator selects a "root"
method using the priority order: `__lt__` > `__le__` > `__gt__` > `__ge__`. Synthesized methods use
the signature from the highest-priority method. Methods that are explicitly defined are not
overridden.

```py
from functools import total_ordering

@total_ordering
class MultiSig:
    def __init__(self, value: int):
        self.value = value

    def __eq__(self, other: object) -> bool:
        return True
    # __lt__ accepts `object` (highest priority, used as root)
    def __lt__(self, other: object) -> bool:
        return True
    # __gt__ only accepts `MultiSig` (not overridden by decorator)
    def __gt__(self, other: "MultiSig") -> bool:
        return True

a = MultiSig(10)
b = MultiSig(20)

# __le__ and __ge__ are synthesized with __lt__'s signature (accepts `object`)
reveal_type(a <= b)  # revealed: bool
reveal_type(a <= 15)  # revealed: bool
reveal_type(a >= b)  # revealed: bool
reveal_type(a >= 15)  # revealed: bool

# __gt__ keeps its original signature (only accepts MultiSig)
reveal_type(a > b)  # revealed: bool
a > 15  # error: [unsupported-operator]
```

## Overloaded ordering method

When the source ordering method is overloaded, the synthesized comparison methods should preserve
all overloads:

```py
from functools import total_ordering
from typing import overload

@total_ordering
class Flexible:
    def __init__(self, value: int):
        self.value = value

    def __eq__(self, other: object) -> bool:
        return True

    @overload
    def __lt__(self, other: "Flexible") -> bool: ...
    @overload
    def __lt__(self, other: int) -> bool: ...
    def __lt__(self, other: "Flexible | int") -> bool:
        if isinstance(other, Flexible):
            return self.value < other.value
        return self.value < other

a = Flexible(10)
b = Flexible(20)

# Synthesized __le__ preserves overloads from __lt__
reveal_type(a <= b)  # revealed: bool
reveal_type(a <= 15)  # revealed: bool

# Synthesized __ge__ also preserves overloads
reveal_type(a >= b)  # revealed: bool
reveal_type(a >= 15)  # revealed: bool

# But comparison with an unsupported type should still error
a <= "string"  # error: [unsupported-operator]
```

## Using `__gt__` as the root comparison method

When a class defines `__eq__` and `__gt__`, the decorator synthesizes `__lt__`, `__le__`, and
`__ge__`:

```py
from functools import total_ordering

@total_ordering
class Priority:
    def __init__(self, level: int):
        self.level = level

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Priority):
            return NotImplemented
        return self.level == other.level

    def __gt__(self, other: "Priority") -> bool:
        return self.level > other.level

p1 = Priority(1)
p2 = Priority(2)

# User-defined comparison methods work
reveal_type(p1 == p2)  # revealed: bool
reveal_type(p1 > p2)  # revealed: bool

# Synthesized comparison methods are available
reveal_type(p1 < p2)  # revealed: bool
reveal_type(p1 <= p2)  # revealed: bool
reveal_type(p1 >= p2)  # revealed: bool
```

## Inherited `__eq__`

A class only needs to define a single comparison method. The `__eq__` method can be inherited from
`object`:

```py
from functools import total_ordering

@total_ordering
class Score:
    def __init__(self, value: int):
        self.value = value

    def __lt__(self, other: "Score") -> bool:
        return self.value < other.value

s1 = Score(85)
s2 = Score(90)

# `__eq__` is inherited from object.
reveal_type(s1 == s2)  # revealed: bool

# Synthesized comparison methods are available.
reveal_type(s1 <= s2)  # revealed: bool
reveal_type(s1 > s2)  # revealed: bool
reveal_type(s1 >= s2)  # revealed: bool
```

## Inherited ordering methods

The decorator also works when the ordering method is inherited from a superclass:

```py
from functools import total_ordering

class Base:
    def __lt__(self, other: "Base") -> bool:
        return True

@total_ordering
class Child(Base):
    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Child):
            return NotImplemented
        return True

c1 = Child()
c2 = Child()

# Synthesized methods work even though `__lt__` is inherited.
reveal_type(c1 <= c2)  # revealed: bool
reveal_type(c1 > c2)  # revealed: bool
reveal_type(c1 >= c2)  # revealed: bool
```

## Method precedence with inheritance

The decorator always prefers `__lt__` > `__le__` > `__gt__` > `__ge__`, regardless of whether the
method is defined locally or inherited. In this example, the inherited `__lt__` takes precedence
over the locally-defined `__gt__`:

```py
from functools import total_ordering
from typing import Literal

class Base:
    def __lt__(self, other: "Base") -> Literal[True]:
        return True

@total_ordering
class Child(Base):
    # __gt__ is defined locally, but __lt__ (inherited) takes precedence
    def __gt__(self, other: "Child") -> Literal[False]:
        return False

c1 = Child()
c2 = Child()

# __lt__ is inherited from Base
reveal_type(c1 < c2)  # revealed: Literal[True]

# __gt__ is defined locally on Child
reveal_type(c1 > c2)  # revealed: Literal[False]

# __le__ and __ge__ are synthesized from __lt__ (the highest-priority method),
# even though __gt__ is defined locally on the class itself
reveal_type(c1 <= c2)  # revealed: bool
reveal_type(c1 >= c2)  # revealed: bool
```

## Explicitly-defined methods are not overridden

When a class explicitly defines multiple comparison methods, the decorator does not override them.
We use a narrower return type (`Literal[True]`) to verify that the explicit methods are preserved:

```py
from functools import total_ordering
from typing import Literal

@total_ordering
class Temperature:
    def __init__(self, celsius: float):
        self.celsius = celsius

    def __lt__(self, other: "Temperature") -> Literal[True]:
        return True

    def __gt__(self, other: "Temperature") -> Literal[True]:
        return True

t1 = Temperature(20.0)
t2 = Temperature(25.0)

# User-defined methods preserve their return type.
reveal_type(t1 < t2)  # revealed: Literal[True]
reveal_type(t1 > t2)  # revealed: Literal[True]

# Synthesized methods have `bool` return type.
reveal_type(t1 <= t2)  # revealed: bool
reveal_type(t1 >= t2)  # revealed: bool
```

## Combined with `@dataclass`

The decorator works with `@dataclass`:

```py
from dataclasses import dataclass
from functools import total_ordering

@total_ordering
@dataclass
class Point:
    x: int
    y: int

    def __lt__(self, other: "Point") -> bool:
        return (self.x, self.y) < (other.x, other.y)

p1 = Point(1, 2)
p2 = Point(3, 4)

# Dataclass-synthesized `__eq__` is available.
reveal_type(p1 == p2)  # revealed: bool

# User-defined comparison method works.
reveal_type(p1 < p2)  # revealed: bool

# Synthesized comparison methods are available.
reveal_type(p1 <= p2)  # revealed: bool
reveal_type(p1 > p2)  # revealed: bool
reveal_type(p1 >= p2)  # revealed: bool
```

## Missing ordering method

If a class has `@total_ordering` but doesn't define any ordering method (itself or in a superclass),
a diagnostic is emitted at the decorator site:

```py
from functools import total_ordering

@total_ordering  # error: [invalid-total-ordering]
class NoOrdering:
    def __eq__(self, other: object) -> bool:
        return True

n1 = NoOrdering()
n2 = NoOrdering()

# Comparison operators also error because no methods were synthesized.
n1 <= n2  # error: [unsupported-operator]
n1 >= n2  # error: [unsupported-operator]
```

## Without the decorator

Without `@total_ordering`, classes that only define `__lt__` will not have `__le__` or `__ge__`
synthesized:

```py
class NoDecorator:
    def __init__(self, value: int):
        self.value = value

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, NoDecorator):
            return NotImplemented
        return self.value == other.value

    def __lt__(self, other: "NoDecorator") -> bool:
        return self.value < other.value

n1 = NoDecorator(1)
n2 = NoDecorator(2)

# User-defined methods work.
reveal_type(n1 == n2)  # revealed: bool
reveal_type(n1 < n2)  # revealed: bool

# Note: `n1 > n2` works because Python reflects it to `n2 < n1`
reveal_type(n1 > n2)  # revealed: bool

# These comparison operators are not available.
n1 <= n2  # error: [unsupported-operator]
n1 >= n2  # error: [unsupported-operator]
```

## Non-bool return type

When the root ordering method returns a non-bool type (like `int`), the synthesized methods return a
union of that type and `bool`. This is because `@total_ordering` generates methods like:

```python
def __le__(self, other):
    return self < other or self == other
```

If `__lt__` returns `int`, then the synthesized `__le__` could return either `int` (from
`self < other`) or `bool` (from `self == other`). Since `bool` is a subtype of `int`, the union
simplifies to `int`:

```py
from functools import total_ordering

@total_ordering
class IntReturn:
    def __init__(self, value: int):
        self.value = value

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, IntReturn):
            return NotImplemented
        return self.value == other.value

    def __lt__(self, other: "IntReturn") -> int:
        return self.value - other.value

a = IntReturn(10)
b = IntReturn(20)

# User-defined __lt__ returns int.
reveal_type(a < b)  # revealed: int

# Synthesized methods return int (the union int | bool simplifies to int
# because bool is a subtype of int in Python).
reveal_type(a <= b)  # revealed: int
reveal_type(a > b)  # revealed: int
reveal_type(a >= b)  # revealed: int
```

When the root method returns a type that is not a supertype of `bool`, the union is preserved:

```py
from functools import total_ordering

@total_ordering
class StrReturn:
    def __init__(self, value: str):
        self.value = value

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, StrReturn):
            return NotImplemented
        return self.value == other.value

    def __lt__(self, other: "StrReturn") -> str:
        return self.value

a = StrReturn("a")
b = StrReturn("b")

# User-defined __lt__ returns str.
reveal_type(a < b)  # revealed: str

# Synthesized methods return str | bool.
reveal_type(a <= b)  # revealed: str | bool
reveal_type(a > b)  # revealed: str | bool
reveal_type(a >= b)  # revealed: str | bool
```

## Function call form

When `total_ordering` is called as a function (not as a decorator), the same validation is
performed:

```py
from functools import total_ordering

class NoOrderingMethod:
    def __eq__(self, other: object) -> bool:
        return True

# error: [invalid-total-ordering]
InvalidOrderedClass = total_ordering(NoOrderingMethod)
```

When the class does define an ordering method, no error is emitted:

```py
from functools import total_ordering

class HasOrderingMethod:
    def __eq__(self, other: object) -> bool:
        return True

    def __lt__(self, other: "HasOrderingMethod") -> bool:
        return True

# No error (class defines `__lt__`).
ValidOrderedClass = total_ordering(HasOrderingMethod)
reveal_type(ValidOrderedClass)  # revealed: type[HasOrderingMethod]
```

## Function call form with `type()`

When `total_ordering` is called on a class created with `type()`, the same validation is performed:

```py
from functools import total_ordering

def lt_impl(self, other) -> bool:
    return True

# No error: the functional class defines `__lt__` in its namespace
ValidFunctional = total_ordering(type("ValidFunctional", (), {"__lt__": lt_impl}))

# error: [invalid-total-ordering]
InvalidFunctional = total_ordering(type("InvalidFunctional", (), {}))
```

## Inherited from functional class

When a class inherits from a functional class that defines an ordering method, `@total_ordering`
correctly detects it:

```py
from functools import total_ordering

def lt_impl(self, other) -> bool:
    return True

def eq_impl(self, other) -> bool:
    return True

# Functional class with __lt__ method
OrderedBase = type("OrderedBase", (), {"__lt__": lt_impl})

# A class inheriting from OrderedBase gets the ordering method
@total_ordering
class Ordered(OrderedBase):
    def __eq__(self, other: object) -> bool:
        return True

o1 = Ordered()
o2 = Ordered()

# Inherited __lt__ is available
reveal_type(o1 < o2)  # revealed: bool

# @total_ordering synthesizes the other methods
reveal_type(o1 <= o2)  # revealed: bool
reveal_type(o1 > o2)  # revealed: bool
reveal_type(o1 >= o2)  # revealed: bool
```

When the functional base class does not define any ordering method, `@total_ordering` emits an
error:

```py
from functools import total_ordering

# Functional class without ordering methods (invalid for @total_ordering)
NoOrderBase = type("NoOrderBase", (), {})

@total_ordering  # error: [invalid-total-ordering]
class NoOrder(NoOrderBase):
    def __eq__(self, other: object) -> bool:
        return True
```
