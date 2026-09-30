from collections import abc
from collections.abc import AsyncIterator as AI, Generator, Iterator as I
from contextlib import asynccontextmanager, contextmanager
from contextlib import contextmanager as cm
from typing import Iterator, AsyncIterator
import contextlib as cl
import typing as t


@contextmanager
def simple() -> Iterator[int]:
    yield 1


@asynccontextmanager
async def asynchronous() -> AsyncIterator[str]:
    yield "value"


@cm
def aliased() -> I[str]:
    yield "value"


@cl.asynccontextmanager
async def aliased_async() -> AI[int]:
    yield 1


@cl.contextmanager
def qualified() -> abc.Iterator[tuple[int, str]]:
    yield (1, "value")


@contextmanager
def quoted() -> "t.Iterator[str]":
    yield "value"


@asynccontextmanager
async def quoted_async() -> 't.AsyncIterator[int]':
    yield 1


@contextmanager
def complex_string() -> "Iter" "ator[int]":
    yield 1


@contextmanager
def trailing_comma() -> Iterator[
    int,  # yielded type
]:
    yield 1


@contextmanager
def parenthesized_item() -> Iterator[(int),]:
    yield 1


@contextmanager
def parenthesized_tuple() -> Iterator[(int,)]:
    yield 1


@contextmanager
def quoted_parenthesized_item() -> "Iterator[(int),]":
    yield 1


@contextmanager
def returns_value() -> Iterator[int]:
    yield 1
    return "done"


@contextmanager
def yields_from() -> Iterator[int]:
    yield from (1, 2)


@other_decorator
@contextmanager
def outer_decorator() -> Iterator[int]:
    yield 1


# These do not apply a context manager directly to a generator function.
@contextmanager
@other_decorator
def inner_decorator() -> Iterator[int]:
    yield 1


@contextmanager
def returns_iterator() -> Iterator[int]:
    return iter([1])


@contextmanager
def nested_yield() -> Iterator[int]:
    def inner():
        yield 1
    return inner()


@contextmanager
def lambda_yield() -> Iterator[int]:
    inner = lambda: (yield 1)
    return inner()


def not_decorated() -> Iterator[int]:
    yield 1


@contextmanager
def already_correct() -> t.Generator[int, None, None]:
    yield 1


@contextmanager
def wrong_iterator() -> AsyncIterator[int]:
    yield 1


@asynccontextmanager
async def wrong_async_iterator() -> Iterator[int]:
    yield 1


@contextmanager
def bare_iterator() -> Iterator:
    yield 1


@contextmanager
def invalid_arguments() -> Iterator[int, str]:
    yield 1


# A shadowed typing alias forces reuse of collections.abc.Generator, which
# cannot be subscripted on Python 3.7.
def shadowed_typing(t, object):
    @contextmanager
    def version_specific() -> Iterator[int]:
        yield 1
        return "done"


def shadowed_generator(t, Generator):
    @contextmanager
    def cannot_fix() -> Iterator[int]:
        yield 1
