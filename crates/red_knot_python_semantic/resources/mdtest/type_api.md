# Type API (`knot_extensions`)

This document describes the internal `knot_extensions` API for creating and manipulating types as
well as testing various type system properties.

## Type extensions

The Python language itself allows us to perform a variety of operations on types. For example, we
can build a union of types like `int | None`, or we can use type constructors such as `list[int]`
and `type[int]` to create new types. But some type-level operations that we rely on in Red Knot,
like intersections, cannot yet be expressed in Python. The `knot_extensions` module provides the
`Intersection` and `Not` type constructors (special forms) which allow us to construct these types
directly.

### Negation

```py
from typing import Literal
from knot_extensions import Not, static_assert

def negate(n1: Not[int], n2: Not[Not[int]], n3: Not[Not[Not[int]]]) -> None:
    reveal_type(n1)  # revealed: ~int
    reveal_type(n2)  # revealed: int
    reveal_type(n3)  # revealed: ~int

def static_truthiness(not_one: Not[Literal[1]]) -> None:
    static_assert(not_one != 1)
    static_assert(not (not_one == 1))

# error: "Special form `knot_extensions.Not` expected exactly one type parameter"
n: Not[int, str]
```

### Intersection

```py
from knot_extensions import Intersection, Not, is_subtype_of, static_assert
from typing_extensions import Literal, Never

class S: ...
class T: ...

def x(x1: Intersection[S, T], x2: Intersection[S, Not[T]]) -> None:
    reveal_type(x1)  # revealed: S & T
    reveal_type(x2)  # revealed: S & ~T

def y(y1: Intersection[int, object], y2: Intersection[int, bool], y3: Intersection[int, Never]) -> None:
    reveal_type(y1)  # revealed: int
    reveal_type(y2)  # revealed: bool
    reveal_type(y3)  # revealed: Never

def z(z1: Intersection[int, Not[Literal[1]], Not[Literal[2]]]) -> None:
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
from knot_extensions import Unknown, static_assert, is_assignable_to, is_fully_static

static_assert(is_assignable_to(Unknown, int))
static_assert(is_assignable_to(int, Unknown))

static_assert(not is_fully_static(Unknown))

def explicit_unknown(x: Unknown, y: tuple[str, Unknown], z: Unknown = 1) -> None:
    reveal_type(x)  # revealed: Unknown
    reveal_type(y)  # revealed: tuple[str, Unknown]
    reveal_type(z)  # revealed: Unknown | Literal[1]
```

`Unknown` can be subclassed, just like `Any`:

```py
class C(Unknown): ...

# revealed: tuple[Literal[C], Unknown, Literal[object]]
reveal_type(C.__mro__)

# error: "Special form `knot_extensions.Unknown` expected no type parameter"
u: Unknown[str]
```

### `AlwaysTruthy` and `AlwaysFalsy`

`AlwaysTruthy` and `AlwaysFalsy` represent the sets of all possible objects whose truthiness is
always truthy or falsy, respectively.

They do not accept any type arguments.

```py
from typing_extensions import Literal

from knot_extensions import AlwaysFalsy, AlwaysTruthy, is_subtype_of, static_assert

static_assert(is_subtype_of(Literal[True], AlwaysTruthy))
static_assert(is_subtype_of(Literal[False], AlwaysFalsy))

static_assert(not is_subtype_of(int, AlwaysFalsy))
static_assert(not is_subtype_of(str, AlwaysFalsy))

def _(t: AlwaysTruthy, f: AlwaysFalsy):
    reveal_type(t)  # revealed: AlwaysTruthy
    reveal_type(f)  # revealed: AlwaysFalsy

def f(
    a: AlwaysTruthy[int],  # error: [invalid-type-form]
    b: AlwaysFalsy[str],  # error: [invalid-type-form]
):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
```

## Static assertions

### Basics

The `knot_extensions` module provides a `static_assert` function that can be used to enforce
properties at type-check time. The function takes an arbitrary expression and raises a type error if
the expression is not of statically known truthiness.

