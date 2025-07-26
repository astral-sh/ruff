# Replace

The replace function and protocol added in Python 3.13:
<https://docs.python.org/3/whatsnew/3.13.html#copy>

## `replace()` function

It is present in the `copy` module.

```toml
[environment]
python-version = "3.13"
```

```py
from copy import replace
```

## `__replace__` protocol

```toml
[environment]
python-version = "3.13"
```

### Dataclasses

```py
from dataclasses import dataclass
from copy import replace

@dataclass
class Point:
    x: int
    y: int

a = Point(1, 2)

# It accepts keyword arguments
reveal_type(a.__replace__)  # revealed: (*, x: int = int, y: int = int) -> Point
b = a.__replace__(x=3, y=4)
reveal_type(b)  # revealed: Point
b = replace(a, x=3, y=4)
reveal_type(b)  # revealed: Point

# It does not require all keyword arguments
c = a.__replace__(x=3)
reveal_type(c)  # revealed: Point
d = replace(a, x=3)
reveal_type(d)  # revealed: Point
```

## Version support check

It is not present in Python < 3.13

```toml
[environment]
python-version = "3.12"
```

```py
from copy import replace  # error: [unresolved-import]
```
