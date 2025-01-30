"""Regression test for https://github.com/astral-sh/ruff/issues/15812"""

from typing import Union

from typing_extensions import TypeAliasType

Json = TypeAliasType(
    value="Union[dict[str, Json], list[Json], str, int, float, bool, None]",
    name="Json",
)
