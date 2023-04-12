from dataclasses import dataclass
from typing import NamedTuple


def default_function() -> list[int]:
    return []


class ImmutableType(NamedTuple):
    something: int = 8


@dataclass()
class A:
    hidden_mutable_default: list[int] = default_function()


DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES = ImmutableType(40)
DEFAULT_A_FOR_ALL_DATACLASSES = A([1, 2, 3])


@dataclass
class B:
    hidden_mutable_default: list[int] = default_function()
    another_dataclass: A = A()
    not_optimal: ImmutableType = ImmutableType(20)
    good_variant: ImmutableType = DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES
    okay_variant: A = DEFAULT_A_FOR_ALL_DATACLASSES
