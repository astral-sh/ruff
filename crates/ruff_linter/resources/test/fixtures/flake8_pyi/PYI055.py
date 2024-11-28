import builtins
from typing import Union

s: builtins.type[int] | builtins.type[str] | builtins.type[complex]
t: type[int] | type[str] | type[float]
u: builtins.type[int] | type[str] | builtins.type[complex]
v: Union[type[float], type[complex]]
w: Union[type[float | int], type[complex]]
x: Union[Union[type[Union[float, int]], type[complex]]]
y: Union[Union[Union[type[float | int], type[complex]]]]
z: Union[type[complex], Union[Union[type[Union[float, int]]]]]


def func(arg: type[int] | str | type[float]) -> None:
    ...


# OK
x: type[int | str | float]
y: builtins.type[int | str | complex]
z: Union[float, complex]


def func(arg: type[int, float] | str) -> None:
    ...


# OK
item: type[requests_mock.Mocker] | type[httpretty] = requests_mock.Mocker


def func():
    # PYI055
    x: type[requests_mock.Mocker] | type[httpretty] | type[str] = requests_mock.Mocker
    y: Union[type[requests_mock.Mocker], type[httpretty], type[str]] = requests_mock.Mocker
    z: Union[  # comment
        type[requests_mock.Mocker], # another comment
        type[httpretty], type[str]] = requests_mock.Mocker


def func():
    from typing import Union as U

    # PYI055
    x: Union[type[requests_mock.Mocker], type[httpretty], type[str]] = requests_mock.Mocker


def convert_union(union: UnionType) -> _T | None:
    converters: tuple[
        type[_T] | type[Converter[_T]] | Converter[_T] | Callable[[str], _T], ...  # PYI055
    ] = union.__args__
    ...

def convert_union(union: UnionType) -> _T | None:
    converters: tuple[
        Union[type[_T] | type[Converter[_T]] | Converter[_T] | Callable[[str], _T]], ...  # PYI055
    ] = union.__args__
    ...

def convert_union(union: UnionType) -> _T | None:
    converters: tuple[
        Union[type[_T] | type[Converter[_T]]] | Converter[_T] | Callable[[str], _T], ...  # PYI055
    ] = union.__args__
    ...

def convert_union(union: UnionType) -> _T | None:
    converters: tuple[
        Union[type[_T] | type[Converter[_T]] | str] | Converter[_T] | Callable[[str], _T], ...  # PYI055
    ] = union.__args__
    ...


# `type[float, int]`` is not valid, use `type[float|int]` or `type[Union[float, int]]`
# OK for PYI055, should be covered by another check.
a: Union[type[float, int], type[complex]]  
b: Union[Union[type[float, int], type[complex]]]
c: Union[Union[Union[type[float, int], type[complex]]]]
d: Union[type[complex], Union[Union[type[float, int]]]]
