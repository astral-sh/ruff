from typing import (
    Union,
)

from typing_extensions import (
    TypeAlias,
)

# Type aliases not flagged
TA0: TypeAlias = int
TA1: TypeAlias = int | float | bool
TA2: TypeAlias = Union[int, float, bool]


def good1(arg: int) -> int | bool: ...


def good2(arg: int, arg2: int | bool) -> None: ...


def f0(arg1: float | int) -> None: ...  # PYI041


def f1(arg1: float, *, arg2: float | list[str] | type[bool] | complex) -> None: ...  # PYI041


def f2(arg1: int, /, arg2: int | int | float) -> None: ...  # PYI041


def f3(arg1: int, *args: Union[int | int | float]) -> None: ...  # PYI041


async def f4(**kwargs: int | int | float) -> None: ...  # PYI041

def f5(
    arg: Union[  # comment 
        float, # another
        complex, int]
    ) -> None: ...  # PYI041

def f6(
    arg: (
        int | # comment
        float |  # another
        complex
    )    
    ) -> None: ... # PYI041

def f5(arg1: int, *args: Union[int, int, float]) -> None: ...  # PYI041


def f6(arg1: int, *args: Union[Union[int, int, float]]) -> None: ...  # PYI041


def f7(arg1: int, *args: Union[Union[Union[int, int, float]]]) -> None: ...  # PYI041


def f8(arg1: int, *args: Union[Union[Union[int | int | float]]]) -> None: ...  # PYI041


class Foo:
    def good(self, arg: int) -> None: ...

    def bad(self, arg: int | float | complex) -> None: ...  # PYI041

    def bad2(self, arg: int | Union[float, complex]) -> None: ...  # PYI041

    def bad3(self, arg: Union[Union[float, complex], int]) -> None: ...  # PYI041

    def bad4(self, arg: Union[float | complex, int]) -> None: ...  # PYI041

    def bad5(self, arg: int | (float | complex)) -> None: ...  # PYI041


# https://github.com/astral-sh/ruff/issues/18298
# fix must not yield runtime `None | None | ...` (TypeError)
class Issue18298:
    def f1(self, arg: None | int | None | float = None) -> None: ...  # PYI041 - with fix

    def f3(self, arg: None | float | None | int | None = None) -> None: ...  # PYI041 - with fix
