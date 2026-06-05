import contextlib
from contextlib import asynccontextmanager

import pytest
import pytest_asyncio


# Errors

async def unsafe_yield():
    with open(""):
        yield  # ASYNC119


async def async_with():
    async with open(""):
        yield  # ASYNC119


async def warn_on_each_yield():
    with open(""):
        yield  # ASYNC119
        yield  # ASYNC119
    with open(""):
        yield  # ASYNC119
        yield  # ASYNC119


async def yield_in_nested_with():
    with open(""):
        with open(""):
            yield  # ASYNC119


# OK

async def yield_not_in_context_manager():
    yield
    with open(""):
        ...
    yield


async def yield_in_nested_sync_function():
    with open(""):
        def foo():
            yield


async def yield_in_nested_async_function():
    with open(""):
        async def foo():
            yield


async def yield_after_nested_function():
    with open(""):
        async def foo():
            yield
        yield  # ASYNC119


@asynccontextmanager
async def safe_with_decorator():
    with open(""):
        yield


@contextlib.asynccontextmanager
async def safe_with_qualified_decorator():
    with open(""):
        yield


def sync_generator():
    with open(""):
        yield


@pytest.fixture
async def safe_pytest_fixture():
    with open(""):
        yield


@pytest_asyncio.fixture
async def safe_pytest_asyncio_fixture():
    with open(""):
        yield
