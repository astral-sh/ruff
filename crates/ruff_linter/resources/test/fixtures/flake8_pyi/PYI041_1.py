from typing import (
    TYPE_CHECKING,
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


def f5(arg1: int, *args: Union[int, int, float]) -> None: 
    ...


def f6(arg1: int, *args: Union[Union[int, int, float]]) -> None: 
    ...


def f7(arg1: int, *args: Union[Union[Union[int, int, float]]]) -> None: 
    ...


def f8(arg1: int, *args: Union[Union[Union[int | int | float]]]) -> None: 
    ...


def f9(
    arg: Union[  # comment 
        float, # another
        complex, int]
    ) -> None: 
    ...

def f10(
    arg: (
        int | # comment
        float |  # another
        complex
    )    
    ) -> None: 
    ...


class Foo:
    def good(self, arg: int) -> None:
        ...

    def bad(self, arg: int | float | complex) -> None:
        ...

    def bad2(self, arg: int | Union[float, complex]) -> None: 
        ...

    def bad3(self, arg: Union[Union[float, complex], int]) -> None: 
        ...

    def bad4(self, arg: Union[float | complex, int]) -> None: 
        ...

    def bad5(self, arg: int | (float | complex)) -> None: 
        ...


# https://github.com/astral-sh/ruff/issues/18298
# fix must not yield runtime `None | None | ...` (TypeError)
class Issue18298:
    def f1(self, arg: None | int | None | float = None) -> None:  # PYI041 - no fix
        pass

    if TYPE_CHECKING:

        def f2(self, arg: None | int | None | float = None) -> None: ...  # PYI041 - with fix

    else:

        def f2(self, arg=None) -> None:
            pass

    def f3(self, arg: None | float | None | int | None = None) -> None:  # PYI041 - with fix
        pass
