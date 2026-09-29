from collections import abc
from collections.abc import AsyncIterator as AI, Iterator as I
from contextlib import asynccontextmanager as acm, contextmanager as cm
from typing import Iterator, AsyncIterator
import contextlib as cl
import typing as t


@cm
def simple() -> Iterator[int]:
    yield 1


@acm
async def asynchronous() -> AsyncIterator[str]:
    yield "value"


@cl.contextmanager
def aliased() -> I[str]:
    yield "value"


@cl.asynccontextmanager
async def aliased_async() -> AI[int]:
    yield 1


@cm
def qualified() -> abc.Iterator[tuple[int, str]]:
    yield (1, "value")


@cm
def quoted() -> "t.Iterator[str]":
    yield "value"


@acm
async def quoted_async() -> 't.AsyncIterator[int]':
    yield 1


@cm
def complex_string() -> "Iter" "ator[int]":
    yield 1


@cm
def trailing_comma() -> Iterator[
    int,  # yielded type
]:
    yield 1


@cm
def parenthesized_item() -> Iterator[(int),]:
    yield 1


@cm
def parenthesized_tuple() -> Iterator[(int,)]:
    yield 1


@cm
def quoted_parenthesized_item() -> "Iterator[(int),]":
    yield 1


@cm
def returns_value() -> Iterator[int]:
    yield 1
    return "done"


@cm
def yields_from() -> Iterator[int]:
    yield from (1, 2)


@other_decorator
@cm
def outer_decorator() -> Iterator[int]:
    yield 1


# These do not apply a context manager directly to a generator function.
@cm
@other_decorator
def inner_decorator() -> Iterator[int]:
    yield 1


@cm
def returns_iterator() -> Iterator[int]:
    return iter([1])


@cm
def nested_yield() -> Iterator[int]:
    def inner():
        yield 1
    return inner()


@cm
def lambda_yield() -> Iterator[int]:
    inner = lambda: (yield 1)
    return inner()


def not_decorated() -> Iterator[int]:
    yield 1


@cm
def already_correct() -> t.Generator[int, None, None]:
    yield 1


@cm
def wrong_iterator() -> AsyncIterator[int]:
    yield 1


@acm
async def wrong_async_iterator() -> Iterator[int]:
    yield 1


@cm
def bare_iterator() -> Iterator:
    yield 1


@cm
def invalid_arguments() -> Iterator[int, str]:
    yield 1
