from typing import TYPE_CHECKING, List

if TYPE_CHECKING:
    pass  # TCH005


if False:
    pass  # TCH005

if 0:
    pass  # TCH005


def example():
    if TYPE_CHECKING:
        pass  # TCH005
    return


class Test:
    if TYPE_CHECKING:
        pass  # TCH005
    x = 2


if TYPE_CHECKING:
    if 2:
        pass


if TYPE_CHECKING:
    x: List


if False:
    x: List

if 0:
    x: List
