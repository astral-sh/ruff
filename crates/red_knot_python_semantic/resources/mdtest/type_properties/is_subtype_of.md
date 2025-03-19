# Subtype relation

The `is_subtype_of(S, T)` relation below checks if type `S` is a subtype of type `T`.

A fully static type `S` is a subtype of another fully static type `T` iff the set of values
represented by `S` is a subset of the set of values represented by `T`.

See the [typing documentation] for more information.

## Basic builtin types

- `bool` is a subtype of `int`. This is modeled after Python's runtime behavior, where `int` is a
    supertype of `bool` (present in `bool`s bases and MRO).
- `int` is not a subtype of `float`/`complex`, although this is muddied by the
    [special case for float and complex] where annotations of `float` and `complex` are interpreted
    as `int | float` and `int | float | complex`, respectively.

```py
from knot_extensions import is_subtype_of, static_assert, TypeOf

type JustFloat = TypeOf[1.0]
type JustComplex = TypeOf[1j]

static_assert(is_subtype_of(bool, bool))
static_assert(is_subtype_of(bool, int))
static_assert(is_subtype_of(bool, object))

static_assert(is_subtype_of(int, int))
static_assert(is_subtype_of(int, object))

static_assert(is_subtype_of(object, object))

static_assert(not is_subtype_of(int, bool))
static_assert(not is_subtype_of(int, str))
static_assert(not is_subtype_of(object, int))

static_assert(not is_subtype_of(int, JustFloat))
static_assert(not is_subtype_of(int, JustComplex))

static_assert(is_subtype_of(TypeError, Exception))
static_assert(is_subtype_of(FloatingPointError, Exception))
```

## Class hierarchies

```py
from knot_extensions import is_subtype_of, static_assert
from typing_extensions import Never

class A: ...
class B1(A): ...
class B2(A): ...
class C(B1, B2): ...

static_assert(is_subtype_of(B1, A))
static_assert(not is_subtype_of(A, B1))

static_assert(is_subtype_of(B2, A))
static_assert(not is_subtype_of(A, B2))

static_assert(not is_subtype_of(B1, B2))
static_assert(not is_subtype_of(B2, B1))

static_assert(is_subtype_of(C, B1))
static_assert(is_subtype_of(C, B2))
static_assert(not is_subtype_of(B1, C))
static_assert(not is_subtype_of(B2, C))
static_assert(is_subtype_of(C, A))
static_assert(not is_subtype_of(A, C))

static_assert(is_subtype_of(Never, A))
static_assert(is_subtype_of(Never, B1))
static_assert(is_subtype_of(Never, B2))
static_assert(is_subtype_of(Never, C))

static_assert(is_subtype_of(A, object))
static_assert(is_subtype_of(B1, object))
static_assert(is_subtype_of(B2, object))
static_assert(is_subtype_of(C, object))
```

## Literal types

```py
from typing_extensions import Literal, LiteralString
from knot_extensions import is_subtype_of, static_assert, TypeOf

type JustFloat = TypeOf[1.0]

# Boolean literals
static_assert(is_subtype_of(Literal[True], bool))
static_assert(is_subtype_of(Literal[True], int))
static_assert(is_subtype_of(Literal[True], object))

# Integer literals
static_assert(is_subtype_of(Literal[1], int))
static_assert(is_subtype_of(Literal[1], object))

static_assert(not is_subtype_of(Literal[1], bool))

static_assert(not is_subtype_of(Literal[1], JustFloat))

# String literals
static_assert(is_subtype_of(Literal["foo"], LiteralString))
static_assert(is_subtype_of(Literal["foo"], str))
static_assert(is_subtype_of(Literal["foo"], object))

static_assert(is_subtype_of(LiteralString, str))
static_assert(is_subtype_of(LiteralString, object))

# Bytes literals
static_assert(is_subtype_of(Literal[b"foo"], bytes))
static_assert(is_subtype_of(Literal[b"foo"], object))
```

## Tuple types

