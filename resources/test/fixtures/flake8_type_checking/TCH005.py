from typing import TYPE_CHECKING, List

if TYPE_CHECKING:
    pass  # TCH005


def example():
    if TYPE_CHECKING:
        pass  # TYP005
    return


class Test:
    if TYPE_CHECKING:
        pass  # TYP005
    x = 2


if TYPE_CHECKING:
    if 2:
        pass


if TYPE_CHECKING:
    x: List