```py
from knot_extensions import static_assert
from typing import TYPE_CHECKING
import sys

static_assert(True)
static_assert(False)  # error: "Static assertion error: argument evaluates to `False`"

static_assert(False or True)
static_assert(True and True)
static_assert(False or False)  # error: "Static assertion error: argument evaluates to `False`"
static_assert(False and True)  # error: "Static assertion error: argument evaluates to `False`"

static_assert(1 + 1 == 2)
static_assert(1 + 1 == 3)  # error: "Static assertion error: argument evaluates to `False`"

static_assert("a" in "abc")
static_assert("d" in "abc")  # error: "Static assertion error: argument evaluates to `False`"

n = None
static_assert(n is None)

static_assert(TYPE_CHECKING)

static_assert(sys.version_info >= (3, 6))
```

### Narrowing constraints

Static assertions can be used to enforce narrowing constraints:

```py
from knot_extensions import static_assert

def f(x: int) -> None:
    if x != 0:
        static_assert(x != 0)
    else:
        # `int` can be subclassed, so we cannot assert that `x == 0` here:
        # error: "Static assertion error: argument of type `bool` has an ambiguous static truthiness"
        static_assert(x == 0)
```

### Truthy expressions

See also: <https://docs.python.org/3/library/stdtypes.html#truth-value-testing>

```py
from knot_extensions import static_assert

static_assert(True)
static_assert(False)  # error: "Static assertion error: argument evaluates to `False`"

static_assert(None)  # error: "Static assertion error: argument of type `None` is statically known to be falsy"

static_assert(1)
static_assert(0)  # error: "Static assertion error: argument of type `Literal[0]` is statically known to be falsy"

static_assert((0,))
static_assert(())  # error: "Static assertion error: argument of type `tuple[()]` is statically known to be falsy"

static_assert("a")
static_assert("")  # error: "Static assertion error: argument of type `Literal[""]` is statically known to be falsy"

static_assert(b"a")
static_assert(b"")  # error: "Static assertion error: argument of type `Literal[b""]` is statically known to be falsy"
```

### Error messages

We provide various tailored error messages for wrong argument types to `static_assert`:

```py
from knot_extensions import static_assert

static_assert(2 * 3 == 6)

# error: "Static assertion error: argument evaluates to `False`"
static_assert(2 * 3 == 7)

# error: "Static assertion error: argument of type `bool` has an ambiguous static truthiness"
static_assert(int(2.0 * 3.0) == 6)

class InvalidBoolDunder:
    def __bool__(self) -> int:
        return 1

# error: [unsupported-bool-conversion]  "Boolean conversion is unsupported for type `InvalidBoolDunder`; the return type of its bool method (`int`) isn't assignable to `bool"
static_assert(InvalidBoolDunder())
```

### Custom error messages

Alternatively, users can provide custom error messages:

```py
from knot_extensions import static_assert

# error: "Static assertion error: I really want this to be true"
static_assert(1 + 1 == 3, "I really want this to be true")

error_message = "A custom message "
error_message += "constructed from multiple string literals"
# error: "Static assertion error: A custom message constructed from multiple string literals"
static_assert(False, error_message)
```

There are limitations to what we can still infer as a string literal. In those cases, we simply fall
back to the default message:

```py
shouted_message = "A custom message".upper()
# error: "Static assertion error: argument evaluates to `False`"
static_assert(False, shouted_message)
```

## Type predicates

The `knot_extensions` module also provides predicates to test various properties of types. These are
implemented as functions that return `Literal[True]` or `Literal[False]` depending on the result of
the test.

### Equivalence

```py
from knot_extensions import is_equivalent_to, static_assert
from typing_extensions import Never, Union

static_assert(is_equivalent_to(type, type[object]))
static_assert(is_equivalent_to(tuple[int, Never], Never))
static_assert(is_equivalent_to(int | str, Union[int, str]))

static_assert(not is_equivalent_to(int, str))
static_assert(not is_equivalent_to(int | str, int | str | bytes))
```

### Subtyping

```py
from knot_extensions import is_subtype_of, static_assert

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
from knot_extensions import is_assignable_to, static_assert
from typing import Any