```py
from knot_extensions import is_subtype_of, static_assert

class A1: ...
class B1(A1): ...
class A2: ...
class B2(A2): ...
class Unrelated: ...

static_assert(is_subtype_of(B1, A1))
static_assert(is_subtype_of(B2, A2))

# Zero-element tuples
static_assert(is_subtype_of(tuple[()], tuple[()]))
static_assert(not is_subtype_of(tuple[()], tuple[Unrelated]))

# One-element tuples
static_assert(is_subtype_of(tuple[B1], tuple[A1]))
static_assert(not is_subtype_of(tuple[B1], tuple[Unrelated]))
static_assert(not is_subtype_of(tuple[B1], tuple[()]))
static_assert(not is_subtype_of(tuple[B1], tuple[A1, Unrelated]))

# Two-element tuples
static_assert(is_subtype_of(tuple[B1, B2], tuple[A1, A2]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[Unrelated, A2]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[A1, Unrelated]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[Unrelated, Unrelated]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[()]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[A1]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[A1, A2, Unrelated]))

static_assert(is_subtype_of(tuple[int], tuple))
```

## Union types

```py
from knot_extensions import is_subtype_of, static_assert
from typing import Literal

class A: ...
class B1(A): ...
class B2(A): ...
class Unrelated1: ...
class Unrelated2: ...

static_assert(is_subtype_of(B1, A))
static_assert(is_subtype_of(B2, A))

# Union on the right hand side
static_assert(is_subtype_of(B1, A | Unrelated1))
static_assert(is_subtype_of(B1, Unrelated1 | A))

static_assert(not is_subtype_of(B1, Unrelated1 | Unrelated2))

# Union on the left hand side
static_assert(is_subtype_of(B1 | B2, A))
static_assert(is_subtype_of(B1 | B2 | A, object))

static_assert(not is_subtype_of(B1 | Unrelated1, A))
static_assert(not is_subtype_of(Unrelated1 | B1, A))

# Union on both sides
static_assert(is_subtype_of(B1 | bool, A | int))
static_assert(is_subtype_of(B1 | bool, int | A))

static_assert(not is_subtype_of(B1 | bool, Unrelated1 | int))
static_assert(not is_subtype_of(B1 | bool, int | Unrelated1))

# Example: Unions of literals
static_assert(is_subtype_of(Literal[1, 2, 3], int))
static_assert(not is_subtype_of(Literal[1, "two", 3], int))
```

## Intersection types

```py
from typing_extensions import Literal, LiteralString
from knot_extensions import Intersection, Not, is_subtype_of, static_assert

class A: ...
class B1(A): ...
class B2(A): ...
class C(B1, B2): ...
class Unrelated: ...

static_assert(is_subtype_of(B1, A))
static_assert(is_subtype_of(B2, A))
static_assert(is_subtype_of(C, A))
static_assert(is_subtype_of(C, B1))
static_assert(is_subtype_of(C, B2))

# For complements, the subtyping relation is reversed:
static_assert(is_subtype_of(Not[A], Not[B1]))
static_assert(is_subtype_of(Not[A], Not[B2]))
static_assert(is_subtype_of(Not[A], Not[C]))
static_assert(is_subtype_of(Not[B1], Not[C]))
static_assert(is_subtype_of(Not[B2], Not[C]))

# The intersection of two types is a subtype of both:
static_assert(is_subtype_of(Intersection[B1, B2], B1))
static_assert(is_subtype_of(Intersection[B1, B2], B2))
# … and of their common supertype:
static_assert(is_subtype_of(Intersection[B1, B2], A))

# A common subtype of two types is a subtype of their intersection:
static_assert(is_subtype_of(C, Intersection[B1, B2]))
# … but not the other way around:
static_assert(not is_subtype_of(Intersection[B1, B2], C))

# "Removing" B1 from A leaves a subtype of A.
static_assert(is_subtype_of(Intersection[A, Not[B1]], A))
static_assert(is_subtype_of(Intersection[A, Not[B1]], Not[B1]))

# B1 and B2 are not disjoint, so this is not true:
static_assert(not is_subtype_of(B2, Intersection[A, Not[B1]]))
# … but for two disjoint subtypes, it is:
static_assert(is_subtype_of(Literal[2], Intersection[int, Not[Literal[1]]]))

# A and Unrelated are not related, so this is not true:
static_assert(not is_subtype_of(Intersection[A, Not[B1]], Not[Unrelated]))
# … but for a disjoint type like `None`, it is:
static_assert(is_subtype_of(Intersection[A, Not[B1]], Not[None]))

# Complements of types are still subtypes of `object`:
static_assert(is_subtype_of(Not[A], object))

# More examples:
static_assert(is_subtype_of(type[str], Not[None]))
static_assert(is_subtype_of(Not[LiteralString], object))

static_assert(not is_subtype_of(Intersection[int, Not[Literal[2]]], Intersection[int, Not[Literal[3]]]))
static_assert(not is_subtype_of(Not[Literal[2]], Not[Literal[3]]))
static_assert(not is_subtype_of(Not[Literal[2]], Not[int]))
static_assert(not is_subtype_of(int, Not[Literal[3]]))
static_assert(not is_subtype_of(Literal[1], Intersection[int, Not[Literal[1]]]))
```

