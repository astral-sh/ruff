# Call `type[...]`

## Single class

### Trivial constructor

```py
class C: ...

def _(subclass_of_c: type[C]):
    reveal_type(subclass_of_c())  # revealed: C
```

### Non-trivial constructor

```py
class C:
    def __init__(self, x: int): ...

def _(subclass_of_c: type[C]):
    reveal_type(subclass_of_c(1))  # revealed: C

    # TODO: Those should all be errors
    reveal_type(subclass_of_c("a"))  # revealed: C
    reveal_type(subclass_of_c())  # revealed: C
    reveal_type(subclass_of_c(1, 2))  # revealed: C
```

## Dynamic base

```py
from typing import Any
from knot_extensions import Unknown

def _(subclass_of_any: type[Any], subclass_of_unknown: type[Unknown]):
    reveal_type(subclass_of_any())  # revealed: Any
    reveal_type(subclass_of_any("any", "args", 1, 2))  # revealed: Any
    reveal_type(subclass_of_unknown())  # revealed: Unknown
    reveal_type(subclass_of_unknown("any", "args", 1, 2))  # revealed: Unknown
```

## Unions of classes

```py
class A: ...
class B: ...

def _(subclass_of_ab: type[A | B]):
    reveal_type(subclass_of_ab())  # revealed: A | B
```
