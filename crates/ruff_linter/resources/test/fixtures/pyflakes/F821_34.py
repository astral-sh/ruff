from __future__ import annotations

from typing import Callable


def demonstrate_bare_local_annotation():
    x: int
    print(x)

demonstrate_bare_local_annotation()


def make_closure_pair() -> tuple[Callable[[], int], Callable[[int], None]]:
    x: int

    def get_value() -> int:
        return x

    def set_value(new_value: int) -> None:
        nonlocal x
        x = new_value

    return get_value, set_value

get_value, set_value = make_closure_pair()
set_value(123)
print(get_value())
