"""Fixer for __metaclass__ = X -> (metaclass=X) methods.

The various forms of classef (inherits nothing, inherits once, inherits
many) don't parse the same in the CST so we look at ALL classes for
a __metaclass__ and if we find one normalize the inherits to all be
an arglist.

For one-liner classes ('class X: pass') there is no indent/dedent so
we normalize those into having a suite.

Moving the __metaclass__ into the classdef can also cause the class
body to be empty so there is some special casing for that as well.

This fixer also tries very hard to keep original indenting and spacing
in all those corner cases.

"""

from collections.abc import Generator
from typing import ClassVar, Literal

from .. import fixer_base
from ..pytree import Base

def has_metaclass(parent):
    """we have to check the cls_node without changing it.
    There are two possibilities:
      1)  clsdef => suite => simple_stmt => expr_stmt => Leaf('__meta')
      2)  clsdef => simple_stmt => expr_stmt => Leaf('__meta')
    """

def fixup_parse_tree(cls_node) -> None:
    """one-line classes don't get a suite in the parse tree so we add
    one to normalize the tree
    """

def fixup_simple_stmt(parent, i, stmt_node) -> None:
    """if there is a semi-colon all the parts count as part of the same
    simple_stmt.  We just want the __metaclass__ part so we move
    everything after the semi-colon into its own simple_stmt node
    """

def remove_trailing_newline(node) -> None: ...
def find_metas(cls_node) -> Generator[tuple[Base, int, Base], None, None]: ...
def fixup_indent(suite) -> None:
    """If an INDENT is followed by a thing with a prefix then nuke the prefix
    Otherwise we get in trouble when removing __metaclass__ at suite start
    """

class FixMetaclass(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results) -> None: ...
