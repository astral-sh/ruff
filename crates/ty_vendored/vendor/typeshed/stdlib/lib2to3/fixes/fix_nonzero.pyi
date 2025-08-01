"""Fixer for __nonzero__ -> __bool__ methods."""

from typing import ClassVar, Literal

from .. import fixer_base

class FixNonzero(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
