from typing import (
    Union,
)

from typing_extensions import (
    TypeAlias,
)

TA0: TypeAlias = int
TA1: TypeAlias = int | float | bool
TA2: TypeAlias = Union[int, float, bool]


def good1(arg: int) -> int | bool:
    ...


def good2(arg: int, arg2: int | bool) -> None:
    ...


def f0(arg1: float | int) -> None:
    ...


def f1(arg1: float, *, arg2: float | list[str] | type[bool] | complex) -> None:
    ...


def f2(arg1: int, /, arg2: int | int | float) -> None:
    ...


def f3(arg1: int, *args: Union[int | int | float]) -> None:
    ...


async def f4(**kwargs: int | int | float) -> None:
    ...


class Foo:
    def good(self, arg: int) -> None:
        ...

    def bad(self, arg: int | float | complex) -> None:
        ...