static_assert(is_assignable_to(int, Any))
static_assert(is_assignable_to(Any, str))
static_assert(not is_assignable_to(int, str))
```

### Disjointness

```py
from knot_extensions import is_disjoint_from, static_assert
from typing import Literal

static_assert(is_disjoint_from(None, int))
static_assert(not is_disjoint_from(Literal[2] | str, int))
```

### Fully static types

```py
from knot_extensions import is_fully_static, static_assert
from typing import Any

static_assert(is_fully_static(int | str))
static_assert(is_fully_static(type[int]))

static_assert(not is_fully_static(int | Any))
static_assert(not is_fully_static(type[Any]))
```

### Singleton types

```py
from knot_extensions import is_singleton, static_assert
from typing import Literal

static_assert(is_singleton(None))
static_assert(is_singleton(Literal[True]))

static_assert(not is_singleton(int))
static_assert(not is_singleton(Literal["a"]))
```

### Single-valued types

```py
from knot_extensions import is_single_valued, static_assert
from typing import Literal

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
from knot_extensions import TypeOf, is_subtype_of, static_assert

# This is incorrect and therefore fails with ...
# error: "Static assertion error: argument evaluates to `False`"
static_assert(is_subtype_of(str, type[str]))

# Correct, returns True:
static_assert(is_subtype_of(TypeOf[str], type[str]))

class Base: ...
class Derived(Base): ...
```

`TypeOf` can also be used in annotations:

```py
def type_of_annotation() -> None:
    t1: TypeOf[Base] = Base
    t2: TypeOf[Base] = Derived  # error: [invalid-assignment]

    # Note how this is different from `type[…]` which includes subclasses:
    s1: type[Base] = Base
    s2: type[Base] = Derived  # no error here

# error: "Special form `knot_extensions.TypeOf` expected exactly one type parameter"
t: TypeOf[int, str, bytes]

# error: [invalid-type-form] "`knot_extensions.TypeOf` requires exactly one argument when used in a type expression"
def f(x: TypeOf) -> None:
    reveal_type(x)  # revealed: Unknown
```

## `CallableTypeOf`

The `CallableTypeOf` special form can be used to extract the `Callable` structural type inhabited by
a given callable object. This can be used to get the externally visibly signature of the object,
which can then be used to test various type properties.

It accepts a single type parameter which is expected to be a callable object.

```py
from knot_extensions import CallableTypeOf

def f1():
    return

def f2() -> int:
    return 1

def f3(x: int, y: str) -> None:
    return

# error: [invalid-type-form] "Special form `knot_extensions.CallableTypeOf` expected exactly one type parameter"
c1: CallableTypeOf[f1, f2]

# error: [invalid-type-form] "Expected the first argument to `knot_extensions.CallableTypeOf` to be a callable object, but got an object of type `Literal["foo"]`"
c2: CallableTypeOf["foo"]

# error: [invalid-type-form] "`knot_extensions.CallableTypeOf` requires exactly one argument when used in a type expression"
def f(x: CallableTypeOf) -> None:
    reveal_type(x)  # revealed: Unknown
```

Using it in annotation to reveal the signature of the callable object:

```py
class Foo:
    def __init__(self, x: int) -> None:
        pass

    def __call__(self, x: int) -> str:
        return "foo"

def _(
    c1: CallableTypeOf[f1],
    c2: CallableTypeOf[f2],
    c3: CallableTypeOf[f3],
    c4: CallableTypeOf[Foo],
    c5: CallableTypeOf[Foo(42).__call__],
) -> None:
    reveal_type(c1)  # revealed: () -> Unknown
    reveal_type(c2)  # revealed: () -> int
    reveal_type(c3)  # revealed: (x: int, y: str) -> None

    # TODO: should be `(x: int) -> Foo`
    reveal_type(c4)  # revealed: (...) -> Foo

    # TODO: `self` is bound here; this should probably be `(x: int) -> str`?
    reveal_type(c5)  #  revealed: (self, x: int) -> str
```
