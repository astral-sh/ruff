from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Tuple


def foo():
    # UP037
    x: "Tuple[int, int]" = (0, 0)
    print(x)


# OK
X: "Tuple[int, int]" = (0, 0)
