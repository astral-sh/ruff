from __future__ import annotations

from typing_extensions import TYPE_CHECKING

if TYPE_CHECKING:
    from pandas import DataFrame


def example() -> DataFrame:
    x = DataFrame()
