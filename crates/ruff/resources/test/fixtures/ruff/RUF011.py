import datetime
import typing
from typing import NamedTuple
from dataclasses import field

def default_function() -> list[int]:
    return []


class ImmutableType(NamedTuple):
    something: int = 8


class A:
    hidden_mutable_default: list[int] = default_function()
    class_variable: typing.ClassVar[list[int]] = default_function()
    fine_date: datetime.date = datetime.date(2042, 1, 1)


DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES = ImmutableType(40)
DEFAULT_A_FOR_ALL_DATACLASSES = A([1, 2, 3])


class B:
    hidden_mutable_default: list[int] = default_function()
    another_class: A = A()
    not_optimal: ImmutableType = ImmutableType(20)
    good_variant: ImmutableType = DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES
    okay_variant: A = DEFAULT_A_FOR_ALL_DATACLASSES
    not_fine_field: list[int] = field(default_vactory=list)
