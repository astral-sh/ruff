"""Fixer for dict methods.

d.keys() -> list(d.keys())
d.items() -> list(d.items())
d.values() -> list(d.values())

d.iterkeys() -> iter(d.keys())
d.iteritems() -> iter(d.items())
d.itervalues() -> iter(d.values())

d.viewkeys() -> d.keys()
d.viewitems() -> d.items()
d.viewvalues() -> d.values()

Except in certain very specific contexts: the iter() can be dropped
when the context is list(), sorted(), iter() or for...in; the list()
can be dropped when the context is list() or sorted() (but not iter()
or for...in!). Special contexts that apply to both: list(), sorted(), tuple()
set(), any(), all(), sum().

Note: iter(d.keys()) could be written as iter(d) but since the
original d.iterkeys() was also redundant we don't fix this.  And there
are (rare) contexts where it makes a difference (e.g. when passing it
as an argument to a function that introspects the argument).
"""

from _typeshed import Incomplete
from typing import ClassVar, Literal

from .. import fixer_base

iter_exempt: set[str]

class FixDict(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
    P1: ClassVar[str]
    p1: ClassVar[Incomplete]
    P2: ClassVar[str]
    p2: ClassVar[Incomplete]
    def in_special_context(self, node, isiter): ...
