## Single-valued types

A type is single-valued iff it is not empty and all inhabitants of it compare equal.

```py
import types
from typing_extensions import Any, Literal, LiteralString, Never, Callable
from ty_extensions import is_single_valued, static_assert, TypeOf

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

An enum literal is only considered single-valued if it has no custom `__eq__`/`__ne__` method, or if
these methods always return `True`/`False`, respectively. Otherwise, the single member of the enum
literal type might not compare equal to itself.

```py
from ty_extensions import is_single_valued, static_assert, TypeOf
from enum import Enum

class NormalEnum(Enum):
    NO = 0
    YES = 1

class ComparesEqualEnum(Enum):
    NO = 0
    YES = 1

    def __eq__(self, other: object) -> Literal[True]:
        return True

class CustomEqEnum(Enum):
    NO = 0
    YES = 1

    def __eq__(self, other: object) -> bool:
        return False

class CustomNeEnum(Enum):
    NO = 0
    YES = 1

    def __ne__(self, other: object) -> bool:
        return False

static_assert(is_single_valued(Literal[NormalEnum.NO]))
static_assert(is_single_valued(Literal[NormalEnum.YES]))

static_assert(is_single_valued(Literal[ComparesEqualEnum.NO]))
static_assert(is_single_valued(Literal[ComparesEqualEnum.YES]))

static_assert(not is_single_valued(Literal[CustomEqEnum.NO]))
static_assert(not is_single_valued(Literal[CustomEqEnum.YES]))

static_assert(not is_single_valued(Literal[CustomNeEnum.NO]))
static_assert(not is_single_valued(Literal[CustomNeEnum.YES]))
```
