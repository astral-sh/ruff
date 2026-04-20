"""Fixer for apply().

This converts apply(func, v, k) into (func)(*v, **k).
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixApply(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
