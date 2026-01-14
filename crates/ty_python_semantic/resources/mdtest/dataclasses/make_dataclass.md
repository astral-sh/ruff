# `make_dataclass`

Tests for the functional form of dataclass creation via `dataclasses.make_dataclass`.

## Basic usage

```py
from dataclasses import make_dataclass

Point = make_dataclass("Point", [("x", int), ("y", int)])

p = Point(1, 2)
reveal_type(p)  # revealed: Point

reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int
```

## Field with string only (type defaults to Any)

```py
from dataclasses import make_dataclass

C1 = make_dataclass("C1", ["x", "y"])

c = C1(1, "hello")
reveal_type(c)  # revealed: C1

reveal_type(c.x)  # revealed: Any
reveal_type(c.y)  # revealed: Any
```

## Mixed field formats

```py
from dataclasses import make_dataclass

C2 = make_dataclass("C2", ["x", ("y", int), ("z", str)])

c = C2(1, 2, "hello")
reveal_type(c)  # revealed: C2

reveal_type(c.x)  # revealed: Any
reveal_type(c.y)  # revealed: int
reveal_type(c.z)  # revealed: str
```

## Class type

```py
from dataclasses import make_dataclass

Point2 = make_dataclass("Point2", [("x", int), ("y", int)])

reveal_type(Point2)  # revealed: <class 'Point2'>
```

## Fields with defaults

The third element of a 3-tuple specifies a default value:

```py
from dataclasses import make_dataclass

PointWithDefault = make_dataclass("PointWithDefault", [("x", int), ("y", int, 0)])

# error: [missing-argument] "No argument provided for required parameter `x`"
PointWithDefault()

# Good - y has a default
p1 = PointWithDefault(1)
p2 = PointWithDefault(1, 2)

reveal_type(PointWithDefault.__init__)  # revealed: (self: PointWithDefault, x: int, y: int = 0) -> None
```

## Dataclass methods

### `__init__`

```py
from dataclasses import make_dataclass

Point3 = make_dataclass("Point3", [("x", int), ("y", int)])

# Good
p1 = Point3(1, 2)
p2 = Point3(x=1, y=2)

# error: [missing-argument]
p3 = Point3(1)

# error: [missing-argument]
p4 = Point3()
```

### `__eq__`

```py
from dataclasses import make_dataclass

Point4 = make_dataclass("Point4", [("x", int), ("y", int)])

p1 = Point4(1, 2)
p2 = Point4(1, 2)

reveal_type(p1 == p2)  # revealed: bool
```

## Dataclass parameters

### `init=False`

```py
from dataclasses import make_dataclass

PointNoInit = make_dataclass("PointNoInit", [("x", int), ("y", int)], init=False)

# error: [too-many-positional-arguments]
p = PointNoInit(1, 2)
```

### `eq=False`

```py
from dataclasses import make_dataclass

PointNoEq = make_dataclass("PointNoEq", [("x", int), ("y", int)], eq=False)

p1 = PointNoEq(1, 2)
p2 = PointNoEq(1, 2)

# Falls back to object.__eq__
reveal_type(p1 == p2)  # revealed: bool
```

### `order=True`

```py
from dataclasses import make_dataclass

PointOrder = make_dataclass("PointOrder", [("x", int), ("y", int)], order=True)

p1 = PointOrder(1, 2)
p2 = PointOrder(3, 4)

reveal_type(p1 < p2)  # revealed: bool
reveal_type(p1 <= p2)  # revealed: bool
reveal_type(p1 > p2)  # revealed: bool
reveal_type(p1 >= p2)  # revealed: bool
```

### `frozen=True`

```py
from dataclasses import make_dataclass

PointFrozen = make_dataclass("PointFrozen", [("x", int), ("y", int)], frozen=True)

p = PointFrozen(1, 2)

# frozen dataclasses generate __hash__
reveal_type(hash(p))  # revealed: int
```

### `kw_only=True`

```py
from dataclasses import make_dataclass

PointKwOnly = make_dataclass("PointKwOnly", [("x", int), ("y", int)], kw_only=True)

# Good
p1 = PointKwOnly(x=1, y=2)

# error: [missing-argument] "No arguments provided for required parameters `x`, `y`"
# error: [too-many-positional-arguments] "Too many positional arguments: expected 0, got 2"
p2 = PointKwOnly(1, 2)
```

### `match_args=True` (default)

```py
from dataclasses import make_dataclass

Point5 = make_dataclass("Point5", [("x", int), ("y", int)])

reveal_type(Point5.__match_args__)  # revealed: tuple[Literal["x"], Literal["y"]]
```

## `__dataclass_fields__`

```py
from dataclasses import make_dataclass, Field

Point6 = make_dataclass("Point6", [("x", int), ("y", int)])

reveal_type(Point6.__dataclass_fields__)  # revealed: dict[str, Field[Any]]
```

## Base classes

The `bases` keyword argument specifies base classes:

