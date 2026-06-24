from __future__ import annotations

from typing import Callable


# Error: bare annotation without assignment in same scope.
def demonstrate_bare_local_annotation():
    x: int
    print(x)


# Error: bare annotation read from closure without nonlocal.
def demonstrate_closure_without_nonlocal() -> Callable[[], int]:
    x: int

    def get_value() -> int:
        return x

    return get_value


# OK: nonlocal declaration exists, so the annotation may be initialized.
def make_closure_pair() -> tuple[Callable[[], int], Callable[[int], None]]:
    x: int

    def get_value() -> int:
        return x

    def set_value(new_value: int) -> None:
        nonlocal x
        x = new_value

    return get_value, set_value


# OK: nonlocal declaration exists, outer scope reads after inner scope may assign.
def demonstrate_nonlocal_rebinding_then_outer_read() -> int:
    x: int

    def set_value(new_value: int) -> None:
        nonlocal x
        x = new_value

    set_value(1)
    return x
