from collections.abc import Generator, AsyncGenerator


def func() -> Generator[int, None, None]:
    yield 42


def func() -> Generator[int, None]:
    yield 42


def func() -> Generator[int]:
    yield 42


def func() -> Generator[int, int, int]:
    foo = yield 42
    return foo


def func() -> Generator[int, int, None]:
    _ = yield 42
    return None


def func() -> Generator[int, None, int]:
    yield 42
    return 42


async def func() -> AsyncGenerator[int, None]:
    yield 42


async def func() -> AsyncGenerator[int]:
    yield 42


async def func() -> AsyncGenerator[int, int]:
    foo = yield 42
    return foo


from typing import Generator, AsyncGenerator


def func() -> Generator[str, None, None]:
    yield "hello"


async def func() -> AsyncGenerator[str, None]:
    yield "hello"


async def func() -> AsyncGenerator[  # type: ignore
    str,
    None
]:
    yield "hello"
