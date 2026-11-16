"""Fixer for unicode.

* Changes unicode to str and unichr to chr.

* If "...\\u..." is not unicode literal change it into "...\\\\u...".

* Change u"..." into "...".

"""

from _typeshed import StrPath
from typing import ClassVar, Literal

from .. import fixer_base
from ..pytree import Node

class FixUnicode(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    unicode_literals: bool
    def start_tree(self, tree: Node, filename: StrPath) -> None: ...
    def transform(self, node, results): ...
