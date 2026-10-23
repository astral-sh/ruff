# Generic inference converges when loops pass recursively growing values directly or via callbacks.

from typing import Callable, Iterable, Protocol


def consume_iterable[T](values: Iterable[T] | int) -> list[T]:
    raise NotImplementedError


def wrap_iterable(paths: list[int], start: int):
    values = (start,)
    for _ in paths:
        values = (consume_iterable(values),)


def consume_callback[T](value: Callable[[], Iterable[T]]) -> list[T]:
    raise NotImplementedError


def wrap_callback(paths: list[int], start: int):
    values = (start,)
    for _ in paths:
        values = (consume_callback(lambda: values),)


# Walking a recursive protocol with growing type arguments must terminate.
class Recursive[T](Protocol):
    def next(self) -> tuple[T, "Recursive[list[T]]"]: ...


def consume_recursive[T](value: Recursive[T]) -> T:
    raise NotImplementedError


def wrap_recursive(paths: list[int]):
    values = None
    for _ in paths:
        values = [values]
    if values is not None:
        consume_recursive(values[0])
