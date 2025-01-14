# Subtype relation

## Basic types

```py
from typing_extensions import Literal
from knot_extensions import is_subtype_of, static_assert

static_assert(is_subtype_of(str, object))
static_assert(is_subtype_of(int, object))
static_assert(is_subtype_of(bool, object))

static_assert(is_subtype_of(bool, int))
static_assert(is_subtype_of(TypeError, Exception))
static_assert(is_subtype_of(FloatingPointError, Exception))

static_assert(not is_subtype_of(object, int))
static_assert(not is_subtype_of(int, str))
static_assert(not is_subtype_of(int, Literal[1]))

static_assert(not is_subtype_of(Literal[1], str))
static_assert(not is_subtype_of(Literal[1, 2], Literal[1]))
static_assert(not is_subtype_of(Literal[1, 2], Literal[1, 3]))
```

## Non-fully-static types

`Any`, `Unknown`, `Todo` and derivatives thereof do not participate in subtyping.

```py
from typing import Any
from typing_extensions import Literal
from knot_extensions import Unknown, is_subtype_of, static_assert

static_assert(not is_subtype_of(Unknown, Unknown))
static_assert(not is_subtype_of(Unknown, Literal[1]))
static_assert(not is_subtype_of(Literal[1], Unknown))

static_assert(not is_subtype_of(Any, Any))
static_assert(not is_subtype_of(Any, Literal[1]))

static_assert(not is_subtype_of(Literal[1], Any))
static_assert(not is_subtype_of(Literal[1], Unknown | str))
```

## `Never`

`Never` is a subtype of all types.

```py
from typing_extensions import Literal, Never
from knot_extensions import AlwaysTruthy, AlwaysFalsy, is_subtype_of, static_assert

static_assert(is_subtype_of(Never, Literal[1]))

static_assert(is_subtype_of(Never, AlwaysTruthy))
static_assert(is_subtype_of(Never, AlwaysFalsy))
```

## Literal types

```py
from typing_extensions import Literal, LiteralString
from knot_extensions import is_subtype_of, static_assert

static_assert(is_subtype_of(Literal[1], int))
static_assert(is_subtype_of(Literal[1], object))

static_assert(is_subtype_of(Literal[True], bool))
static_assert(is_subtype_of(Literal[True], int))
static_assert(is_subtype_of(Literal[True], object))

static_assert(is_subtype_of(Literal["foo"], LiteralString))
static_assert(is_subtype_of(Literal["foo"], str))
static_assert(is_subtype_of(Literal["foo"], object))

static_assert(is_subtype_of(LiteralString, str))
static_assert(is_subtype_of(LiteralString, object))

static_assert(is_subtype_of(Literal[b"foo"], bytes))
static_assert(is_subtype_of(Literal[b"foo"], object))
```

## Tuple types

```py
from typing_extensions import Literal
from knot_extensions import is_subtype_of, static_assert

static_assert(is_subtype_of(tuple[()], tuple[()]))
static_assert(is_subtype_of(tuple[Literal[42]], tuple[int]))
static_assert(is_subtype_of(tuple[Literal[42], Literal["foo"]], tuple[int, str]))
static_assert(is_subtype_of(tuple[int, Literal["foo"]], tuple[int, str]))
static_assert(is_subtype_of(tuple[Literal[42], str], tuple[int, str]))

static_assert(not is_subtype_of(tuple[()], tuple[Literal[1]]))
static_assert(not is_subtype_of(tuple[Literal[42]], tuple[str]))

static_assert(not is_subtype_of(tuple[tuple[Any, ...]], tuple[Literal[2]]))
static_assert(not is_subtype_of(tuple[Literal[2]], tuple[tuple[Any, ...]]))
```

## Union types

```py
from typing_extensions import Literal
from knot_extensions import is_subtype_of, static_assert

static_assert(is_subtype_of(Literal[1], int | str))
static_assert(is_subtype_of(str | int, object))
static_assert(is_subtype_of(Literal[1, 2], Literal[1, 2, 3]))
```

