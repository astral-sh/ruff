from collections.abc import AsyncIterator, Iterator
from contextlib import asynccontextmanager, contextmanager
from typing import AsyncGenerator, Generator, Iterator as TypingIterator


@contextmanager
def synchronous() -> Iterator[int]:
    yield 1


@asynccontextmanager
async def asynchronous() -> AsyncIterator[int]:
    yield 1


@contextmanager
def already_typing() -> TypingIterator[int]:
    yield 1
