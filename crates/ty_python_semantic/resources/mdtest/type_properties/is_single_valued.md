## Single-valued types

A type is single-valued iff it is not empty and all inhabitants of it compare equal.

```py
import types
from typing_extensions import Any, Literal, LiteralString, Never, Callable
from knot_extensions import is_single_valued, static_assert, TypeOf

static_assert(is_single_valued(None))
static_assert(is_single_valued(Literal[True]))
static_assert(is_single_valued(Literal[1]))
static_assert(is_single_valued(Literal["abc"]))
static_assert(is_single_valued(Literal[b"abc"]))

static_assert(is_single_valued(tuple[()]))
static_assert(is_single_valued(tuple[Literal[True], Literal[1]]))

static_assert(not is_single_valued(str))
static_assert(not is_single_valued(Never))
static_assert(not is_single_valued(Any))

static_assert(not is_single_valued(Literal[1, 2]))

static_assert(not is_single_valued(tuple[None, int]))

static_assert(not is_single_valued(Callable[..., None]))
static_assert(not is_single_valued(Callable[[int, str], None]))

class A:
    def method(self): ...

static_assert(is_single_valued(TypeOf[A().method]))
static_assert(is_single_valued(TypeOf[types.FunctionType.__get__]))
static_assert(is_single_valued(TypeOf[A.method.__get__]))
```
