import typing
from typing import Optional


def f(x: Optional[str]) -> None:
    ...


def f(x: typing.Optional[str]) -> None:
    ...


def f() -> None:
    x: Optional[str]
    x = Optional[str]


def f(x: list[Optional[int]]) -> None:
    ...


def f(x: Optional[int : float]) -> None:
    ...


def f(x: Optional[str, int : float]) -> None:
    ...


def f(x: Optional[int, float]) -> None:
    ...


# Regression test for: https://github.com/astral-sh/ruff/issues/7131
class ServiceRefOrValue:
    service_specification: Optional[
        list[ServiceSpecificationRef]
        | list[ServiceSpecification]
    ] = None


# Regression test for: https://github.com/astral-sh/ruff/issues/7201
class ServiceRefOrValue:
    service_specification: Optional[str]is not True = None
