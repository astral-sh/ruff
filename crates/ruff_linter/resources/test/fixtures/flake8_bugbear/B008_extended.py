from typing import List

import fastapi
import custom
from fastapi import Query


def okay(db=fastapi.Depends(get_db)):
    ...


def okay(data: List[str] = fastapi.Query(None)):
    ...


def okay(data: List[str] = Query(None)):
    ...


def okay(data: custom.ImmutableTypeA = foo()):
    ...


def error_due_to_missing_import(data: List[str] = Depends(None)):
    ...


class Class:
    pass


def okay(obj=Class()):
    ...


def error(obj=OtherClass()):
    ...


# https://github.com/astral-sh/ruff/issues/12717

from typing import NewType

N = NewType("N", int)
L = NewType("L", list[str])

def okay(obj = N()): ...
def error(obj = L()): ...
