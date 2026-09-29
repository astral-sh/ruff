from collections.abc import AsyncIterator as _AsyncIterator
from contextlib import asynccontextmanager as _asynccontextmanager, contextmanager as _contextmanager
from typing import Generator as _Generator, Iterator as _Iterator


@_contextmanager
def synchronous() -> "_Iterator[int]":
    yield 1


@_asynccontextmanager
async def asynchronous() -> "_AsyncIterator[str]":
    yield "value"
