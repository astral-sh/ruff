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

class EmptyTupleSubclass(tuple[()]): ...
class HeterogeneousTupleSubclass(tuple[Literal[True], Literal[1]]): ...

# N.B. this follows from the fact that `EmptyTupleSubclass` is a subtype of `tuple[()]`,
# and any property recognised for `tuple[()]` should therefore also be recognised for
# `EmptyTupleSubclass` since an `EmptyTupleSubclass` instance can be used anywhere where
# `tuple[()]` is accepted. This is only sound, however, if we ban `__eq__` and `__ne__`
# from being overridden on a tuple subclass. This is something we plan to do as part of
# our implementation of the Liskov Substitution Principle
# (https://github.com/astral-sh/ty/issues/166)
static_assert(is_single_valued(EmptyTupleSubclass))
static_assert(is_single_valued(HeterogeneousTupleSubclass))

static_assert(not is_single_valued(str))
static_assert(not is_single_valued(Never))
static_assert(not is_single_valued(Any))

static_assert(not is_single_valued(Literal[1, 2]))

static_assert(not is_single_valued(tuple[None, int]))

class MultiValuedHeterogeneousTupleSubclass(tuple[None, int]): ...

static_assert(not is_single_valued(MultiValuedHeterogeneousTupleSubclass))

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

class SingleValuedEnum(Enum):
    VALUE = 1

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

class StrEnum(str, Enum):
    A = "a"
    B = "b"

class IntEnum(int, Enum):
    A = 1
    B = 2

static_assert(is_single_valued(Literal[NormalEnum.NO]))
static_assert(is_single_valued(Literal[NormalEnum.YES]))
static_assert(not is_single_valued(NormalEnum))

static_assert(is_single_valued(Literal[SingleValuedEnum.VALUE]))
static_assert(is_single_valued(SingleValuedEnum))

static_assert(is_single_valued(Literal[ComparesEqualEnum.NO]))
static_assert(is_single_valued(Literal[ComparesEqualEnum.YES]))
static_assert(not is_single_valued(ComparesEqualEnum))

static_assert(not is_single_valued(Literal[CustomEqEnum.NO]))
static_assert(not is_single_valued(Literal[CustomEqEnum.YES]))
static_assert(not is_single_valued(CustomEqEnum))

static_assert(not is_single_valued(Literal[CustomNeEnum.NO]))
static_assert(not is_single_valued(Literal[CustomNeEnum.YES]))
static_assert(not is_single_valued(CustomNeEnum))

static_assert(is_single_valued(Literal[StrEnum.A]))
static_assert(is_single_valued(Literal[StrEnum.B]))
static_assert(not is_single_valued(StrEnum))

static_assert(is_single_valued(Literal[IntEnum.A]))
static_assert(is_single_valued(Literal[IntEnum.B]))
static_assert(not is_single_valued(IntEnum))
```
