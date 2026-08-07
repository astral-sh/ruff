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


def f():
    import pathlib

    type Paths = list[pathlib.Path]


def f():
    import pathlib

    type Paths = list[pathlib.Path]

    print(Paths)


def f():
    import pathlib

    type Paths = list[pathlib.Path]
    type PathsMapping = dict[str, Paths]

    # FIXME: false positive for indirect runtime use of Paths
    print(PathsMapping)
