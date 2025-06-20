"""Tests to determine standard library import classification.

For typing-only import detection tests, see `TC002.py`.
"""


def f():
    import os

    x: os


def f():
    import os

    print(os)


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
