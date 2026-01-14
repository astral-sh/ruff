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
from ty_extensions import reveal_mro

Point2 = make_dataclass("Point2", [("x", int), ("y", int)])

reveal_type(Point2)  # revealed: <class 'Point2'>
reveal_mro(Point2)  # revealed: (<class 'Point2'>, <class 'object'>)
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

## Fields with `field()` defaults

Using `dataclasses.field()` as the third element of a 3-tuple:

```py
from dataclasses import make_dataclass, field

PointWithField = make_dataclass(
    "PointWithField",
    [
        ("x", int),
        ("y", int, field(default=0)),
        ("z", list, field(default_factory=list)),
    ],
)

# error: [missing-argument] "No argument provided for required parameter `x`"
PointWithField()

# Good - y and z have defaults
p1 = PointWithField(1)
p2 = PointWithField(1, 2)
p3 = PointWithField(1, 2, [3])

reveal_type(p1.x)  # revealed: int
reveal_type(p1.y)  # revealed: int
reveal_type(p1.z)  # revealed: list[Unknown]
```

## Fields with `init=False` via `field()`

```py
from dataclasses import make_dataclass, field

PointPartialInit = make_dataclass(
    "PointPartialInit",
    [
        ("x", int),
        ("y", int, field(init=False, default=0)),
    ],
)

# Only x is in __init__
p = PointPartialInit(1)
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int

# error: [unknown-argument] "Argument `y` does not match any known parameter"
PointPartialInit(1, y=2)
```

## Fields with `kw_only=True` via `field()`

```py
from dataclasses import make_dataclass, field

PointKwOnlyField = make_dataclass(
    "PointKwOnlyField",
    [
        ("x", int),
        ("y", int, field(kw_only=True)),
    ],
)

# x is positional, y is keyword-only
p1 = PointKwOnlyField(1, y=2)
reveal_type(p1.x)  # revealed: int
reveal_type(p1.y)  # revealed: int

# error: [missing-argument] "No argument provided for required parameter `y`"
# error: [too-many-positional-arguments] "Too many positional arguments: expected 1, got 2"
PointKwOnlyField(1, 2)

reveal_type(PointKwOnlyField.__init__)  # revealed: (self: PointKwOnlyField, x: int, *, y: int) -> None
```

## Fields with `kw_only=False` overriding class-level `kw_only=True`

Per-field `kw_only=False` overrides the class-level default:

```py
from dataclasses import make_dataclass, field

MixedKwOnly = make_dataclass(
    "MixedKwOnly",
    [
        ("x", int, field(kw_only=False)),  # Override: positional
        ("y", int),  # Uses class default: keyword-only
    ],
    kw_only=True,  # Default all fields to keyword-only
)

# x is positional (overridden), y is keyword-only (class default)
p1 = MixedKwOnly(1, y=2)
reveal_type(p1.x)  # revealed: int
reveal_type(p1.y)  # revealed: int

reveal_type(MixedKwOnly.__init__)  # revealed: (self: MixedKwOnly, x: int, *, y: int) -> None
```

## Fields with combined `field()` options

```py
from dataclasses import make_dataclass, field

ComplexFields = make_dataclass(
    "ComplexFields",
    [
        ("required", int),
        ("with_default", int, field(default=10)),
        ("with_factory", list, field(default_factory=list)),
        ("kw_with_default", str, field(kw_only=True, default="hello")),
    ],
)

# Only 'required' is required; others have defaults
c1 = ComplexFields(1)
c2 = ComplexFields(1, 20)
c3 = ComplexFields(1, 20, [1, 2, 3])
c4 = ComplexFields(1, kw_with_default="world")

reveal_type(c1.required)  # revealed: int
reveal_type(c1.with_default)  # revealed: int
reveal_type(c1.with_factory)  # revealed: list[Unknown]
reveal_type(c1.kw_with_default)  # revealed: str

# fmt: off
reveal_type(ComplexFields.__init__)  # revealed: (self: ComplexFields, required: int, with_default: int = 10, with_factory: list[Unknown] = ..., *, kw_with_default: str = "hello") -> None
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

## Dataclass parameters

### `init=False`

```py
from dataclasses import make_dataclass

PointNoInit = make_dataclass("PointNoInit", [("x", int), ("y", int)], init=False)

# error: [too-many-positional-arguments]
p = PointNoInit(1, 2)
```

### `repr=False`

```py
from dataclasses import make_dataclass

PointNoRepr = make_dataclass("PointNoRepr", [("x", int), ("y", int)], repr=False)

p = PointNoRepr(1, 2)
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int

# The class is still created and usable, repr=False just affects __repr__
reveal_type(p)  # revealed: PointNoRepr
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

