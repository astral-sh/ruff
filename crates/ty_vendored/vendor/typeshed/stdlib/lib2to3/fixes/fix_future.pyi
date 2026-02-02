"""Remove __future__ imports

from __future__ import foo is replaced with an empty line.
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixFuture(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
