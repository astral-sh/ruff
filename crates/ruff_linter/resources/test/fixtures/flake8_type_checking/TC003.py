"""Tests to determine standard library import classification.

For typing-only import detection tests, see `TC002.py`.
"""


def f():
    import os

    x: os


def f():
    import os

    print(os)
