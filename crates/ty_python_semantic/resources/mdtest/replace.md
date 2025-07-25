# Replace

The replace function and protocol added in Python 3.13:
<https://docs.python.org/3/whatsnew/3.13.html#copy>

```toml
[environment]
python-version = "3.13"
```

## `replace()` function

It is present in the `copy` module.

```py
from copy import replace
```

## `__replace__` protocol

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

e = a.__replace__(x="wrong")  # error: [invalid-argument-type]
e = replace(a, x="wrong")  # TODO: error: [invalid-argument-type]
```
