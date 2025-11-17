"""Tests to determine standard library import classification.

For typing-only import detection tests, see `TC002.py`.
"""


def f():
    import os

    x: os


def f():
    import os

    print(os)


# regression test for https://github.com/astral-sh/ruff/issues/21121
from dataclasses import KW_ONLY, dataclass


@dataclass
class DataClass:
    a: int
    _: KW_ONLY  # should be an exception to TC003, even with future-annotations
    b: int
