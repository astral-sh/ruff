"""Regression test for https://github.com/astral-sh/ruff/issues/15812"""

from typing import Union

from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T")
V = TypeVar("V")
Json = TypeAliasType(
    "Json",
    "Union[dict[str, Json], list[Json], str, int, float, bool, T, V, None]",
    type_params=(T, V),
)
