"""Fixer for itertools.(imap|ifilter|izip) --> (map|filter|zip) and
itertools.ifilterfalse --> itertools.filterfalse (bugs 2360-2363)

imports from itertools are fixed in fix_itertools_import.py

If itertools is imported as something else (ie: import itertools as it;
it.izip(spam, eggs)) method calls will not get fixed.
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixItertools(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    it_funcs: str
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
