"""Fixer for import statements.
If spam is being imported from the local directory, this import:
    from spam import eggs
Becomes:
    from .spam import eggs

And this import:
    import spam
Becomes:
    from . import spam
"""

from _typeshed import StrPath
from collections.abc import Generator
from typing import ClassVar, Literal

from .. import fixer_base
from ..pytree import Node

def traverse_imports(names) -> Generator[str, None, None]:
    """
    Walks over all the names imported in a dotted_as_names node.
    """

class FixImport(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    skip: bool
    def start_tree(self, tree: Node, name: StrPath) -> None: ...
    def transform(self, node, results): ...
    def probably_a_local_import(self, imp_name): ...
