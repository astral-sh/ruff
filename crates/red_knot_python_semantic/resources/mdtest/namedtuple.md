# Named tuples

`NamedTuple` is a type-safe way to define named tuples — a tuple where each field can be accessed by
name, and not just by its numeric position within the tuple:

```py
from typing import NamedTuple

class Coordinates(NamedTuple):
    x: int
    y: int

coordinates = Coordinates(x=12, y=45)
reveal_type(coordinates)  # revealed: Coordinates
reveal_type(coordinates.x)  # revealed: Literal[12]
reveal_type(coordinates.y)  # revealed: Literal[45]
```
