"""Adjust some old Python 2 idioms to their modern counterparts.

* Change some type comparisons to isinstance() calls:
    type(x) == T -> isinstance(x, T)
    type(x) is T -> isinstance(x, T)
    type(x) != T -> not isinstance(x, T)
    type(x) is not T -> not isinstance(x, T)

* Change "while 1:" into "while True:".

* Change both

    v = list(EXPR)
    v.sort()
    foo(v)

and the more general

    v = EXPR
    v.sort()
    foo(v)

into

    v = sorted(EXPR)
    foo(v)
"""

from typing import ClassVar, Final, Literal

from .. import fixer_base

CMP: Final[str]
TYPE: Final[str]

class FixIdioms(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[False]]
    PATTERN: ClassVar[str]
    def match(self, node): ...
    def transform(self, node, results): ...
    def transform_isinstance(self, node, results): ...
    def transform_while(self, node, results) -> None: ...
    def transform_sort(self, node, results) -> None: ...
