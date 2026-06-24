"""Fix "for x in f.xreadlines()" -> "for x in f".

This fixer will also convert g(f.xreadlines) into g(f.__iter__).
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixXreadlines(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
