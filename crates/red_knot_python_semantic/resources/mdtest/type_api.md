# Type API

This document describes the internal `red_knot` API for creating and manipulating types as well as
testing various type system properties.

## Type extensions

The Python language itself allows us to perform a variety of operations on types. For example, we
can build a union of types like `int | None`, or we can use type constructors such as `list[int]`
and `type[int]` to create new types. But some type-level operations that we rely on in Red Knot,
like intersections, cannot yet be expressed in Python. The `red_knot` module provides the
`Intersection` and `Not` type constructors (special forms) which allow us to construct these types
directly.

### Negation

```py
from red_knot import Not

x: Not[int]
y: Not[Not[int]]
z: Not[Not[Not[int]]]

not_one: Not[Literal[1]]

def _() -> None:
    reveal_type(x)  # revealed: ~int
    reveal_type(y)  # revealed: int
    reveal_type(z)  # revealed: ~int

    reveal_type(not_one == 1)  # revealed: Literal[False]
    reveal_type(not_one != 1)  # revealed: Literal[True]
```

### Intersection

```py
from red_knot import Intersection, Not, is_subtype_of, static_assert
from typing_extensions import Never

class S: ...
class T: ...

x1: Intersection[S, T]
x2: Intersection[S, Not[T]]

def x() -> None:
    reveal_type(x1)  # revealed: S & T
    reveal_type(x2)  # revealed: S & ~T

y1: Intersection[int, object]
y2: Intersection[int, bool]
y3: Intersection[int, Never]

def y() -> None:
    reveal_type(y1)  # revealed: int
    reveal_type(y2)  # revealed: bool
    reveal_type(y3)  # revealed: Never

z1: Intersection[int, Not[Literal[1]], Not[Literal[2]]]

def z() -> None:
    reveal_type(z1)  # revealed: int & ~Literal[1] & ~Literal[2]

class A: ...
class B: ...
class C: ...

type ABC = Intersection[A, B, C]

static_assert(is_subtype_of(ABC, A))
static_assert(is_subtype_of(ABC, B))
static_assert(is_subtype_of(ABC, C))

class D: ...

static_assert(not is_subtype_of(ABC, D))
```

### Unknown type

The `Unknown` type is a special type that we use to represent actually unknown types (no
annotation), as opposed to `Any` which represents an explicitly unknown type.

```py
from red_knot import Unknown, static_assert, is_assignable_to, is_fully_static

static_assert(is_assignable_to(Unknown, int))
static_assert(is_assignable_to(int, Unknown))

static_assert(not is_fully_static(Unknown))

x: Unknown = 1

def _() -> None:
    reveal_type(x)  # revealed: Unknown
```

## Type predicates

The `red_knot` module also provides predicates to test various properties of types. These are
implemented as functions that return `Literal[True]` or `Literal[False]` depending on the result of
the test.

### Equivalence

```py
from red_knot import is_equivalent_to, static_assert
from typing_extensions import Never, Union

static_assert(is_equivalent_to(type, type[object]))
static_assert(is_equivalent_to(tuple[int, Never], Never))
static_assert(is_equivalent_to(int | str, Union[int, str]))

static_assert(not is_equivalent_to(int, str))
static_assert(not is_equivalent_to(int | str, int | str | bytes))
```

### Subtyping

```py
from red_knot import is_subtype_of, static_assert

static_assert(is_subtype_of(bool, int))
static_assert(not is_subtype_of(str, int))

static_assert(is_subtype_of(bool, int | str))
static_assert(is_subtype_of(str, int | str))
static_assert(not is_subtype_of(bytes, int | str))

class Base: ...
class Derived(Base): ...
class Unrelated: ...

static_assert(is_subtype_of(Derived, Base))
static_assert(not is_subtype_of(Base, Derived))
static_assert(is_subtype_of(Base, Base))

static_assert(not is_subtype_of(Unrelated, Base))
static_assert(not is_subtype_of(Base, Unrelated))
```

### Assignability

```py
from red_knot import is_assignable_to, static_assert
from typing import Any

static_assert(is_assignable_to(int, Any))
static_assert(is_assignable_to(Any, str))
static_assert(not is_assignable_to(int, str))
```

### Disjointness

```py
from red_knot import is_disjoint_from, static_assert

static_assert(is_disjoint_from(None, int))
static_assert(not is_disjoint_from(Literal[2] | str, int))
```

### Fully static types

```py
from red_knot import is_fully_static, static_assert
from typing import Any

static_assert(is_fully_static(int | str))
static_assert(is_fully_static(type[int]))

static_assert(not is_fully_static(int | Any))
static_assert(not is_fully_static(type[Any]))
```

### Singleton types

```py
from red_knot import is_singleton, static_assert

static_assert(is_singleton(None))
static_assert(is_singleton(Literal[True]))

static_assert(not is_singleton(int))
static_assert(not is_singleton(Literal["a"]))
```

### Single-valued types

```py
from red_knot import is_single_valued, static_assert

static_assert(is_single_valued(None))
static_assert(is_single_valued(Literal[True]))
static_assert(is_single_valued(Literal["a"]))

static_assert(not is_single_valued(int))
static_assert(not is_single_valued(Literal["a"] | Literal["b"]))
```

## `TypeOf`

We use `TypeOf` to get the inferred type of an expression. This is useful when we want to refer to
it in a type expression. For example, if we want to make sure that the class literal type `str` is a
subtype of `type[str]`, we can not use `is_subtype_of(str, type[str])`, as that would test if the
type `str` itself is a subtype of `type[str]`. Instead, we can use `TypeOf[str]` to get the type of
the expression `str`:

```py
from red_knot import TypeOf, is_subtype_of, static_assert

# This is incorrect and therefore fails with ...
# error: "Static assertion error: argument evaluates to `False`"
static_assert(is_subtype_of(str, type[str]))

# Correct, returns True:
static_assert(is_subtype_of(TypeOf[str], type[str]))
```

## Error handling

### Failed assertions

We provide various tailored error messages for wrong argument types to `static_assert`:

```py
from red_knot import static_assert

static_assert(2 * 3 == 6)

# error: "Static assertion error: argument evaluates to `False`"
static_assert(2 * 3 == 7)

# error: "Static assertion error: argument does not have a statically known truthiness (type is `bool`)"
static_assert(int(2.0 * 3.0) == 6)

# error: "Static assertion error: expected argument type `Literal[True]`, got: `Literal[6]`."
static_assert(2 * 3)
```

### Wrong number of arguments for type predicates

```py
from red_knot import is_subtype_of, is_fully_static

# error: "Expected 2 arguments, got 0"
is_subtype_of()

# error: "Expected 2 arguments, got 1"
is_subtype_of(int)

# error: "Expected 2 arguments, got 3"
is_subtype_of(int, int, int)

# error: "Expected 2 arguments, got 4"
is_subtype_of(int, int, int, int)

# error: "Expected 1 argument, got 0"
is_fully_static()

# error: "Expected 1 argument, got 2"
is_fully_static(int, int)
```

### Wrong argument number for types and type constructors

```py
from red_knot import Not, Unknown, TypeOf

# error: "Special form `red_knot.Unknown` expected no type parameter"
u: Unknown[str]

# error: "Expected 1 type argument, got 2"
n: Not[int, str]

# error: "Expected 1 type argument, got 3"
t: TypeOf[int, str, bytes]
```
