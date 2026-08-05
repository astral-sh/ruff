"""Regression test for https://github.com/astral-sh/ruff/issues/4171."""

from collections.abc import Callable
from typing import Any, TypeVar

from typing_extensions import dataclass_transform


def default_function() -> list[int]:
    return []


def model_field(*, default: Any = ..., resolver: Callable[[], Any] | None = None) -> Any:
    return default


_T = TypeVar("_T")


@dataclass_transform(kw_only_default=True, field_specifiers=(model_field,))
def create_model(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@create_model(init=False)
class CustomerModel:
    # A call to a declared field specifier is fine, mirroring `dataclasses.field()`.
    fine_resolver: int = model_field(resolver=lambda: 0)

    # A call that isn't a declared field specifier should still be flagged.
    hidden_mutable_default: list[int] = default_function()


# A `dataclass_transform` with no `field_specifiers` treats every call as a plain default.
@dataclass_transform()
def create_model_no_specifiers(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@create_model_no_specifiers()
class NoFieldSpecifiers:
    hidden_mutable_default: list[int] = default_function()


# Classes decorated by a function that is not itself `dataclass_transform`-marked
# should not be treated as dataclasses.
def not_a_transform(*, init: bool = True) -> Callable[[type[_T]], type[_T]]:
    def wrap(cls: type[_T]) -> type[_T]:
        return cls

    return wrap


@not_a_transform()
class NotATransform:
    fine_default: list[int] = default_function()
