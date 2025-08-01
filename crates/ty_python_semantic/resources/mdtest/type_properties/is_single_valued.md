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

Tuple types are not single-valued, because the type `tuple[int, str]` means "any two-element
instance of `tuple` where the first element is `int` and the second is `str`, *or* any instance of a
subclass of `tuple[int, str]`". It's easy to create a subclass of `tuple[int, str]` that is
Liskov-compliant but not single-valued, which therefore means that `tuple[int, str]` cannot be
single-valued, as it is a supertype of its subclass, so can only be single-valued if all its
subtypes are single-valued:

```py
from ty_extensions import is_single_valued, static_assert

class EmptyTupleSubclass(tuple[()]):
    def __eq__(self, other: object) -> bool:
        return isinstance(other, EmptyTupleSubclass) and len(other) == 0

static_assert(not is_single_valued(tuple[()]))
static_assert(not is_single_valued(EmptyTupleSubclass))

class SingleElementTupleSubclass(tuple[int]):
    def __eq__(self, other: object) -> bool:
        return isinstance(other, SingleElementTupleSubclass) and len(other) == 1 and isinstance(other[0], int)

static_assert(not is_single_valued(tuple[int]))
static_assert(not is_single_valued(SingleElementTupleSubclass))

class TwoElementTupleSubclass(tuple[str, bytes]):
    def __eq__(self, other: object) -> bool:
        return (
            isinstance(other, TwoElementTupleSubclass)
            and len(other) == 2
            and isinstance(other[0], str)
            and isinstance(other[1], bytes)
        )

static_assert(not is_single_valued(tuple[str, bytes]))
static_assert(not is_single_valued(TwoElementTupleSubclass))
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
