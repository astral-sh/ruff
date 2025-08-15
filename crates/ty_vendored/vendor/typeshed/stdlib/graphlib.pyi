import sys
from _typeshed import SupportsItems
from collections.abc import Iterable
from typing import Any, Generic, TypeVar, overload

__all__ = ["TopologicalSorter", "CycleError"]

_T = TypeVar("_T")

if sys.version_info >= (3, 11):
    from types import GenericAlias

class TopologicalSorter(Generic[_T]):
    """Provides functionality to topologically sort a graph of hashable nodes"""

    @overload
    def __init__(self, graph: None = None) -> None: ...
    @overload
    def __init__(self, graph: SupportsItems[_T, Iterable[_T]]) -> None: ...
    def add(self, node: _T, *predecessors: _T) -> None:
        """Add a new node and its predecessors to the graph.

        Both the *node* and all elements in *predecessors* must be hashable.

        If called multiple times with the same node argument, the set of dependencies
        will be the union of all dependencies passed in.

        It is possible to add a node with no dependencies (*predecessors* is not provided)
        as well as provide a dependency twice. If a node that has not been provided before
        is included among *predecessors* it will be automatically added to the graph with
        no predecessors of its own.

        Raises ValueError if called after "prepare".
        """

    def prepare(self) -> None:
        """Mark the graph as finished and check for cycles in the graph.

        If any cycle is detected, "CycleError" will be raised, but "get_ready" can
        still be used to obtain as many nodes as possible until cycles block more
        progress. After a call to this function, the graph cannot be modified and
        therefore no more nodes can be added using "add".

        Raise ValueError if nodes have already been passed out of the sorter.

        """

    def is_active(self) -> bool:
        """Return ``True`` if more progress can be made and ``False`` otherwise.

        Progress can be made if cycles do not block the resolution and either there
        are still nodes ready that haven't yet been returned by "get_ready" or the
        number of nodes marked "done" is less than the number that have been returned
        by "get_ready".

        Raises ValueError if called without calling "prepare" previously.
        """

    def __bool__(self) -> bool: ...
    def done(self, *nodes: _T) -> None:
        """Marks a set of nodes returned by "get_ready" as processed.

        This method unblocks any successor of each node in *nodes* for being returned
        in the future by a call to "get_ready".

        Raises ValueError if any node in *nodes* has already been marked as
        processed by a previous call to this method, if a node was not added to the
        graph by using "add" or if called without calling "prepare" previously or if
        node has not yet been returned by "get_ready".
        """

    def get_ready(self) -> tuple[_T, ...]:
        """Return a tuple of all the nodes that are ready.

        Initially it returns all nodes with no predecessors; once those are marked
        as processed by calling "done", further calls will return all new nodes that
        have all their predecessors already processed. Once no more progress can be made,
        empty tuples are returned.

        Raises ValueError if called without calling "prepare" previously.
        """

    def static_order(self) -> Iterable[_T]:
        """Returns an iterable of nodes in a topological order.

        The particular order that is returned may depend on the specific
        order in which the items were inserted in the graph.

        Using this method does not require to call "prepare" or "done". If any
        cycle is detected, :exc:`CycleError` will be raised.
        """
    if sys.version_info >= (3, 11):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """Represent a PEP 585 generic type

            E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
            """

class CycleError(ValueError):
    """Subclass of ValueError raised by TopologicalSorter.prepare if cycles
    exist in the working graph.

    If multiple cycles exist, only one undefined choice among them will be reported
    and included in the exception. The detected cycle can be accessed via the second
    element in the *args* attribute of the exception instance and consists in a list
    of nodes, such that each node is, in the graph, an immediate predecessor of the
    next node in the list. In the reported list, the first and the last node will be
    the same, to make it clear that it is cyclic.
    """
