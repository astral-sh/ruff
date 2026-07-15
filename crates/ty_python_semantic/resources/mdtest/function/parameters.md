# Function parameter types

Within a function scope, the declared type of each parameter is its annotated type (or Unknown if
not annotated). The initial inferred type is the annotated type of the parameter, if any. If there
is no annotation but there is a default value, the declared type is inferred by promoting the
default value's type (literals are widened to their base types, e.g. `Literal["foo"]` → `str`;
singletons like `None` are widened to `T | Unknown`).

The variadic parameter is a variadic tuple of its annotated type; the variadic-keywords parameter is
a dictionary from strings to its annotated type.

## Parameter kinds

```py
from typing import Literal

def f(a, b: int, c=1, d: int = 2, /, e=3, f: Literal[4] = 4, *args: object, g=5, h: Literal[6] = 6, **kwargs: str):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: int
    reveal_type(d)  # revealed: int
    reveal_type(e)  # revealed: int
    reveal_type(f)  # revealed: Literal[4]
    reveal_type(g)  # revealed: int
    reveal_type(h)  # revealed: Literal[6]
    reveal_type(args)  # revealed: tuple[object, ...]
    reveal_type(kwargs)  # revealed: dict[str, str]
```

## Unannotated parameters with defaults

Unannotated parameters with defaults get a declared type inferred by promoting the default value's
type. Literals are widened to their base types; singletons like `None` are widened to `T | Unknown`
to avoid being overly restrictive.

```py
def f(a="foo", b=1, c=True, d=None, e=b"bytes"):
    reveal_type(a)  # revealed: str
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: bool
    reveal_type(d)  # revealed: None | Unknown
    reveal_type(e)  # revealed: bytes
```

At call sites, the inferred type is checked:

```py
def f(x="foo"): ...

f("bar")  # ok
f(1)  # error: [invalid-argument-type]
```

Since `None | Unknown` includes `Unknown`, a `None`-defaulted parameter accepts any argument type:

```py
def f(x=None): ...

f(None)  # ok
f("anything")  # ok
f(42)  # ok
```

## Unannotated variadic parameters

...are inferred as tuple of Unknown or dict from string to Unknown.

```py
def g(*args, **kwargs):
    reveal_type(args)  # revealed: tuple[Unknown, ...]
    reveal_type(kwargs)  # revealed: dict[str, Unknown]
```

## Annotation is present but not a fully static type

If there is an annotation, we respect it fully and don't union in the default value type.

```py
from typing import Any

def f(x: Any = 1):
    reveal_type(x)  # revealed: Any
```

## Default value type must be assignable to annotated type

The default value type must be assignable to the annotated type. If not, we emit a diagnostic, and
fall back to inferring the annotated type, ignoring the default value type.

```py
# error: [invalid-parameter-default]
def f(x: int = "foo"):
    reveal_type(x)  # revealed: int

# The check is assignable-to, not subtype-of, so this is fine:
from typing import Any

def g(x: Any = "foo"):
    reveal_type(x)  # revealed: Any
```

## TypedDict defaults use annotation context

```py
from typing import TypedDict

class Foo(TypedDict):
    x: int

def x(a: Foo = {"x": 42}): ...
def y(a: Foo = dict(x=42)): ...
```

## TypedDict defaults still validate keys and value types

```py
from typing import TypedDict

class Foo(TypedDict):
    x: int
    y: int

# error: [missing-typed-dict-key]
def missing_key(a: Foo = {"x": 42}): ...

# error: [invalid-argument-type]
def wrong_type(a: Foo = {"x": "s", "y": 1}): ...

# error: [invalid-key]
def extra_key(a: Foo = {"x": 1, "y": 2, "z": 3}): ...
```

## Stub functions

```toml
[environment]
python-version = "3.12"
```

### In Protocol

```py
from typing import Protocol

class Foo(Protocol):
    def x(self, y: bool = ...): ...
    def y[T](self, y: T = ...) -> T: ...

class GenericFoo[T](Protocol):
    def x(self, y: bool = ...) -> T: ...
```

### In abstract method

```py
from abc import abstractmethod

class Bar:
    @abstractmethod
    def x(self, y: bool = ...): ...
    @abstractmethod
    def y[T](self, y: T = ...) -> T: ...
```

### In function overload

```py
from typing import overload

@overload
def x(y: None = ...) -> None: ...
@overload
def x(y: int) -> str: ...
def x(y: int | None = None) -> str | None: ...
```

### In `if TYPE_CHECKING` blocks

We generally view code in `if TYPE_CHECKING` blocks as having the same semantics and exemptions to
code in stub files:

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    def foo(x: bool = ...): ...  # fine
```
