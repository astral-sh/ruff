def f():
    from typing import Literal

    # OK
    x: Literal["foo"]


def f():
    from ..typical import Literal

    # OK
    x: Literal["foo"]


def f():
    from .typical import Literal

    # F821
    x: Literal["foo"]


def f():
    from .atypical import Literal

    # F821
    x: Literal["foo"]