## Special types

### `Never`

`Never` is a subtype of all types.

```py
from typing_extensions import Literal, Never
from knot_extensions import AlwaysTruthy, AlwaysFalsy, is_subtype_of, static_assert

static_assert(is_subtype_of(Never, Never))
static_assert(is_subtype_of(Never, Literal[True]))
static_assert(is_subtype_of(Never, bool))
static_assert(is_subtype_of(Never, int))
static_assert(is_subtype_of(Never, object))

static_assert(is_subtype_of(Never, AlwaysTruthy))
static_assert(is_subtype_of(Never, AlwaysFalsy))
```

### `AlwaysTruthy` and `AlwaysFalsy`

```py
from knot_extensions import AlwaysTruthy, AlwaysFalsy, Intersection, Not, is_subtype_of, static_assert
from typing_extensions import Literal, LiteralString

static_assert(is_subtype_of(Literal[1], AlwaysTruthy))
static_assert(is_subtype_of(Literal[0], AlwaysFalsy))

static_assert(is_subtype_of(AlwaysTruthy, object))
static_assert(is_subtype_of(AlwaysFalsy, object))

static_assert(not is_subtype_of(Literal[1], AlwaysFalsy))
static_assert(not is_subtype_of(Literal[0], AlwaysTruthy))

static_assert(not is_subtype_of(str, AlwaysTruthy))
static_assert(not is_subtype_of(str, AlwaysFalsy))

# TODO: No errors
# error: [static-assert-error]
static_assert(is_subtype_of(bool, Literal[False] | AlwaysTruthy))
# error: [static-assert-error]
static_assert(is_subtype_of(bool, Literal[True] | AlwaysFalsy))
# error: [static-assert-error]
static_assert(is_subtype_of(LiteralString, Literal[""] | AlwaysTruthy))
static_assert(not is_subtype_of(Literal[True] | AlwaysFalsy, Literal[False] | AlwaysTruthy))

# TODO: No errors
# The condition `is_subtype_of(T & U, U)` must still be satisfied after the following transformations:
# `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal[""]]], AlwaysTruthy))
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal["", "a"]]], AlwaysTruthy))
# `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal[""]]], Not[AlwaysFalsy]))
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal["", "a"]]], Not[AlwaysFalsy]))
```

### Module literals

```py
from types import ModuleType
from knot_extensions import TypeOf, is_subtype_of, static_assert
from typing_extensions import assert_type
import typing

assert_type(typing, TypeOf[typing])

static_assert(is_subtype_of(TypeOf[typing], ModuleType))
```

### Slice literals

```py
from knot_extensions import TypeOf, is_subtype_of, static_assert

static_assert(is_subtype_of(TypeOf[1:2:3], slice))
```

### Special forms

```py
from typing import _SpecialForm, Literal
from knot_extensions import TypeOf, is_subtype_of, static_assert

static_assert(is_subtype_of(TypeOf[Literal], _SpecialForm))
static_assert(is_subtype_of(TypeOf[Literal], object))

static_assert(not is_subtype_of(_SpecialForm, TypeOf[Literal]))
```

## Class literal types and `type[…]`

### Basic

```py
from typing import _SpecialForm
from typing_extensions import Literal, assert_type
from knot_extensions import TypeOf, is_subtype_of, static_assert

class Meta(type): ...
class HasCustomMetaclass(metaclass=Meta): ...

type LiteralBool = TypeOf[bool]
type LiteralInt = TypeOf[int]
type LiteralStr = TypeOf[str]
type LiteralObject = TypeOf[object]

assert_type(bool, LiteralBool)
assert_type(int, LiteralInt)
assert_type(str, LiteralStr)
assert_type(object, LiteralObject)

# bool

