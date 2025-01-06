from dataclasses import dataclass
from enum import Enum, Flag, IntEnum, IntFlag, StrEnum, ReprEnum

import attr
import attrs


## Errors

@dataclass
class E(Enum): ...


@dataclass  # Foobar
class E(Flag): ...


@dataclass()
class E(IntEnum): ...


@dataclass()  # Foobar
class E(IntFlag): ...


@dataclass(
    frozen=True
)
class E(StrEnum): ...


@dataclass(  # Foobar
    frozen=True
)
class E(ReprEnum): ...


@dataclass(
    frozen=True
)  # Foobar
class E(Enum): ...


## No errors

@attrs.define
class E(Enum): ...


@attrs.frozen
class E(Enum): ...


@attrs.mutable
class E(Enum): ...


@attr.s
class E(Enum): ...
