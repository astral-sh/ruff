"""Module implements the Mersenne Twister random number generator."""

from typing_extensions import TypeAlias

# Actually Tuple[(int,) * 625]
_State: TypeAlias = tuple[int, ...]

class Random:
    """Random() -> create a random number generator with its own internal state."""

    def __init__(self, seed: object = ...) -> None: ...
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
