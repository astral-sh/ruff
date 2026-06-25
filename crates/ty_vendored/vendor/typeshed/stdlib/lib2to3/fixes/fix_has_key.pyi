"""Fixer for has_key().

Calls to .has_key() methods are expressed in terms of the 'in'
operator:

    d.has_key(k) -> k in d

CAVEATS:
1) While the primary target of this fixer is dict.has_key(), the
   fixer will change any has_key() method call, regardless of its
   class.

2) Cases like this will not be converted:

    m = d.has_key
    if m(k):
        ...

   Only *calls* to has_key() are converted. While it is possible to
   convert the above to something like

    m = d.__contains__
    if m(k):
        ...

   this is currently not done.
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixHasKey(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
