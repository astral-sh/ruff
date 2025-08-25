# https://github.com/python/typeshed/blob/main/stubs/networkx/networkx/algorithms/community/louvain.pyi
from _typeshed import Incomplete
from collections.abc import Generator

from networkx.classes.graph import Graph, _Node
from networkx.utils.backends import _dispatchable
from numpy.random import RandomState

__all__ = ["louvain_communities", "louvain_partitions"]

@_dispatchable
def louvain_communities(
        G: Graph[_Node],
        weight: str | None = "weight",
        resolution: float | None = 1,
        threshold: float | None = 1e-07,
        max_level: int | None = None,
        seed: int | RandomState | None = None,
) -> list[set[Incomplete]]: ...
@_dispatchable
def louvain_partitions(
        G: Graph[_Node],
        weight: str | None = "weight",
        resolution: float | None = 1,
        threshold: float | None = 1e-07,
        seed: int | RandomState | None = None,
) -> Generator[list[set[Incomplete]], None, None]: ...