### `total_ordering` with `order=True`

Using `total_ordering` on a dataclass with `order=True` is redundant since the comparison methods
are already synthesized. However, this doesn't cause an error:

```py
from dataclasses import make_dataclass
from functools import total_ordering

# No error - but this is redundant since order=True already provides comparison methods
PointOrdered = total_ordering(make_dataclass("PointOrdered", [("x", int)], order=True))

p1 = PointOrdered(1)
p2 = PointOrdered(2)
reveal_type(p1 < p2)  # revealed: bool
```

### `total_ordering` without `order=True`

Using `total_ordering` on a dataclass without `order=True` requires at least one ordering method to
be defined. Since `make_dataclass` with `order=False` doesn't synthesize any comparison methods,
this results in an error:

```py
from dataclasses import make_dataclass
from functools import total_ordering

# error: [invalid-total-ordering] "`@functools.total_ordering` requires at least one ordering method (`__lt__`, `__le__`, `__gt__`, or `__ge__`) to be defined: `PointNoOrder` does not define `__lt__`, `__le__`, `__gt__`, or `__ge__`"
PointNoOrder = total_ordering(make_dataclass("PointNoOrder", [("x", int)], order=False))
```

### `frozen=True`

```py
from dataclasses import make_dataclass

PointFrozen = make_dataclass("PointFrozen", [("x", int), ("y", int)], frozen=True)

p = PointFrozen(1, 2)

# frozen dataclasses generate __hash__
reveal_type(hash(p))  # revealed: int

# frozen dataclasses are immutable
p.x = 42  # error: [invalid-assignment]
p.y = 56  # error: [invalid-assignment]
```

### `unsafe_hash=True`

```py
from dataclasses import make_dataclass

PointUnsafeHash = make_dataclass("PointUnsafeHash", [("x", int), ("y", int)], unsafe_hash=True)

p = PointUnsafeHash(1, 2)

# unsafe_hash=True generates __hash__ even without frozen=True
reveal_type(hash(p))  # revealed: int
```

### `eq=True` without `frozen=True` sets `__hash__` to `None`

```py
from dataclasses import make_dataclass

# By default, eq=True and frozen=False, which sets __hash__ to None
PointDefaultHash = make_dataclass("PointDefaultHash", [("x", int)])

p = PointDefaultHash(1)

# __hash__ is None, so hash() fails at runtime
reveal_type(PointDefaultHash.__hash__)  # revealed: None
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

### `match_args=False`

```py
from dataclasses import make_dataclass

PointNoMatchArgs = make_dataclass("PointNoMatchArgs", [("x", int), ("y", int)], match_args=False)

# error: [unresolved-attribute] "Class `PointNoMatchArgs` has no attribute `__match_args__`"
reveal_type(PointNoMatchArgs.__match_args__)  # revealed: Unknown
```

### `slots=True`

Functional dataclasses with `slots=True` and non-empty fields are understood as disjoint bases,
causing an `instance-layout-conflict` error when combined with other slotted classes:

```py
from dataclasses import make_dataclass

PointSlots = make_dataclass("PointSlots", [("x", int), ("y", int)], slots=True)

p = PointSlots(1, 2)
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int

# Combining two slotted classes with non-empty __slots__ causes a layout conflict
OtherSlots = make_dataclass("OtherSlots", [("z", int)], slots=True)

class Combined(PointSlots, OtherSlots): ...  # error: [instance-layout-conflict]

# Empty slots are fine
EmptySlots = make_dataclass("EmptySlots", [], slots=True)

class CombinedWithEmpty(PointSlots, EmptySlots): ...  # No error
```

### `weakref_slot=True`

The `weakref_slot` parameter (Python 3.11+) adds a `__weakref__` slot when combined with
`slots=True`:

```toml
[environment]
python-version = "3.11"
```

```py
from dataclasses import make_dataclass
import weakref

PointWeakref = make_dataclass("PointWeakref", [("x", int)], slots=True, weakref_slot=True)

p = PointWeakref(1)
reveal_type(p.x)  # revealed: int

# __weakref__ attribute is available
reveal_type(p.__weakref__)  # revealed: Any | None
```

### Combining multiple flags

Multiple flags can be combined:

```py
from dataclasses import make_dataclass

# frozen=True enables hashing and order=True enables comparisons
PointFrozenOrdered = make_dataclass(
    "PointFrozenOrdered",
    [("x", int), ("y", int)],
    frozen=True,
    order=True,
)

p1 = PointFrozenOrdered(1, 2)
p2 = PointFrozenOrdered(3, 4)

# frozen dataclasses are hashable
reveal_type(hash(p1))  # revealed: int