static_assert(is_subtype_of(LiteralBool, LiteralBool))
static_assert(is_subtype_of(LiteralBool, type[bool]))
static_assert(is_subtype_of(LiteralBool, type[int]))
static_assert(is_subtype_of(LiteralBool, type[object]))
static_assert(is_subtype_of(LiteralBool, type))
static_assert(is_subtype_of(LiteralBool, object))

static_assert(not is_subtype_of(LiteralBool, LiteralInt))
static_assert(not is_subtype_of(LiteralBool, LiteralObject))
static_assert(not is_subtype_of(LiteralBool, bool))

static_assert(not is_subtype_of(type, type[bool]))

# int

static_assert(is_subtype_of(LiteralInt, LiteralInt))
static_assert(is_subtype_of(LiteralInt, type[int]))
static_assert(is_subtype_of(LiteralInt, type[object]))
static_assert(is_subtype_of(LiteralInt, type))
static_assert(is_subtype_of(LiteralInt, object))

static_assert(not is_subtype_of(LiteralInt, LiteralObject))
static_assert(not is_subtype_of(LiteralInt, int))

static_assert(not is_subtype_of(type, type[int]))

# LiteralString

static_assert(is_subtype_of(LiteralStr, type[str]))
static_assert(is_subtype_of(LiteralStr, type))
static_assert(is_subtype_of(LiteralStr, type[object]))

static_assert(not is_subtype_of(type[str], LiteralStr))

# custom metaclasses

type LiteralHasCustomMetaclass = TypeOf[HasCustomMetaclass]

static_assert(is_subtype_of(LiteralHasCustomMetaclass, Meta))
static_assert(is_subtype_of(Meta, type[object]))
static_assert(is_subtype_of(Meta, type))

static_assert(not is_subtype_of(Meta, type[type]))
```

### Unions of class literals

```py
from typing_extensions import assert_type
from knot_extensions import TypeOf, is_subtype_of, static_assert

class Base: ...
class Derived(Base): ...
class Unrelated: ...

type LiteralBase = TypeOf[Base]
type LiteralDerived = TypeOf[Derived]
type LiteralUnrelated = TypeOf[Unrelated]

assert_type(Base, LiteralBase)
assert_type(Derived, LiteralDerived)
assert_type(Unrelated, LiteralUnrelated)

static_assert(is_subtype_of(LiteralBase, type))
static_assert(is_subtype_of(LiteralBase, object))

static_assert(is_subtype_of(LiteralBase, type[Base]))
static_assert(is_subtype_of(LiteralDerived, type[Base]))
static_assert(is_subtype_of(LiteralDerived, type[Derived]))

static_assert(not is_subtype_of(LiteralBase, type[Derived]))
static_assert(is_subtype_of(type[Derived], type[Base]))

static_assert(is_subtype_of(LiteralBase | LiteralUnrelated, type))
static_assert(is_subtype_of(LiteralBase | LiteralUnrelated, object))
```

## Non-fully-static types

`Any`, `Unknown`, `Todo` and derivatives thereof do not participate in subtyping.

```py
from knot_extensions import Unknown, is_subtype_of, static_assert, Intersection
from typing_extensions import Any

static_assert(not is_subtype_of(Any, Any))
static_assert(not is_subtype_of(Any, int))
static_assert(not is_subtype_of(int, Any))
static_assert(not is_subtype_of(Any, object))
static_assert(not is_subtype_of(object, Any))

static_assert(not is_subtype_of(int, Any | int))
static_assert(not is_subtype_of(Intersection[Any, int], int))
static_assert(not is_subtype_of(tuple[int, int], tuple[int, Any]))

# The same for `Unknown`:
static_assert(not is_subtype_of(Unknown, Unknown))
static_assert(not is_subtype_of(Unknown, int))
static_assert(not is_subtype_of(int, Unknown))
static_assert(not is_subtype_of(Unknown, object))
static_assert(not is_subtype_of(object, Unknown))

static_assert(not is_subtype_of(int, Unknown | int))
static_assert(not is_subtype_of(Intersection[Unknown, int], int))
static_assert(not is_subtype_of(tuple[int, int], tuple[int, Unknown]))
```

## Callable

The general principle is that a callable type is a subtype of another if it's more flexible in what
it accepts and more specific in what it returns.

References:

- <https://typing.python.org/en/latest/spec/callables.html#assignability-rules-for-callables>
- <https://typing.python.org/en/latest/spec/callables.html#assignment>

### Return type

Return types are covariant.

```py
from typing import Callable
from knot_extensions import is_subtype_of, static_assert

