"""
Regression test for an ecosystem hit on
https://github.com/astral-sh/ruff/pull/21125.

We should mark all of the components of special dataclass annotations as
runtime-required, not just the first layer.
"""

from dataclasses import dataclass
from typing import ClassVar, Optional


@dataclass(frozen=True)
class EmptyCell:
    _singleton: ClassVar[Optional["EmptyCell"]] = None
    # the behavior of _singleton above should match a non-ClassVar
    _doubleton: "EmptyCell"
