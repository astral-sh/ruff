import typing
from typing import TypeVar

_T = typing.TypeVar("_T")
_P = TypeVar("_P")

# OK
_UsedTypeVar = TypeVar("_UsedTypeVar")
def func(arg: _UsedTypeVar) -> _UsedTypeVar: ...

_A, _B = TypeVar("_A"), TypeVar("_B")
_C = _D = TypeVar("_C")