static_assert(is_subtype_of(Callable[[], int], Callable[[], float]))
static_assert(not is_subtype_of(Callable[[], float], Callable[[], int]))
```

### Parameter types

Parameter types are contravariant.

#### Positional-only

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def float_param(a: float, /) -> None: ...
def int_param(a: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[float_param], CallableTypeFromFunction[int_param]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_param], CallableTypeFromFunction[float_param]))
```

Parameter name is not required to be the same for positional-only parameters at the same position:

```py
def int_param_different_name(b: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[int_param], CallableTypeFromFunction[int_param_different_name]))
static_assert(is_subtype_of(CallableTypeFromFunction[int_param_different_name], CallableTypeFromFunction[int_param]))
```

Multiple positional-only parameters are checked in order:

```py
def multi_param1(a: float, b: int, c: str, /) -> None: ...
def multi_param2(b: int, c: bool, a: str, /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[multi_param1], CallableTypeFromFunction[multi_param2]))
static_assert(not is_subtype_of(CallableTypeFromFunction[multi_param2], CallableTypeFromFunction[multi_param1]))
```

#### Positional-only with default value

If the parameter has a default value, it's treated as optional. This means that the parameter at the
corresponding position in the supertype does not need to have a default value.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def float_with_default(a: float = 1, /) -> None: ...
def int_with_default(a: int = 1, /) -> None: ...
def int_without_default(a: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[float_with_default], CallableTypeFromFunction[int_with_default]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[float_with_default]))

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[int_without_default]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_without_default], CallableTypeFromFunction[int_with_default]))
```

As the parameter itself is optional, it can be omitted in the supertype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_without_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[int_with_default]))
```

The subtype can include as many positional-only parameters as long as they have the default value:

```py
def multi_param(a: float = 1, b: int = 2, c: str = "3", /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[multi_param], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[multi_param]))
```

#### Positional-only with other kinds

If a parameter is declared as positional-only, then the corresponding parameter in the supertype
cannot be any other parameter kind.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def positional_only(a: int, /) -> None: ...
def standard(a: int) -> None: ...
def keyword_only(*, a: int) -> None: ...
def variadic(*a: int) -> None: ...
def keyword_variadic(**a: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[standard]))
static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[keyword_only]))
static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[variadic]))
static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[keyword_variadic]))
```

#### Standard

A standard parameter is either a positional or a keyword parameter.

Unlike positional-only parameters, standard parameters should have the same name in the subtype.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def int_param_a(a: int) -> None: ...
def int_param_b(b: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[int_param_a], CallableTypeFromFunction[int_param_b]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_param_b], CallableTypeFromFunction[int_param_a]))
```

Apart from the name, it behaves the same as positional-only parameters.

```py
def float_param(a: float) -> None: ...
def int_param(a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[float_param], CallableTypeFromFunction[int_param]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_param], CallableTypeFromFunction[float_param]))
```

With the same rules for default values as well.

```py
def float_with_default(a: float = 1) -> None: ...
def int_with_default(a: int = 1) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[float_with_default], CallableTypeFromFunction[int_with_default]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[float_with_default]))

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[int_param]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_param], CallableTypeFromFunction[int_with_default]))

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[int_with_default]))
```

Multiple standard parameters are checked in order along with their names:

```py
def multi_param1(a: float, b: int, c: str) -> None: ...
def multi_param2(a: int, b: bool, c: str) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[multi_param1], CallableTypeFromFunction[multi_param2]))
static_assert(not is_subtype_of(CallableTypeFromFunction[multi_param2], CallableTypeFromFunction[multi_param1]))
```

The subtype can include as many standard parameters as long as they have the default value:

```py
def multi_param_default(a: float = 1, b: int = 2, c: str = "s") -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[multi_param_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[multi_param_default]))
```

#### Standard with keyword-only

A keyword-only parameter in the supertype can be substituted with the corresponding standard
parameter in the subtype with the same name. This is because a standard parameter is more flexible
than a keyword-only parameter.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def standard_a(a: int) -> None: ...
def keyword_b(*, b: int) -> None: ...

# The name of the parameters are different
static_assert(not is_subtype_of(CallableTypeFromFunction[standard_a], CallableTypeFromFunction[keyword_b]))

