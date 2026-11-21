# A default or a second argument supplies a concrete type while each iteration adds a tuple layer.
from collections.abc import Sequence


def wrap[T = int](items: list[T]) -> list[tuple[T]]:
    return [(item,) for item in items]


def defaulted():
    items = [1]
    for _ in range(2):
        items = wrap(items)
    return items


def wrap_with_seed[U](items: Sequence[U], seed: U) -> Sequence[tuple[U]]:
    raise NotImplementedError


def seeded(start: Sequence[int]):
    items = start
    for _ in range(2):
        items = wrap_with_seed(items, 1)
    return items
