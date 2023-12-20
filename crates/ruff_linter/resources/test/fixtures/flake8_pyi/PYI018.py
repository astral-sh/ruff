import typing
import typing_extensions
from typing import TypeVar
from typing_extensions import ParamSpec, TypeVarTuple

_T = typing.TypeVar("_T")
_Ts = typing_extensions.TypeVarTuple("_Ts")
_P = ParamSpec("_P")
_P2 = typing.ParamSpec("_P2")
_Ts2 = TypeVarTuple("_Ts2")

# OK
_UsedTypeVar = TypeVar("_UsedTypeVar")
def func(arg: _UsedTypeVar) -> _UsedTypeVar: ...

_A, _B = TypeVar("_A"), TypeVar("_B")
_C = _D = TypeVar("_C")