## Intersection types

```py
from typing_extensions import Literal, LiteralString
from knot_extensions import Intersection, Not, is_subtype_of, static_assert

static_assert(is_subtype_of(Intersection[int, Not[Literal[2]]], int))
static_assert(is_subtype_of(Intersection[int, Not[Literal[2]]], Not[Literal[2]]))
static_assert(is_subtype_of(Not[int], Not[Literal[2]]))
static_assert(is_subtype_of(Literal[1], Intersection[int, Not[Literal[2]]]))
static_assert(is_subtype_of(Intersection[str, Not[Literal["foo"]]], Not[Literal[2]]))
static_assert(is_subtype_of(Not[LiteralString], object))

static_assert(is_subtype_of(type[str], Not[None]))
static_assert(is_subtype_of(Not[LiteralString], object))

static_assert(not is_subtype_of(Intersection[int, Not[Literal[2]]], Intersection[int, Not[Literal[3]]]))
static_assert(not is_subtype_of(Not[Literal[2]], Not[Literal[3]]))
static_assert(not is_subtype_of(Not[Literal[2]], Not[int]))

static_assert(not is_subtype_of(int, Not[Literal[3]]))
static_assert(not is_subtype_of(Literal[1], Intersection[int, Not[Literal[1]]]))
```

## Class literal types

```py
from types import ModuleType
from typing import _SpecialForm
from typing_extensions import Literal, LiteralString
import typing
from knot_extensions import TypeOf, is_subtype_of, static_assert

class Meta(type): ...
class HasCustomMetaclass(metaclass=Meta): ...

static_assert(is_subtype_of(TypeOf[bool], type[int]))
static_assert(is_subtype_of(TypeOf[int], TypeOf[int]))
static_assert(is_subtype_of(TypeOf[int], object))

static_assert(is_subtype_of(TypeOf[Literal], _SpecialForm))
static_assert(is_subtype_of(TypeOf[Literal], object))

static_assert(is_subtype_of(TypeOf[HasCustomMetaclass], Meta))
static_assert(is_subtype_of(Meta, type[object]))
static_assert(not is_subtype_of(Meta, type[type]))

static_assert(is_subtype_of(tuple[int], tuple))
static_assert(is_subtype_of(TypeOf[str], type))

static_assert(is_subtype_of(TypeOf[typing], ModuleType))
static_assert(is_subtype_of(TypeOf[1:2:3], slice))

static_assert(not is_subtype_of(type, type[int]))
static_assert(not is_subtype_of(TypeOf[str], type[Any]))
static_assert(not is_subtype_of(type[str], TypeOf[str]))

static_assert(not is_subtype_of(TypeOf[int], TypeOf[object]))
static_assert(not is_subtype_of(TypeOf[int], int))

static_assert(not is_subtype_of(_SpecialForm, TypeOf[Literal]))
```

## `AlwaysTruthy` and `AlwaysFalsy`

```py
from knot_extensions import AlwaysTruthy, AlwaysFalsy, is_subtype_of, static_assert

static_assert(is_subtype_of(Literal[1], AlwaysTruthy))
static_assert(is_subtype_of(Literal[0], AlwaysFalsy))

static_assert(is_subtype_of(AlwaysTruthy, object))
static_assert(is_subtype_of(AlwaysFalsy, object))

static_assert(not is_subtype_of(Literal[1], AlwaysFalsy))
static_assert(not is_subtype_of(Literal[0], AlwaysTruthy))

static_assert(not is_subtype_of(str, AlwaysTruthy))
static_assert(not is_subtype_of(str, AlwaysFalsy))
```

## Unions

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

## Intersections

```py
from knot_extensions import Intersection, is_subtype_of, static_assert

class A: ...
class B: ...

static_assert(not is_subtype_of(A, B))
static_assert(is_subtype_of(Intersection[A, B], A))
static_assert(is_subtype_of(Intersection[A, B], B))
```
