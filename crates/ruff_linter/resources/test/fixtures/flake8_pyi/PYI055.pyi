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

# OK
item: type[requests_mock.Mocker] | type[httpretty] = requests_mock.Mocker

def func():
    # PYI055
    item: type[requests_mock.Mocker] | type[httpretty] | type[str] = requests_mock.Mocker
    item2: Union[type[requests_mock.Mocker], type[httpretty], type[str]] = requests_mock.Mocker
