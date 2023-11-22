"""Tests to determine first-party import classification.

For typing-only import detection tests, see `TCH002.py`.
"""


def f():
    import TYP001

    x: TYP001


def f():
    import TYP001

    print(TYP001)


def f():
    from . import TYP001

    x: TYP001


def f():
    from . import TYP001

    print(TYP001)
