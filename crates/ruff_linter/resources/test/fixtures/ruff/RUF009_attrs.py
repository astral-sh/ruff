import datetime
import re
import typing
from collections import OrderedDict
from fractions import Fraction
from pathlib import Path
from typing import ClassVar, NamedTuple

import attr
from attrs import Factory, define, field, frozen


def default_function() -> list[int]:
    return []


class ImmutableType(NamedTuple):
    something: int = 8


@attr.s
class A:
    hidden_mutable_default: list[int] = default_function()
    class_variable: typing.ClassVar[list[int]] = default_function()
    another_class_var: ClassVar[list[int]] = default_function()

    fine_path: Path = Path()
    fine_date: datetime.date = datetime.date(2042, 1, 1)
    fine_timedelta: datetime.timedelta = datetime.timedelta(hours=7)
    fine_tuple: tuple[int] = tuple([1])
    fine_regex: re.Pattern = re.compile(r".*")
    fine_float: float = float("-inf")
    fine_int: int = int(12)
    fine_complex: complex = complex(1, 2)
    fine_str: str = str("foo")
    fine_bool: bool = bool("foo")
    fine_fraction: Fraction = Fraction(1, 2)


DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES = ImmutableType(40)
DEFAULT_A_FOR_ALL_DATACLASSES = A([1, 2, 3])


@define
class B:
    hidden_mutable_default: list[int] = default_function()
    another_dataclass: A = A()
    not_optimal: ImmutableType = ImmutableType(20)
    good_variant: ImmutableType = DEFAULT_IMMUTABLETYPE_FOR_ALL_DATACLASSES
    okay_variant: A = DEFAULT_A_FOR_ALL_DATACLASSES

    fine_dataclass_function: list[int] = field(default_factory=list)
    attrs_factory: dict[str, str] = Factory(OrderedDict)


class IntConversionDescriptor:
    def __init__(self, *, default):
        self._default = default

    def __set_name__(self, owner, name):
        self._name = "_" + name

    def __get__(self, obj, type):
        if obj is None:
            return self._default

        return getattr(obj, self._name, self._default)

    def __set__(self, obj, value):
        setattr(obj, self._name, int(value))


@frozen
class InventoryItem:
    quantity_on_hand: IntConversionDescriptor = IntConversionDescriptor(default=100)


# Test for:
# https://github.com/astral-sh/ruff/issues/17424
@frozen
class C:
    foo: int = 1


@attr.frozen
class D:
    foo: int = 1


@define
class E:
    c: C = C()
    d: D = D()


@attr.s
class F:
    foo: int = 1


@attr.mutable
class G:
    foo: int = 1


@attr.attrs
class H:
    f: F = F()
    g: G = G()


@attr.define
class I:
    f: F = F()
    g: G = G()


@attr.frozen
class J:
    f: F = F()
    g: G = G()


@attr.mutable
class K:
    f: F = F()
    g: G = G()


# Regression test for https://github.com/astral-sh/ruff/issues/19014
# These are all valid field calls and should not cause diagnostics.
@attr.define
class TestAttrField:
    attr_field_factory: list[int] = attr.field(factory=list)
    attr_field_default: list[int] = attr.field(default=attr.Factory(list))
    attr_factory: list[int] = attr.Factory(list)
    attr_ib: list[int] = attr.ib(factory=list)
    attr_attr: list[int] = attr.attr(factory=list)
    attr_attrib: list[int] = attr.attrib(factory=list)


@attr.attributes
class TestAttrAttributes:
    x: list[int] = list()  # RUF009
