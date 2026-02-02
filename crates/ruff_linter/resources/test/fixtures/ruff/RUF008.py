import typing
from dataclasses import dataclass, field
from typing import ClassVar, Sequence

KNOWINGLY_MUTABLE_DEFAULT = []


@dataclass
class A:
    mutable_default: list[int] = []
    immutable_annotation: typing.Sequence[int] = []
    without_annotation = []
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: typing.ClassVar[list[int]] = []


@dataclass
class B:
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: ClassVar[list[int]] = []

# Lint should account for deferred annotations
# See https://github.com/astral-sh/ruff/issues/15857
@dataclass
class AWithQuotes:
    mutable_default: 'list[int]' = []
    immutable_annotation: 'typing.Sequence[int]' = []
    without_annotation = []
    correct_code: 'list[int]' = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: 'list[int]' = field(default_factory=list)
    class_variable: 'typing.ClassVar[list[int]]'= []


# Mutable defaults wrapped in field() calls
@dataclass
class C:
    mutable_default: list[int] = field(default=[])
    mutable_default2: dict[str, int] = field(default={})
    mutable_default3: set[int] = field(default=set())
    mutable_default4: dict[str, int] = field(default=dict())
    correct_factory: list[int] = field(default_factory=list)
    immutable_default: tuple[int, ...] = field(default=())
    immutable_default2: str = field(default="hello")
    immutable_default3: int = field(default=1)
    non_mutable_var: list[int] = field(default=KNOWINGLY_MUTABLE_DEFAULT)
    class_variable: ClassVar[list[int]] = field(default=[])
