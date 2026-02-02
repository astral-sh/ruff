from typing_extensions import TypeVar, Callable, Concatenate, ParamSpec

_T = TypeVar("_T")
_P = ParamSpec("_P")

def f(self, callable: Callable[Concatenate[_T, _P], _T]) -> Callable[_P, _T]: ...
