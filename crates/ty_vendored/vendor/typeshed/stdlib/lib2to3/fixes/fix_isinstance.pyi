"""Fixer that cleans up a tuple argument to isinstance after the tokens
in it were fixed.  This is mainly used to remove double occurrences of
tokens as a leftover of the long -> int / unicode -> str conversion.

eg.  isinstance(x, (int, long)) -> isinstance(x, (int, int))
       -> isinstance(x, int)
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixIsinstance(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
