import datetime
import re
import typing
from dataclasses import dataclass, field
from pathlib import Path
from typing import ClassVar, NamedTuple


def default_function() -> list[int]:
    return []


class ImmutableType(NamedTuple):
    something: int = 8


@dataclass()
class A:
    hidden_mutable_default: list[int] = default_function()
    class_variable: typing.ClassVar[list[int]] = default_function()
    another_class_var: ClassVar[list[int]] = default_function()

    fine_path: Path = Path()
    fine_date: datetime.date = datetime.date(2042, 1, 1)
    fine_timedelta: datetime.timedelta = datetime.timedelta(hours=7)
    fine_tuple: tuple[int] = tuple([1])
    fine_regex: re.Pattern = re.compile(r".*")


DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES = ImmutableType(40)
DEFAULT_A_FOR_ALL_DATACLASSES = A([1, 2, 3])


@dataclass
class B:
    hidden_mutable_default: list[int] = default_function()
    another_dataclass: A = A()
    not_optimal: ImmutableType = ImmutableType(20)
    good_variant: ImmutableType = DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES
    okay_variant: A = DEFAULT_A_FOR_ALL_DATACLASSES

    fine_dataclass_function: list[int] = field(default_factory=list)
