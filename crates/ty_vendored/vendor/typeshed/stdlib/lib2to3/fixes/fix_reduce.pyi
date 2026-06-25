"""Fixer for reduce().

Makes sure reduce() is imported from the functools module if reduce is
used in that module.
"""

from lib2to3 import fixer_base
from typing import ClassVar, Literal

class FixReduce(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    order: ClassVar[Literal["pre"]]
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
