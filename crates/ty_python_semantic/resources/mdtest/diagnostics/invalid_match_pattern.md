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

## Invalid `__match_args__`

```py
class Point:
    __match_args__ = ["x", "y"]

def describe(p: Point) -> None:
    match p:
        case Point(x):  # error: [invalid-match-pattern]
            pass
```

```py
class Vec:
    __match_args__ = "coords"

def describe(v: Vec) -> None:
    match v:
        case Vec(a):  # error: [invalid-match-pattern]
            pass
```

## Tuple subclass

```py
class MatchArgs(tuple[str, ...]): ...

class Point:
    __match_args__ = MatchArgs(("x",))

def describe(p: Point) -> None:
    match p:
        case Point(x):  # error: [invalid-match-pattern]
            pass
```

## Unknown `__match_args__` tuple length (no error)

```py
class Point:
    __match_args__: tuple[str, ...] = ("x", "y")

def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):
            pass
```

## PEP 695 type alias

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal

type MatchArgs = tuple[Literal["x"], Literal["y"]]

class Point:
    __match_args__: MatchArgs = ("x", "y")

def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):  # error: [invalid-match-pattern]
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

## Missing `__match_args__`

```python
class Plain: ...

def describe(p: Plain) -> None:
    match p:
        case Plain(value):  # error: [invalid-match-pattern]
            pass
```

## Annotation-only `__match_args__`

`model.pyi`:

```pyi
from typing import Literal

class StubModel:
    __match_args__: tuple[Literal["value"]]
```

`main.py`:

```python
from typing import Literal

from model import StubModel

class SourceModel:
    __match_args__: tuple[Literal["value"]]

def describe(source: SourceModel, stub: StubModel) -> None:
    match source:
        case SourceModel(_):  # error: [invalid-match-pattern]
            pass

    match stub:
        case StubModel(_):
            pass
```

## Dataclass with `match_args=False`

```python
from dataclasses import dataclass

@dataclass(match_args=False)
class NoMatch:
    x: int
    y: int

def describe(n: NoMatch) -> None:
    match n:
        case NoMatch(x, y):  # error: [invalid-match-pattern]
            pass
```

## Built-in match-self classes

```python
def one_positional(value: int) -> None:
    match value:
        case int(_):
            pass

def two_positionals(value: int) -> None:
    match value:
        case int(_, _):  # error: [invalid-match-pattern]
            pass
```

## Invalid pattern classes do not cascade

```python
from typing import Protocol, TypedDict

class Payload(TypedDict):
    value: int

class HasValue(Protocol):
    value: int

def describe(value: object) -> None:
    match value:
        # error: [isinstance-against-typed-dict]
        case Payload(_):
            pass

    match value:
        # error: [isinstance-against-protocol]
        case HasValue(_):
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
