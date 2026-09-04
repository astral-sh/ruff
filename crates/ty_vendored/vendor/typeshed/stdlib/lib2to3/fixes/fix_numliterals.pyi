"""Fixer that turns 1L into 1, 0755 into 0o755."""

from typing import ClassVar, Literal

from .. import fixer_base

class FixNumliterals(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[False]]
    def match(self, node): ...
    def transform(self, node, results): ...
