"""Module implements the Mersenne Twister random number generator."""

import sys
from typing_extensions import Self, TypeAlias, disjoint_base

# Actually Tuple[(int,) * 625]
_State: TypeAlias = tuple[int, ...]

@disjoint_base
class Random:
    """Random() -> create a random number generator with its own internal state."""

    if sys.version_info >= (3, 10):
        def __init__(self, seed: object = ..., /) -> None: ...
    else:
        def __new__(self, seed: object = ..., /) -> Self: ...

    def seed(self, n: object = None, /) -> None:
        """seed([n]) -> None.

        Defaults to use urandom and falls back to a combination
        of the current time and the process identifier.
        """

    def getstate(self) -> _State:
        """getstate() -> tuple containing the current state."""

    def setstate(self, state: _State, /) -> None:
        """setstate(state) -> None.  Restores generator state."""

    def random(self) -> float:
        """random() -> x in the interval [0, 1)."""

    def getrandbits(self, k: int, /) -> int:
        """getrandbits(k) -> x.  Generates an int with k random bits."""
