from typing import TYPE_CHECKING, List

if TYPE_CHECKING:
    pass  # TC005


def example():
    if TYPE_CHECKING:
        pass  # TC005
    return


class Test:
    if TYPE_CHECKING:
        pass  # TC005
    x = 2


if TYPE_CHECKING:
    if 2:
        pass


if TYPE_CHECKING:
    x: List


from typing_extensions import TYPE_CHECKING

if TYPE_CHECKING:
    pass  # TC005

# https://github.com/astral-sh/ruff/issues/11368
if TYPE_CHECKING:
    pass
else:
    pass
if TYPE_CHECKING:
    pass
elif test:
    pass