```py
from dataclasses import make_dataclass

class Base:
    def greet(self) -> str:
        return "Hello"

Derived = make_dataclass("Derived", [("value", int)], bases=(Base,))

d = Derived(42)
reveal_type(d)  # revealed: Derived
reveal_type(d.value)  # revealed: int
reveal_type(d.greet())  # revealed: str
```

## Dynamic fields (unknown fields)

When the fields argument is dynamic (not a literal), we fall back to gradual typing.

```py
from dataclasses import make_dataclass

def get_fields():
    return [("x", int)]

fields = get_fields()
PointDynamic = make_dataclass("PointDynamic", fields)

p = PointDynamic(1)  # No error - accepts any arguments
reveal_type(p.x)  # revealed: Any
reveal_type(p.unknown)  # revealed: Any
```

## Argument validation

### Too many positional arguments

Only `cls_name` and `fields` are positional arguments:

```py
from dataclasses import make_dataclass

# error: [too-many-positional-arguments] "Too many positional arguments to function `make_dataclass`: expected 2, got 3"
Point = make_dataclass("Point", [("x", int)], (object,))
```

### Unknown keyword argument

```py
from dataclasses import make_dataclass

# error: [unknown-argument] "Argument `unknown` does not match any known parameter of function `make_dataclass`"
Point = make_dataclass("Point", [("x", int)], unknown=True)
```

### Invalid type for `cls_name`

```py
from dataclasses import make_dataclass

# error: [invalid-argument-type] "Invalid argument to parameter `cls_name` of `make_dataclass()`"
Point = make_dataclass(123, [("x", int)])
```

### Invalid type for boolean parameters

```py
from dataclasses import make_dataclass

# error: [invalid-argument-type] "Invalid argument to parameter `init` of `make_dataclass()`"
C1 = make_dataclass("C1", [("x", int)], init="yes")

# error: [invalid-argument-type] "Invalid argument to parameter `repr` of `make_dataclass()`"
C2 = make_dataclass("C2", [("x", int)], repr="no")

# error: [invalid-argument-type] "Invalid argument to parameter `eq` of `make_dataclass()`"
C3 = make_dataclass("C3", [("x", int)], eq=None)

# error: [invalid-argument-type] "Invalid argument to parameter `order` of `make_dataclass()`"
C4 = make_dataclass("C4", [("x", int)], order=1)

# error: [invalid-argument-type] "Invalid argument to parameter `frozen` of `make_dataclass()`"
C5 = make_dataclass("C5", [("x", int)], frozen="true")

# error: [invalid-argument-type] "Invalid argument to parameter `kw_only` of `make_dataclass()`"
C6 = make_dataclass("C6", [("x", int)], kw_only=[])
```

### Invalid type for `namespace`

```py
from dataclasses import make_dataclass

# error: [invalid-argument-type] "Invalid argument to parameter `namespace` of `make_dataclass()`"
Point = make_dataclass("Point", [("x", int)], namespace="invalid")
```

### Invalid type for `module`

```py
from dataclasses import make_dataclass

# error: [invalid-argument-type] "Invalid argument to parameter `module` of `make_dataclass()`"
Point = make_dataclass("Point", [("x", int)], module=123)
```

### Valid `namespace` and `module`

```py
from dataclasses import make_dataclass

# These are all valid
Point1 = make_dataclass("Point1", [("x", int)], namespace=None)
Point2 = make_dataclass("Point2", [("x", int)], namespace={"custom_attr": 42})
Point3 = make_dataclass("Point3", [("x", int)], module=None)
Point4 = make_dataclass("Point4", [("x", int)], module="my_module")
```

## Invalid bases

### TypedDict and Generic bases

These special forms are not allowed as bases for classes created via `make_dataclass()`.

```py
from dataclasses import make_dataclass
from typing import TypedDict, Generic

# error: [invalid-base] "Invalid base for class created via `make_dataclass()`"
A = make_dataclass("A", [("x", int)], bases=(TypedDict,))

# error: [invalid-base] "Invalid base for class created via `make_dataclass()`"
B = make_dataclass("B", [("x", int)], bases=(Generic,))
```

### Protocol base

Protocol bases use a different lint (`unsupported-dynamic-base`) because they're technically valid
Python but not supported by ty for MRO computation.

```py
from dataclasses import make_dataclass
from typing import Protocol

# error: [unsupported-dynamic-base] "Unsupported base for class created via `make_dataclass()`"
C = make_dataclass("C", [("x", int)], bases=(Protocol,))
```

### Final class base

Cannot inherit from a `@final` class.

```py
from dataclasses import make_dataclass
from typing import final

@final
class FinalClass:
    pass

# error: [subclass-of-final-class] "Class `D` cannot inherit from final class `FinalClass`"
D = make_dataclass("D", [("x", int)], bases=(FinalClass,))
```

### Enum base

Creating an enum class via `make_dataclass()` is not supported.

```py
from dataclasses import make_dataclass
from enum import Enum

# error: [invalid-base] "Invalid base for class created via `make_dataclass()`"
E = make_dataclass("E", [("x", int)], bases=(Enum,))
```