def standard_float(a: float) -> None: ...
def keyword_int(*, a: int) -> None: ...

# Here, the name of the parameters are the same
static_assert(is_subtype_of(CallableTypeFromFunction[standard_float], CallableTypeFromFunction[keyword_int]))

def standard_with_default(a: int = 1) -> None: ...
def keyword_with_default(*, a: int = 1) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[keyword_with_default]))
static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[empty]))
```

The position of the keyword-only parameters does not matter:

```py
def multi_standard(a: float, b: int, c: str) -> None: ...
def multi_keyword(*, b: bool, c: str, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[multi_standard], CallableTypeFromFunction[multi_keyword]))
```

#### Standard with positional-only

A positional-only parameter in the supertype can be substituted with the corresponding standard
parameter in the subtype at the same position. This is because a standard parameter is more flexible
than a positional-only parameter.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def standard_a(a: int) -> None: ...
def positional_b(b: int, /) -> None: ...

# The names are not important in this context
static_assert(is_subtype_of(CallableTypeFromFunction[standard_a], CallableTypeFromFunction[positional_b]))

def standard_float(a: float) -> None: ...
def positional_int(a: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[standard_float], CallableTypeFromFunction[positional_int]))

def standard_with_default(a: int = 1) -> None: ...
def positional_with_default(a: int = 1, /) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[positional_with_default]))
static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[empty]))
```

The position of the positional-only parameters matter:

```py
def multi_standard(a: float, b: int, c: str) -> None: ...
def multi_positional1(b: int, c: bool, a: str, /) -> None: ...

# Here, the type of the parameter `a` makes the subtype relation invalid
def multi_positional2(b: int, a: float, c: str, /) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[multi_standard], CallableTypeFromFunction[multi_positional1]))
static_assert(not is_subtype_of(CallableTypeFromFunction[multi_standard], CallableTypeFromFunction[multi_positional2]))
```

#### Standard with variadic

A standard parameter in the supertype cannot be substituted with a variadic or keyword-variadic
parameter in the subtype.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def standard(a: int) -> None: ...
def variadic(*a: int) -> None: ...
def keyword_variadic(**a: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[standard], CallableTypeFromFunction[variadic]))
static_assert(not is_subtype_of(CallableTypeFromFunction[standard], CallableTypeFromFunction[keyword_variadic]))
```

#### Variadic

The name of the variadic parameter does not need to be the same in the subtype.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def variadic_float(*args2: float) -> None: ...
def variadic_int(*args1: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[variadic_float], CallableTypeFromFunction[variadic_int]))
static_assert(not is_subtype_of(CallableTypeFromFunction[variadic_int], CallableTypeFromFunction[variadic_float]))
```

The variadic parameter does not need to be present in the supertype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[variadic_int], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[variadic_int]))
```

#### Variadic with positional-only

If the subtype has a variadic parameter then any unmatched positional-only parameter from the
supertype should be checked against the variadic parameter.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def variadic(a: int, /, *args: float) -> None: ...

# Here, the parameter `b` and `c` are unmatched
def positional_only(a: int, b: float, c: int, /) -> None: ...

# Here, the parameter `b` is unmatched and there's also a variadic parameter
def positional_variadic(a: int, b: float, /, *args: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[variadic], CallableTypeFromFunction[positional_only]))
static_assert(is_subtype_of(CallableTypeFromFunction[variadic], CallableTypeFromFunction[positional_variadic]))
```

This is valid only for positional-only parameters, not any other parameter kind:

```py
# Parameter 1 is matched with the one at the same position, parameter 2 is unmatched so uses the
# variadic parameter but the standard parameter `c` remains and cannot be matched.
def mixed(a: int, b: float, /, c: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[variadic], CallableTypeFromFunction[mixed]))
```

#### Keyword-only

