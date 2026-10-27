# Generic inference converges when loops pass recursively growing values directly or via callbacks.

from typing import Callable, Generic, Iterable, Protocol, TypeVar, overload

T = TypeVar("T")


def consume_iterable(values: Iterable[T] | int) -> list[T]:
    raise NotImplementedError


def wrap_iterable(paths: list[int], start: int):
    values = (start,)
    for _ in paths:
        values = (consume_iterable(values),)


def consume_callback(value: Callable[[], Iterable[T]]) -> list[T]:
    raise NotImplementedError


def wrap_callback(paths: list[int], start: int):
    values = (start,)
    for _ in paths:
        values = (consume_callback(lambda: values),)


def nest_callable(value: Callable[[T], T]) -> Callable[[list[T]], list[T]]:
    raise NotImplementedError


def wrap_callable(initial: Callable[[int], int], running: bool):
    value = initial
    while running:
        value = nest_callable(value)


# Walking a recursive protocol with growing type arguments must terminate.
class Recursive(Protocol[T]):
    def next(self) -> tuple[T, "Recursive[list[T]]"]: ...


def consume_recursive(value: Recursive[T]) -> T:
    raise NotImplementedError


def wrap_recursive(paths: list[int]):
    values = None
    for _ in paths:
        values = [values]
    if values is not None:
        consume_recursive(values[0])


# Repeated overloaded generic calls converge with conditional loop feedback.
T_co = TypeVar("T_co", covariant=True)


class Box(Generic[T_co]):
    def get(self) -> T_co:
        raise NotImplementedError


@overload
def extract(a: Box[T_co], b: object, /) -> T_co: ...
@overload
def extract(a: object, b: Box[T_co], /) -> T_co: ...
def extract(a, b):
    raise NotImplementedError


def repeat_extraction(value, choices):
    result = value
    for choice in choices:
        result = extract(result, result)
        if choice:
            result = extract(result, value)
