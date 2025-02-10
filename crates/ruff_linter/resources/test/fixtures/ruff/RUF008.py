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
