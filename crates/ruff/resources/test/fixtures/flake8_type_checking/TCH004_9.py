from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Tuple, List, Dict

x: Tuple


class C:
    x: List


def f():
    x: Dict
