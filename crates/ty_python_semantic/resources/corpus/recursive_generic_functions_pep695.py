# Generic inference converges when loops pass recursively growing values directly or via callbacks.

from typing import Callable, Iterable, Protocol, overload


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


def nest_callable[T](value: Callable[[T], T]) -> Callable[[list[T]], list[T]]:
    raise NotImplementedError


def wrap_callable(initial: Callable[[int], int], running: bool):
    value = initial
    while running:
        value = nest_callable(value)


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


# Repeated overloaded generic calls converge with conditional loop feedback.
class Box[T]:
    def get(self) -> T:
        raise NotImplementedError


@overload
def extract[T](a: Box[T], b: object, /) -> T: ...
@overload
def extract[T](a: object, b: Box[T], /) -> T: ...
def extract(a, b):
    raise NotImplementedError


def repeat_extraction(value, choices):
    result = value
    for choice in choices:
        result = extract(result, result)
        if choice:
            result = extract(result, value)
