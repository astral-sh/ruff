import builtins
from typing import Union


w: builtins.type[int] | builtins.type[str] | builtins.type[complex]
x: type[int] | type[str] | type[float]
y: builtins.type[int] | type[str] | builtins.type[complex]
z: Union[type[float], type[complex]]
z: Union[type[float, int], type[complex]]


def func(arg: type[int] | str | type[float]) -> None: ...

# OK
x: type[int, str, float]
y: builtins.type[int, str, complex]
z: Union[float, complex]


def func(arg: type[int, float] | str) -> None: ...
