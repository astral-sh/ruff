import typing
from typing import Sequence

KNOWINGLY_MUTABLE_DEFAULT = []


class A:
    mutable_default: list[int] = []
    immutable_annotation: typing.Sequence[int] = []
    without_annotation = []
    ignored_via_comment: list[int] = []  # noqa: RUF011
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT


class B:
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    ignored_via_comment: list[int] = []  # noqa: RUF011
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
