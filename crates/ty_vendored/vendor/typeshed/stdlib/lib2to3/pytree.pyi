"""
Python parse tree definitions.

This is a very concrete parse tree; we need to keep every token and
even the comments and whitespace between tokens.

There's also a pattern matching implementation here.
"""

from _typeshed import Incomplete, SupportsGetItem, SupportsLenAndGetItem, Unused
from abc import abstractmethod
from collections.abc import Iterable, Iterator, MutableSequence
from typing import ClassVar, Final
from typing_extensions import Self, TypeAlias

from .fixer_base import BaseFix
from .pgen2.grammar import Grammar

_NL: TypeAlias = Node | Leaf
_Context: TypeAlias = tuple[str, int, int]
_Results: TypeAlias = dict[str, _NL]
_RawNode: TypeAlias = tuple[int, str, _Context, list[_NL] | None]

HUGE: Final = 0x7FFFFFFF

def type_repr(type_num: int) -> str | int: ...

class Base:
    """
    Abstract base class for Node and Leaf.

    This provides some default functionality and boilerplate using the
    template pattern.

    A node may be a subnode of at most one parent.
    """

    type: int
    parent: Node | None
    prefix: str
    children: list[_NL]
    was_changed: bool
    was_checked: bool
    def __eq__(self, other: object) -> bool:
        """
        Compare two nodes for equality.

        This calls the method _eq().
        """
    __hash__: ClassVar[None]  # type: ignore[assignment]
    @abstractmethod
    def _eq(self, other: Base) -> bool:
        """
        Compare two nodes for equality.

        This is called by __eq__ and __ne__.  It is only called if the two nodes
        have the same type.  This must be implemented by the concrete subclass.
        Nodes should be considered equal if they have the same structure,
        ignoring the prefix string and other context information.
        """

    @abstractmethod
    def clone(self) -> Self:
        """
        Return a cloned (deep) copy of self.

        This must be implemented by the concrete subclass.
        """

    @abstractmethod
    def post_order(self) -> Iterator[Self]:
        """
        Return a post-order iterator for the tree.

        This must be implemented by the concrete subclass.
        """

    @abstractmethod
    def pre_order(self) -> Iterator[Self]:
        """
        Return a pre-order iterator for the tree.

        This must be implemented by the concrete subclass.
        """

    def replace(self, new: _NL | list[_NL]) -> None:
        """Replace this node with a new one in the parent."""

    def get_lineno(self) -> int:
        """Return the line number which generated the invocant node."""

    def changed(self) -> None: ...
    def remove(self) -> int | None:
        """
        Remove the node from the tree. Returns the position of the node in its
        parent's children before it was removed.
        """

    @property
    def next_sibling(self) -> _NL | None:
        """
        The node immediately following the invocant in their parent's children
        list. If the invocant does not have a next sibling, it is None
        """

    @property
    def prev_sibling(self) -> _NL | None:
        """
        The node immediately preceding the invocant in their parent's children
        list. If the invocant does not have a previous sibling, it is None.
        """

    def leaves(self) -> Iterator[Leaf]: ...
    def depth(self) -> int: ...
    def get_suffix(self) -> str:
        """
        Return the string immediately following the invocant node. This is
        effectively equivalent to node.next_sibling.prefix
        """

class Node(Base):
    """Concrete implementation for interior nodes."""

    fixers_applied: MutableSequence[BaseFix] | None
    # Is Unbound until set in refactor.RefactoringTool
    future_features: frozenset[Incomplete]
    # Is Unbound until set in pgen2.parse.Parser.pop
    used_names: set[str]
    def __init__(
        self,
        type: int,
        children: Iterable[_NL],
        context: Unused = None,
        prefix: str | None = None,
        fixers_applied: MutableSequence[BaseFix] | None = None,
    ) -> None:
        """
        Initializer.

        Takes a type constant (a symbol number >= 256), a sequence of
        child nodes, and an optional context keyword argument.

        As a side effect, the parent pointers of the children are updated.
        """

    def _eq(self, other: Base) -> bool:
        """Compare two nodes for equality."""

    def clone(self) -> Node:
        """Return a cloned (deep) copy of self."""

    def post_order(self) -> Iterator[Self]:
        """Return a post-order iterator for the tree."""

    def pre_order(self) -> Iterator[Self]:
        """Return a pre-order iterator for the tree."""

    def set_child(self, i: int, child: _NL) -> None:
        """
        Equivalent to 'node.children[i] = child'. This method also sets the
        child's parent attribute appropriately.
        """

    def insert_child(self, i: int, child: _NL) -> None:
        """
        Equivalent to 'node.children.insert(i, child)'. This method also sets
        the child's parent attribute appropriately.
        """

    def append_child(self, child: _NL) -> None:
        """
        Equivalent to 'node.children.append(child)'. This method also sets the
        child's parent attribute appropriately.
        """

    def __unicode__(self) -> str:
        """
        Return a pretty string representation.

        This reproduces the input source exactly.
        """

