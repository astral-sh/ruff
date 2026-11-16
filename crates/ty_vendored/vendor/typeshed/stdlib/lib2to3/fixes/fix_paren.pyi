"""Fixer that adds parentheses where they are required

This converts ``[x for x in 1, 2]`` to ``[x for x in (1, 2)]``.
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixParen(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
