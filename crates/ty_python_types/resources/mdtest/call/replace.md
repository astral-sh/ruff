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

# TODO: this should be `time`, once we support specialization of generic protocols
reveal_type(t)  # revealed: Unknown
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

reveal_type(Point.__replace__)  # revealed: (self: Point, *, x: int = ..., y: int = ...) -> Point
```

The `__replace__` method can either be called directly or through the `replace` function:

```py
a = Point(1, 2)

b = a.__replace__(x=3, y=4)
reveal_type(b)  # revealed: Point

b = replace(a, x=3, y=4)
# TODO: this should be `Point`, once we support specialization of generic protocols
reveal_type(b)  # revealed: Unknown
```

A call to `replace` does not require all keyword arguments:

```py
c = a.__replace__(y=4)
reveal_type(c)  # revealed: Point

d = replace(a, y=4)
# TODO: this should be `Point`, once we support specialization of generic protocols
reveal_type(d)  # revealed: Unknown
```

Invalid calls to `__replace__` or `replace` will raise an error:

```py
e = a.__replace__(x="wrong")  # error: [invalid-argument-type]

# TODO: this should ideally also be emit an error
e = replace(a, x="wrong")
```

### NamedTuples

NamedTuples also support the `__replace__` protocol:

```py
from typing import NamedTuple
from copy import replace

class Point(NamedTuple):
    x: int
    y: int

reveal_type(Point.__replace__)  # revealed: (self: Self, *, x: int = ..., y: int = ...) -> Self
```

The `__replace__` method can either be called directly or through the `replace` function:

```py
a = Point(1, 2)

b = a.__replace__(x=3, y=4)
reveal_type(b)  # revealed: Point

b = replace(a, x=3, y=4)
# TODO: this should be `Point`, once we support specialization of generic protocols
reveal_type(b)  # revealed: Unknown
```

Invalid calls to `__replace__` will raise an error:

```py
# error: [unknown-argument] "Argument `z` does not match any known parameter"
a.__replace__(z=42)
```
