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

#### Positional-only with default value

If the parameter has a default value, it's treated as optional. This means that the parameter at the
corresponding position in the other function does not need to have a default value.

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

As the parameter itself is optional, it can be omitted in the subtype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[int_with_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[int_without_default], CallableTypeFromFunction[empty]))
static_assert(not is_subtype_of(CallableTypeFromFunction[empty], CallableTypeFromFunction[int_with_default]))
```

#### Positional-only with other kinds

If a parameter is declared as positional-only, then the corresponding parameter in the subtype
cannot be any other parameter kind.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def positional_only(a: int, /) -> None: ...
def standard(a: int) -> None: ...
def keyword_only(*, a: int) -> None: ...
def variadic(*args: int) -> None: ...
def keyword_variadic(**kwargs: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[standard]))
static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[keyword_only]))
static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[variadic]))
static_assert(not is_subtype_of(CallableTypeFromFunction[positional_only], CallableTypeFromFunction[keyword_variadic]))
```

But, a positional-only parameter can be a subtype of a standard parameter:

```py
static_assert(is_subtype_of(CallableTypeFromFunction[standard], CallableTypeFromFunction[positional_only]))
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

#### Standard with other kinds

If the corresponding parameter in the subtype is a keyword-only parameter, it behaves in the same
way. This is because keyword-only parameter is one of the kind of standard parameter.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def standard_a(a: int) -> None: ...
def keyword_b(*, b: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[standard_a], CallableTypeFromFunction[keyword_b]))

def standard_float(a: float) -> None: ...
def keyword_int(*, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[standard_float], CallableTypeFromFunction[keyword_int]))

def standard_with_default(a: int = 1) -> None: ...
def keyword_with_default(*, a: int = 1) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[keyword_with_default]))
static_assert(is_subtype_of(CallableTypeFromFunction[standard_with_default], CallableTypeFromFunction[empty]))
```

And, the same is for positional-only parameter except that the names are not required to be the
same.

```py
from knot_extensions import CallableTypeFromFunction, is_subtype_of, static_assert

def standard_a(a: int) -> None: ...
def positional_b(b: int, /) -> None: ...

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

And, with other kinds of parameter:

```py
def variadic(*args: int) -> None: ...
def keyword_variadic(**kwargs: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[standard_a], CallableTypeFromFunction[variadic]))
static_assert(not is_subtype_of(CallableTypeFromFunction[standard_a], CallableTypeFromFunction[keyword_variadic]))
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

A variadic parameter can be omitted in the subtype:

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

def variadic(*args: float) -> None: ...
def positional_only(a: int, b: float, /) -> None: ...
def positional_variadic(a: int, /, *args: int) -> None: ...

static_assert(is_subtype_of(CallableTypeFromFunction[variadic], CallableTypeFromFunction[positional_only]))
static_assert(is_subtype_of(CallableTypeFromFunction[variadic], CallableTypeFromFunction[positional_variadic]))
```

This is valid only for positional-only parameter, not any other parameter kind:

```py
def mixed(a: int, /, b: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeFromFunction[variadic], CallableTypeFromFunction[mixed]))
```

[special case for float and complex]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
[typing documentation]: https://typing.readthedocs.io/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
