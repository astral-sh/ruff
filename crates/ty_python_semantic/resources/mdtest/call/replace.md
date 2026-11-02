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

reveal_type(Point.__replace__)  # revealed: (self: Point, *, x: int = ..., y: int = ...) -> Point
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

# error: [invalid-argument-type] "Argument to function `replace` is incorrect: Expected `int`, found `Literal["wrong"]`"
e = replace(a, x="wrong")
```

### Dataclass transforms

Classes transformed through a base class or metaclass also support the `__replace__` protocol.

```py
from copy import replace
from typing import dataclass_transform

@dataclass_transform()
class ModelBase: ...

class BaseModel(ModelBase):
    value: int

# revealed: (self: BaseModel, *, value: int = ...) -> BaseModel
reveal_type(BaseModel.__replace__)

base_model = BaseModel(value=1)
reveal_type(base_model.__replace__(value=2))  # revealed: BaseModel
reveal_type(replace(base_model, value=2))  # revealed: BaseModel

@dataclass_transform()
class ModelMetaclass(type): ...

class MetaclassModel(metaclass=ModelMetaclass):
    value: int

# revealed: (self: MetaclassModel, *, value: int = ...) -> MetaclassModel
reveal_type(MetaclassModel.__replace__)

metaclass_model = MetaclassModel(value=1)
reveal_type(metaclass_model.__replace__(value=2))  # revealed: MetaclassModel
reveal_type(replace(metaclass_model, value=2))  # revealed: MetaclassModel
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
reveal_type(b)  # revealed: Point
```

Invalid calls to `__replace__` will raise an error:

```py
# error: [unknown-argument] "Argument `z` does not match any known parameter"
a.__replace__(z=42)
```

## Before Python 3.13

Dataclass transforms do not synthesize `__replace__` before the replacement protocol exists.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import dataclass_transform

@dataclass_transform()
class ModelBase: ...

class Model(ModelBase):
    value: int

Model.__replace__  # error: [unresolved-attribute]
Model(value=1).__replace__  # error: [unresolved-attribute]
```
