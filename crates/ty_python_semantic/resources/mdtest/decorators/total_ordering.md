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