# order=True enables comparisons
reveal_type(p1 < p2)  # revealed: bool
reveal_type(p1 <= p2)  # revealed: bool
reveal_type(p1 > p2)  # revealed: bool
reveal_type(p1 >= p2)  # revealed: bool

# frozen dataclasses are immutable
p1.x = 42  # error: [invalid-assignment]
```

### `slots=True` with `frozen=True`

```py
from dataclasses import make_dataclass

SlottedFrozen = make_dataclass(
    "SlottedFrozen",
    [("x", int)],
    slots=True,
    frozen=True,
)

p = SlottedFrozen(1)
reveal_type(hash(p))  # revealed: int

# Frozen, so immutable
p.x = 42  # error: [invalid-assignment]
```

### `kw_only=True` with `frozen=True`

```py
from dataclasses import make_dataclass

KwOnlyFrozen = make_dataclass(
    "KwOnlyFrozen",
    [("x", int), ("y", int)],
    kw_only=True,
    frozen=True,
)

# All arguments must be keyword-only
p = KwOnlyFrozen(x=1, y=2)
reveal_type(hash(p))  # revealed: int

# error: [missing-argument] "No arguments provided for required parameters `x`, `y`"
# error: [too-many-positional-arguments]
KwOnlyFrozen(1, 2)
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
from ty_extensions import reveal_mro

class Base:
    def greet(self) -> str:
        return "Hello"

Derived = make_dataclass("Derived", [("value", int)], bases=(Base,))
reveal_mro(Derived)  # revealed: (<class 'Derived'>, <class 'Base'>, <class 'object'>)

d = Derived(42)
reveal_type(d)  # revealed: Derived
reveal_type(d.value)  # revealed: int
reveal_type(d.greet())  # revealed: str
```

## Dynamic fields (unknown fields)

When the fields argument is dynamic (not a literal), we fall back to gradual typing.

```py
from dataclasses import make_dataclass
from ty_extensions import reveal_mro

def get_fields():
    return [("x", int)]

fields = get_fields()
PointDynamic = make_dataclass("PointDynamic", fields)

p = PointDynamic(1)  # No error - accepts any arguments
reveal_type(p.x)  # revealed: Any

# The class is still inferred as inheriting directly from `object`
# (`Unknown` is not inserted into the MRO)
reveal_mro(PointDynamic)  # revealed: (<class 'PointDynamic'>, <class 'object'>)

# ...but nonetheless, we assume that all attributes are available,
# similar to attribute access on `Unknown`
reveal_type(p.unknown)  # revealed: Any
```

## Starred arguments

When `*args` or `**kwargs` are used, we can't statically determine the arguments, so we fall back to
gradual typing.

```py
from dataclasses import make_dataclass

args = ("Point", [("x", int)])
PointStarred = make_dataclass(*args)

p = PointStarred(1)  # No error - accepts any arguments
reveal_type(p.x)  # revealed: Unknown

kwargs = {"cls_name": "Point2", "fields": [("y", str)]}
Point2 = make_dataclass(**kwargs)

p2 = Point2("hello")  # No error - accepts any arguments
reveal_type(p2.y)  # revealed: Unknown
```

## Argument validation

### Too few positional arguments

Both `cls_name` and `fields` are required:

```py
from dataclasses import make_dataclass

# error: [missing-argument] "No argument provided for required parameter `fields` of function `make_dataclass`"
Point = make_dataclass("Point")
```

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

# error: [invalid-argument-type] "Invalid argument to parameter `unsafe_hash` of `make_dataclass()`"
C7 = make_dataclass("C7", [("x", int)], unsafe_hash="yes")

# error: [invalid-argument-type] "Invalid argument to parameter `match_args` of `make_dataclass()`"
C8 = make_dataclass("C8", [("x", int)], match_args=1)

# error: [invalid-argument-type] "Invalid argument to parameter `slots` of `make_dataclass()`"
C9 = make_dataclass("C9", [("x", int)], slots="yes")

# error: [invalid-argument-type] "Invalid argument to parameter `weakref_slot` of `make_dataclass()`"
C10 = make_dataclass("C10", [("x", int)], weakref_slot=None)
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

### Invalid type for `bases`

At runtime, `make_dataclass` requires `bases` to be a tuple (not a list or other iterable).

```py
from dataclasses import make_dataclass

# error: [invalid-argument-type] "Invalid argument to parameter `bases` of `make_dataclass()`: Expected `tuple`, found `Literal[12345]`"
Point1 = make_dataclass("Point1", [("x", int)], bases=12345)

# error: [invalid-argument-type] "Invalid argument to parameter `bases` of `make_dataclass()`: Expected `tuple`, found `list[Unknown | <class 'object'>]`"
Point2 = make_dataclass("Point2", [("x", int)], bases=[object])
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
Python but not supported by ty for dynamic classes.

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

