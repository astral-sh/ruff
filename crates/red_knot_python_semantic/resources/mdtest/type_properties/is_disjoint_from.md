# Tests for disjointness

If two types can be disjoint, it means that it is known that no possible runtime object could ever
inhabit both types simultaneously.

## `Never`

`Never` is disjoint from every type, including itself.

```py
from typing_extensions import Never
from knot_extensions import is_disjoint_from, static_assert

static_assert(is_disjoint_from(Never, Never))
static_assert(is_disjoint_from(Never, None))
static_assert(is_disjoint_from(Never, int))
```

## `None`

```py
from typing_extensions import Literal
from knot_extensions import AlwaysFalsy, AlwaysTruthy, is_disjoint_from, static_assert

static_assert(is_disjoint_from(None, Literal[True]))
static_assert(is_disjoint_from(None, Literal[1]))
static_assert(is_disjoint_from(None, Literal["test"]))
static_assert(is_disjoint_from(None, Literal[b"test"]))
static_assert(is_disjoint_from(None, LiteralString))
static_assert(is_disjoint_from(None, int))
static_assert(is_disjoint_from(None, type[object]))
static_assert(is_disjoint_from(None, AlwaysTruthy))

static_assert(not is_disjoint_from(None, None))
static_assert(not is_disjoint_from(None, object))
static_assert(not is_disjoint_from(None, AlwaysFalsy))
```

## Basic

```py
from typing_extensions import Literal, LiteralString
from knot_extensions import AlwaysFalsy, AlwaysTruthy, Intersection, Not, TypeOf, is_disjoint_from, static_assert

static_assert(is_disjoint_from(Literal[True], Literal[False]))
static_assert(is_disjoint_from(Literal[True], Literal[1]))
static_assert(is_disjoint_from(Literal[False], Literal[0]))

static_assert(is_disjoint_from(Literal[1], Literal[2]))

static_assert(is_disjoint_from(Literal["a"], Literal["b"]))

static_assert(is_disjoint_from(Literal[b"a"], LiteralString))
static_assert(is_disjoint_from(Literal[b"a"], Literal[b"b"]))
static_assert(is_disjoint_from(Literal[b"a"], Literal["a"]))

static_assert(is_disjoint_from(type[object], TypeOf[Literal]))
static_assert(is_disjoint_from(type[str], LiteralString))

static_assert(is_disjoint_from(AlwaysFalsy, AlwaysTruthy))

static_assert(not is_disjoint_from(Any, int))

static_assert(not is_disjoint_from(Literal[True], Literal[True]))
static_assert(not is_disjoint_from(Literal[False], Literal[False]))
static_assert(not is_disjoint_from(Literal[True], bool))
static_assert(not is_disjoint_from(Literal[True], int))

static_assert(not is_disjoint_from(Literal[1], Literal[1]))

static_assert(not is_disjoint_from(Literal["a"], Literal["a"]))
static_assert(not is_disjoint_from(Literal["a"], LiteralString))
static_assert(not is_disjoint_from(Literal["a"], str))

static_assert(not is_disjoint_from(int, int))
static_assert(not is_disjoint_from(LiteralString, LiteralString))

static_assert(not is_disjoint_from(str, LiteralString))
static_assert(not is_disjoint_from(str, type))
static_assert(not is_disjoint_from(str, type[Any]))
static_assert(not is_disjoint_from(str, AlwaysFalsy))
static_assert(not is_disjoint_from(str, AlwaysTruthy))
```

## Tuple types

