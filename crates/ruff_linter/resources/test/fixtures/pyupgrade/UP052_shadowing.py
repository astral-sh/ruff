from contextlib import asynccontextmanager, contextmanager
from typing import AsyncIterator, Iterator

object = 42


@contextmanager
def value_return() -> Iterator[int]:
    yield 1
    return "done"


@asynccontextmanager
async def async_value() -> AsyncIterator[int]:
    yield 1


def still_used() -> Iterator[int]:
    return iter([1])


def outer():
    Generator = 1

    @contextmanager
    def shadowed() -> Iterator[int]:
        yield 1

    return Generator, shadowed
