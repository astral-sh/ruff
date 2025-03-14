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

#### Integers

```py
def _(x: int):
    if x != 1:
        reveal_type(x)  # revealed: int & ~Literal[1]

        reveal_type(x != 1)  # revealed: Literal[True]
        reveal_type(x != 2)  # revealed: bool

        reveal_type(x == 1)  # revealed: Literal[False]
        reveal_type(x == 2)  # revealed: bool
```

### Identity comparisons

```py
class A: ...

def _(o: object):
    a = A()
    n = None

    if o is not None:
        reveal_type(o)  # revealed:  ~None
        reveal_type(o is n)  # revealed: Literal[False]
        reveal_type(o is not n)  # revealed: Literal[True]
```

## Diagnostics

### Unsupported operators for positive contributions

Raise an error if any of the positive contributions to the intersection type are unsupported for the
given operator:

```py
class Container:
    def __contains__(self, x) -> bool:
        return False

class NonContainer: ...

def _(x: object):
    if isinstance(x, Container):
        if isinstance(x, NonContainer):
            reveal_type(x)  # revealed: Container & NonContainer

            # error: [unsupported-operator] "Operator `in` is not supported for types `int` and `NonContainer`"
            reveal_type(2 in x)  # revealed: bool
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
