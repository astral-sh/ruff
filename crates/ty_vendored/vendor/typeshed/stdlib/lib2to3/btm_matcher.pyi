"""A bottom-up tree matching algorithm implementation meant to speed
up 2to3's matching process. After the tree patterns are reduced to
their rarest linear path, a linear Aho-Corasick automaton is
created. The linear automaton traverses the linear paths from the
leaves to the root of the AST and returns a set of nodes for further
matching. This reduces significantly the number of candidate nodes.
"""

from _typeshed import Incomplete, SupportsGetItem
from collections import defaultdict
from collections.abc import Iterable

from .fixer_base import BaseFix
from .pytree import Leaf, Node

class BMNode:
    """Class for a node of the Aho-Corasick automaton used in matching"""

    count: Incomplete
    transition_table: Incomplete
    fixers: Incomplete
    id: Incomplete
    content: str
    def __init__(self) -> None: ...

class BottomMatcher:
    """The main matcher class. After instantiating the patterns should
    be added using the add_fixer method
    """

    match: Incomplete
    root: Incomplete
    nodes: Incomplete
    fixers: Incomplete
    logger: Incomplete
    def __init__(self) -> None: ...
    def add_fixer(self, fixer: BaseFix) -> None:
        """Reduces a fixer's pattern tree to a linear path and adds it
        to the matcher(a common Aho-Corasick automaton). The fixer is
        appended on the matching states and called when they are
        reached
        """

    def add(self, pattern: SupportsGetItem[int | slice, Incomplete] | None, start: BMNode) -> list[BMNode]:
        """Recursively adds a linear pattern to the AC automaton"""

    def run(self, leaves: Iterable[Leaf]) -> defaultdict[BaseFix, list[Node | Leaf]]:
        """The main interface with the bottom matcher. The tree is
        traversed from the bottom using the constructed
        automaton. Nodes are only checked once as the tree is
        retraversed. When the automaton fails, we give it one more
        shot(in case the above tree matches as a whole with the
        rejected leaf), then we break for the next leaf. There is the
        special case of multiple arguments(see code comments) where we
        recheck the nodes

        Args:
           The leaves of the AST tree to be matched

        Returns:
           A dictionary of node matches with fixers as the keys
        """

    def print_ac(self) -> None:
        """Prints a graphviz diagram of the BM automaton(for debugging)"""

def type_repr(type_num: int) -> str | int: ...
