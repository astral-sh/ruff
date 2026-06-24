import typing
from typing import ClassVar, Sequence

import attr, attrs
from attr import s
from attrs import define, frozen

KNOWINGLY_MUTABLE_DEFAULT = []


@define
class A:
    mutable_default: list[int] = []
    immutable_annotation: typing.Sequence[int] = []
    without_annotation = []
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: typing.ClassVar[list[int]] = []


@frozen
class B:
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: ClassVar[list[int]] = []


@attr.s
class C:
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: ClassVar[list[int]] = []

@s
class D:
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: ClassVar[list[int]] = []


# Mutable defaults wrapped in field() calls
@define
class E:
    mutable_default: list[int] = attrs.field(default=[])  # RUF008
    mutable_default2: dict[str, int] = attrs.field(default={})  # RUF008
    mutable_default3: set[int] = attrs.field(default=set())  # RUF008
    mutable_default4: dict[str, int] = attrs.field(default=dict())  # RUF008
    correct_factory: list[int] = attrs.field(factory=list)  # okay
    immutable_default: tuple[int, ...] = attrs.field(default=())  # okay
    immutable_default2: str = attrs.field(default="hello")  # okay
    immutable_default3: int = attrs.field(default=1)  # okay
    non_mutable_var: list[int] = attrs.field(default=KNOWINGLY_MUTABLE_DEFAULT)  # okay
    class_variable: ClassVar[list[int]] = attrs.field(default=[])  # okay


@attr.s
class F:
    mutable_default: list[int] = attr.ib(default=[])  # RUF008
    mutable_default2: dict[str, int] = attr.ib(default={})  # RUF008
    correct_factory: list[int] = attr.ib(factory=list)  # okay
    immutable_default: tuple[int, ...] = attr.ib(default=())  # okay


@attr.s
class G:
    mutable_default: list[int] = attr.attrib(default=[])  # RUF008
    mutable_default2: set[int] = attr.attrib(default=set())  # RUF008
    correct_factory: list[int] = attr.attrib(factory=list)  # okay