```py
from typing_extensions import Literal
from knot_extensions import TypeOf, is_disjoint_from, static_assert

static_assert(is_disjoint_from(tuple[()], TypeOf[object]))
static_assert(is_disjoint_from(tuple[()], TypeOf[Literal]))

static_assert(is_disjoint_from(tuple[None], None))
static_assert(is_disjoint_from(tuple[None], Literal[b"a"]))
static_assert(is_disjoint_from(tuple[None], Literal["a"]))
static_assert(is_disjoint_from(tuple[None], Literal[1]))
static_assert(is_disjoint_from(tuple[None], Literal[True]))

static_assert(is_disjoint_from(tuple[Literal[1]], tuple[Literal[2]]))
static_assert(is_disjoint_from(tuple[Literal[1], Literal[2]], tuple[Literal[1]]))
static_assert(is_disjoint_from(tuple[Literal[1], Literal[2]], tuple[Literal[1], Literal[3]]))

static_assert(not is_disjoint_from(tuple[Literal[1], Literal[2]], tuple[Literal[1], int]))
```

## Unions

```py
from typing_extensions import Literal
from knot_extensions import AlwaysFalsy, AlwaysTruthy, Intersection, is_disjoint_from, static_assert

static_assert(is_disjoint_from(Literal[1, 2], Literal[3]))
static_assert(is_disjoint_from(Literal[1, 2], Literal[3, 4]))
static_assert(is_disjoint_from(Literal[1, 2], AlwaysFalsy))

static_assert(is_disjoint_from(Intersection[int, Literal[1]], Literal[2]))

static_assert(not is_disjoint_from(Literal[1, 2], Literal[2]))
static_assert(not is_disjoint_from(Literal[1, 2], Literal[2, 3]))
static_assert(not is_disjoint_from(Literal[0, 1], AlwaysTruthy))

static_assert(not is_disjoint_from(Intersection[int, Literal[2]], Literal[2]))
```

## Class, module and function literals

```py
from types import ModuleType, FunctionType
from knot_extensions import TypeOf, is_disjoint_from, static_assert

class A: ...
class B: ...

static_assert(is_disjoint_from(TypeOf[A], TypeOf[B]))
static_assert(is_disjoint_from(TypeOf[A], type[B]))
static_assert(not is_disjoint_from(type[A], TypeOf[A]))
static_assert(not is_disjoint_from(type[A], type[B]))

import random
import math

static_assert(is_disjoint_from(TypeOf[random], TypeOf[math]))
static_assert(not is_disjoint_from(TypeOf[random], ModuleType))
static_assert(not is_disjoint_from(TypeOf[random], object))

def f(): ...
def g(): ...

static_assert(is_disjoint_from(TypeOf[f], TypeOf[g]))
static_assert(not is_disjoint_from(TypeOf[f], FunctionType))
static_assert(not is_disjoint_from(TypeOf[f], object))
```

## Module literals

```py
from knot_extensions import TypeOf, is_disjoint_from, static_assert
```

## Instance types versus `type[T]` types

An instance type is disjoint from a `type[T]` type if the instance type is `@final` and the class of
the instance type is not a subclass of `T`'s metaclass.

```py
from typing import final
from knot_extensions import is_disjoint_from, static_assert

@final
class Foo: ...

static_assert(is_disjoint_from(Foo, type[int]))
static_assert(is_disjoint_from(type[object], Foo))
static_assert(is_disjoint_from(type[dict], Foo))

# Instance types can be disjoint from `type[]` types
# even if the instance type is a subtype of `type`

@final
class Meta1(type): ...

class UsesMeta1(metaclass=Meta1): ...

static_assert(not is_disjoint_from(Meta1, type[UsesMeta1]))

class Meta2(type): ...
class UsesMeta2(metaclass=Meta2): ...

static_assert(not is_disjoint_from(Meta2, type[UsesMeta2]))
static_assert(is_disjoint_from(Meta1, type[UsesMeta2]))
```

## `type[T]` versus `type[S]`

By the same token, `type[T]` is disjoint from `type[S]` if the metaclass of `T` is disjoint from the
metaclass of `S`.

```py
from typing import final
from knot_extensions import static_assert, is_disjoint_from

@final
class Meta1(type): ...

class Meta2(type): ...
class UsesMeta1(metaclass=Meta1): ...
class UsesMeta2(metaclass=Meta2): ...

static_assert(is_disjoint_from(type[UsesMeta1], type[UsesMeta2]))
```
