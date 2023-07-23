import typing
from typing import TypeVar

_T = typing.TypeVar("_T")
_P = TypeVar("_P")

# OK
_UsedTypeVar = TypeVar("_UsedTypeVar")
def func(arg: _UsedTypeVar) -> _UsedTypeVar: ...
