"""Regression test for https://github.com/astral-sh/ruff/issues/4171."""

from collections.abc import Callable
from typing import ClassVar, TypeVar

from typing_extensions import dataclass_transform

_T = TypeVar("_T")


@dataclass_transform()
def create_model(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@create_model()
class CustomerModel:
    mutable_default: list[int] = []
    class_variable: ClassVar[list[int]] = []
