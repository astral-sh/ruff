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


# Test for: https://github.com/astral-sh/ruff/issues/18508
# Optional[None] should not be offered a fix
foo: Optional[None] = None


from typing import NamedTuple, Optional

import typing_extensions
from typing_extensions import (
    NamedTuple as NamedTupleTE,
    Optional as OptionalTE,
)

# Regression test for https://github.com/astral-sh/ruff/issues/18619
# Don't emit lint for `NamedTuple`
a1: Optional[NamedTuple] = None
a2: typing.Optional[NamedTuple] = None
a3: OptionalTE[NamedTuple] = None
a4: typing_extensions.Optional[NamedTuple] = None
a5: Optional[typing.NamedTuple] = None
a6: typing.Optional[typing.NamedTuple] = None
a7: OptionalTE[typing.NamedTuple] = None
a8: typing_extensions.Optional[typing.NamedTuple] = None
a9: "Optional[NamedTuple]" = None
a10: Optional[NamedTupleTE] = None


# Test for: https://github.com/astral-sh/ruff/issues/19746
# Nested Optional types should be flattened
nested_optional: Optional[Optional[str]] = None
nested_optional_typing: typing.Optional[Optional[int]] = None
triple_nested_optional: Optional[Optional[Optional[str]]] = None