For keyword-only parameters, the name should be the same:

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def keyword_int(*, a: int) -> None: ...
def keyword_float(*, a: float) -> None: ...
def keyword_b(*, b: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[keyword_float], CallableTypeFromFunction[keyword_int]))
static_assert(not is_subtype_of(CallableTypeFromFunction[keyword_int], CallableTypeFromFunction[keyword_float]))
static_assert(not is_subtype_of(CallableTypeFromFunction[keyword_int], CallableTypeFromFunction[keyword_b]))
```

But, the order of the keyword-only parameters is not required to be the same:

```py
def keyword_ab(*, a: float, b: float) -> None: ...
def keyword_ba(*, b: int, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[keyword_ab], CallableTypeFromFunction[keyword_ba]))
static_assert(not is_subtype_of(CallableTypeFromFunction[keyword_ba], CallableTypeFromFunction[keyword_ab]))
```

#### Keyword-only with default

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def float_with_default(*, a: float = 1) -> None: ...
def int_with_default(*, a: int = 1) -> None: ...
def int_keyword(*, a: int) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[float_with_default], CallableTypeFromFunction[int_with_default]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[float_with_default]))

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[int_keyword]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_keyword], CallableTypeFromFunction[int_with_default]))

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[int_with_default]))
```

Keyword-only parameters with default values can be mixed with the ones without default values in any
order:

```py
# A keyword-only parameter with a default value follows the one without a default value (it's valid)
def mixed(*, b: int = 1, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[mixed], CallableTypeFromFunction[int_keyword]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_keyword], CallableTypeFromFunction[mixed]))
```

#### Keyword-only with standard

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def keywords1(*, a: int, b: int) -> None: ...
def standard(b: float, a: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[keywords1], CallableTypeFromFunction[standard]))
static_assert(is_subtype_of(CallableTypeFromFunction[standard], CallableTypeFromFunction[keywords1]))
```

The subtype can include additional standard parameters as long as it has the default value:

```py
def standard_with_default(b: float, a: float, c: float = 1) -> None: ...
def standard_without_default(b: float, a: float, c: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[standard_without_default], CallableTypeFromFunction[keywords1]))
static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[keywords1]))
```

Here, we mix keyword-only parameters with standard parameters:

```py
def keywords2(*, a: int, c: int, b: int) -> None: ...
def mixed(b: float, a: float, *, c: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[keywords2], CallableTypeFromFunction[mixed]))
static_assert(is_subtype_of(CallableTypeFromFunction[mixed], CallableTypeFromFunction[keywords2]))
```

But, we shouldn't consider any unmatched positional-only parameters:

```py
def mixed_positional(b: float, /, a: float, *, c: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[mixed_positional], CallableTypeFromFunction[keywords2]))
```

But, an unmatched variadic parameter is still valid:

```py
def mixed_variadic(*args: float, a: float, b: float, c: float, **kwargs: float) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[mixed_variadic], CallableTypeFromFunction[keywords2]))
```

#### Keyword-variadic

The name of the keyword-variadic parameter does not need to be the same in the subtype.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def kwargs_float(**kwargs2: float) -> None: ...
def kwargs_int(**kwargs1: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[kwargs_float], CallableTypeFromFunction[kwargs_int]))
static_assert(not is_subtype_of(CallableTypeFromFunction[kwargs_int], CallableTypeFromFunction[kwargs_float]))
```

A variadic parameter can be omitted in the subtype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[kwargs_int], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[kwargs_int]))
```

#### Keyword-variadic with keyword-only

If the subtype has a keyword-variadic parameter then any unmatched keyword-only parameter from the
supertype should be checked against the keyword-variadic parameter.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def kwargs(**kwargs: float) -> None: ...
def keyword_only(*, a: int, b: float, c: bool) -> None: ...
def keyword_variadic(*, a: int, **kwargs: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[kwargs], CallableTypeFromFunction[keyword_only]))
static_assert(is_subtype_of(CallableTypeFromFunction[kwargs], CallableTypeFromFunction[keyword_variadic]))
```

This is valid only for keyword-only parameters, not any other parameter kind:

```py
def mixed1(a: int, *, b: int) -> None: ...

# Same as above but with the default value
def mixed2(a: int = 1, *, b: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[kwargs], CallableTypeFromFunction[mixed1]))
static_assert(not is_subtype_of(CallableTypeFromFunction[kwargs], CallableTypeFromFunction[mixed2]))
```

#### Empty

When the supertype has an empty list of parameters, then the subtype can have any kind of parameters
as long as they contain the default values for non-variadic parameters.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def empty() -> None: ...
def mixed(a: int = 1, /, b: int = 2, *args: int, c: int = 3, **kwargs: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[mixed], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[mixed]))
```

[special case for float and complex]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
[typing documentation]: https://typing.readthedocs.io/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
