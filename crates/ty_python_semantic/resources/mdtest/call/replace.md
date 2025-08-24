# `replace`

The `replace` function and the `replace` protocol were added in Python 3.13:
<https://docs.python.org/3/whatsnew/3.13.html#copy>

```toml
[environment]
python-version = "3.13"
```

## Basic

```py
from copy import replace
from datetime import time

t = time(12, 0, 0)
t = replace(t, minute=30)

reveal_type(t)  # revealed: time
```

## The `__replace__` protocol

### Dataclasses

Dataclasses support the `__replace__` protocol:

```py
from dataclasses import dataclass
from copy import replace

@dataclass
class Point:
    x: int
    y: int

reveal_type(Point.__replace__)  # revealed: (self: Point, *, x: int = int, y: int = int) -> Point
```

The `__replace__` method can either be called directly or through the `replace` function:

```py
a = Point(1, 2)

b = a.__replace__(x=3, y=4)
reveal_type(b)  # revealed: Point

b = replace(a, x=3, y=4)
reveal_type(b)  # revealed: Point
```

A call to `replace` does not require all keyword arguments:

```py
c = a.__replace__(y=4)
reveal_type(c)  # revealed: Point

d = replace(a, y=4)
reveal_type(d)  # revealed: Point
```

Invalid calls to `__replace__` or `replace` will raise an error:

```py
e = a.__replace__(x="wrong")  # error: [invalid-argument-type]

# TODO: this should ideally also be emit an error
e = replace(a, x="wrong")
```
