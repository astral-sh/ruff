"""Fixer for execfile.

This converts usages of the execfile function into calls to the built-in
exec() function.
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixExecfile(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