class Leaf(Base):
    """Concrete implementation for leaf nodes."""

    lineno: int
    column: int
    value: str
    fixers_applied: MutableSequence[BaseFix]
    def __init__(
        self,
        type: int,
        value: str,
        context: _Context | None = None,
        prefix: str | None = None,
        fixers_applied: MutableSequence[BaseFix] = [],
    ) -> None:
        """
        Initializer.

        Takes a type constant (a token number < 256), a string value, and an
        optional context keyword argument.
        """

    def _eq(self, other: Base) -> bool:
        """Compare two nodes for equality."""

    def clone(self) -> Leaf:
        """Return a cloned (deep) copy of self."""

    def post_order(self) -> Iterator[Self]:
        """Return a post-order iterator for the tree."""

    def pre_order(self) -> Iterator[Self]:
        """Return a pre-order iterator for the tree."""

    def __unicode__(self) -> str:
        """
        Return a pretty string representation.

        This reproduces the input source exactly.
        """

def convert(gr: Grammar, raw_node: _RawNode) -> _NL:
    """
    Convert raw node information to a Node or Leaf instance.

    This is passed to the parser driver which calls it whenever a reduction of a
    grammar rule produces a new complete node, so that the tree is build
    strictly bottom-up.
    """

class BasePattern:
    """
    A pattern is a tree matching pattern.

    It looks for a specific node type (token or symbol), and
    optionally for a specific content.

    This is an abstract base class.  There are three concrete
    subclasses:

    - LeafPattern matches a single leaf node;
    - NodePattern matches a single node (usually non-leaf);
    - WildcardPattern matches a sequence of nodes of variable length.
    """

    type: int
    content: str | None
    name: str | None
    def optimize(self) -> BasePattern:  # sic, subclasses are free to optimize themselves into different patterns
        """
        A subclass can define this as a hook for optimizations.

        Returns either self or another node with the same effect.
        """

    def match(self, node: _NL, results: _Results | None = None) -> bool:
        """
        Does this pattern exactly match a node?

        Returns True if it matches, False if not.

        If results is not None, it must be a dict which will be
        updated with the nodes matching named subpatterns.

        Default implementation for non-wildcard patterns.
        """

    def match_seq(self, nodes: SupportsLenAndGetItem[_NL], results: _Results | None = None) -> bool:
        """
        Does this pattern exactly match a sequence of nodes?

        Default implementation for non-wildcard patterns.
        """

    def generate_matches(self, nodes: SupportsGetItem[int, _NL]) -> Iterator[tuple[int, _Results]]:
        """
        Generator yielding all matches for this pattern.

        Default implementation for non-wildcard patterns.
        """

class LeafPattern(BasePattern):
    def __init__(self, type: int | None = None, content: str | None = None, name: str | None = None) -> None:
        """
        Initializer.  Takes optional type, content, and name.

        The type, if given must be a token type (< 256).  If not given,
        this matches any *leaf* node; the content may still be required.

        The content, if given, must be a string.

        If a name is given, the matching node is stored in the results
        dict under that key.
        """

class NodePattern(BasePattern):
    wildcards: bool
    def __init__(self, type: int | None = None, content: str | None = None, name: str | None = None) -> None:
        """
        Initializer.  Takes optional type, content, and name.

        The type, if given, must be a symbol type (>= 256).  If the
        type is None this matches *any* single node (leaf or not),
        except if content is not None, in which it only matches
        non-leaf nodes that also match the content pattern.

        The content, if not None, must be a sequence of Patterns that
        must match the node's children exactly.  If the content is
        given, the type must not be None.

        If a name is given, the matching node is stored in the results
        dict under that key.
        """

class WildcardPattern(BasePattern):
    """
    A wildcard pattern can match zero or more nodes.

    This has all the flexibility needed to implement patterns like:

    .*      .+      .?      .{m,n}
    (a b c | d e | f)
    (...)*  (...)+  (...)?  (...){m,n}

    except it always uses non-greedy matching.
    """

    min: int
    max: int
    def __init__(self, content: str | None = None, min: int = 0, max: int = 0x7FFFFFFF, name: str | None = None) -> None:
        """
        Initializer.

        Args:
            content: optional sequence of subsequences of patterns;
                     if absent, matches one node;
                     if present, each subsequence is an alternative [*]
            min: optional minimum number of times to match, default 0
            max: optional maximum number of times to match, default HUGE
            name: optional name assigned to this match

        [*] Thus, if content is [[a, b, c], [d, e], [f, g, h]] this is
            equivalent to (a b c | d e | f g h); if content is None,
            this is equivalent to '.' in regular expression terms.
            The min and max parameters work as follows:
                min=0, max=maxint: .*
                min=1, max=maxint: .+
                min=0, max=1: .?
                min=1, max=1: .
            If content is not None, replace the dot with the parenthesized
            list of alternatives, e.g. (a b c | d e | f g h)*
        """

class NegatedPattern(BasePattern):
    def __init__(self, content: str | None = None) -> None:
        """
        Initializer.

        The argument is either a pattern or None.  If it is None, this
        only matches an empty sequence (effectively '$' in regex
        lingo).  If it is not None, this matches whenever the argument
        pattern doesn't have any matches.
        """

def generate_matches(
    patterns: SupportsGetItem[int | slice, BasePattern] | None, nodes: SupportsGetItem[int | slice, _NL]
) -> Iterator[tuple[int, _Results]]:
    """
    Generator yielding matches for a sequence of patterns and nodes.

    Args:
        patterns: a sequence of patterns
        nodes: a sequence of nodes

    Yields:
        (count, results) tuples where:
        count: the entire sequence of patterns matches nodes[:count];
        results: dict containing named submatches.
    """
