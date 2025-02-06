import typing
import typing_extensions
from typing import TypeVar
from typing_extensions import ParamSpec, TypeVarTuple


# OK
_UsedTypeVar = TypeVar("_UsedTypeVar")
def func(arg: _UsedTypeVar) -> _UsedTypeVar: ...

_A, _B = TypeVar("_A"), TypeVar("_B")
_C = _D = TypeVar("_C")