## Deferred evaluation

### String annotations (forward references)

String annotations (forward references) are properly evaluated to types:

```py
from dataclasses import make_dataclass

Point = make_dataclass("Point", [("x", "int"), ("y", "int")])
p = Point(1, 2)

reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int
```

### Recursive references

Recursive references in functional syntax are supported:

```py
from dataclasses import make_dataclass

Node = make_dataclass("Node", [("value", int), ("next", "Node | None")])
n = Node(1, None)

reveal_type(n.value)  # revealed: int
reveal_type(n.next)  # revealed: Node | None
```

### Mutually recursive types

Mutually recursive types work correctly:

```py
from dataclasses import make_dataclass

A = make_dataclass("A", [("x", "B | None")])
B = make_dataclass("B", [("x", "C")])
C = make_dataclass("C", [("x", A)])

a = A(x=B(x=C(x=A(x=None))))

reveal_type(a.x)  # revealed: B | None

if a.x is not None:
    reveal_type(a.x)  # revealed: B
    reveal_type(a.x.x)  # revealed: C
    reveal_type(a.x.x.x)  # revealed: A
    reveal_type(a.x.x.x.x)  # revealed: B | None

A(x=42)  # error: [invalid-argument-type]

# error: [invalid-argument-type]
# error: [missing-argument]
A(x=C())

# error: [invalid-argument-type]
A(x=C(x=A(x=None)))
```

### Complex recursive type with generics

String annotations work with generic types:

```py
from dataclasses import make_dataclass

TreeNode = make_dataclass("TreeNode", [("value", int), ("children", "list[TreeNode]")])

t = TreeNode(1, [])
reveal_type(t.value)  # revealed: int
reveal_type(t.children)  # revealed: list[TreeNode]
```

### make_dataclass as base class with forward references

When `make_dataclass` is used as a base class for a static class, forward references to the outer
class are resolved:

```py
from dataclasses import make_dataclass

class X(make_dataclass("XBase", [("next", "X | None")])):
    pass

x = X(next=None)
reveal_type(x.next)  # revealed: X | None

# Recursive construction works
x2 = X(next=X(next=None))
reveal_type(x2.next)  # revealed: X | None
```

## Edge cases

### Empty fields list

A dataclass with no fields is valid:

```py
from dataclasses import make_dataclass

Empty = make_dataclass("Empty", [])

e = Empty()
reveal_type(e)  # revealed: Empty

# No fields, so __init__ takes no arguments
# error: [too-many-positional-arguments]
Empty(1)
```

### Equality methods with `eq=True` (default)

```py
from dataclasses import make_dataclass

PointEq = make_dataclass("PointEq", [("x", int), ("y", int)])

p1 = PointEq(1, 2)
p2 = PointEq(1, 2)
p3 = PointEq(3, 4)

# __eq__ is synthesized
reveal_type(p1 == p2)  # revealed: bool
reveal_type(p1 == p3)  # revealed: bool

# __ne__ is also available (from object, but works correctly)
reveal_type(p1 != p2)  # revealed: bool
```

### `namespace` parameter

The `namespace` parameter allows adding custom attributes/methods to the class.

```py
from dataclasses import make_dataclass

def custom_method(self) -> str:
    return f"Point({self.x}, {self.y})"

PointWithMethod = make_dataclass(
    "PointWithMethod",
    [("x", int), ("y", int)],
    namespace={"describe": custom_method, "version": 1},
)

p = PointWithMethod(1, 2)
reveal_type(p.x)  # revealed: int

# TODO: We don't currently track namespace additions, so these are unresolved
# error: [unresolved-attribute]
p.describe()

# error: [unresolved-attribute]
reveal_type(p.version)  # revealed: Unknown
```

### Single field

```py
from dataclasses import make_dataclass

Single = make_dataclass("Single", [("value", int)])

s = Single(42)
reveal_type(s.value)  # revealed: int
reveal_type(Single.__init__)  # revealed: (self: Single, value: int) -> None
```

### Many fields

```py
from dataclasses import make_dataclass

ManyFields = make_dataclass(
    "ManyFields",
    [
        ("a", int),
        ("b", str),
        ("c", float),
        ("d", bool),
        ("e", list),
    ],
)

m = ManyFields(1, "hello", 3.14, True, [])
reveal_type(m.a)  # revealed: int
reveal_type(m.b)  # revealed: str
reveal_type(m.c)  # revealed: int | float
reveal_type(m.d)  # revealed: bool
reveal_type(m.e)  # revealed: list[Unknown]
```
