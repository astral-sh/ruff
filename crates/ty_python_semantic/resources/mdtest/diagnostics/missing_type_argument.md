# Missing type argument

## Standard library generics

```toml
[rules]
missing-type-argument = "error"
```

```py
x: list  # error: [missing-type-argument]
y: dict  # error: [missing-type-argument]
z: set  # error: [missing-type-argument]
w: frozenset  # error: [missing-type-argument]

# OK — explicitly parameterized
a: list[int]
b: dict[str, int]
c: set[str]
d: frozenset[bytes]
```

## User-defined generic classes

```toml
[environment]
python-version = "3.12"

[rules]
missing-type-argument = "error"
```

```py
class Container[T]:
    value: T

class Pair[T, U]:
    first: T
    second: U

x: Container  # error: [missing-type-argument]
y: Pair  # error: [missing-type-argument]

# OK — parameterized
a: Container[int]
b: Pair[str, int]
```

## PEP 696 — all type parameters have defaults

```toml
[environment]
python-version = "3.13"

[rules]
missing-type-argument = "error"
```

```py
class AllDefaults[T = int, U = str]:
    pass

# OK — all type params have defaults, bare usage is fine
x: AllDefaults
```

## PEP 696 — partial defaults

```toml
[environment]
python-version = "3.13"

[rules]
missing-type-argument = "error"
```

```py
class PartialDefaults[T, U = str]:
    pass

x: PartialDefaults  # error: [missing-type-argument]

# OK — required param provided
y: PartialDefaults[int]
```

## Bare `tuple`

```toml
[rules]
missing-type-argument = "error"
```

```py
x: tuple  # error: [missing-type-argument]
y: tuple[int, str]
```

## Dotted names

```toml
[rules]
missing-type-argument = "error"
```

```py
import collections.abc

x: collections.abc.Sequence  # error: [missing-type-argument]
y: collections.abc.Sequence[int]
```

## Disabled by default (enabled in tests)

```toml
[rules]
missing-type-argument = "ignore"
```

```py
x: list
y: dict
```

## Function signatures

```toml
[rules]
missing-type-argument = "error"
```

```py
def process(
    items: list,  # error: [missing-type-argument]
    mapping: dict,  # error: [missing-type-argument]
) -> set:  # error: [missing-type-argument]
    return set()
```

## Variable annotations

```toml
[rules]
missing-type-argument = "error"
```

```py
class MyClass:
    items: list  # error: [missing-type-argument]
    data: dict  # error: [missing-type-argument]
    names: list[str]
```

## Generics involving `ParamSpec`

```toml
[environment]
python-version = "3.12"

[rules]
missing-type-argument = "error"
```

```py
from typing import ParamSpec, Generic, Callable

P = ParamSpec("P")

class Wrapper(Generic[P]):
    pass

x: Wrapper  # error: [missing-type-argument]

# OK — parameterized
y: Wrapper[[int, str]]
```

## Bare `Callable`

```toml
[rules]
missing-type-argument = "error"
```

```py
from typing import Callable
import collections.abc

def f(cb: Callable) -> None:  # error: [missing-type-argument]
    pass

def g(cb: collections.abc.Callable) -> None:  # error: [missing-type-argument]
    pass

# OK — explicitly parameterized
def h(cb: Callable[[int], str]) -> None:
    pass
```

## Type argument to `cast`

```toml
[rules]
missing-type-argument = "error"
```

```py
from typing import cast

x = cast(list, [1, 2, 3])  # error: [missing-type-argument]

# OK — parameterized
y = cast(list[int], object())
```

## Base classes

Bare generic base classes implicitly inherit from a specialization with `Unknown` type arguments, so
they trigger the rule.

```toml
[rules]
missing-type-argument = "error"
```

```py
class MyList(list):  # error: [missing-type-argument]
    pass

class MyDict(dict):  # error: [missing-type-argument]
    pass
```
