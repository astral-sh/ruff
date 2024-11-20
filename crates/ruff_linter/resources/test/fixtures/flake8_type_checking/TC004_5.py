from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import List, Sequence, Set


def example(a: List[int], /, b: Sequence[int], *, c: Set[int]):
    return
