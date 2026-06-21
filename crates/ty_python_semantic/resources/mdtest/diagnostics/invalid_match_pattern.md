# Invalid Match Pattern

Tests for `[invalid-match-pattern]` diagnostic (issue #3738).

## Basic positional overflow

```py
class Point:
    __match_args__ = ("x", "y")

def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):  # error: [invalid-match-pattern]
            pass
```

```python
from typing_extensions import LiteralString

class Position:
    __match_args__: LiteralString = "field"

def check(x: Position) -> None:
    match x:
        case Position(a, b):  # error: [invalid-match-pattern]
            pass
```

## Limitations

```py
class Point:
    __match_args__ = ["x", "y"]

def describe(p: Point) -> None:
    match p:
        # We cannot infer the length from the `list[str]` type as it has no length information.
        # This will raise a `TypeError` at runtime.
        case Point(x, y, z):
            pass
```

## Exact match (no error)

```py
class Point:
    __match_args__ = ("x", "y")

def describe(p: Point) -> None:
    match p:
        case Point(x, y):
            pass
```

## Fewer positionals than __match_args__ (no error)

```py
class Point:
    __match_args__ = ("x", "y", "z")

def describe(p: Point) -> None:
    match p:
        case Point(x, y):
            pass
```

## String __match_args__ (single field)

```py
class Vec:
    __match_args__ = "coords"

def describe(v: Vec) -> None:
    match v:
        case Vec(a, b):  # error: [invalid-match-pattern]
            pass
```

## Empty __match_args__

```py
class Foo:
    __match_args__ = ()

def bar(x: Foo) -> None:
    match x:
        case Foo(a):  # error: [invalid-match-pattern]
            pass
```

## Dataclass with default __match_args__

```python
from dataclasses import dataclass

@dataclass
class Point:
    x: int
    y: int

def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):  # error: [invalid-match-pattern]
            pass
```

## Dataclass with match_args=False (no __match_args__ synthesized)

```python
from dataclasses import dataclass

@dataclass(match_args=False)
class NoMatch:
    x: int
    y: int

# __match_args__ is not synthesized, so positional patterns
# in a class pattern will fail — but this is a different error
# (no __match_args__ means the class pattern won't match).
# This test just verifies no false positive for invalid-match-pattern.
def describe(n: NoMatch) -> None:
    match n:
        case NoMatch(x, y):
            pass
```

## Dataclass with kw_only=True

```python
from dataclasses import dataclass

@dataclass(kw_only=True)
class NoMatch:
    x: int
    y: int

def describe(n: NoMatch) -> None:
    match n:
        case NoMatch(x, y):  # error: [invalid-match-pattern]
            pass
```

```python
from dataclasses import dataclass, field

@dataclass
class Point:
    x: int
    y: int = field(kw_only=True)

def describe(n: Point) -> None:
    match n:
        case Point(x, y):  # error: [invalid-match-pattern]
            pass
```

## NamedTuple __match_args__

```python
from typing import NamedTuple

class Point(NamedTuple):
    x: int
    y: int

def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):  # error: [invalid-match-pattern]
            pass
```

## Inherited __match_args__

```py
class Base:
    __match_args__ = ("a", "b")

class Derived(Base):
    pass

def check(d: Derived) -> None:
    match d:
        case Derived(a, b, c):  # error: [invalid-match-pattern]
            pass
```
