from collections.abc import Generator
from contextlib import contextmanager
from typing import Iterator


@contextmanager
def context() -> Iterator[int]:
    yield 1
